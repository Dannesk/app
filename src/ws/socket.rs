//! The app's one socket. Connects to the proxy, which fans it to relay,
//! the Bitcoin relay inside indexd, rates and bookd (workspace/TRANSPORT-SPEC.md). This task owns the connection, the
//! reconnect, the framing, and the four transport bools; what each service's
//! frames MEAN is in `relay.rs` and `rates.rs`.
//!
//! Framing (§3): every frame is Binary, `[tag][flags][payload]`. Tag 0x00 is
//! the proxy's own link frame; 0x01–0x04 name a service. Flag bit 0 = zstd.
//!
//! Transport bools (§6, §7): `relay_ws_status` etc. keep their old meaning —
//! "can an answer from that server reach us" — and are now `socket up AND
//! link up`, where the link state comes from the proxy's frame. A service
//! dying behind the proxy no longer costs us the socket; it costs us one link,
//! and when that link rises we re-sync exactly what a reconnect used to.

use crate::channel::{CHANNEL, WSCommand};
use crate::ws::config::*;
use crate::ws::rates;
use crate::ws::relay::{self, RelayState};
use crate::ws::RatesCommand;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashSet;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Receiver;
use tokio::time::{timeout, Duration, Instant};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, client_async_tls, tungstenite::Message};

/// A connect that hasn't completed in this long is treated as failed. WITHOUT
/// this the task can hang forever: a weak or captive-portal network will
/// complete the TCP handshake and then stall the TLS one, and a bare
/// `connect_async` has no deadline of its own.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// No traffic at all for this long ⇒ assume a half-open socket and reconnect.
/// Safe because the proxy GUARANTEES traffic: a ping and a link frame every
/// 30 s. Three missed beats is a dead link, not a quiet market.
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// This socket carries balances and signing, so it retries fast — but not at a
/// flat rate forever, which on a dead network meant hundreds of futile attempts
/// an hour, all of them waking the radio.
const MIN_BACKOFF: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Default, Clone, Copy)]
struct Links {
    relay: bool,
    btc: bool,
    rates: bool,
    book: bool,
}

/// The health-map prefix each link's service reports under.
const LINK_PREFIXES: [(fn(&Links) -> bool, &str); 4] =
    [(|l| l.relay, "relay:"), (|l| l.btc, "btc:"), (|l| l.rates, "rates:"), (|l| l.book, "bookd:")];

/// Publish the transport bools, and take down the components of every link
/// that fell since `before` — a service we cannot reach is not `up`, whatever
/// its last frame said.
fn publish_with(before: Links, connected: bool, links: Links) {
    for (link, prefix) in LINK_PREFIXES {
        if link(&before) && !(connected && link(&links)) {
            CHANNEL.mark_link_down(prefix);
        }
    }
    publish(connected, links);
}

fn publish(connected: bool, links: Links) {
    let _ = CHANNEL.proxy_ws_status_tx.send(connected);
    let _ = CHANNEL.relay_ws_status_tx.send(connected && links.relay);
    let _ = CHANNEL.btc_ws_status_tx.send(connected && links.btc);
    let _ = CHANNEL.rates_ws_status_tx.send(connected && links.rates);
    let _ = CHANNEL.book_ws_status_tx.send(connected && links.book);
}

fn frame(tag: u8, payload: &str) -> Message {
    let mut out = Vec::with_capacity(payload.len() + 2);
    out.push(tag);
    out.push(0);
    out.extend_from_slice(payload.as_bytes());
    Message::Binary(out.into())
}

/// Connect, stream, and reconnect FOREVER. There is no reason to ever stop
/// trying: the app is long-lived and the user may walk back into coverage at
/// any moment.
pub async fn run_websocket(
    mut commands_rx: Receiver<WSCommand>,
    mut outgoing_rx: Receiver<Message>,
    mut rates_cmd_rx: Receiver<RatesCommand>,
    mut shutdown_rx: Receiver<()>,
) -> Result<(), String> {
    let mut backoff = MIN_BACKOFF;
    let mut relay_state = RelayState::default();
    // The books this client currently wants. Kept here, not on the server,
    // because bookd forgets a connection the moment its link drops.
    let mut books: HashSet<String> = HashSet::new();

    loop {
        let mut links = Links::default();
        publish(false, links);

        let connected = match timeout(CONNECT_TIMEOUT, connect_v4_first(WS_URL)).await {
            Ok(Ok((stream, _))) => Some(stream),
            Ok(Err(_)) | Err(_) => None,
        };
        let Some(ws_stream) = connected else {
            if backoff_or_shutdown(&mut backoff, &mut shutdown_rx).await {
                return Ok(());
            }
            continue;
        };

        let (mut ws_sink, mut ws_stream) = ws_stream.split();

        // The session's first frame, and the relay gate's opener (`auth_init`
        // until 2026-09-20). The proxy keeps it — nothing upstream reads it.
        // `v` names the handshake this build speaks, so the proxy can one day
        // ask more of a newer client without breaking this one (AUTH.md, C/D).
        let hello = format!(r#"{{"type":"hello","v":1,"app":"{}"}}"#, env!("CARGO_PKG_VERSION"));
        if ws_sink.send(frame(TAG_RELAY, &hello)).await.is_err() {
            if backoff_or_shutdown(&mut backoff, &mut shutdown_rx).await {
                return Ok(());
            }
            continue;
        }
        // The hello is all a connection says for itself. Everything else is
        // said on behalf of a WALLET — see the top of the loop below.
        let mut rates_asked = false;
        let mut books_told = false;

        // Backoff is reset by RECEIVING something, not by connecting — a proxy
        // that accepts and immediately drops would otherwise reset the delay on
        // every attempt and hot-loop.
        let mut healthy = false;
        let idle = tokio::time::sleep(IDLE_TIMEOUT);
        tokio::pin!(idle);

        loop {
            // THE WALLET IS THE GATE on rates and bookd — never the connection.
            // With no wallet there is nothing to price and no book to hold, so
            // this client says nothing on those streams and the proxy, which
            // dials a service on the first message for it (2026-09-20), never
            // opens them for it.
            //
            // `links.rates` / `links.book` are the SERVICE's state as the proxy
            // reports it to every client — up or down, nothing to do with
            // whether we are subscribed. So: a wallet, and a service that is
            // up, and we have not yet said it since that service came up ⟹ say
            // what the wallet needs, with the commands it always had —
            // `history` to rates, `subscribe` for each held book. That covers
            // a reconnect, app start (the wallet is known after the first
            // command), an import mid-session, and a service coming back from
            // an outage having forgotten us. History REPLACES the series it
            // lands on, so asking again also fills the gap.
            if relay_state.has_wallet() {
                let mut ok = true;
                if links.rates && !rates_asked {
                    rates_asked = true;
                    ok = ws_sink.send(frame(TAG_RATES, &rates::history_frame(&rates::history_assets()))).await.is_ok();
                }
                if links.book && !books_told {
                    books_told = true;
                    for pair in &books {
                        ok = ok && ws_sink.send(frame(TAG_BOOK, &rates::subscribe_frame(pair, true))).await.is_ok();
                    }
                }
                if !ok {
                    break;
                }
            }
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    let _ = ws_sink.close().await;
                    publish(false, links);
                    return Ok(());
                }
                _ = &mut idle => {
                    // Half-open: the proxy owes us a beat and didn't send one.
                    break;
                }
                Some(cmd) = commands_rx.recv() => {
                    relay_state.track(&cmd);
                    if relay_state.is_news(&cmd) {
                        relay_state.spawn_command(cmd);
                    }
                }
                Some(msg) = outgoing_rx.recv() => {
                    // Bare payloads from the command bridges, each tagged for
                    // the service it belongs to. Dropped when that link is
                    // down — the link rise re-syncs the asks, and a signing
                    // payload is reported to its flow as unsent, which is a
                    // fact rather than a guess while the bytes are still
                    // here. Its flow checked the link microseconds earlier;
                    // this is the window between that check and now.
                    if let Message::Text(payload) = msg
                        && let Some((tag, wrapped)) = relay::wrap(payload.as_str())
                    {
                        let link_up = if tag == TAG_BTC { links.btc } else { links.relay };
                        if !link_up {
                            relay::report_unsent(payload.as_str());
                        } else if ws_sink.send(frame(tag, &wrapped)).await.is_err() {
                            break;
                        }
                    }
                }
                Some(cmd) = rates_cmd_rx.recv() => {
                    rates::track(&mut books, &cmd);
                    let (tag, payload) = match &cmd {
                        RatesCommand::SubscribeBook(pair) => (TAG_BOOK, rates::subscribe_frame(pair, true)),
                        RatesCommand::UnsubscribeBook(pair) => (TAG_BOOK, rates::subscribe_frame(pair, false)),
                    };
                    if ws_sink.send(frame(tag, &payload)).await.is_err() {
                        break;
                    }
                }
                result = ws_stream.next() => {
                    // Any frame at all proves the link is alive, pings included.
                    idle.as_mut().reset(Instant::now() + IDLE_TIMEOUT);
                    if !healthy {
                        healthy = true;
                        backoff = MIN_BACKOFF;
                    }
                    match result {
                        Some(Ok(Message::Binary(data))) => {
                            let Some((tag, text)) = decode(&data) else { continue };
                            match tag {
                                TAG_PROXY => {
                                    let before = links;
                                    links = parse_links(&text, links);
                                    publish_with(before, true, links);
                                    // A link that rose is a service that forgot us.
                                    if links.relay && !before.relay {
                                        for payload in relay_state.resync_relay_payloads() {
                                            if let Some((tag, wrapped)) = relay::wrap(&payload) {
                                                let _ = ws_sink.send(frame(tag, &wrapped)).await;
                                            }
                                        }
                                    }
                                    if links.btc && !before.btc {
                                        // Its push routing is per connection:
                                        // the list that follows the re-sync's
                                        // reply must go out even if unchanged.
                                        relay_state.forget_live_list();
                                        for payload in relay_state.resync_btc_payloads() {
                                            if let Some((tag, wrapped)) = relay::wrap(&payload) {
                                                let _ = ws_sink.send(frame(tag, &wrapped)).await;
                                            }
                                        }
                                    }
                                    // rates and bookd: a service that went
                                    // down forgot us, so what the wallet needs
                                    // is said again when it is back — by the
                                    // block at the top of the loop.
                                    if !links.rates { rates_asked = false; }
                                    if !links.book { books_told = false; }
                                }
                                TAG_RELAY | TAG_BTC => relay_state.handle_frame(text).await,
                                TAG_RATES | TAG_BOOK => rates::process_message(&text),
                                _ => {}
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = ws_sink.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        // The socket went: every link with it, and every component behind them.
        publish_with(links, false, Links::default());

        // Nothing signed waits out a reconnect. A blob queued in the instant
        // the socket broke would otherwise sit in the mpsc through the backoff
        // and go out on the next connection — after the log had told the user
        // it was not sent, and after they may have signed again. Reported now,
        // while it is still ours to report on. Everything else is put back for
        // the next connection, exactly as it would have waited.
        let mut held = Vec::new();
        while let Ok(msg) = outgoing_rx.try_recv() {
            if let Message::Text(payload) = &msg
                && relay::report_unsent(payload.as_str())
            {
                continue;
            }
            held.push(msg);
        }
        if let Some(tx) = crate::ws::CRYPTO_OUTGOING_TX.get() {
            for msg in held {
                let _ = tx.try_send(msg);
            }
        }
        if backoff_or_shutdown(&mut backoff, &mut shutdown_rx).await {
            return Ok(());
        }
    }
}

/// `[tag][flags][payload]` → `(tag, payload as text)`, decompressing when the
/// flag says so. `None` for a short or non-UTF-8 frame.
fn decode(data: &[u8]) -> Option<(u8, String)> {
    if data.len() < 2 {
        return None;
    }
    let (tag, flags, body) = (data[0], data[1], &data[2..]);
    let bytes = if flags & FLAG_ZSTD != 0 {
        zstd::decode_all(body).ok()?
    } else {
        body.to_vec()
    };
    Some((tag, String::from_utf8(bytes).ok()?))
}

/// The proxy's `{"type":"status","components":{"link:relay":{"up":…},…}}`.
/// A missing key keeps its previous value.
fn parse_links(text: &str, mut links: Links) -> Links {
    let Ok(data) = serde_json::from_str::<serde_json::Value>(text) else { return links };
    let Some(components) = data.get("components").and_then(|v| v.as_object()) else { return links };
    let up = |name: &str| components.get(name).and_then(|c| c.get("up")).and_then(|v| v.as_bool());
    if let Some(v) = up("link:relay") { links.relay = v; }
    if let Some(v) = up("link:btc") { links.btc = v; }
    if let Some(v) = up("link:rates") { links.rates = v; }
    if let Some(v) = up("link:book") { links.book = v; }
    links
}

/// Wait out the current backoff, then grow it. Returns `true` if a shutdown
/// arrived instead — the wait has to stay interruptible, or quitting the app
/// would block on it.
async fn backoff_or_shutdown(backoff: &mut Duration, shutdown_rx: &mut Receiver<()>) -> bool {
    let stop = tokio::select! {
        _ = shutdown_rx.recv() => true,
        _ = tokio::time::sleep(*backoff) => false,
    };
    *backoff = (*backoff * 2).min(MAX_BACKOFF);
    stop
}

/// Drop-in replacement for `connect_async` that dials IPv4 first.
///
/// `connect_async` resolves the hostname itself and tries the addresses
/// SEQUENTIALLY in resolver order. On a router that advertises IPv6 but
/// silently blackholes it (packets vanish, no error ever comes back — common
/// on home and hotel wifi), the resolver sorts the v6 address first, that one
/// dead attempt eats the caller's entire connect budget, and the working v4
/// address never gets a turn — every connect times out on an otherwise fine
/// network. Browsers survive exactly this with happy-eyeballs (RFC 8305)
/// fallback; this is the sequential flavor: every IPv4 address first under a
/// short per-attempt deadline, then IPv6, then TLS + the websocket handshake
/// over the first TCP stream that answered.
async fn connect_v4_first(
    url: &str,
) -> tokio_tungstenite::tungstenite::Result<(
    WebSocketStream<MaybeTlsStream<TcpStream>>,
    tokio_tungstenite::tungstenite::handshake::client::Response,
)> {
    use tokio_tungstenite::tungstenite::{Error, client::IntoClientRequest, error::UrlError};

    let request = url.into_client_request()?;
    let uri = request.uri();
    let host = uri
        .host()
        .ok_or(Error::Url(UrlError::NoHostName))?
        .to_string();
    let port = uri.port_u16().unwrap_or(match uri.scheme_str() {
        Some("wss") => 443,
        _ => 80,
    });

    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(Error::Io)?
        .collect();

    const PER_ATTEMPT: Duration = Duration::from_secs(3);
    let mut tcp = None;
    let mut last_err = Error::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no addresses resolved",
    ));
    let ordered = addrs
        .iter()
        .filter(|a| a.is_ipv4())
        .chain(addrs.iter().filter(|a| a.is_ipv6()));
    for addr in ordered {
        match timeout(PER_ATTEMPT, TcpStream::connect(addr)).await {
            Ok(Ok(stream)) => {
                tcp = Some(stream);
                break;
            }
            Ok(Err(e)) => last_err = Error::Io(e),
            Err(_) => {
                last_err = Error::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("TCP connect to {addr} timed out"),
                ))
            }
        }
    }
    match tcp {
        Some(stream) => client_async_tls(request, stream).await,
        None => Err(last_err),
    }
}
