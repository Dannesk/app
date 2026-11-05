use dannesk_btc_codec::bip39::Mnemonic;
use iced::Task;
use rand::Rng;
use crate::controller::message::{Message, PlainField, SecureField};
use crate::controller::app_state::{
    AppState, BtcFeeTier, BtcView, EnableInputMode, ImportMode, SendAnchor,
};
use crate::ui::managebtc::btcsend::{self, FeeQuote};
use crate::secure::SecureString;
use crate::channel::CHANNEL;
use crate::bridge::btc_import_logic::BTCImportLogic;
use crate::bridge::btc_create_logic::BTCCreateLogic;

pub fn handle(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::BtcImportWalletClicked => {
            reset_setup_form(state);
            state.btc_view = BtcView::Import;
        }
        Message::BtcCreateWalletClicked => {
            reset_setup_form(state);
            state.btc_view = BtcView::Create;
        }
        Message::BtcGenerateMnemonic => {
            if state.create_mnemonic.as_str().split_whitespace().count() == 0 {
                let mut entropy = [0u8; 32];
                rand::rng().fill_bytes(&mut entropy);
                let mnemonic = Mnemonic::from_entropy(&entropy).unwrap();
                state.create_mnemonic = SecureString::new(mnemonic.to_string());
            }
        }
        Message::BtcCopyMnemonic => {
            let words = state.create_mnemonic.as_str().to_owned();
            if !words.is_empty() {
                state.btc_copy_feedback = true;
                crate::utils::clipboard::copy_secret(words);
                return Task::perform(
                    async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                    |_| Message::BtcCopyMnemonicFeedback,
                );
            }
        }
        Message::BtcCopyMnemonicFeedback => {
            state.btc_copy_feedback = false;
        }
        Message::BtcCreateSubmitClicked => {
            if !crate::ui::components::wallet_setup::can_finish(
                crate::ui::components::wallet_setup::SetupFlow::Create,
                &state.create_mnemonic,
                state.btc_word25,
                &state.btc_bip39_input,
                state.btc_import_mode,
                &state.btc_encryption_input,
            ) {
                return Task::none();
            }
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                abandon_setup(state, "Create Bitcoin wallet");
                return Task::none();
            };

            let mnemonic = state.create_mnemonic.take();
            let bip39 = state.btc_bip39_input.take_trimmed();
            state.hold_entropy(SecureField::BtcEncryption);
            let encryption = state.btc_encryption_input.take_trimmed();
            let mode = state.btc_import_mode;
            let script_type = crate::ui::managebtc::btcsetup::create_type(state.btc_script_type);

            return Task::perform(
                async move { BTCCreateLogic::process(mnemonic, bip39, encryption, mode, script_type, ws_tx).await },
                |result| match result {
                    Ok(()) => Message::BtcCreateCompleted,
                    Err(_) => Message::BtcCreateFailed,
                },
            );
        }
        Message::BtcCreateCompleted => {
            close_setup(state);
        }
        Message::BtcCreateFailed => {
            close_setup(state);
        }
        Message::BtcWord25Chosen(yes) => {
            state.btc_word25 = yes;
            if !yes {
                state.btc_bip39_input.clear();
                state.btc_bip39_reveal = false;
            }
        }
        Message::BtcScriptTypeChosen(t) => {
            state.btc_script_type = t;
        }
        Message::BtcImportModeChanged(mode) => {
            if state.btc_import_mode != mode {
                state.btc_encryption_input.clear();
                state.btc_encryption_reveal = false;
                state.btc_import_mode = mode;
            }
        }
        Message::BtcImportSubmitClicked => {
            if !crate::ui::components::wallet_setup::can_finish(
                crate::ui::components::wallet_setup::SetupFlow::Import,
                &state.btc_seed_text,
                state.btc_word25,
                &state.btc_bip39_input,
                state.btc_import_mode,
                &state.btc_encryption_input,
            ) {
                return Task::none();
            }
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                abandon_setup(state, "Import Bitcoin wallet");
                return Task::none();
            };

            let mnemonic = state.btc_seed_text.take();
            let bip39 = state.btc_bip39_input.take_trimmed();
            state.hold_entropy(SecureField::BtcEncryption);
            let encryption = state.btc_encryption_input.take_trimmed();
            let mode = state.btc_import_mode;
            let script_type = state.btc_script_type;

            return Task::perform(
                async move { BTCImportLogic::process(mnemonic, bip39, encryption, mode, script_type, ws_tx).await },
                |result| match result {
                    Ok(()) => Message::BtcImportCompleted,
                    Err(_) => Message::BtcImportFailed,
                },
            );
        }
        Message::BtcImportCompleted => {
            close_setup(state);
        }
        Message::BtcImportFailed => {
            close_setup(state);
        }
        Message::BtcTxSelected(txid) => {
            state.btc_tx_selected =
                if state.btc_tx_selected.as_deref() == Some(txid.as_str()) { None } else { Some(txid) };
        }
        Message::BtcCopyTxid(txid) => {
            state.btc_txid_copied = Some(txid.clone());
            return Task::batch([
                iced::clipboard::write(txid).discard(),
                Task::perform(
                    async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                    |_| Message::BtcCopyTxidFeedback,
                ),
            ]);
        }
        Message::BtcCopyTxidFeedback => {
            state.btc_txid_copied = None;
        }
        Message::BtcBackClicked => {
            close_setup(state);
        }
        Message::BtcDeleteKey => {
            state.btc_key_mgmt_restoring = false;
            let (_, wallet_address, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
            let Some(address) = wallet_address else { return Task::none(); };
            state.btc_key_purge_requested = true;
            return Task::perform(
                async move {
                    crate::bridge::btc_wallet_operations::BitcoinWalletOperations::delete_key(
                        address,
                    )
                    .await
                },
                |_| Message::Sync,
            );
        }
        Message::BtcRemoveWallet => {
            let (_, wallet_address, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
            let Some(address) = wallet_address else { return Task::none(); };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                return Task::none();
            };
            if state.btc_remove_wallet_requested { return Task::none(); }
            state.btc_remove_wallet_requested = true;
            return Task::perform(
                async move {
                    crate::bridge::btc_wallet_operations::BitcoinWalletOperations::remove_wallet(
                        address, ws_tx,
                    )
                    .await
                },
                |_| Message::BtcWalletRemoved,
            );
        }
        Message::BtcWalletRemoved => {
            state.btc_remove_wallet_requested = false;
            forget_wallet(state);
        }
        Message::BtcCopyAddress => {
            let addr = CHANNEL.bitcoin_wallet_rx.borrow().1.clone();
            if let Some(address) = addr {
                state.btc_copy_feedback = true;
                return Task::batch([
                    iced::clipboard::write(address).discard(),
                    Task::perform(
                        async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                        |_| Message::BtcCopyAddressFeedback,
                    ),
                ]);
            }
        }
        Message::BtcCopyAddressFeedback => {
            state.btc_copy_feedback = false;
        }
        Message::BtcBalanceUnitToggled => {
            let sats = !state.btc_balance_sats;
            set_balance_sats(state, sats);
        }
        Message::BtcCopyMasterAddress => {
            let addr = CHANNEL.bitcoin_wallet_rx.borrow().1.clone();
            if let Some(address) = addr {
                state.btc_master_copy_feedback = true;
                return Task::batch([
                    iced::clipboard::write(address).discard(),
                    Task::perform(
                        async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                        |_| Message::BtcCopyMasterAddressFeedback,
                    ),
                ]);
            }
        }
        Message::BtcCopyMasterAddressFeedback => {
            state.btc_master_copy_feedback = false;
        }
        Message::BtcSendFeeFaceToggled => {
            state.btc_send_fee_face = !state.btc_send_fee_face;
            close_custom(state);
            push_send_draft(state);
        }
        Message::BtcSendFeeTierSelected(tier) => {
            match tier {
                BtcFeeTier::Custom => {
                    let tiers_known = btcsend::quote().is_some_and(|q| q.tiers.is_some());
                    if tiers_known && !state.btc_send_custom_open {
                        state.btc_send_fee = state.btc_send_custom.clone();
                        state.btc_send_custom_open = true;
                    }
                }
                named => {
                    state.btc_send_fee_tier = named;
                    state.btc_send_fee_face = false;
                    close_custom(state);
                    follow_max(state);
                }
            }
            push_send_draft(state);
        }
        Message::BtcSendSetFee => {
            if let Some(rate) = state.btc_send_fee.trim().parse::<f32>().ok().filter(|r| r.is_finite() && *r > 0.0) {
                state.btc_send_custom = crate::ui::managebtc::node::rate_text(rate);
                state.btc_send_fee_tier = BtcFeeTier::Custom;
                state.btc_send_fee_face = false;
                close_custom(state);
                follow_max(state);
            }
            push_send_draft(state);
        }
        Message::BtcSendMaxClicked => {
            if let Some(max) = in_use_now(state).map(|f| f.max).filter(|m| *m > 0) {
                set_max_amount(state, max);
                state.btc_send_max_on = true;
            }
            push_send_draft(state);
        }
        Message::BtcSendAnchorToggled => {
            state.btc_send_anchor = match state.btc_send_anchor {
                SendAnchor::Amount => SendAnchor::Fiat,
                SendAnchor::Fiat => SendAnchor::Amount,
            };
            state.btc_send_max_on = false;
        }
        Message::BtcSendRecipientCleared => {
            state.btc_send_recipient.clear();
            push_send_draft(state);
        }
        Message::BtcSendContinueClicked => {
            let quote = btcsend::quote();
            let gate = btcsend::gate(state, quote.as_ref());
            let fee = quote.as_ref().and_then(|q| btcsend::in_use(state, q));
            if let (true, Some(f)) = (gate.ready, fee) {
                state.btc_send_recipient = state.btc_send_recipient.trim().to_string();
                state.btc_send_locked = Some(f.fee);
                state.btc_send_step = 2;
            }
        }
        Message::BtcSendReviewCancelled => {
            state.btc_send_step = 0;
            state.btc_send_locked = None;
            state.btc_send_passphrase.clear();
            state.btc_send_passphrase_reveal = false;
            state.btc_send_bip39.clear();
            state.btc_send_bip39_reveal = false;
            state.btc_send_seed_text.clear();
            state.btc_send_seed_reveal = false;
        }
        Message::BtcSendSubmitClicked => {
            if state.btc_send_step != 2 {
                return Task::none();
            }
            let (Some(locked), Some(sats)) = (state.btc_send_locked, btcsend::amount_sats(state)) else {
                return Task::none();
            };
            let (_, wallet_address, key_deleted, key_mode) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
            let Some(address) = wallet_address else {
                abandon_send();
                return Task::none();
            };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                abandon_send();
                return Task::none();
            };
            let mode = match EnableInputMode::detect(key_deleted, key_mode) {
                EnableInputMode::Passphrase => "passphrase",
                EnableInputMode::Seed => "seed",
            }
            .to_string();
            let passphrase = state.btc_send_passphrase.take_trimmed();
            let mnemonic = state.btc_send_seed_text.take();
            let bip39 = state.btc_send_bip39.take_trimmed();
            let recipient = state.btc_send_recipient.trim().to_string();
            let amount = format!("{}.{:08}", sats / 100_000_000, sats % 100_000_000);
            let fee = locked.to_string();
            state.btc_send_passphrase_reveal = false;
            state.btc_send_seed_reveal = false;
            state.btc_send_bip39_reveal = false;
            return Task::perform(
                async move {
                    crate::bridge::btc_send_logic::BtcSendLogic::process(
                        crate::bridge::btc_send_logic::BtcSendParams {
                            mode,
                            passphrase,
                            mnemonic,
                            bip39_pass: bip39,
                            recipient,
                            amount,
                            fee,
                            wallet_address: address,
                            asset: "BTC".to_string(),
                            ws_tx,
                        },
                    )
                    .await
                },
                |result| match result {
                    Ok(()) => Message::BtcSendCompleted,
                    Err(e) => Message::BtcSendFailed(e),
                },
            );
        }
        Message::BtcSendCompleted => {
        }
        Message::BtcSendFailed(_) => {
        }

        Message::BtcSendDispatched => {
            clear_btc_send_form(state);
            clear_btc_bump_form(state);
            state.btc_view = BtcView::Menu;
        }

        Message::BtcBumpClicked(txid) => {
            clear_btc_bump_form(state);
            state.btc_bump_txid = Some(txid);
        }
        Message::BtcBumpDismissed => {
            clear_btc_bump_form(state);
        }
        Message::BtcBumpFeeTierSelected(t) => {
            state.btc_bump_fee_tier = t;
        }
        Message::BtcBumpSubmitClicked => {
            let Some(txid) = state.btc_bump_txid.clone() else { return Task::none() };
            let Some(fee) = crate::ui::managebtc::btcbump::fee_to_sign(state) else {
                return Task::none();
            };
            let (_, wallet_address, key_deleted, key_mode) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
            let Some(address) = wallet_address else {
                abandon_send();
                return Task::none();
            };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                abandon_send();
                return Task::none();
            };
            let mode = match EnableInputMode::detect(key_deleted, key_mode) {
                EnableInputMode::Passphrase => "passphrase",
                EnableInputMode::Seed => "seed",
            }
            .to_string();
            let passphrase = state.btc_bump_passphrase.take_trimmed();
            let mnemonic = state.btc_bump_seed_text.take();
            let bip39 = state.btc_bump_bip39.take_trimmed();
            state.btc_bump_passphrase_reveal = false;
            state.btc_bump_seed_reveal = false;
            state.btc_bump_bip39_reveal = false;
            return Task::perform(
                async move {
                    crate::bridge::btc_bump_logic::BtcBumpLogic::process(
                        crate::bridge::btc_bump_logic::BtcBumpParams {
                            mode,
                            passphrase,
                            mnemonic,
                            bip39_pass: bip39,
                            txid,
                            fee: fee.to_string(),
                            wallet_address: address,
                            ws_tx,
                        },
                    )
                    .await
                },
                |result| match result {
                    Ok(()) => Message::BtcBumpCompleted,
                    Err(e) => Message::BtcBumpFailed(e),
                },
            );
        }
        Message::BtcBumpCompleted | Message::BtcBumpFailed(_) => {
        }
        Message::BtcReimportConfirm => {
            if state.btc_reimport_spinning { return Task::none(); }
            if state.btc_reimport_seed_text.as_str().split_whitespace().count() != 24
                || !crate::ui::components::wallet_setup::mnemonic_checksum_ok(
                    state.btc_reimport_seed_text.as_str(),
                )
                || state.btc_reimport_passphrase.trimmed_char_len()
                    < crate::ui::components::wallet_setup::KEY_MIN
            {
                return Task::none();
            }
            state.btc_reimport_spinning = true;
            state.btc_reimport_error = None;
            let mnemonic = state.btc_reimport_seed_text.take();
            let bip39 = state.btc_reimport_bip39.take_trimmed();
            state.hold_entropy(SecureField::BtcReimportPassphrase);
            let passphrase = state.btc_reimport_passphrase.take_trimmed();
            return Task::perform(
                async move {
                    crate::bridge::btc_wallet_operations::BitcoinWalletOperations::reimport_key(
                        mnemonic, bip39, passphrase,
                    ).await
                },
                |result| match result {
                    Ok(()) => Message::BtcReimportSuccess,
                    Err(e) => Message::BtcReimportFailed(e),
                },
            );
        }
        Message::BtcReimportSuccess => {
            clear_btc_reimport_form(state);
            state.btc_key_purge_requested = false;
            state.btc_key_mgmt_restoring = false;
        }
        Message::BtcReimportFailed(e) => {
            state.btc_reimport_spinning = false;
            state.btc_reimport_error = Some(e);
        }

        Message::BtcKeyMgmtRestoreToggled => {
            if state.btc_reimport_spinning { return Task::none(); }
            clear_btc_reimport_form(state);
            state.btc_key_mgmt_restoring = !state.btc_key_mgmt_restoring;
        }
        _ => {}
    }
    Task::none()
}

pub(crate) fn set_balance_sats(state: &mut AppState, sats: bool) {
    state.btc_balance_sats = sats;
    let _ = crate::bridge::json_storage::update_json::<serde_json::Value>("settings.json", |json| {
        if let Some(obj) = json.as_object_mut() {
            obj.insert("btc_sats".to_string(), serde_json::json!(sats));
        }
    });
}

pub(crate) fn clear_btc_receive_line(state: &mut AppState) {
    state.btc_copy_feedback = false;
}

pub(crate) fn clear_btc_reimport_form(state: &mut AppState) {
    state.release_entropy();
    state.btc_reimport_seed_text.clear();
    state.btc_reimport_seed_reveal = false;
    state.btc_reimport_passphrase.clear();
    state.btc_reimport_passphrase_reveal = false;
    state.btc_reimport_bip39.clear();
    state.btc_reimport_bip39_reveal = false;
    state.btc_reimport_error = None;
    state.btc_reimport_spinning = false;
}

fn reset_setup_form(state: &mut AppState) {
    state.release_entropy();
    state.btc_seed_text.clear();
    state.btc_seed_reveal = false;
    state.btc_encryption_input.clear();
    state.btc_encryption_reveal = false;
    state.btc_bip39_input.clear();
    state.btc_bip39_reveal = false;
    state.btc_word25 = false;
    state.create_mnemonic.clear();
    state.create_seed_reveal = false;
    state.btc_import_mode = ImportMode::default();
    state.btc_script_type = Default::default();
    state.btc_copy_feedback = false;
}

fn close_setup(state: &mut AppState) {
    reset_setup_form(state);
    state.btc_view = BtcView::Menu;
}

fn abandon_setup(state: &mut AppState, title: &'static str) {
    let mut log = crate::channel::ActivityLogState::new(title, &[("connect", "Contacting the network")]);
    log.fail("connect", "No connection to the network — nothing was sent.".to_string());
    let _ = crate::channel::CHANNEL.activity_tx.send(Some(log));
    close_setup(state);
}

fn abandon_send() {
    let mut log = crate::channel::ActivityLogState::new(
        "Send bitcoin",
        &[("connect", "Contacting the network")],
    );
    log.fail("connect", "No connection to the network — nothing was sent.".to_string());
    let _ = crate::channel::CHANNEL.activity_tx.send(Some(log));
}

pub fn after_plain_edit(state: &mut AppState, field: PlainField) {
    match field {
        PlainField::BtcSendAmount => {
            state.btc_send_anchor = SendAnchor::Amount;
            state.btc_send_max_on = false;
            sync_fiat_from_amount(state);
        }
        PlainField::BtcSendFiat => {
            state.btc_send_anchor = SendAnchor::Fiat;
            state.btc_send_max_on = false;
            sync_amount_from_fiat(state);
        }
        PlainField::BtcBumpFee => state.btc_bump_fee_tier = BtcFeeTier::Custom,
        _ => {}
    }
    push_send_draft(state);
}

pub fn push_send_draft(state: &AppState) {
    let draft = Some(btcsend::draft(state));
    CHANNEL.btc_send_draft_tx.send_if_modified(|d| {
        if *d == draft {
            false
        } else {
            *d = draft;
            true
        }
    });
}

fn close_custom(state: &mut AppState) {
    state.btc_send_custom_open = false;
    state.btc_send_fee.clear();
}

fn in_use_now(state: &AppState) -> Option<FeeQuote> {
    let q = btcsend::quote()?;
    btcsend::in_use(state, &q)
}

fn set_max_amount(state: &mut AppState, sats: u64) {
    state.btc_send_amount = crate::utils::format_token_amount(sats as f64 / 1e8, 8);
    state.btc_send_anchor = SendAnchor::Amount;
    sync_fiat_from_amount(state);
}

fn follow_max(state: &mut AppState) {
    if !state.btc_send_max_on {
        return;
    }
    if let Some(max) = in_use_now(state).map(|f| f.max)
        && btcsend::amount_sats(state) != Some(max)
    {
        set_max_amount(state, max);
    }
}

fn sync_fiat_from_amount(state: &mut AppState) {
    let rate = crate::utils::price::cross("BTC", state.base_currency.code());
    state.btc_send_fiat_amount = match state.btc_send_amount.trim().parse::<f64>() {
        Ok(amount) => crate::utils::fiat_amount(amount * rate),
        Err(_) => String::new(),
    };
}

fn sync_amount_from_fiat(state: &mut AppState) {
    let rate = crate::utils::price::cross("BTC", state.base_currency.code());
    state.btc_send_amount = match state.btc_send_fiat_amount.trim().parse::<f64>() {
        Ok(fiat) if rate > 0.0 => format!("{:.8}", fiat / rate),
        _ => String::new(),
    };
}

pub fn resync_send_twin(state: &mut AppState) {
    if crate::utils::price::cross("BTC", state.base_currency.code()) > 0.0 {
        match state.btc_send_anchor {
            SendAnchor::Amount => sync_fiat_from_amount(state),
            SendAnchor::Fiat => sync_amount_from_fiat(state),
        }
    }
    follow_max(state);
    push_send_draft(state);
}

pub(crate) fn clear_btc_bump_form(state: &mut AppState) {
    state.btc_bump_txid = None;
    state.btc_bump_fee_tier = BtcFeeTier::default();
    state.btc_bump_fee = String::new();
    state.btc_bump_passphrase.clear();
    state.btc_bump_passphrase_reveal = false;
    state.btc_bump_bip39.clear();
    state.btc_bump_bip39_reveal = false;
    state.btc_bump_seed_text.clear();
    state.btc_bump_seed_reveal = false;
}

pub(crate) fn clear_btc_send_form(state: &mut AppState) {
    state.btc_send_step = 0;
    state.btc_send_recipient = String::new();
    state.btc_send_amount = String::new();
    state.btc_send_fiat_amount = String::new();
    state.btc_send_anchor = SendAnchor::Amount;
    state.btc_send_fee = String::new();
    state.btc_send_fee_tier = BtcFeeTier::Med;
    state.btc_send_custom = String::new();
    state.btc_send_custom_open = false;
    state.btc_send_fee_face = false;
    state.btc_send_locked = None;
    state.btc_send_max_on = false;
    state.btc_send_passphrase.clear();
    state.btc_send_passphrase_reveal = false;
    state.btc_send_bip39.clear();
    state.btc_send_bip39_reveal = false;
    state.btc_send_seed_text.clear();
    state.btc_send_seed_reveal = false;
    push_send_draft(state);
}

pub(crate) fn forget_wallet(state: &mut AppState) {
    state.btc_view = BtcView::Menu;
    state.btc_key_purge_requested = false;
    clear_btc_receive_line(state);
    clear_btc_send_form(state);
    clear_btc_bump_form(state);
}
