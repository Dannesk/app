pub mod message;
pub mod app_state;
pub mod subscription;
pub mod xrp;
pub mod btc;
pub mod panes;

use iced::Task;
use std::time::Duration;
use crate::channel::CHANNEL;
use crate::controller::message::Message;
use crate::controller::app_state::XrpView;
use crate::controller::app_state::{AppState, Tab};

const SHORT_HISTORY_CAP: usize = 120;           
const SHORT_BUCKET_MS: u64 = 30 * 1000;           
const LONG_HISTORY_CAP: usize = 288;              
const LONG_BUCKET_MS: u64 = 5 * 60 * 1000;        
const SHORT_SPAN_MS: u64 = SHORT_HISTORY_CAP as u64 * SHORT_BUCKET_MS;
const LONG_SPAN_MS: u64 = LONG_HISTORY_CAP as u64 * LONG_BUCKET_MS;

fn within_span(series: Vec<(u64, f32)>, span_ms: u64) -> std::collections::VecDeque<(u64, f32)> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let oldest = now_ms.saturating_sub(span_ms);
    series.into_iter().filter(|&(ts, _)| ts >= oldest).collect()
}

pub use subscription::subscriptions;

fn wipe_wallet_data() {
    use crate::bridge::json_storage::{remove_json, write_json};
    use crate::channel::{TransactionState, BtcTransactionState, WSCommand};

    let (_, xrp_address, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
    let (_, btc_address, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
    let ws_tx = crate::ws::CRYPTO_COMMANDS_TX.get().cloned();

    crate::gate::delete();

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

    for file in &["xrp_encrypt.json", "xrp.json", "btc_encrypt.json", "btc.json", "settings.json"] {
        let _ = remove_json(file);
    }
    let _ = write_json("settings.json", &crate::controller::app_state::default_settings());

    let _ = CHANNEL.wallet_balance_tx.send((0.0, None, false, crate::channel::KeyMode::Standard));
    let _ = CHANNEL.transactions_tx.send(TransactionState::default());
    CHANNEL.clear_tokens();
    let _ = CHANNEL.bitcoin_wallet_tx.send((0.0, None, false, crate::channel::KeyMode::Standard));
    let _ = CHANNEL.btc_transactions_tx.send(BtcTransactionState::default());
    let _ = CHANNEL.btc_utxos_tx.send((None, Vec::new()));
    let _ = CHANNEL.pending_trade_tx.send(None);
    let _ = CHANNEL.activity_tx.send(None);
}

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
        Message::SecurePasteRequested(field) => {
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
        Message::SignCredentialCleared { secret, phrase, bip39 } => {
            for field in [secret, phrase, bip39] {
                state.replace_secure_field(field, String::new());
                state.set_secure_reveal(field, false);
            }
        }
        Message::EscapeDismiss => {
            state.xrp_grid.menu_open = false;
            state.btc_grid.menu_open = false;
            state.send_asset_picker_open = false;
            panes::dismiss_pair_search(state);
        }
        Message::PlainEdit(field, op) => {
            if state.apply_plain_edit(field, op) {
                after_plain_edit(state, field);
            }
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
        Message::GateDigit(c) => {
            if state.gate_wiping || !c.is_ascii_digit() {
                return Task::none();
            }
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
                match crate::gate::verify_pin(&pin) {
                    Ok(true) => {
                        crate::gate::clear_attempts();
                        state.gate_unlocked = true;
                        crate::startup_trace::stamp("PIN accepted");
                        state.gate_error = None;
                    }
                    Ok(false) => return gate_wrong_attempt(state),
                    Err(e) => gate_fail(state, e),
                }
            } else {
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
                crate::gate::delete();
                state.gate_exists = false;
            } else {
                state.gate_exists = crate::gate::exists();
                if !state.gate_exists {
                    state.gate_unlocked = false;
                    state.gate_pin_draft = None;
                    state.gate_pin_input.clear();
                    state.gate_error = None;
                }
            }
        }
        Message::Sync => {
            panes::note_ledger(state);
            xrp::trade_expiry_check();
            crate::controller::xrp::resync_send_twin(state);
            crate::controller::btc::resync_send_twin(state);
            state.xrp_grid.flush_if_settled(panes::XRP_KEY);
            state.btc_grid.flush_if_settled(panes::BTC_KEY);
            if state.selected_tab == Tab::Btc && state.btc_grid.has(panes::PaneKind::Receive) {
                crate::controller::btc::refresh_receive_address(state, false);
            }
            if state.xrp_view == XrpView::Menu {
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
            let assets: Vec<(String, f32)> = crate::channel::CHANNEL.rates_rx.borrow()
                .iter().map(|(a, &p)| (a.clone(), p)).collect();

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
            if state.selected_tab == Tab::Btc {
                crate::controller::btc::refresh_receive_address(state, true);
            }
            crate::controller::xrp::sync_trade_books(state);
        }
        Message::OpenSettings => {
            state.settings_open = true;
            state.prefs_erase_confirm = false;
        }
        Message::CloseSettings => {
            state.settings_open = false;
            state.prefs_erase_confirm = false;
        }
        Message::ActivityChanged(val) => {
            state.activity_log = val;
        }
        Message::ActivityTick(now) => {
            state.activity_tick = now;
        }
        Message::HistoryLoadMore(list) => return Task::future(crate::bridge::history_logic::load_more(list)).discard(),
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
            state.base_currency = ccy;
            let base_code = ccy.code();
            let _ = json_storage::update_json::<Value>("settings.json", |json| {
                if let Some(obj) = json.as_object_mut() {
                    obj.insert("base_currency".to_string(), serde_json::json!(base_code));
                    obj.remove("local_currency");
                }
            });
        }
        Message::PrefsEraseAsked => {
            state.prefs_erase_confirm = true;
        }
        Message::PrefsEraseDismissed => {
            state.prefs_erase_confirm = false;
        }
        Message::PrefsEraseConfirmed => {
            if std::mem::take(&mut state.prefs_erase_confirm) {
                wipe_wallet_data();
                state.gate_exists = false;
                xrp::forget_wallet(state);
                btc::forget_wallet(state);
                state.reset_settings();
            }
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
        | Message::BtcDropAsked
        | Message::BtcDropDismissed
        | Message::BtcBalanceUnitToggled
        | Message::BtcCopyMasterAddress
        | Message::BtcCopyMasterAddressFeedback
        | Message::BtcCopyTxid(_)
        | Message::BtcCopyTxidFeedback
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
        | Message::BtcReimportConfirm
        | Message::BtcReimportSuccess
        | Message::BtcReimportFailed(_)
        | Message::BtcKeyMgmtRestoreToggled
        | Message::BtcTxSelected(_)) => return btc::handle(state, msg),
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

fn after_plain_edit(state: &mut AppState, field: crate::controller::message::PlainField) {
    if field.is_btc() {
        crate::controller::btc::after_plain_edit(state, field);
    } else {
        crate::controller::xrp::after_plain_edit(state, field);
    }
}
