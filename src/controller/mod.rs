pub mod message;
pub mod app_state;
pub mod subscription;
pub mod xrp;
pub mod btc;
pub mod panes;

use iced::Task;
use std::time::Duration;
use crate::channel::{CHANNEL, HistoryList};
use crate::controller::message::Message;
use crate::controller::app_state::XrpView;
use crate::controller::app_state::{AppState, Tab};

const SHORT_HISTORY_CAP: usize = 120;             // 1 hour at 30s buckets
const SHORT_BUCKET_MS: u64 = 30 * 1000;           // 30s
const LONG_HISTORY_CAP: usize = 288;              // 24 hours at 5-min buckets
const LONG_BUCKET_MS: u64 = 5 * 60 * 1000;        // 5 min
/// What each series' label promises: the short one is read as an hour, the
/// long one as `24h`. Seeded points older than this are dropped on arrival.
const SHORT_SPAN_MS: u64 = SHORT_HISTORY_CAP as u64 * SHORT_BUCKET_MS;
const LONG_SPAN_MS: u64 = LONG_HISTORY_CAP as u64 * LONG_BUCKET_MS;

/// A seeded series, cut to the window its label promises.
///
/// The rates server caps history by COUNT, not by age, and resumes from disk
/// after an outage on purpose (so a restart never shows a flat line). Run
/// intermittently — the dev box — that makes a "24h" series reach back weeks,
/// with multi-day gaps, and the `24h` change and the Balance day line were
/// stating a two-month move (verified 2026-09-03: XRP +28% over 63 days). The
/// label is the client's claim, so the client enforces it: anything older than
/// the span is dropped here, and the local extension keeps the deque honest
/// from then on. Too few points left ⟹ the consumers draw `—` / no line.
fn within_span(series: Vec<(u64, f32)>, span_ms: u64) -> std::collections::VecDeque<(u64, f32)> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let oldest = now_ms.saturating_sub(span_ms);
    series.into_iter().filter(|&(ts, _)| ts >= oldest).collect()
}

pub use subscription::subscriptions;

/// Total wipe: delete the on-disk wallet metadata + the gate + `settings.json`,
/// reset the live channels, and fire a best-effort backend unsubscribe for each
/// wallet. Shared by Settings "erase everything" and the gate's 5-fail
/// self-destruct so the two can't drift — after either, the next launch is a
/// fresh first-run (set a new PIN).
///
/// **No activity log, on purpose.** This used to spawn the two per-wallet
/// `remove_wallet` tasks, each of which raises its own 3-step log — but erase is
/// local work, and its only backend touch (the `delete_wallet` notify) is
/// fire-and-forget: it never awaits a reply. The log narrates round-trips that
/// wait for a response; this isn't one. (Two logs also raced one channel.)
///
/// A dropped send is NOT harmless, and the comment here said it was until
/// 2026-09-20: it claimed "the relay TTL-evicts inactive wallets", which was
/// true of Redis and stopped being true when the stores moved to redb on
/// 2026-09-14. Between those dates a removal sent while the link was down was
/// simply lost — the proxy discards hub frames when the upstream is down, and
/// the files naming the address are gone a moment later. Two things now cover
/// it: `RelayState` re-sends the removal on every link rise for the life of
/// the process, and the relay sweeps a wallet it has not heard from in
/// `FOLLOWED_AGE_SECS`, taking everything it owns.
/// Everything runs synchronously here, so the function returns nothing. Android
/// twin: `bridge/erase_all.rs`.
fn wipe_wallet_data() {
    use crate::bridge::json_storage::remove_json;
    use crate::channel::{TransactionState, BtcTransactionState, WSCommand};

    let (_, xrp_address, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
    let (_, btc_address, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
    let ws_tx = crate::ws::CRYPTO_COMMANDS_TX.get().cloned();

    crate::gate::delete();

    // The removals go out FIRST. They are fire-and-forget and carry the only
    // copy of the address that will ever exist once the files below are gone,
    // so a crash between the two steps must not be the thing that loses them.
    // `RelayState::track` also holds each one in memory and re-sends it on
    // every link rise, because the proxy DISCARDS a hub frame while the link
    // is down and there is no reply to tell us either way.
    if let Some(tx) = &ws_tx {
        if let Some(address) = xrp_address {
            let _ = tx.try_send(WSCommand {
                command: "delete_wallet".to_string(),
                wallet: Some(address),
                ..Default::default()
            });
        }
        if let Some(address) = btc_address {
            let _ = tx.try_send(WSCommand {
                command: "delete_bitcoin_wallet".to_string(),
                wallet: Some(address),
                ..Default::default()
            });
        }
    }

    // Unconditional: `remove_json` is idempotent, and it also takes the `.tmp`
    // sibling a crashed write can leave behind. Guarding on the main file
    // existing would skip that cleanup in exactly the case that needs it — a
    // power cut mid-write leaves a tmp full of ciphertext and no file beside it,
    // which would then survive the wipe that promises nothing is left.
    for file in &["xrp_encrypt.json", "xrp.json", "btc_encrypt.json", "btc.json", "settings.json"] {
        let _ = remove_json(file);
    }

    let _ = CHANNEL.wallet_balance_tx.send((0.0, None, false, crate::channel::KeyMode::Standard));
    let _ = CHANNEL.transactions_tx.send(TransactionState::default());
    CHANNEL.clear_tokens();
    let _ = CHANNEL.bitcoin_wallet_tx.send((0.0, None, false, crate::channel::KeyMode::Standard));
    let _ = CHANNEL.btc_transactions_tx.send(BtcTransactionState::default());
    let _ = CHANNEL.activity_tx.send(None);
}

/// How long a history page may go unanswered before its end row reads
/// `couldn't load · retry ›`. A page is a store read on the relay and one
/// round trip through the proxy — seconds is already generous.
const HISTORY_PAGE_TIMEOUT: Duration = Duration::from_secs(10);

/// `load 20 more ›`: mark the list fetching, ask its relay for the next page
/// from the settled rows the app holds, and arm the timeout for exactly this
/// request. A second click while one is in flight is ignored — the end row
/// is not a link then, but the message can still arrive from a stale frame.
fn history_load_more(list: HistoryList) -> Task<Message> {
    use crate::channel::{PageStatus, WSCommand};
    let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get() else { return Task::none() };
    let (wallet, command, offset, seq) = match list {
        HistoryList::BtcTransactions => {
            let wallet = CHANNEL.bitcoin_wallet_rx.borrow().1.clone();
            let mut seq = None;
            let mut offset = 0;
            CHANNEL.btc_transactions_tx.send_if_modified(|s| {
                if s.page.status == PageStatus::Fetching {
                    return false;
                }
                offset = s.held();
                seq = Some(s.page.start());
                true
            });
            (wallet, "get_bitcoin_history", offset, seq)
        }
        HistoryList::XrpTransactions | HistoryList::XrpOrders => {
            let wallet = CHANNEL.wallet_balance_rx.borrow().1.clone();
            let mut seq = None;
            let mut offset = 0;
            CHANNEL.transactions_tx.send_if_modified(|s| {
                if s.page(list).status == PageStatus::Fetching {
                    return false;
                }
                offset = s.held(list);
                seq = Some(s.page_mut(list).start());
                true
            });
            (wallet, "get_history", offset, seq)
        }
    };
    let (Some(wallet), Some(seq)) = (wallet, seq) else { return Task::none() };
    let _ = ws_tx.try_send(WSCommand {
        command: command.to_string(),
        wallet: Some(wallet),
        history_kind: Some(list.kind()),
        history_offset: Some(offset),
        ..Default::default()
    });
    Task::perform(
        async { tokio::time::sleep(HISTORY_PAGE_TIMEOUT).await },
        move |_| Message::HistoryTimedOut(list, seq),
    )
}

/// A wrong gate attempt: tick the persisted fail streak, escalate the message,
/// and on the 5th miss erase everything (wallets + gate — see
/// `wipe_wallet_data`) then quit after a brief on-screen beat.
/// Post a gate failure: the sentence, and the instant it landed. The timestamp
/// is what the screen's shake and its red field are derived from — one clock,
/// no tween state, and it cannot drift out of sync with the message.
fn gate_fail(state: &mut AppState, message: String) {
    state.gate_error = Some(message);
    state.gate_error_at = Some(std::time::Instant::now());
}

fn gate_wrong_attempt(state: &mut AppState) -> Task<Message> {
    match crate::gate::register_fail() {
        0 => {
            wipe_wallet_data();
            state.gate_wiping = true;
            state.gate_exists = false;
            state.gate_error = None;
            state.gate_error_at = None;
            Task::perform(
                async { tokio::time::sleep(Duration::from_millis(1800)).await },
                |_| Message::GateWipeExit,
            )
        }
        1 => {
            gate_fail(state, "1 attempt left \u{2014} then everything is erased".to_string());
            Task::none()
        }
        n => {
            gate_fail(state, format!("wrong pin \u{2014} {n} attempts left"));
            Task::none()
        }
    }
}

pub fn handle_message(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        // Chain-agnostic, so both are handled here rather than falling through
        // to the BTC/XRP split at the bottom of this match.
        Message::SecurePasteRequested(field) => {
            // `unwrap_or_clone` moves the pasted String out of its `Arc` (held
            // once, here) instead of copying a phrase into a second buffer.
            return iced::clipboard::read_text()
                .map(move |text| Message::SecurePasted(field, text.ok().map(std::sync::Arc::unwrap_or_clone)));
        }
        Message::SecurePasted(field, text) => {
            if let Some(text) = text {
                state.replace_secure_field(field, text);
            }
        }
        Message::SecureClearRequested(field) => {
            state.replace_secure_field(field, String::new());
        }
        // The reveals go dark too. `replace_secure_field` leaves them alone —
        // right for a paste, wrong here: a field left revealed would show the
        // NEXT phrase in the clear, which is not what someone who just asked
        // for the last one to be forgotten is expecting.
        Message::SignCredentialCleared { secret, phrase, bip39 } => {
            for field in [secret, phrase, bip39] {
                state.replace_secure_field(field, String::new());
                state.set_secure_reveal(field, false);
            }
        }
        // Chain-agnostic like the secure-field arms above: Esc closes
        // whichever overlay is open.
        Message::EscapeDismiss => {
            // The `panels +` dropdown, like every other overlay.
            state.xrp_grid.menu_open = false;
            state.btc_grid.menu_open = false;
            // The asset picker is a stack with no close glyph — Esc is one of
            // its three doors, beside the scrim and the back chevron.
            state.send_asset_picker_open = false;
            // The top bar's pair search: Esc closes it, the chip is back.
            panes::dismiss_pair_search(state);
        }
        // Non-secret grid fields. Chain-agnostic in shape, so they are routed
        // here beside their secret twins; what each field means afterwards (the
        // BTC/USD cross, a tier selecting itself) belongs to the flow and is
        // handled there.
        Message::PlainEdit(field, op) => {
            // The trade ticket's one amount field always edits the anchored
            // side, so nothing here can type into a derived number any more
            // (the takeover that cleared it first went with the second
            // field, 2026-09-16 — see `xrptrade::ticket`).
            if state.apply_plain_edit(field, op) {
                after_plain_edit(state, field);
            }
            // No task: the quote is a pure function of the ticket and the book
            // frame now, so the next render walks it.
            return Task::none();
        }
        Message::PlainPasteRequested(field) => {
            return iced::clipboard::read_text()
                .map(move |text| Message::PlainPasted(field, text.ok().map(std::sync::Arc::unwrap_or_clone)));
        }
        Message::PlainPasted(field, text) => {
            if let Some(text) = text {
                state.replace_plain_field(field, text);
                after_plain_edit(state, field);
            }
            return Task::none();
        }
        Message::SecureEdit(field, op) => {
            state.apply_secure_edit(field, op);

        }
        // ── The launch gate's keyboard ──────────────────────────────────────
        // No text widget: the gate owns the whole screen, so keystrokes arrive
        // from a keyboard subscription and land here. Digits only, capped at
        // PIN_DIGITS, and the last one submits itself.
        //
        // Worth knowing before changing this: a wrong attempt is not free. It
        // ticks a persisted counter and the 5th consecutive miss wipes all
        // wallet data and quits (`gate::MAX_FAILS`, `gate_wrong_attempt`).
        // Auto-submit removes the beat where a fat-fingered digit could be
        // backspaced before committing. That is acceptable only because a
        // SUCCESSFUL unlock resets the counter to zero, so a caught typo costs
        // one attempt and nothing that survives it — five in a row means the
        // PIN is genuinely lost, which is what the wipe is for.
        Message::GateDigit(c) => {
            if state.gate_wiping || !c.is_ascii_digit() {
                return Task::none();
            }
            // A fresh keystroke clears stale feedback.
            state.gate_error = None;
            state.gate_error_at = None;
            if state.gate_pin_input.chars().count() < crate::gate::PIN_DIGITS {
                state.gate_pin_input.push(c);
                if state.gate_pin_input.chars().count() == crate::gate::PIN_DIGITS {
                    return Task::done(Message::GatePinSubmitted);
                }
            }
        }
        Message::GateBackspace => {
            state.gate_pin_input.pop();
            state.gate_error = None;
            state.gate_error_at = None;
        }
        Message::GateBack => {
            // Confirm → choose, with the first PIN back in the field. Retyping
            // six digits you already got right is the dead end this avoids.
            if let Some(first) = state.gate_pin_draft.take() {
                state.gate_pin_input = first;
            }
            state.gate_error = None;
            state.gate_error_at = None;
        }
        Message::GateWipeExit => {
            return iced::exit();
        }
        Message::GatePinSubmitted => {
            use crate::gate::PIN_DIGITS;
            if state.gate_pin_input.chars().count() != PIN_DIGITS {
                gate_fail(state, format!("enter all {PIN_DIGITS} digits"));
                return Task::none();
            }
            crate::startup_trace::stamp("PIN entered");
            let pin = std::mem::take(&mut state.gate_pin_input);
            if state.gate_exists {
                // ── Enter mode ──
                match crate::gate::verify_pin(&pin) {
                    Ok(true) => {
                        crate::gate::clear_attempts();
                        state.gate_unlocked = true;
                        crate::startup_trace::stamp("PIN accepted");
                        state.gate_error = None;
                    }
                    Ok(false) => return gate_wrong_attempt(state),
                    // The record could not be read, so we do not know that the
                    // digits were wrong — and only knowing that may spend one of
                    // the five attempts, the fifth of which erases every wallet.
                    Err(e) => gate_fail(state, e),
                }
            } else {
                // ── Create mode: first entry drafts, second must match ──
                match state.gate_pin_draft.take() {
                    None => {
                        state.gate_pin_draft = Some(pin);
                        state.gate_error = None;
                    }
                    Some(first) if first == pin => {
                        match crate::gate::set_pin(&pin) {
                            Ok(()) => {
                                state.gate_exists = true;
                                state.gate_unlocked = true;
                                state.gate_error = None;
                            }
                            Err(e) => gate_fail(state, format!("could not save pin: {e}")),
                        }
                    }
                    Some(_) => {
                        gate_fail(state, "pins don\u{2019}t match".to_string());
                    }
                }
            }
        }
        Message::GateEnabledToggled(on) => {
            use crate::bridge::json_storage;
            use serde_json::Value;
            state.gate_enabled = on;
            let _ = json_storage::update_json::<Value>("settings.json", |json| {
                if let Some(obj) = json.as_object_mut() {
                    obj.insert("gate_enabled".to_string(), serde_json::json!(on));
                }
            });
            if !on {
                // Gate off: forget the PIN entirely rather than leaving a hash
                // on disk for a screen nobody sees. The gate is the PIN's only
                // consumer now — signing uses a passphrase or the phrase — so a
                // retained PIN could only resurface months later as digits the
                // user no longer remembers.
                crate::gate::delete();
                state.gate_exists = false;
            } else {
                // Re-read from disk rather than trusting this session's snapshot.
                state.gate_exists = crate::gate::exists();
                if !state.gate_exists {
                    // Re-enabling after a forget: relock straight into set-up.
                    state.gate_unlocked = false;
                    state.gate_pin_draft = None;
                    state.gate_pin_input.clear();
                    state.gate_error = None;
                }
            }
        }
        Message::Sync => {
            // The network pane's tape: one bar per validated ledger, folded
            // in off the node frame that woke this Sync (a no-op otherwise).
            panes::note_ledger(state);
            // The send forms' derived amounts follow the rate that woke this
            // Sync (a no-op otherwise): the typed field is the anchor, the
            // other is its readout. See `SendAnchor`.
            crate::controller::xrp::resync_send_twin(state);
            crate::controller::btc::resync_send_twin(state);
            // The grid's layout, written once a change has stood for
            // `panes::SETTLE` — one clock comparison when nothing changed.
            state.xrp_grid.flush_if_settled(panes::XRP_KEY);
            state.btc_grid.flush_if_settled(panes::BTC_KEY);
            // The BTC receive pane's rotating address follows the UTXO set:
            // one fingerprint compare per Sync while the pane is up, a
            // re-derivation only when a coin arrived or left.
            if state.selected_tab == Tab::Btc && state.btc_grid.has(panes::PaneKind::Receive) {
                crate::controller::btc::refresh_receive_address(state, false);
            }
            // The trade card's derived amount is a field, not a read-out, so
            // its text has to follow the quote: rewrite it here while the
            // order step is open (see xrp::refresh_trade_twin). And a new
            // ledger is a new question — the book frame that woke this Sync
            // carries its index, which is part of the quote key — so the
            // ticket re-asks on every ledger while it is open, on any step:
            // "live until you sign".
            if state.xrp_view == XrpView::Menu {
                // The pane grid: the ticket pane is always on its compose
                // step, so its twin follows the quote on every ledger; the
                // books are held whenever the wallet exists.
                crate::controller::xrp::trade_grid_init(state);
                if state.trade_step == 1 {
                    crate::controller::xrp::follow_wallet_side(state);
                    crate::controller::xrp::refresh_trade_twin(state);
                }
            }
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            // Accumulate per-asset USD price history (keyed by asset code).
            // Any pair's series is derived later by dividing two asset series.
            let assets: Vec<(String, f32)> = crate::channel::CHANNEL.rates_rx.borrow()
                .iter().map(|(a, &p)| (a.clone(), p)).collect();

            // The history SEED is not asked for here any more (2026-09-20): it
            // is the message that opens the rates stream, so the socket task
            // sends it on every connection once a wallet exists
            // (`ws::socket`, `ws::rates::history_assets`). Local accumulation
            // alone can only ever show what this session has watched — one
            // sample per 30s into the short series, one per 5min into the long
            // one — so without the seed a fresh launch had no 24h series at
            // all, and the dashboard's `24h` change and its chart both read `—`
            // until the app had been open for hours.
            for (asset, price) in assets {
                if price == 0.0 { continue }
                let short = state.rate_history_short.entry(asset.clone()).or_default();
                if short.back().map_or(true, |&(last_ts, _)| now_ms.saturating_sub(last_ts) >= SHORT_BUCKET_MS) {
                    short.push_back((now_ms, price));
                    if short.len() > SHORT_HISTORY_CAP {
                        short.pop_front();
                    }
                }
                let long = state.rate_history_long.entry(asset).or_default();
                if long.back().map_or(true, |&(last_ts, _)| now_ms.saturating_sub(last_ts) >= LONG_BUCKET_MS) {
                    long.push_back((now_ms, price));
                    if long.len() > LONG_HISTORY_CAP {
                        long.pop_front();
                    }
                }
            }
            return Task::none();
        }
        Message::RateHistoryReceived(history) => {
            for (asset, short) in history {
                state.rate_history_short.insert(asset, within_span(short, SHORT_SPAN_MS));
            }
        }
        Message::RateHistoryLongReceived(history) => {
            for (asset, long) in history {
                state.rate_history_long.insert(asset, within_span(long, LONG_SPAN_MS));
            }
        }
        Message::WindowResized(w, h) => {
            state.window_width = w;
            state.window_height = h;
        }
        Message::Grid(msg) => return panes::handle(state, panes::Chain::Xrp, msg),
        Message::BtcGrid(msg) => return panes::handle(state, panes::Chain::Btc, msg),
        Message::GridShortcut(msg) => return panes::handle_shortcut(state, msg),
        Message::TabChanged(tab) => {
            state.selected_tab = tab;
            state.settings_open = false;
            // Arriving on the BTC grid is the receive pane's "open": offer
            // the first unused address, as the receive page did on open.
            if state.selected_tab == Tab::Btc {
                crate::controller::btc::refresh_receive_address(state, true);
            }
            // Books are gated on the wallet, not the tab, so this holds the
            // set steady across a switch — the XRP panes stay warm instead of
            // re-fetching (and re-jittering) on every return. Kept as a cheap
            // reconcile point: it corrects the set if anything drifted.
            crate::controller::xrp::sync_trade_books(state);
        }
        Message::OpenSettings => {
            state.settings_open = true;
        }
        Message::CloseSettings => {
            state.settings_open = false;
        }
        Message::ActivityChanged(val) => {
            // The core's watchdog (`channel::activity_watchdog`) fails a hung
            // step in the channel itself, so what arrives here is the log to
            // show. A finished log holds the screen until the user clicks Done
            // (the action row in ui/activity_log.rs).
            state.activity_log = val;
        }
        Message::ActivityTick(now) => {
            state.activity_tick = now;
            // The one place an order can be declared dead without an answer:
            // the ledger index passing the blob's own expiry. This pump only
            // runs while the log is on screen, which is exactly when there is
            // something to tell. See `xrp::trade_expiry_check`.
            xrp::trade_expiry_check();
        }
        // `load 20 more ›`: the next page of one history list, asked for by
        // how many settled rows of it the app holds. Chain-agnostic here —
        // the list names its chain, and both relays answer the same way.
        Message::HistoryLoadMore(list) => return history_load_more(list),
        Message::HistoryTimedOut(list, seq) => match list {
            HistoryList::BtcTransactions => CHANNEL.btc_transactions_tx.send_modify(|s| s.page.fail(seq)),
            _ => CHANNEL.transactions_tx.send_modify(|s| s.page_mut(list).fail(seq)),
        },
        // Nothing to store: the message's arrival rebuilds the view, and that
        // layout re-shapes every paragraph with the system's fonts behind ours
        // (`utils/fonts.rs`).
        Message::SystemFontsIndexed => {}
        Message::FetchTick => {}
        Message::FirstFrame => {
            crate::startup_trace::first_frame();
            return crate::utils::fonts::system_fonts_task();
        }
        Message::ActivityDismiss => {
            state.activity_log = None;
            let _ = crate::channel::CHANNEL.activity_tx.send(None);
        }
        Message::ThemeSet(custom) => {
            use crate::bridge::json_storage;
            use serde_json::Value;
            state.theme = custom.build();
            let _ = json_storage::update_json::<Value>("settings.json", |json| {
                if let Some(obj) = json.as_object_mut() {
                    obj.insert("theme".to_string(), serde_json::json!(custom));
                }
            });
        }
        Message::FocusNext => {
            return iced::widget::operation::focus_next();
        }
        Message::ToggleHideBalance => {
            use crate::bridge::json_storage;
            use serde_json::Value;
            state.hide_balance = !state.hide_balance;
            let hide = state.hide_balance;
            let _ = json_storage::update_json::<Value>("settings.json", |json| {
                if let Some(obj) = json.as_object_mut() {
                    obj.insert("is_hidden".to_string(), serde_json::json!(hide));
                }
            });
        }
        Message::DisplayCcySet(ccy) => {
            use crate::bridge::json_storage;
            use serde_json::Value;
            // The one currency control in the app: the choice becomes the base
            // every screen reads in, and is persisted. `local_currency` — the
            // old appended-4th-tab field — is scrubbed from older files.
            state.base_currency = ccy;
            let base_code = ccy.code();
            let _ = json_storage::update_json::<Value>("settings.json", |json| {
                if let Some(obj) = json.as_object_mut() {
                    obj.insert("base_currency".to_string(), serde_json::json!(base_code));
                    obj.remove("local_currency");
                }
            });
        }
        Message::PrefsEraseConfirmed => {
            // One press, no confirmation — see `Message::PrefsEraseConfirmed`.
            // Synchronous, no activity log (see wipe_wallet_data) — nothing to await.
            wipe_wallet_data();
            // Stay in Settings rather than force-navigating to Balance. The wipe
            // reports itself: with no wallet left the Data row re-reads its own
            // state and goes inert ("none", no link), which is the confirmation.
            // The user leaves via the back circle.
            // The wipe took the gate with it (see wipe_wallet_data); keep this
            // session's state consistent. Next launch = first-run setup.
            state.gate_exists = false;
        }
        msg @ (Message::BtcImportWalletClicked
        | Message::BtcCreateWalletClicked
        | Message::BtcGenerateMnemonic
        | Message::BtcCreateSubmitClicked
        | Message::BtcCopyMnemonic
        | Message::BtcCreateCompleted
        | Message::BtcCreateFailed
        | Message::BtcImportModeChanged(_)
        | Message::BtcWord25Chosen(_)
        | Message::BtcScriptTypeChosen(_)
        | Message::BtcImportSubmitClicked
        | Message::BtcImportCompleted
        | Message::BtcImportFailed
        | Message::BtcBackClicked
        | Message::BtcDeleteKey
        | Message::BtcRemoveWallet
        | Message::BtcWalletRemoved
        | Message::BtcCopyAddress
        | Message::BtcGenerateReceiveAddress
        | Message::BtcRemoveReceiveAddress(_)
        | Message::BtcSelectReceiveAddress(_)
        | Message::BtcTogglePoolFace
        | Message::BtcCopyPoolAddress(_)
        | Message::BtcCopyMasterAddress
        | Message::BtcCopyMasterAddressFeedback
        | Message::BtcCopyTxid(_)
        | Message::BtcSendFeeTierSelected(_)
        | Message::BtcSendContinueClicked
        | Message::BtcSendSubmitClicked
        | Message::BtcSendCompleted
        | Message::BtcSendFailed(_)
        | Message::BtcSendDispatched
        | Message::BtcBumpClicked(_)
        | Message::BtcBumpDismissed
        | Message::BtcBumpFeeTierSelected(_)
        | Message::BtcBumpSubmitClicked
        | Message::BtcBumpCompleted
        | Message::BtcBumpFailed(_)
        | Message::BtcCopyMnemonicFeedback
        | Message::BtcCopyAddressFeedback
        | Message::BtcReimportSpinnerTick
        | Message::BtcReimportConfirm
        | Message::BtcReimportSuccess
        | Message::BtcReimportFailed(_)
        | Message::BtcKeyMgmtRestoreToggled
        | Message::BtcTxSelected(_)) => return btc::handle(state, msg),
        // Everything else is XRP's. The list above is an allow-list, which
        // means a new `Btc*` message is routed here by DEFAULT and silently
        // does nothing — the failure has no symptom at the call site, only a
        // flag somewhere that never flips. That shipped once: a new BTC send
        // message landed here instead, its handler never ran, and the signing
        // modal was left unusable. The name is the only signal available at
        // runtime, so the name is what gets checked.
        msg => {
            debug_assert!(
                !format!("{msg:?}").starts_with("Btc"),
                "{msg:?} fell through to the XRP handler — add it to the BTC \
                 arm above. See controller/mod.rs's dispatch list.",
            );
            return xrp::handle(state, msg);
        }
    }
    Task::none()
}

/// The per-field follow-up after a plain grid edit, routed to the flow that
/// owns the field.
fn after_plain_edit(state: &mut AppState, field: crate::controller::message::PlainField) {
    if field.is_btc() {
        crate::controller::btc::after_plain_edit(state, field);
    } else {
        crate::controller::xrp::after_plain_edit(state, field);
    }
}
