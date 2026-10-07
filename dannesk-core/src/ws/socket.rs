//! The app's one socket. Connects to the proxy, which fans it to relay,
//! the Bitcoin relay inside indexd, rates and bookd (workspace/TRANSPORT-SPEC.md). This task owns the connection, the
//! reconnect, the framing, and the four transport bools; what each service's
//! frames MEAN is in `relay.rs` and `rates.rs`.
//!
//! The socket is the Noise socket of `dannesk-noise-protocol` on plain TCP
//! (2026-10-07): the proxy is dialled by address, the handshake is one
//! exchange, and the hello — with the wallet's balance asks, when the wallet
//! is known — rides INSIDE the handshake's first message. A balance is two
//! round trips from a cold start: the TCP connect and that exchange. What the
//! websocket used to do for us, the proxy's own stream (tag 0x00) now does: a
//! `ping` every 30 s that we answer with a `pong`, and a `close` carrying the
//! websocket's codes before the stream ends.
//!
//! Framing (§3): every frame is `[tag][flags][payload]`. Tag 0x00 is the
//! proxy's own stream; 0x01–0x04 name a service. Flag bit 0 = zstd.
//!
//! Transport bools (§6, §7): `relay_ws_status` etc. keep their old meaning —
//! "can an answer from that server reach us" — and are now `socket up AND
//! link up`, where the link state comes from the proxy's frame. A service
//! dying behind the proxy no longer costs us the socket; it costs us one link,
//! and when that link rises we re-sync exactly what a reconnect used to.

use crate::channel::{CHANNEL, Phase, WSCommand};
use crate::ws::config::*;
use crate::ws::rates;
use crate::ws::relay::{self, RelayState};
use crate::ws::RatesCommand;
use dannesk_noise_protocol::stream as noise;
use std::collections::HashSet;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Receiver;
use tokio::time::{timeout, Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

/// A connect — the TCP dial, and then the handshake — that hasn't completed
/// in this long is treated as failed. WITHOUT this the task can hang forever:
/// a weak or captive-portal network will complete the TCP handshake and then
/// stall, and neither `TcpStream::connect` nor the handshake has a deadline
/// of its own.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// A second connect this long after the first, if the first is still
/// waiting. A lost SYN otherwise costs the kernel's one-second resend, on a
/// link that loses about a packet in three (measured on launches, 2026-10-06
/// and -07: the connect took 0.7–1.0 s against a 0.2 s round trip). On a
/// quiet link the first attempt answers before this fires.
const SECOND_ATTEMPT: Duration = Duration::from_millis(250);

/// The largest frame the proxy may send us in one piece. A history frame is
/// tens of KB; a whole-wallet Bitcoin frame for a long-used wallet, more.
const MAX_INBOUND_FRAME: usize = 16 * 1024 * 1024;

/// Dials the proxy: one TCP connect, and a second one [`SECOND_ATTEMPT`]
/// later if the first has not answered. The first to complete is the socket;
/// the other is dropped, which closes it. One that fails outright leaves the
/// other to decide.
async fn dial() -> std::io::Result<TcpStream> {
    let first = TcpStream::connect(PROXY_ADDR);
    tokio::pin!(first);
    let second = async {
        tokio::time::sleep(SECOND_ATTEMPT).await;
        TcpStream::connect(PROXY_ADDR).await
    };
    tokio::pin!(second);
    tokio::select! {
        done = &mut first => match done {
            Ok(stream) => Ok(stream),
            Err(_) => second.await,
        },
        done = &mut second => match done {
            Ok(stream) => Ok(stream),
            Err(_) => first.await,
        },
    }
}

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

fn frame(tag: u8, payload: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 2);
    out.push(tag);
    out.push(0);
    out.extend_from_slice(payload.as_bytes());
    out
}

/// The `type` of a frame on the proxy's own stream: `status`, `ping`, `close`.
fn proxy_frame_type(text: &str) -> Option<String> {
    let data = serde_json::from_str::<serde_json::Value>(text).ok()?;
    data.get("type").and_then(|t| t.as_str()).map(str::to_string)
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
    // The launch phase (`channel::Launch`) is settled by the FIRST attempt
    // alone: connected, or failed. Later reconnects say nothing — the screens
    // read them off the link bools, as they always did.
    let mut launched = false;

    loop {
        let mut links = Links::default();
        publish(false, links);

        crate::ws::trace("socket: connecting");
        let stream = match timeout(CONNECT_TIMEOUT, dial()).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(_)) | Err(_) => {
                crate::ws::trace("socket: connect failed, retrying after the backoff");
                settle_launch(&mut launched, Phase::Failed);
                if backoff_or_shutdown(&mut backoff, &mut shutdown_rx).await {
                    return Ok(());
                }
                continue;
            }
        };
        // Small frames, latency first: nothing here gains from coalescing.
        let _ = stream.set_nodelay(true);
        crate::ws::trace("socket: tcp connected");

        // THE FIRST MESSAGE. The session's hello first: the relay gate's
        // opener (`auth_init` until 2026-09-20), which the proxy keeps —
        // nothing upstream reads it. `v` names the handshake this build
        // speaks, so the proxy can one day ask more of a newer client without
        // breaking this one (AUTH.md, C/D). `app` is the app's version,
        // registered through `crate::init`; this crate's own version is not
        // what a user reports.
        //
        // Then what the wallet needs re-stated on a fresh connection — the
        // cached-balance asks, and any removal still owed — exactly what a
        // link rise sends, so that the balance comes back in the same round
        // trip as the proxy's first word rather than one later. At app start
        // the wallet is not known to this task until its open-time asks have
        // come down the command channel; they are read now, having queued
        // while the connect was in flight, and folded in here instead of
        // going out as frames after the link report. Anything else queued
        // meanwhile waits for the session, as it always did.
        let app_version = crate::app().map_or("unregistered", |app| app.version);
        let hello = format!(r#"{{"type":"hello","v":1,"app":"{}"}}"#, app_version);
        let mut first: Vec<Vec<u8>> = vec![frame(TAG_RELAY, &hello)];
        let mut deferred: Vec<WSCommand> = Vec::new();
        while let Ok(cmd) = commands_rx.try_recv() {
            if cmd.command == "get_cached_balance" || cmd.command == "get_bitcoin_cached_balance" {
                relay_state.track(&cmd);
            } else {
                deferred.push(cmd);
            }
        }
        // The books the trade screen already wants — with an XRP wallet,
        // every token's, from the controller's first sync — are only a
        // want-set here, so they are read the same way and ride along.
        while let Ok(cmd) = rates_cmd_rx.try_recv() {
            rates::track(&mut books, &cmd);
        }
        // The Bitcoin relay's push routing is per connection: the live list
        // that follows the whole-wallet reply must go out even if unchanged.
        relay_state.forget_live_list();
        let relay_asks = relay_state.resync_relay_payloads();
        let btc_asks = relay_state.resync_btc_payloads();
        for payload in relay_asks.iter().chain(btc_asks.iter()) {
            if let Some((tag, wrapped)) = relay::wrap(payload) {
                first.push(frame(tag, &wrapped));
            }
        }
        // And what the wallet needs from rates and bookd — `history` for the
        // price series, `subscribe` for each book the trade screen holds —
        // which the loop below would otherwise say once their links are
        // reported up: said here instead, so the prices come back in the same
        // flight as the balance (user, 2026-10-07). With no wallet, nothing:
        // there is nothing to price and no page to show it on.
        let wallet_known = relay_state.has_wallet();
        if wallet_known {
            first.push(frame(TAG_RATES, &rates::history_frame(&rates::history_assets())));
            for pair in &books {
                first.push(frame(TAG_BOOK, &rates::subscribe_frame(pair, true)));
            }
        }
        // Consumed by the proxy's first link report: a link that is up then
        // has these already, and a rise is a re-sync only after that.
        let mut asked_in_first = (!relay_asks.is_empty(), !btc_asks.is_empty());
        let first_refs: Vec<&[u8]> = first.iter().map(Vec::as_slice).collect();
        let handshake = noise::connect(stream, &PROXY_KEY.1, PROXY_KEY.0, &first_refs, MAX_INBOUND_FRAME);
        let (mut reader, mut writer) = match timeout(CONNECT_TIMEOUT, handshake).await {
            Ok(Ok(halves)) => halves,
            Ok(Err(_)) | Err(_) => {
                crate::ws::trace("socket: handshake failed, retrying after the backoff");
                settle_launch(&mut launched, Phase::Failed);
                if backoff_or_shutdown(&mut backoff, &mut shutdown_rx).await {
                    return Ok(());
                }
                continue;
            }
        };
        crate::ws::trace("socket: open (handshake done)");
        for cmd in deferred {
            relay_state.track(&cmd);
            if relay_state.is_news(&cmd) {
                relay_state.spawn_command(cmd);
            }
        }
        // The hello is all a connection says for itself. Everything else is
        // said on behalf of a WALLET — in the first message when the wallet
        // was known then, otherwise by the top of the loop below. A service
        // reported down resets these and its rise says it again; the proxy
        // may deliver its held copy of the first ask as well, and a history
        // that arrives twice replaces the same series twice.
        let mut rates_asked = wallet_known;
        let mut books_told = wallet_known;

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
                    ok = writer.send_frame(&frame(TAG_RATES, &rates::history_frame(&rates::history_assets()))).await.is_ok();
                }
                if links.book && !books_told {
                    books_told = true;
                    for pair in &books {
                        ok = ok && writer.send_frame(&frame(TAG_BOOK, &rates::subscribe_frame(pair, true))).await.is_ok();
                    }
                }
                if !ok {
                    break;
                }
            }
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    let _ = writer.shutdown().await;
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
                        } else if writer.send_frame(&frame(tag, &wrapped)).await.is_err() {
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
                    if writer.send_frame(&frame(tag, &payload)).await.is_err() {
                        break;
                    }
                }
                result = reader.recv_frame() => {
                    // Any frame at all proves the link is alive, pings included.
                    idle.as_mut().reset(Instant::now() + IDLE_TIMEOUT);
                    if !healthy {
                        healthy = true;
                        backoff = MIN_BACKOFF;
                    }
                    // A close, an error, or the end of the stream.
                    let Ok(data) = result else { break };
                    let Some((tag, text)) = decode(&data) else { continue };
                    match tag {
                        // The proxy's own stream: the keepalive we answer, the
                        // goodbye, and the link report.
                        TAG_PROXY => match proxy_frame_type(&text).as_deref() {
                            Some("ping") => {
                                if writer.send_frame(&frame(TAG_PROXY, r#"{"type":"pong"}"#)).await.is_err() {
                                    break;
                                }
                            }
                            Some("close") => break,
                            Some("status") => {
                                let before = links;
                                links = parse_links(&text, links);
                                publish_with(before, true, links);
                                // The first report of the first session: the
                                // launch reached the proxy, and the link bools
                                // now say what each figure waits for.
                                settle_launch(&mut launched, Phase::Connected);
                                // A link that rose is a service that forgot us
                                // — unless this is the session's first report
                                // and the asks already rode in the first
                                // message: a link up now has them.
                                let (relay_asked, btc_asked) = asked_in_first;
                                asked_in_first = (false, false);
                                if links.relay && !before.relay && !relay_asked {
                                    for payload in relay_state.resync_relay_payloads() {
                                        if let Some((tag, wrapped)) = relay::wrap(&payload) {
                                            let _ = writer.send_frame(&frame(tag, &wrapped)).await;
                                        }
                                    }
                                }
                                if links.btc && !before.btc && !btc_asked {
                                    // Its push routing is per connection:
                                    // the list that follows the re-sync's
                                    // reply must go out even if unchanged.
                                    relay_state.forget_live_list();
                                    for payload in relay_state.resync_btc_payloads() {
                                        if let Some((tag, wrapped)) = relay::wrap(&payload) {
                                            let _ = writer.send_frame(&frame(tag, &wrapped)).await;
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
                            _ => {}
                        },
                        TAG_RELAY | TAG_BTC => relay_state.handle_frame(text).await,
                        TAG_RATES | TAG_BOOK => {
                            if tag == TAG_RATES {
                                crate::ws::trace("socket: first rates frame in");
                            }
                            rates::process_message(&text)
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

/// The first attempt's outcome, published once; every later call is nothing.
fn settle_launch(launched: &mut bool, phase: Phase) {
    if !*launched {
        *launched = true;
        CHANNEL.launch_tx.send_modify(|launch| launch.phase = phase);
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
