use iced::Subscription;
use crate::controller::message::Message;
use crate::controller::app_state::AppState;
use crate::channel::{ActivityLogState, RateHistoryMap, CHANNEL};
use tokio::sync::watch::Receiver;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

fn on_window_event(event: iced::Event, _status: iced::event::Status, _window: iced::window::Id) -> Option<Message> {
    if let iced::Event::Window(iced::window::Event::Resized(size)) = event {
        Some(Message::WindowResized(size.width, size.height))
    } else {
        None
    }
}

pub fn subscriptions(state: &AppState) -> Subscription<Message> {
    let mut subs = vec![
        watch("wallet_balance", CHANNEL.wallet_balance_rx.clone()),
        watch("bitcoin_wallet", CHANNEL.bitcoin_wallet_rx.clone()),
        watch("rates", CHANNEL.rates_rx.clone()),
        watch("rates_ws_status", CHANNEL.rates_ws_status_rx.clone()),
        watch("relay_ws_status", CHANNEL.relay_ws_status_rx.clone()),
        watch("btc_ws_status", CHANNEL.btc_ws_status_rx.clone()),
        watch("book_ws_status", CHANNEL.book_ws_status_rx.clone()),
        watch("service_health", CHANNEL.service_health_rx.clone()),
        watch("tokens", CHANNEL.tokens_rx.clone()),
        watch("orderbook", CHANNEL.orderbook_rx.clone()),
        watch("transactions", CHANNEL.transactions_rx.clone()),
        // The BTC pair was MISSING: nothing re-rendered when a Bitcoin
        // transaction arrived, so the dashboard's pending count and the pending
        // overlay only refreshed when some unrelated channel happened to fire —
        // in practice the 30 s health tick. A pending transaction appearing up
        // to half a minute late looked like the relay was slow.
        watch("btc_transactions", CHANNEL.btc_transactions_rx.clone()),
        watch("btc_node", CHANNEL.btc_node_rx.clone()),
        watch("xrp_node", CHANNEL.xrp_node_rx.clone()),
        watch("xrp_account", CHANNEL.xrp_account_rx.clone()),
        watch_activity(CHANNEL.activity_rx.clone()),
        watch_btc_send_dispatched(CHANNEL.btc_send_dispatched_rx.clone()),
        watch_rate_history(CHANNEL.rate_history_rx.clone()),
        watch_rate_history_long(CHANNEL.rate_history_long_rx.clone()),
        iced::event::listen_with(on_window_event),
    ];
    // Frame events until the first one lands: the system's fonts are read only
    // once the first frame is out (`utils/fonts.rs`), and a `startup-trace`
    // build stamps it. After that this pushes nothing.
    if crate::utils::fonts::system_fonts_pending() || crate::startup_trace::first_frame_pending() {
        subs.push(iced::window::frames().map(|_| Message::FirstFrame));
    }
    // THE LAUNCH GATE — while it owns the screen it needs a keyboard (there is no
    // text widget on it) and a redraw pump for the caret blink and the wrong-PIN
    // shake. Both are derived from clocks, so the pump is the only state either
    // of them needs.
    //
    // The rate is NOT fixed at 60fps any more (2026-09-15). It was, because the
    // guilloché bloom redrew every frame; with a static mark in its place there
    // is often nothing moving here at all, so `pump_ms` answers None and this
    // pushes NO subscription — a gate someone is reading costs zero frames.
    // `enterpin` owns that call, since it is that screen's timings it reads.
    // Changing the Duration swaps the recipe, which is exactly the intent: the
    // old subscription is dropped and the new one runs.
    if !state.gate_unlocked {
        if let Some(pump) = crate::ui::enterpin::pump_ms(state) {
            subs.push(iced::time::every(Duration::from_millis(pump)).map(|_| Message::Sync));
        }
        subs.push(iced::keyboard::listen().with(()).filter_map(|(_, event)| {
            let iced::keyboard::Event::KeyPressed { ref key, .. } = event else {
                return None;
            };
            match key {
                iced::keyboard::Key::Character(c) => {
                    c.chars().next().filter(char::is_ascii_digit).map(Message::GateDigit)
                }
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Backspace) => {
                    Some(Message::GateBackspace)
                }
                _ => None,
            }
        }));
    }
    if state.reimport_spinning {
        subs.push(
            iced::time::every(Duration::from_millis(16))
                .map(|_| Message::ReimportSpinnerTick),
        );
    }
    if state.btc_reimport_spinning {
        subs.push(
            iced::time::every(Duration::from_millis(16))
                .map(|_| Message::BtcReimportSpinnerTick),
        );
    }
    // Tick while the log is on screen at all — not just while a step is active —
    // so the completion cascade (which runs *after* the last step finishes) still
    // animates. Bounded: the log auto-dismisses ≤4s after reaching a terminal state.
    if state.activity_log.is_some() {
        subs.push(
            iced::time::every(Duration::from_millis(16))
                .map(|_| Message::ActivityTick(Instant::now())),
        );
    }
    // Tab cycles focus across fields (no lock screen any more, so always active).
    subs.push(
        iced::keyboard::listen().map(|event| {
            use iced::keyboard::Event;
            use iced::keyboard::key::Named;
            match event {
                Event::KeyPressed { key, modifiers, .. } => match key.as_ref() {
                    iced::keyboard::Key::Named(Named::Tab) => Message::FocusNext,
                    // The receive modals have no close button — Esc joins
                    // click-outside as their dismissal.
                    iced::keyboard::Key::Named(Named::Escape) => Message::EscapeDismiss,
                    // The pane grid's shortcut (`⌘` on the mock is Ctrl here).
                    // Forwarded raw and gated in the handler — the mapper must
                    // not read state (see the hashing note in memory);
                    // `controller::panes` decides whether the grid is on
                    // screen.
                    //
                    // `ctrl+z` was the second one, for `Undo close`. Both went
                    // 2026-09-12 (user): the undo stack only ever covered
                    // close, preset and reset, so the add, drag, resize and
                    // split that people actually reach for it after did
                    // nothing, and a shortcut that silently no-ops on most of
                    // what precedes it is worse than no shortcut.
                    iced::keyboard::Key::Character("0") if modifiers.command() => {
                        Message::GridShortcut(crate::controller::panes::GridMsg::Reset)
                    }
                    _ => Message::Sync,
                },
                _ => Message::Sync,
            }
        }),
    );
    Subscription::batch(subs)
}

struct WatchHandle<T> {
    id: &'static str,
    rx: Receiver<T>,
}

impl<T> Hash for WatchHandle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

fn watch<T>(id: &'static str, rx: Receiver<T>) -> Subscription<Message>
where
    T: Clone + Send + Sync + 'static,
{
    Subscription::run_with(WatchHandle { id, rx }, |handle| {
        iced::futures::stream::unfold(handle.rx.clone(), |mut rx| async move {
            let _ = rx.changed().await;
            Some((Message::Sync, rx))
        })
    })
}

fn watch_rate_history(rx: Receiver<RateHistoryMap>) -> Subscription<Message> {
    Subscription::run_with(WatchHandle { id: "rate_history", rx }, |handle| {
        iced::futures::stream::unfold(handle.rx.clone(), |mut rx| async move {
            let _ = rx.changed().await;
            let val = rx.borrow().clone();
            Some((Message::RateHistoryReceived(val), rx))
        })
    })
}

fn watch_rate_history_long(rx: Receiver<RateHistoryMap>) -> Subscription<Message> {
    Subscription::run_with(WatchHandle { id: "rate_history_long", rx }, |handle| {
        iced::futures::stream::unfold(handle.rx.clone(), |mut rx| async move {
            let _ = rx.changed().await;
            let val = rx.borrow().clone();
            Some((Message::RateHistoryLongReceived(val), rx))
        })
    })
}

/// A signed transaction has gone out. Edge-triggered — the counter's value is
/// never read, only its change, so a number left sitting here after the flow
/// closed can't be mistaken for a fresh dispatch.
fn watch_btc_send_dispatched(rx: Receiver<u64>) -> Subscription<Message> {
    Subscription::run_with(WatchHandle { id: "btc_send_dispatched", rx }, |handle| {
        iced::futures::stream::unfold(handle.rx.clone(), |mut rx| async move {
            let _ = rx.changed().await;
            Some((Message::BtcSendDispatched, rx))
        })
    })
}

fn watch_activity(rx: Receiver<Option<ActivityLogState>>) -> Subscription<Message> {
    Subscription::run_with(WatchHandle { id: "activity", rx }, |handle| {
        iced::futures::stream::unfold(handle.rx.clone(), |mut rx| async move {
            let _ = rx.changed().await;
            let val = rx.borrow().clone();
            Some((Message::ActivityChanged(val), rx))
        })
    })
}
