use iced::Task;
use crate::controller::message::{Message, PlainField, SecureField, TagPane};
use crate::controller::app_state::{AppState, EnableInputMode, ImportMode, SendAnchor, XrpTokenTab, XrpView, TradeAnchor, TradeContract, TradeSide};
use crate::secure::SecureString;
use crate::channel::CHANNEL;
use crate::bridge::xrp_import_logic::XRPImportLogic;
use crate::bridge::xrp_create_logic::XRPCreateLogic;
use crate::bridge::cancel_logic::CancelLogic;
use crate::bridge::trade_logic::TradeLogic;
use crate::ws::{rates_send, RatesCommand};

pub fn handle(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::ImportWalletClicked => {
            reset_setup_form(state);
            state.xrp_view = XrpView::Import;
        }
        Message::CreateWalletClicked => {
            reset_setup_form(state);
            state.xrp_view = XrpView::Create;
        }
        Message::TxCardToggled(id) => {
            if state.tx_expanded.as_deref() == Some(&id) {
                state.tx_expanded = None;
            } else {
                state.tx_expanded = Some(id);
            }
        }
        Message::CopyTxHash(hash) => {
            state.tx_hash_copied = Some(hash.clone());
            return Task::batch([
                iced::clipboard::write(hash).discard(),
                Task::perform(
                    async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                    |_| Message::CopyTxHashFeedback,
                ),
            ]);
        }
        Message::CopyTxHashFeedback => {
            state.tx_hash_copied = None;
        }
        Message::BackClicked => match state.xrp_view {
            XrpView::Menu => {
                clear_enable_form(state);
                state.xrp_token_tab = XrpTokenTab::Xrp;
            }
            _ => {
                reset_setup_form(state);
                state.xrp_view = XrpView::Menu;
            }
        },
        Message::GenerateMnemonic => {
            if state.create_mnemonic.as_str().split_whitespace().count() == 0 {
                use dannesk_btc_codec::bip39::Mnemonic;
                use rand::RngExt;
                let mut entropy = [0u8; 32];
                rand::rng().fill(&mut entropy);
                let mnemonic = Mnemonic::from_entropy(&entropy).unwrap();
                state.create_mnemonic = SecureString::new(mnemonic.to_string());
            }
        }
        Message::Word25Chosen(yes) => {
            state.xrp_word25 = yes;
            if !yes {
                state.xrp_bip39_input.clear();
                state.xrp_bip39_reveal = false;
            }
        }
        Message::ImportModeChanged(mode) => {
            if state.xrp_import_mode != mode {
                state.xrp_encryption_input.clear();
                state.xrp_encryption_reveal = false;
                state.xrp_import_mode = mode;
            }
        }
        Message::ImportSubmitClicked => {
            if !crate::ui::components::wallet_setup::can_finish(
                crate::ui::components::wallet_setup::SetupFlow::Import,
                &state.xrp_seed_text,
                state.xrp_word25,
                &state.xrp_bip39_input,
                state.xrp_import_mode,
                &state.xrp_encryption_input,
            ) {
                return Task::none();
            }
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                abandon_setup(state, "Import XRP wallet");
                return Task::none();
            };

            let mnemonic = state.xrp_seed_text.take();
            let bip39 = state.xrp_bip39_input.take_trimmed();
            state.hold_entropy(SecureField::XrpEncryption);
            let encryption = state.xrp_encryption_input.take_trimmed();
            let mode = state.xrp_import_mode;

            return Task::perform(
                async move { XRPImportLogic::process(mnemonic, bip39, encryption, mode, ws_tx).await },
                |result| match result {
                    Ok(()) => Message::ImportCompleted,
                    Err(_) => Message::ImportFailed,
                },
            );
        }
        Message::ImportCompleted => {
            close_setup(state);
        }
        Message::ImportFailed => {
            close_setup(state);
        }
        Message::DeleteXrpKey => {
            state.key_mgmt_restoring = false;
            let (_, wallet_address, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
            let Some(address) = wallet_address else { return Task::none(); };
            state.key_purge_requested = true;
            return Task::perform(
                async move {
                    crate::bridge::xrp_wallet_operations::WalletOperations::delete_key(address).await
                },
                |_| Message::Sync,
            );
        }
        Message::RemoveXrpWallet => {
            let (_, wallet_address, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
            let Some(address) = wallet_address else { return Task::none(); };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                return Task::none();
            };
            if state.remove_wallet_requested { return Task::none(); }
            state.remove_wallet_requested = true;
            return Task::perform(
                async move {
                    crate::bridge::xrp_wallet_operations::WalletOperations::remove_wallet(address, ws_tx).await
                },
                |_| Message::XrpWalletRemoved,
            );
        }
        Message::XrpWalletRemoved => {
            state.remove_wallet_requested = false;
            forget_wallet(state);
        }
        Message::CopyAddress => {
            let (_, addr, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
            if let Some(address) = addr.map(|a| receive_address(state, &a)) {
                state.xrp_copy_feedback = true;
                return Task::batch([
                    iced::clipboard::write(address).discard(),
                    Task::perform(
                        async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                        |_| Message::CopyAddressFeedback,
                    ),
                ]);
            }
        }
        Message::CopyAddressFeedback => {
            state.xrp_copy_feedback = false;
        }
        Message::WalletCopyAddress => {
            let (_, addr, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
            if let Some(address) = addr {
                state.xrp_wallet_copy_feedback = true;
                return Task::batch([
                    iced::clipboard::write(address).discard(),
                    Task::perform(
                        async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                        |_| Message::WalletCopyAddressFeedback,
                    ),
                ]);
            }
        }
        Message::WalletCopyAddressFeedback => {
            state.xrp_wallet_copy_feedback = false;
        }
        Message::CopyMnemonic => {
            let words = state.create_mnemonic.as_str().to_owned();
            if !words.is_empty() {
                state.xrp_copy_feedback = true;
                crate::utils::clipboard::copy_secret(words);
                return Task::perform(
                    async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                    |_| Message::CopyMnemonicFeedback,
                );
            }
        }
        Message::CopyMnemonicFeedback => {
            state.xrp_copy_feedback = false;
        }
        Message::XrpTokenTabChanged(tab) => {
            state.xrp_token_tab = tab;
            resync_send_twin(state);
            state.send_asset_picker_open = false;
            clear_enable_form(state);
            if !state.send_amount.trim().is_empty() {
                after_plain_edit(state, PlainField::XrpSendAmount);
            }
        }
        Message::TagEdit(pane) => {
            let (set, editing, draft) = tag_slots(state, pane);
            *draft = set.clone();
            *editing = true;
        }
        Message::TagSet(pane) => {
            let (set, editing, draft) = tag_slots(state, pane);
            let d = draft.trim();
            if !d.is_empty() && d.parse::<u32>().is_err() {
                return Task::none();
            }
            *set = d.to_string();
            draft.clear();
            *editing = false;
            if pane == TagPane::Receive {
                persist_receive_tag(state);
            }
        }
        Message::TagCancelled(pane) => {
            let (_, editing, draft) = tag_slots(state, pane);
            draft.clear();
            *editing = false;
        }
        Message::TagCleared(pane) => {
            let (set, editing, draft) = tag_slots(state, pane);
            set.clear();
            draft.clear();
            *editing = false;
            if pane == TagPane::Receive {
                persist_receive_tag(state);
            }
        }
        Message::CopyReceiveTag => {
            if let Some(tag) = tag_of(&state.receive_tag) {
                state.xrp_tag_copied = true;
                return Task::batch([
                    iced::clipboard::write(tag.to_string()).discard(),
                    Task::perform(
                        async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                        |_| Message::CopyReceiveTagFeedback,
                    ),
                ]);
            }
        }
        Message::CopyReceiveTagFeedback => {
            state.xrp_tag_copied = false;
        }
        Message::SendAssetPickerToggled => {
            state.send_asset_picker_open = !state.send_asset_picker_open;
        }
        Message::EnableTokenClicked(code) => {
            state.xrp_token_tab = XrpTokenTab::Token(code);
            state.show_enable = true;
            clear_enable_form(state);
        }
        Message::EnableDismissed => {
            state.show_enable = false;
            clear_enable_form(state);
        }
        Message::EnableSubmitClicked => {
            let (_, wallet_address, key_deleted, key_mode) = CHANNEL.wallet_balance_rx.borrow().clone();
            let Some(address) = wallet_address else {
                state.enable_error = Some("ERR: NO_WALLET_FOUND".to_string());
                return Task::none();
            };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                state.enable_error = Some("ERR: WS_NOT_READY".to_string());
                return Task::none();
            };
            let asset = match state.xrp_token_tab.code() {
                Some(code) => code.to_string(),
                None => return Task::none(),
            };
            let mode = match EnableInputMode::detect(key_deleted, key_mode) {
                EnableInputMode::Passphrase => "passphrase",
                EnableInputMode::Seed => "seed",
            }
            .to_string();
            let passphrase = state.enable_passphrase.take_trimmed();
            let mnemonic = state.enable_seed_text.take();
            let bip39 = state.enable_bip39.take_trimmed();
            clear_enable_form(state);
            state.show_enable = false;
            return Task::perform(
                async move {
                    crate::bridge::enable_logic::TrustlineEnableLogic::process(
                        mode,
                        passphrase,
                        mnemonic,
                        bip39,
                        address,
                        asset,
                        crate::bridge::enable_logic::ENABLE_LIMIT,
                        "Enable trustline",
                        ws_tx,
                    )
                    .await
                },
                |_| Message::Sync,
            );
        }
        Message::DisableTokenClicked(code) => {
            state.disable_token = Some(code);
            clear_disable_form(state);
        }
        Message::DisableDismissed => {
            state.disable_token = None;
            clear_disable_form(state);
        }
        Message::DisableSubmitClicked => {
            let (_, wallet_address, key_deleted, key_mode) = CHANNEL.wallet_balance_rx.borrow().clone();
            let Some(address) = wallet_address else {
                state.disable_error = Some("ERR: NO_WALLET_FOUND".to_string());
                return Task::none();
            };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                state.disable_error = Some("ERR: WS_NOT_READY".to_string());
                return Task::none();
            };
            let Some(code) = state.disable_token else {
                return Task::none();
            };
            if CHANNEL.token(code).0 != 0.0 {
                state.disable_error = Some("ERR: BALANCE_NOT_ZERO".to_string());
                return Task::none();
            }
            let asset = code.to_string();
            let mode = match EnableInputMode::detect(key_deleted, key_mode) {
                EnableInputMode::Passphrase => "passphrase",
                EnableInputMode::Seed => "seed",
            }
            .to_string();
            let passphrase = state.disable_passphrase.take_trimmed();
            let mnemonic = state.disable_seed_text.take();
            let bip39 = state.disable_bip39.take_trimmed();
            clear_disable_form(state);
            state.disable_token = None;
            return Task::perform(
                async move {
                    crate::bridge::enable_logic::TrustlineEnableLogic::process(
                        mode,
                        passphrase,
                        mnemonic,
                        bip39,
                        address,
                        asset,
                        crate::bridge::enable_logic::DISABLE_LIMIT,
                        "Disable trustline",
                        ws_tx,
                    )
                    .await
                },
                |_| Message::Sync,
            );
        }
        Message::CreateSubmitClicked => {
            if !crate::ui::components::wallet_setup::can_finish(
                crate::ui::components::wallet_setup::SetupFlow::Create,
                &state.create_mnemonic,
                state.xrp_word25,
                &state.xrp_bip39_input,
                state.xrp_import_mode,
                &state.xrp_encryption_input,
            ) {
                return Task::none();
            }
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                abandon_setup(state, "Create XRP wallet");
                return Task::none();
            };

            let mnemonic = state.create_mnemonic.take();
            let bip39 = state.xrp_bip39_input.take_trimmed();
            state.hold_entropy(SecureField::XrpEncryption);
            let encryption = state.xrp_encryption_input.take_trimmed();
            let mode = state.xrp_import_mode;

            return Task::perform(
                async move { XRPCreateLogic::process(mnemonic, bip39, encryption, mode, ws_tx).await },
                |()| Message::CreateCompleted,
            );
        }
        Message::CreateCompleted => {
            close_setup(state);
        }
        Message::SendMaxClicked => {
            state.send_amount = match state.xrp_token_tab {
                XrpTokenTab::Xrp => {
                    let max = xrp_available().map_or(0.0, |s| (s - xrp_fee()).max(0.0));
                    crate::utils::format_token_amount(max, 6)
                }
                XrpTokenTab::Token(code) => exact_amount(CHANNEL.token(code).0),
            };
            after_plain_edit(state, PlainField::XrpSendAmount);
        }
        Message::SendSubmitClicked => {
            let (_, wallet_address, key_deleted, key_mode) = CHANNEL.wallet_balance_rx.borrow().clone();
            let Some(address) = wallet_address else {
                state.send_error = Some("ERR: NO_WALLET_FOUND".to_string());
                return Task::none();
            };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                state.send_error = Some("ERR: WS_NOT_READY".to_string());
                return Task::none();
            };
            let asset = match state.xrp_token_tab {
                XrpTokenTab::Xrp => "XRP".to_string(),
                XrpTokenTab::Token(code) => code.to_string(),
            };
            let mode = match EnableInputMode::detect(key_deleted, key_mode) {
                EnableInputMode::Passphrase => "passphrase",
                EnableInputMode::Seed => "seed",
            }
            .to_string();
            let passphrase = state.send_passphrase.take_trimmed();
            let mnemonic   = state.send_seed_text.take();
            let bip39      = state.send_bip39.take_trimmed();
            let recipient  = state.send_recipient.clone();
            let amount     = state.send_amount.clone();
            let destination_tag = state.send_destination_tag.trim().parse::<u32>().ok();
            clear_send_form(state);
            return Task::perform(
                async move {
                    crate::bridge::xrp_send_logic::XRPSendLogic::process(
                        crate::bridge::xrp_send_logic::SendParams {
                            mode,
                            passphrase,
                            mnemonic,
                            bip39_pass: bip39,
                            recipient,
                            amount,
                            destination_tag,
                            wallet_address: address,
                            asset,
                            ws_tx,
                        },
                    ).await
                },
                |result| match result {
                    Ok(()) => Message::SendCompleted,
                    Err(e) => Message::SendFailed(e),
                },
            );
        }
        Message::SendCompleted => {
            state.xrp_view = XrpView::Menu;
        }
        Message::SendFailed(e) => {
            state.send_error = Some(e);
            state.send_step = 1;
        }
        Message::SendContinueClicked => {
            if state.send_step != 1 {
                return Task::none();
            }
            let addr = state.send_recipient.trim().to_string();
            if addr.is_empty() {
                state.send_error = Some("ERR: RECIPIENT_REQUIRED".to_string());
                return Task::none();
            }
            let Some(resolved) = dannesk_xrpl_codec::xaddress::resolve(&addr) else {
                state.send_error = Some("ERR: INVALID_XRP_ADDR_FORMAT".to_string());
                return Task::none();
            };
            let (_, own, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
            if own.as_deref() == Some(resolved.classic.as_str()) {
                state.send_error = Some("ERR: SELF_SEND".to_string());
                return Task::none();
            }
            if !resolved.from_xaddress {
                let tag = state.send_destination_tag.trim();
                if !tag.is_empty() && tag.parse::<u32>().is_err() {
                    state.send_error = Some("ERR: INVALID_DESTINATION_TAG".to_string());
                    return Task::none();
                }
            }
            let amount = match state.send_amount.trim().parse::<f64>() {
                Ok(v) if v > 0.0 => v,
                Ok(_) => {
                    state.send_error = Some("ERR: MIN_VALUE_REQUIRED".to_string());
                    return Task::none();
                }
                Err(_) => {
                    state.send_error = Some("ERR: INVALID_NUMBER_FORMAT".to_string());
                    return Task::none();
                }
            };
            if !send_fits(state, amount) {
                state.send_error = Some("ERR: INSUFFICIENT_FUNDS // AMOUNT + FEE".to_string());
                return Task::none();
            }
            state.send_recipient = addr;
            state.send_amount = crate::utils::format_token_amount(amount, 6);
            state.send_error = None;
            state.send_step = 2;
        }
        Message::TradeSideSet(side) => {
            state.trade_side_chosen = true;
            if state.trade_side != side {
                state.trade_side = side;
                std::mem::swap(&mut state.trade_pay_asset, &mut state.trade_receive_asset);
                std::mem::swap(&mut state.trade_amount, &mut state.trade_receive);
                state.trade_anchor = match state.trade_anchor {
                    TradeAnchor::Pay => TradeAnchor::Receive,
                    TradeAnchor::Receive => TradeAnchor::Pay,
                };
                state.trade_error = None;
                }
        }
        Message::TradeUnitToggled => {
            state.trade_side_chosen = true;
            state.trade_anchor = match state.trade_anchor {
                TradeAnchor::Pay => TradeAnchor::Receive,
                TradeAnchor::Receive => TradeAnchor::Pay,
            };
            state.trade_error = None;
            refresh_trade_twin(state);
        }
        Message::TradeMaxClicked => {
            let pay = state.trade_pay_asset.as_str();
            state.trade_amount = if pay == "XRP" {
                let owed = xrp_fee() + if trade_can_rest(state) { xrp_reserve().map_or(0.0, |r| r.per_object) } else { 0.0 };
                crate::utils::format_token_amount(xrp_available().map_or(0.0, |a| (a - owed).max(0.0)), 6)
            } else {
                exact_amount(CHANNEL.token(pay).0)
            };
            state.trade_anchor = TradeAnchor::Pay;
            state.trade_error = None;
            refresh_trade_twin(state);
        }
        Message::TradeOrderOptionSet(opt) => {
            state.trade_side_chosen = true;
            state.trade_flags = match opt {
                1 => vec!["tfImmediateOrCancel".to_string()],
                2 => vec!["tfFillOrKill".to_string()],
                _ => vec![],
            };
        }
        Message::TradePairSelected(base, quote) => {
            let side = trade_default_side(&base, &quote);
            state.trade_side_chosen = false;
            orient_trade_pair(state, side, &base, &quote);
            state.trade_amount = String::new();
            state.trade_receive = String::new();
            state.trade_limit_price = String::new();
            state.trade_contract = Some(TradeContract::Limit);
            state.trade_pair_query.clear();
            state.trade_pair_search_open = false;
            state.trade_error = None;
            persist_trade_pair(state);
            sync_trade_books(state);
        }
        Message::TradeMarketSelected => {
            state.trade_side_chosen = true;
            state.trade_contract = Some(TradeContract::Market);
            state.trade_error = None;
        }
        Message::TradeLimitSelected => {
            state.trade_side_chosen = true;
            state.trade_contract = Some(TradeContract::Limit);
            state.trade_error = None;
        }
        Message::TradeBookPriceClicked(val) => {
            if trade_is_limit(state) {
                state.trade_limit_price = val;
                state.trade_error = None;
            }
        }
        Message::TradeContinueClicked => {
            if state.trade_step == 1 {
                match state.trade_contract {
                    None => {
                        state.trade_error = Some("ERR: NO_CONTRACT // pick Market or Limit".to_string());
                        return Task::none();
                    }
                    Some(TradeContract::Market) if trade_book_quote(state).is_none() => {
                        state.trade_error = Some("ERR: NO_QUOTE // waiting for the ledger".to_string());
                        return Task::none();
                    }
                    _ => {}
                }
                let limit = trade_effective_limit(state);
                if limit <= 0.0 {
                    state.trade_error = Some(trade_no_price(state).to_string());
                    return Task::none();
                }
                let typed = match state.trade_anchor {
                    TradeAnchor::Pay => state.trade_amount.trim(),
                    TradeAnchor::Receive => state.trade_receive.trim(),
                };
                if typed.parse::<f64>().is_err() {
                    state.trade_error = Some("ERR: INVALID_NUMBER_FORMAT".to_string());
                    return Task::none();
                }
                let terms = trade_terms(state, limit);
                if terms.pay <= 0.0 || terms.receive <= 0.0 {
                    state.trade_error = Some("ERR: MIN_VALUE_REQUIRED".to_string());
                    return Task::none();
                }
                if !trade_fits(state, terms.pay) {
                    state.trade_error = Some(
                        format!("ERR: INSUFFICIENT_FUNDS // MAX: {:.6}", trade_available(state).unwrap_or(0.0))
                    );
                    return Task::none();
                }
                state.trade_error = None;
                state.trade_step = 2;
            }
        }
        Message::TradeSubmitClicked => {
            let (_, wallet_address, key_deleted, key_mode) = CHANNEL.wallet_balance_rx.borrow().clone();
            let Some(address) = wallet_address else {
                state.trade_error = Some("ERR: NO_WALLET_FOUND".to_string());
                return Task::none();
            };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                state.trade_error = Some("ERR: WS_NOT_READY".to_string());
                return Task::none();
            };
            let mode       = match EnableInputMode::detect(key_deleted, key_mode) {
                EnableInputMode::Passphrase => "passphrase",
                EnableInputMode::Seed => "seed",
            }
            .to_string();
            let limit = trade_effective_limit(state);
            if limit <= 0.0 {
                state.trade_error = Some(trade_no_price(state).to_string());
                return Task::none();
            }
            let terms = trade_terms(state, limit);
            if !trade_fits(state, terms.pay) {
                state.trade_error = Some(
                    format!("ERR: INSUFFICIENT_FUNDS // MAX: {:.6}", trade_available(state).unwrap_or(0.0))
                );
                return Task::none();
            }

            let walk = trade_book_quote(state);
            let _ = CHANNEL.pending_trade_tx.send(Some(crate::channel::PendingTrade {
                pair: format!("{}/{}", state.trade_pay_asset, state.trade_receive_asset),
                anchor: match state.trade_anchor {
                    TradeAnchor::Pay => "pay",
                    TradeAnchor::Receive => "receive",
                }
                .to_string(),
                bound: limit,
                expected_vwap: if trade_is_market(state) { walk.as_ref().map(|q| q.vwap) } else { None },
                ledger_index: walk
                    .as_ref()
                    .map(|q| q.ledger)
                    .filter(|i| *i > 0)
                    .or_else(|| CHANNEL.xrp_node_rx.borrow().ledger_index)
                    .unwrap_or(0),
                last_ledger: None,
                sequence: None,
                hash: None,
                tx_id: None,
                pay: terms.pay,
                receive: terms.receive,
            }));

            let passphrase = state.trade_passphrase.take_trimmed();
            let mnemonic   = state.trade_seed_text.take();
            let bip39      = state.trade_bip39.take_trimmed();
            let base_asset = state.trade_pay_asset.clone();
            let quote_asset = state.trade_receive_asset.clone();
            let pay_amount = if trade_pays_whole_balance(state) { exact_amount(terms.pay) } else { amount_string(terms.pay) };
            let receive_amount = amount_string(terms.receive);
            let mut flags = trade_tif(state);
            if state.trade_anchor == TradeAnchor::Pay {
                flags.push("tfSell".to_string());
            }
            return Task::perform(
                async move {
                    TradeLogic::process(
                        mode, passphrase, mnemonic, bip39,
                        base_asset, quote_asset, pay_amount, receive_amount, flags,
                        address, ws_tx, None,
                    ).await
                },
                |_| Message::TradeCompleted,
            );
        }
        Message::TradeCompleted => {
            clear_trade_form_after_order(state);
            state.xrp_view = XrpView::Menu;
            trade_grid_init(state);
        }
        Message::CancelOrderClicked(seq) => {
            clear_cancel_form(state);
            state.cancel_offer_sequence = Some(seq);
        }
        Message::CancelDismissed => {
            clear_cancel_form(state);
        }
        Message::CancelSubmitClicked => {
            let Some(offer_sequence) = state.cancel_offer_sequence else {
                state.cancel_error = Some("ERR: NO_SEQUENCE".to_string());
                return Task::none();
            };
            let (_, wallet_address, key_deleted, key_mode) = CHANNEL.wallet_balance_rx.borrow().clone();
            let Some(address) = wallet_address else {
                state.cancel_error = Some("ERR: NO_WALLET_FOUND".to_string());
                return Task::none();
            };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                state.cancel_error = Some("ERR: WS_NOT_READY".to_string());
                return Task::none();
            };
            let mode       = match EnableInputMode::detect(key_deleted, key_mode) {
                EnableInputMode::Passphrase => "passphrase",
                EnableInputMode::Seed => "seed",
            }
            .to_string();
            let passphrase = state.cancel_passphrase.take_trimmed();
            let mnemonic   = state.cancel_seed_text.take();
            let bip39      = state.cancel_bip39.take_trimmed();
            clear_cancel_form(state);
            return Task::perform(
                async move {
                    CancelLogic::process(
                        mode, passphrase, mnemonic, bip39,
                        offer_sequence, address, ws_tx,
                    ).await
                },
                |_| Message::CancelCompleted,
            );
        }
        Message::CancelCompleted => {
        }
        Message::ReimportNext => {
            match state.reimport_step {
                1 if state.reimport_seed_text.as_str().split_whitespace().count() == 24
                    && crate::ui::components::wallet_setup::mnemonic_checksum_ok(
                        state.reimport_seed_text.as_str(),
                    ) =>
                {
                    state.reimport_error = None;
                    state.reimport_step = 2;
                }
                2 if crate::ui::components::wallet_setup::can_continue_word25(
                    state.reimport_word25,
                    &state.reimport_bip39,
                ) =>
                {
                    state.reimport_error = None;
                    state.reimport_step = 3;
                }
                _ => {}
            }
        }
        Message::ReimportBack => {
            state.reimport_error = None;
            state.reimport_step = state.reimport_step.saturating_sub(1).max(1);
        }
        Message::ReimportWord25Chosen(yes) => {
            state.reimport_word25 = yes;
            if !yes {
                state.reimport_bip39.clear();
                state.reimport_bip39_reveal = false;
            }
        }
        Message::ReimportConfirm => {
            if state.reimport_spinning { return Task::none(); }
            if state.reimport_seed_text.as_str().split_whitespace().count() != 24
                || !crate::ui::components::wallet_setup::mnemonic_checksum_ok(
                    state.reimport_seed_text.as_str(),
                )
                || state.reimport_passphrase.trimmed_char_len()
                    < crate::ui::components::wallet_setup::KEY_MIN
            {
                return Task::none();
            }
            state.reimport_spinning = true;
            state.reimport_error = None;
            let mnemonic = state.reimport_seed_text.take();
            let bip39 = state.reimport_bip39.take_trimmed();
            state.hold_entropy(SecureField::ReimportPassphrase);
            let passphrase = state.reimport_passphrase.take_trimmed();
            return Task::perform(
                async move {
                    crate::bridge::xrp_wallet_operations::WalletOperations::reimport_key(
                        mnemonic, bip39, passphrase,
                    ).await
                },
                |result| match result {
                    Ok(()) => Message::ReimportSuccess,
                    Err(e) => Message::ReimportFailed(e),
                },
            );
        }
        Message::ReimportSuccess => {
            clear_reimport_form(state);
            state.reimport_step = 1;
            state.key_purge_requested = false;
            state.key_mgmt_restoring = false;
        }
        Message::ReimportFailed(e) => {
            state.reimport_spinning = false;
            state.reimport_error = Some(e);
        }
        Message::KeyMgmtRestoreToggled => {
            if state.reimport_spinning { return Task::none(); }
            clear_reimport_form(state);
            state.key_mgmt_restoring = !state.key_mgmt_restoring;
            state.reimport_step = 1;
        }
        _ => {}
    }
    Task::none()
}

pub fn trade_index_rate(state: &AppState) -> f64 {
    let (mb, mq) = trade_market_pair(state);
    let b = crate::utils::tokens::rate_key(mb).to_string();
    let q = crate::utils::tokens::rate_key(mq).to_string();
    if b.is_empty() || q.is_empty() {
        return 0.0;
    }
    if b == q { 1.0 } else { crate::utils::price::cross(&b, &q) }
}

pub fn trade_book_quote(state: &AppState) -> Option<crate::utils::orderbook::Quote> {
    use crate::utils::orderbook::{oriented, touch, walk, Quote};
    let pay = state.trade_pay_asset.as_str();
    let recv = state.trade_receive_asset.as_str();
    let typed = trade_typed_amount(state);
    if typed <= 0.0 {
        let (book, _) = oriented(pay, recv)?;
        let t = touch(&book)?;
        return Some(Quote {
            vwap: t,
            floor: t,
            filled: 0.0,
            receive: 0.0,
            depth_short: false,
            ledger: book.ledger,
            from_pool: 0.0,
            clob_levels: 0,
        });
    }
    let q = walk(pay, recv, typed, matches!(state.trade_anchor, TradeAnchor::Pay))?;
    trade_frame_ok(q.ledger).then_some(q)
}

fn trade_frame_ok(frame: u64) -> bool {
    let live = CHANNEL.xrp_node_rx.borrow().ledger_index.unwrap_or(0);
    if frame == 0 || live == 0 {
        return false;
    }
    frame + FRAME_STALE_LEDGERS >= live
}

const FRAME_STALE_LEDGERS: u64 = 2;

pub fn trade_market_offered(state: &AppState) -> bool {
    let (pay, recv) = (state.trade_pay_asset.as_str(), state.trade_receive_asset.as_str());
    if pay.is_empty() || recv.is_empty() || (pay != "XRP" && recv != "XRP") {
        return false;
    }
    !crate::utils::orderbook::oriented(pay, recv).is_some_and(|(b, _)| trade_issuer_restricted(&b))
}

pub fn trade_quotable(state: &AppState) -> bool {
    if !trade_market_offered(state) {
        return false;
    }
    let (pay, recv) = (state.trade_pay_asset.as_str(), state.trade_receive_asset.as_str());
    crate::utils::orderbook::oriented(pay, recv)
        .is_some_and(|(b, _)| crate::utils::orderbook::touch(&b).is_some() && trade_frame_ok(b.ledger))
}

fn trade_touch(state: &AppState) -> f64 {
    crate::utils::orderbook::oriented(&state.trade_pay_asset, &state.trade_receive_asset)
        .and_then(|(b, _)| crate::utils::orderbook::touch(&b))
        .unwrap_or(0.0)
}

fn trade_tick_size(state: &AppState) -> Option<u32> {
    crate::utils::orderbook::oriented(&state.trade_pay_asset, &state.trade_receive_asset)
        .and_then(|(b, _)| b.tick_size)
}

pub fn trade_issuer_restricted(book: &crate::channel::OrderBook) -> bool {
    book.require_auth == Some(true) || book.global_freeze == Some(true)
}

fn trade_typed_amount(state: &AppState) -> f64 {
    let s = match state.trade_anchor {
        TradeAnchor::Pay => &state.trade_amount,
        TradeAnchor::Receive => &state.trade_receive,
    };
    s.trim().parse::<f64>().unwrap_or(0.0)
}

fn wanted_books() -> Vec<String> {
    if !xrp_wallet_exists() {
        return Vec::new();
    }
    crate::ws::rates::book_pairs()
}

fn xrp_wallet_exists() -> bool {
    CHANNEL.wallet_balance_rx.borrow().1.is_some()
}

pub fn trade_grid_init(state: &mut AppState) {
    if !xrp_wallet_exists() {
        sync_trade_books(state);
        return;
    }
    if state.trade_step == 0 {
        state.trade_step = 1;
    }
    sync_trade_books(state);
}

pub fn sync_trade_books(state: &mut AppState) {
    let wanted = wanted_books();
    for old in &state.trade_books {
        if !wanted.contains(old) {
            rates_send(RatesCommand::UnsubscribeBook(old.clone()));
        }
    }
    for new in &wanted {
        if !state.trade_books.contains(new) {
            rates_send(RatesCommand::SubscribeBook(new.clone()));
        }
    }
    state.trade_books = wanted;
}

#[derive(Debug, Clone, Copy)]
pub struct TradeTerms {
    pub pay: f64,
    pub receive: f64,
}

pub const SIGN_PLACES: usize = 6;

pub fn trade_terms(state: &AppState, limit: f64) -> TradeTerms {
    use crate::utils::orderbook::{ceil_to, floor_to};
    match state.trade_anchor {
        TradeAnchor::Pay => {
            let typed = state.trade_amount.trim().parse::<f64>().unwrap_or(0.0);
            let pay = if trade_pays_whole_balance(state) { typed } else { floor_to(typed, SIGN_PLACES) };
            let receive = if limit > 0.0 { floor_to(pay * limit, SIGN_PLACES) } else { 0.0 };
            TradeTerms { pay, receive }
        }
        TradeAnchor::Receive => {
            let receive = floor_to(state.trade_receive.trim().parse::<f64>().unwrap_or(0.0), SIGN_PLACES);
            let pay = if limit > 0.0 { ceil_to(receive / limit, SIGN_PLACES) } else { 0.0 };
            TradeTerms { pay, receive }
        }
    }
}

pub fn trade_pays_whole_balance(state: &AppState) -> bool {
    let pay = state.trade_pay_asset.as_str();
    if pay == "XRP" || state.trade_anchor != TradeAnchor::Pay {
        return false;
    }
    let balance = CHANNEL.token(pay).0;
    balance > 0.0 && state.trade_amount.trim().parse::<f64>().ok() == Some(balance)
}

pub fn trade_decimals(code: &str) -> usize {
    if code == "XRP" { 6 } else { 2 }
}

pub fn trade_derived_decimals(code: &str) -> usize {
    if code == "XRP" { 6 } else { 4 }
}

pub fn trade_expected_price(state: &AppState, limit: f64) -> Option<f64> {
    let x = trade_expected(state, limit)?;
    let (pay, recv) = if trade_fill_now(state) {
        let q = trade_book_quote(state)?;
        (q.filled, q.receive)
    } else {
        let terms = trade_terms(state, limit);
        match state.trade_anchor {
            TradeAnchor::Pay => (terms.pay, x),
            TradeAnchor::Receive => (x, terms.receive),
        }
    };
    if pay <= 0.0 || recv <= 0.0 {
        return None;
    }
    Some(trade_market_price(state, recv / pay))
}

pub fn trade_expected(state: &AppState, limit: f64) -> Option<f64> {
    if !trade_is_market(state) || limit <= 0.0 {
        return None;
    }
    let q = trade_book_quote(state)?;
    let terms = trade_terms(state, limit);
    Some(expected_amount(
        state.trade_anchor,
        trade_fill_now(state),
        (q.filled, q.receive),
        (terms.pay, terms.receive),
        limit,
    ))
}

pub(crate) fn expected_amount(
    anchor: TradeAnchor,
    fill_now: bool,
    walk: (f64, f64),
    want: (f64, f64),
    limit: f64,
) -> f64 {
    let (walk_filled, walk_receive) = walk;
    let (want_pay, want_receive) = want;
    if fill_now {
        return match anchor {
            TradeAnchor::Pay => walk_receive,
            TradeAnchor::Receive => walk_filled,
        };
    }
    match anchor {
        TradeAnchor::Pay => walk_receive + (want_pay - walk_filled).max(0.0) * limit,
        TradeAnchor::Receive => walk_filled + (want_receive - walk_receive).max(0.0) / limit,
    }
}

fn trade_fill_now(state: &AppState) -> bool {
    !trade_can_rest(state)
}

pub fn refresh_trade_twin(state: &mut AppState) {
    use crate::utils::orderbook::{ceil_to, floor_to};
    let limit = trade_effective_limit(state);
    let terms = trade_terms(state, limit);
    let expected = trade_expected(state, limit);
    let (value, dec, target) = match state.trade_anchor {
        TradeAnchor::Pay => {
            let dec = trade_derived_decimals(&state.trade_receive_asset);
            (floor_to(expected.unwrap_or(terms.receive), dec), dec, &mut state.trade_receive)
        }
        TradeAnchor::Receive => {
            let dec = trade_derived_decimals(&state.trade_pay_asset);
            (ceil_to(expected.unwrap_or(terms.pay), dec), dec, &mut state.trade_amount)
        }
    };
    let text = if value > 0.0 { crate::utils::format_token_amount(value, dec) } else { String::new() };
    if *target != text {
        *target = text;
    }
}

fn amount_string(x: f64) -> String {
    let raw = format!("{:.*}", SIGN_PLACES, x);
    raw.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn trade_tif(state: &AppState) -> Vec<String> {
    if trade_is_market(state) {
        vec!["tfImmediateOrCancel".to_string()]
    } else {
        state.trade_flags.clone()
    }
}

pub fn trade_is_market(state: &AppState) -> bool {
    state.trade_contract == Some(TradeContract::Market) && trade_quotable(state)
}

pub fn trade_is_limit(state: &AppState) -> bool {
    state.trade_contract == Some(TradeContract::Limit)
}

pub fn trade_can_rest(state: &AppState) -> bool {
    !trade_tif(state).iter().any(|f| f == "tfImmediateOrCancel" || f == "tfFillOrKill")
}

const CUSHION_ALPHA: f64 = 0.5;

const CUSHION_MAX: f64 = 0.01;

fn trade_delta_min(tick_size: Option<u32>) -> f64 {
    let by_tick = tick_size
        .filter(|n| *n >= 1)
        .map(|n| 10f64.powi(-((n as i32) - 1)))
        .unwrap_or(0.0);
    by_tick.max(1e-7)
}

fn trade_cushioned(floor: f64, touch: f64, tick_size: Option<u32>) -> f64 {
    if floor <= 0.0 {
        return 0.0;
    }
    let slope = if touch > 0.0 { ((touch - floor) / touch).max(0.0) } else { 0.0 };
    let d = (CUSHION_ALPHA * slope).clamp(trade_delta_min(tick_size), CUSHION_MAX);
    crate::utils::orderbook::floor_sig(floor * (1.0 - d), 9)
}

pub const BOUND_GUARD_PCT: f64 = 5.0;

pub fn trade_bound_vs_mid(state: &AppState, bound: f64) -> Option<f64> {
    let (base, quote) = trade_market_pair(state);
    let m = crate::utils::liquidity::market(base, quote)?;
    if m.mid <= 0.0 || bound <= 0.0 {
        return None;
    }
    Some((trade_market_price(state, bound) - m.mid) / m.mid * 100.0)
}

pub fn trade_bound_sane(state: &AppState, bound: f64) -> bool {
    let (base, quote) = trade_market_pair(state);
    let Some(m) = crate::utils::liquidity::market(base, quote) else { return false };
    match trade_bound_vs_mid(state, bound) {
        Some(d) => d.abs() <= BOUND_GUARD_PCT || m.stable,
        None => false,
    }
}

fn trade_market_bound(state: &AppState, q: &crate::utils::orderbook::Quote) -> f64 {
    trade_cushioned(q.floor, trade_touch(state), trade_tick_size(state))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarketVerdict {
    LimitOnly,
    Measuring,
    NoFrame,
    Idle,
    NoDepth,
    Walked { coverage_pct: f64, bound_pct: f64, held: bool },
}

pub fn trade_market_verdict(state: &AppState) -> MarketVerdict {
    use crate::utils::orderbook::oriented;
    if !trade_market_offered(state) {
        return MarketVerdict::LimitOnly;
    }
    let (base, quote) = trade_market_pair(state);
    if crate::utils::liquidity::market(base, quote).is_none() {
        return MarketVerdict::Measuring;
    }
    let typed = trade_typed_amount(state);
    if typed <= 0.0 {
        return MarketVerdict::Idle;
    }
    let (pay, recv) = (state.trade_pay_asset.as_str(), state.trade_receive_asset.as_str());
    let Some((book, _)) = oriented(pay, recv) else { return MarketVerdict::NoFrame };
    if !trade_frame_ok(book.ledger) {
        return MarketVerdict::NoFrame;
    }
    let Some(q) = trade_book_quote(state) else { return MarketVerdict::NoDepth };
    let got = match state.trade_anchor {
        TradeAnchor::Pay => q.filled,
        TradeAnchor::Receive => q.receive,
    };
    let bound = trade_market_bound(state, &q);
    let bound_pct = trade_bound_vs_mid(state, bound).unwrap_or(0.0);
    MarketVerdict::Walked {
        coverage_pct: coverage_pct(got, typed),
        bound_pct,
        held: !trade_bound_sane(state, bound),
    }
}

pub(crate) fn coverage_pct(got: f64, want: f64) -> f64 {
    if want <= 0.0 {
        return 0.0;
    }
    (got / want * 100.0).clamp(0.0, 100.0)
}

fn trade_no_price(state: &AppState) -> &'static str {
    if state.trade_contract == Some(TradeContract::Market) && trade_book_quote(state).is_some() {
        "ERR: OFF_MARKET // the book has a hole this ledger, wait one"
    } else {
        "ERR: NO_PRICE"
    }
}

pub fn trade_effective_limit(state: &AppState) -> f64 {
    match state.trade_contract {
        None => 0.0,
        Some(TradeContract::Market) => match trade_book_quote(state).filter(|_| trade_quotable(state)) {
            Some(q) => {
                let b = trade_market_bound(state, &q);
                if trade_bound_sane(state, b) { b } else { 0.0 }
            }
            None => 0.0,
        },
        Some(TradeContract::Limit) => {
            trade_internal_price(state, state.trade_limit_price.trim().parse::<f64>().unwrap_or(0.0))
        }
    }
}

const PAIR_KEY: &str = "xrp_pair";

fn persist_trade_pair(state: &AppState) {
    let (base, quote) = trade_market_pair(state);
    if base.is_empty() || quote.is_empty() {
        return;
    }
    let snapshot = serde_json::json!({ "base": base, "quote": quote });
    let _ = crate::bridge::json_storage::update_json::<serde_json::Value>("settings.json", |json| {
        if let Some(obj) = json.as_object_mut() {
            obj.insert(PAIR_KEY.to_string(), snapshot);
        }
    });
}

pub fn clear_trade_pair(state: &mut AppState) {
    state.trade_pay_asset.clear();
    state.trade_receive_asset.clear();
    state.trade_amount.clear();
    state.trade_receive.clear();
    state.trade_limit_price.clear();
    state.trade_error = None;
    crate::controller::panes::dismiss_pair_search(state);
    let _ = crate::bridge::json_storage::update_json::<serde_json::Value>("settings.json", |json| {
        if let Some(obj) = json.as_object_mut() {
            obj.remove(PAIR_KEY);
        }
    });
}

pub fn restore_trade_pair(state: &mut AppState) {
    let Ok(json) = crate::bridge::json_storage::read_json::<serde_json::Value>("settings.json") else { return };
    let Some(pair) = json.get(PAIR_KEY) else { return };
    let (Some(base), Some(quote)) = (pair.get("base").and_then(|v| v.as_str()), pair.get("quote").and_then(|v| v.as_str())) else {
        return;
    };
    let known = |code: &str| code == "XRP" || crate::utils::tokens::by_code(code).is_some();
    if base == quote || !known(base) || !known(quote) {
        return;
    }
    let side = trade_default_side(base, quote);
    state.trade_side_chosen = false;
    orient_trade_pair(state, side, base, quote);
}

pub fn trade_default_side(base: &str, quote: &str) -> TradeSide {
    if trade_can_pay(quote) {
        TradeSide::Buy
    } else if trade_can_pay(base) {
        TradeSide::Sell
    } else {
        TradeSide::Buy
    }
}

fn trade_can_pay(asset: &str) -> bool {
    if asset == "XRP" {
        xrp_available().is_some_and(|a| a > xrp_fee())
    } else {
        CHANNEL.token(asset).0 > 0.0
    }
}

fn orient_trade_pair(state: &mut AppState, side: TradeSide, base: &str, quote: &str) {
    state.trade_side = side;
    let (pay, recv) = match side {
        TradeSide::Sell => (base, quote),
        TradeSide::Buy => (quote, base),
    };
    state.trade_pay_asset = pay.to_string();
    state.trade_receive_asset = recv.to_string();
    state.trade_anchor = match side {
        TradeSide::Sell => TradeAnchor::Pay,
        TradeSide::Buy => TradeAnchor::Receive,
    };
}

pub fn follow_wallet_side(state: &mut AppState) {
    if state.trade_side_chosen
        || !state.trade_amount.trim().is_empty()
        || !state.trade_receive.trim().is_empty()
        || !state.trade_limit_price.trim().is_empty()
    {
        return;
    }
    let (base, quote) = trade_market_pair(state);
    if base.is_empty() || quote.is_empty() {
        return;
    }
    let (base, quote) = (base.to_string(), quote.to_string());
    let side = trade_default_side(&base, &quote);
    if side != state.trade_side {
        orient_trade_pair(state, side, &base, &quote);
    }
}

pub fn trade_market_pair(state: &AppState) -> (&str, &str) {
    match state.trade_side {
        TradeSide::Sell => (state.trade_pay_asset.as_str(), state.trade_receive_asset.as_str()),
        TradeSide::Buy => (state.trade_receive_asset.as_str(), state.trade_pay_asset.as_str()),
    }
}

pub fn trade_market_price(state: &AppState, internal: f64) -> f64 {
    match state.trade_side {
        TradeSide::Sell => internal,
        TradeSide::Buy => if internal > 0.0 { 1.0 / internal } else { 0.0 },
    }
}

pub fn trade_internal_price(state: &AppState, market: f64) -> f64 {
    trade_market_price(state, market)
}

pub fn trade_available(state: &AppState) -> Option<f64> {
    if state.trade_pay_asset == "XRP" {
        xrp_available()
    } else {
        Some(CHANNEL.token(state.trade_pay_asset.as_str()).0)
    }
}

pub fn trade_fits(state: &AppState, amount: f64) -> bool {
    let Some(r) = xrp_reserve() else { return false };
    let xrp_owed = xrp_fee() + if trade_can_rest(state) { r.per_object } else { 0.0 };
    if state.trade_pay_asset == "XRP" {
        amount + xrp_owed <= r.available
    } else {
        amount <= CHANNEL.token(state.trade_pay_asset.as_str()).0 && xrp_owed <= r.available
    }
}

pub fn trade_expiry_check() {
    let Some(pending) = CHANNEL.pending_trade_rx.borrow().clone() else {
        return;
    };
    let (Some(last_ledger), Some(signed_seq), Some(now)) = (
        pending.last_ledger,
        pending.sequence,
        CHANNEL.xrp_node_rx.borrow().ledger_index,
    ) else {
        return;
    };
    if now <= u64::from(last_ledger) {
        return;
    }

    if CHANNEL.xrp_account_rx.borrow().sequence != Some(signed_seq) {
        return;
    }

    let _ = CHANNEL.pending_trade_tx.send(None);

    let mut log = CHANNEL.activity_rx.borrow().clone();
    let is_the_orders_log = log
        .as_ref()
        .is_some_and(|l| l.title == crate::bridge::trade_logic::LOG_TITLE);
    if !is_the_orders_log {
        return;
    }
    if let Some(l) = log.as_mut() {
        l.restate_failure(
            "The order expired — it never entered a ledger, and nothing was traded.".to_string(),
        );
        let _ = CHANNEL.activity_tx.send(log);
    }
}

pub(crate) fn clear_trade_form(state: &mut AppState) {
    sync_trade_books(state);
    state.trade_pay_asset = String::new();
    state.trade_receive_asset = String::new();
    clear_trade_inputs(state);
}

fn clear_trade_form_after_order(state: &mut AppState) {
    let (base, quote) = trade_market_pair(state);
    let (base, quote) = (base.to_string(), quote.to_string());
    clear_trade_form(state);
    orient_trade_pair(state, trade_default_side(&base, &quote), &base, &quote);
}

pub(crate) fn clear_trade_inputs(state: &mut AppState) {
    state.trade_step = 0;
    state.trade_amount = String::new();
    state.trade_limit_price = String::new();
    state.trade_flags = Vec::new();
    state.trade_contract = Some(TradeContract::Limit);
    let (base, quote) = trade_market_pair(state);
    let (base, quote) = (base.to_string(), quote.to_string());
    let side = trade_default_side(&base, &quote);
    state.trade_side_chosen = false;
    orient_trade_pair(state, side, &base, &quote);
    state.trade_receive = String::new();
    state.trade_pair_query.clear();
    state.trade_error = None;
    state.trade_passphrase.clear();
    state.trade_passphrase_reveal = false;
    state.trade_bip39.clear();
    state.trade_bip39_reveal = false;
    state.trade_seed_text.clear();
    state.trade_seed_reveal = false;
}

pub fn xrp_reserve() -> Option<crate::utils::reserves::Reserve> {
    let total = CHANNEL.wallet_balance_rx.borrow().0;
    let account = *CHANNEL.xrp_account_rx.borrow();
    let node = CHANNEL.xrp_node_rx.borrow();
    crate::utils::reserves::chain_reserve(total, CHANNEL.xrp_exists(), account, &node)
}

pub fn xrp_available() -> Option<f64> {
    xrp_reserve().map(|r| r.available)
}

pub fn xrp_fee() -> f64 {
    CHANNEL.xrp_node_rx.borrow().open_ledger_fee.unwrap_or(0) as f64 / 1e6
}

pub fn send_fits(state: &AppState, amount: f64) -> bool {
    let Some(available) = xrp_available() else { return false };
    match state.xrp_token_tab {
        XrpTokenTab::Xrp => amount + xrp_fee() <= available,
        XrpTokenTab::Token(code) => amount <= CHANNEL.token(code).0 && xrp_fee() <= available,
    }
}

pub fn send_rate(state: &AppState) -> f64 {
    let key = state
        .xrp_token_tab
        .code()
        .and_then(crate::utils::tokens::by_code)
        .map_or("XRP", |t| t.rate_key);
    crate::utils::price::cross(key, state.base_currency.code())
}

fn derive_send_fiat(state: &mut AppState, rate: f64) {
    state.send_fiat_amount = match state.send_amount.trim().parse::<f64>() {
        Ok(amount) => crate::utils::fiat_amount(amount * rate),
        Err(_) => String::new(),
    };
}

fn derive_send_amount(state: &mut AppState, rate: f64) {
    state.send_amount = match state.send_fiat_amount.trim().parse::<f64>() {
        Ok(fiat) if rate > 0.0 => crate::utils::format_token_amount(fiat / rate, 6),
        _ => String::new(),
    };
}

pub fn resync_send_twin(state: &mut AppState) {
    let rate = send_rate(state);
    if rate <= 0.0 {
        return;
    }
    match state.send_anchor {
        SendAnchor::Amount => derive_send_fiat(state, rate),
        SendAnchor::Fiat => derive_send_amount(state, rate),
    }
}

pub fn after_plain_edit(state: &mut AppState, field: PlainField) {
    state.send_error = None;
    let rate = send_rate(state);
    match field {
        PlainField::XrpSendAmount => {
            state.send_anchor = SendAnchor::Amount;
            derive_send_fiat(state, rate);
        }
        PlainField::XrpSendFiat => {
            state.send_anchor = SendAnchor::Fiat;
            derive_send_amount(state, rate);
        }
        PlainField::TradeAmount => {
            state.trade_anchor = TradeAnchor::Pay;
            state.trade_error = None;
        }
        PlainField::TradeReceive => {
            state.trade_anchor = TradeAnchor::Receive;
            state.trade_error = None;
        }
        PlainField::TradeLimit => {
            state.trade_error = None;
        }
        PlainField::TradePairQuery => {}
        _ => {}
    }
}

fn tag_slots(state: &mut AppState, pane: TagPane) -> (&mut String, &mut bool, &mut String) {
    match pane {
        TagPane::Send => (&mut state.send_destination_tag, &mut state.send_tag_editing, &mut state.send_tag_draft),
        TagPane::Receive => (&mut state.receive_tag, &mut state.receive_tag_editing, &mut state.receive_tag_draft),
    }
}

// With a tag set, the X-address: QR, address row and copy stay one string.
pub(crate) fn receive_address(state: &AppState, classic: &str) -> String {
    tag_of(&state.receive_tag)
        .and_then(|tag| dannesk_xrpl_codec::xaddress::encode(classic, Some(tag)))
        .unwrap_or_else(|| classic.to_string())
}

pub(crate) fn tag_of(buf: &str) -> Option<u32> {
    buf.trim().parse::<u32>().ok()
}

pub(crate) fn tag_wrong(buf: &str) -> bool {
    !buf.trim().is_empty() && tag_of(buf).is_none()
}

pub(crate) fn clear_receive_form(state: &mut AppState) {
    state.receive_tag_draft = String::new();
    state.receive_tag_editing = false;
    state.xrp_copy_feedback = false;
    state.xrp_tag_copied = false;
}

const RECEIVE_TAG_KEY: &str = "receive_tag";

fn persist_receive_tag(state: &AppState) {
    let tag = tag_of(&state.receive_tag);
    let _ = crate::bridge::json_storage::update_json::<serde_json::Value>("xrp.json", |json| {
        if let Some(obj) = json.as_object_mut() {
            match tag {
                Some(t) => {
                    obj.insert(RECEIVE_TAG_KEY.to_string(), serde_json::json!(t));
                }
                None => {
                    obj.remove(RECEIVE_TAG_KEY);
                }
            }
        }
    });
}

pub fn restore_receive_tag(state: &mut AppState) {
    let Ok(json) = crate::bridge::json_storage::read_json::<serde_json::Value>("xrp.json") else { return };
    if let Some(t) = json.get(RECEIVE_TAG_KEY).and_then(|v| v.as_u64()).and_then(|t| u32::try_from(t).ok()) {
        state.receive_tag = t.to_string();
    }
}

pub(crate) fn clear_send_form(state: &mut AppState) {
    state.send_step = 0;
    state.send_recipient = String::new();
    state.send_destination_tag = String::new();
    state.send_tag_editing = false;
    state.send_tag_draft = String::new();
    state.send_asset_picker_open = false;
    state.send_amount = String::new();
    state.send_fiat_amount = String::new();
    state.send_anchor = SendAnchor::Amount;
    state.send_passphrase.clear();
    state.send_passphrase_reveal = false;
    state.send_bip39.clear();
    state.send_bip39_reveal = false;
    state.send_seed_text.clear();
    state.send_seed_reveal = false;
    state.send_error = None;
}

pub(crate) fn clear_cancel_form(state: &mut AppState) {
    state.cancel_offer_sequence = None;
    state.cancel_passphrase.clear();
    state.cancel_passphrase_reveal = false;
    state.cancel_bip39.clear();
    state.cancel_bip39_reveal = false;
    state.cancel_seed_text.clear();
    state.cancel_seed_reveal = false;
    state.cancel_error = None;
}

fn close_setup(state: &mut AppState) {
    reset_setup_form(state);
    state.xrp_view = XrpView::Menu;
}

fn reset_setup_form(state: &mut AppState) {
    state.release_entropy();
    state.xrp_seed_text.clear();
    state.xrp_seed_reveal = false;
    state.xrp_encryption_input.clear();
    state.xrp_encryption_reveal = false;
    state.xrp_bip39_input.clear();
    state.xrp_bip39_reveal = false;
    state.xrp_word25 = false;
    state.create_mnemonic.clear();
    state.create_seed_reveal = false;
    state.xrp_import_mode = ImportMode::default();
    state.xrp_copy_feedback = false;
}

fn abandon_setup(state: &mut AppState, title: &'static str) {
    let mut log = crate::channel::ActivityLogState::new(title, &[("connect", "Contacting the network")]);
    log.fail("connect", "No connection to the network — nothing was sent.".to_string());
    let _ = CHANNEL.activity_tx.send(Some(log));
    close_setup(state);
}

pub(crate) fn clear_reimport_form(state: &mut AppState) {
    state.release_entropy();
    state.reimport_seed_text.clear();
    state.reimport_seed_reveal = false;
    state.reimport_passphrase.clear();
    state.reimport_passphrase_reveal = false;
    state.reimport_bip39.clear();
    state.reimport_bip39_reveal = false;
    state.reimport_word25 = false;
    state.reimport_error = None;
    state.reimport_spinning = false;
}

pub use crate::utils::formatting::exact_amount;

pub(crate) fn clear_enable_form(state: &mut AppState) {
    state.enable_passphrase.clear();
    state.enable_passphrase_reveal = false;
    state.enable_bip39.clear();
    state.enable_bip39_reveal = false;
    state.enable_seed_text.clear();
    state.enable_seed_reveal = false;
    state.enable_error = None;
}

pub(crate) fn forget_wallet(state: &mut AppState) {
    state.xrp_view = XrpView::Menu;
    state.receive_tag.clear();
    state.key_purge_requested = false;
    clear_receive_form(state);
    clear_send_form(state);
    clear_trade_inputs(state);
    clear_cancel_form(state);
    clear_enable_form(state);
    clear_disable_form(state);
}

pub(crate) fn clear_disable_form(state: &mut AppState) {
    state.disable_passphrase.clear();
    state.disable_passphrase_reveal = false;
    state.disable_bip39.clear();
    state.disable_bip39_reveal = false;
    state.disable_seed_text.clear();
    state.disable_seed_reveal = false;
    state.disable_error = None;
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_is_clamped_to_a_percent() {
        assert_eq!(coverage_pct(1000.000001, 1000.0), 100.0);
        assert!((coverage_pct(333.0, 1000.0) - 33.3).abs() < 0.01);
        assert_eq!(coverage_pct(0.0, 1000.0), 0.0);
        assert_eq!(coverage_pct(5.0, 0.0), 0.0);
    }

    #[test]
    fn exact_amount_round_trips_the_ledger_string() {
        for s in ["0", "1", "10.5", "10.1234567", "0.000000789", "123456789012345", "0.00000000000001", "1000000.005"] {
            assert_eq!(exact_amount(s.parse::<f64>().unwrap()), s, "{s}");
        }
    }

    const WALK: (f64, f64) = (600.0, 840.0);
    const WANT: (f64, f64) = (1000.0, 1371.4);
    const LIMIT: f64 = 1.3714;

    #[test]
    fn a_depth_short_ticket_never_estimates_above_the_walk() {
        let e = expected_amount(TradeAnchor::Pay, true, WALK, WANT, LIMIT);
        assert!((e - WALK.1).abs() < 1e-9, "{e}");
        assert!(e <= WALK.1 + 1e-9, "an IOC estimate may never exceed the walk");
    }

    #[test]
    fn the_receive_anchor_estimate_is_also_the_walk_under_a_fill_now_tif() {
        let e = expected_amount(TradeAnchor::Receive, true, WALK, WANT, LIMIT);
        assert!((e - WALK.0).abs() < 1e-9, "{e}");
        assert!(e < WANT.0, "the ticket asked to spend more than the book takes");
    }

    #[test]
    fn a_resting_remainder_is_priced_at_the_limit_and_that_is_deliberate() {
        let e = expected_amount(TradeAnchor::Pay, false, WALK, WANT, LIMIT);
        assert!((e - (840.0 + 400.0 * LIMIT)).abs() < 1e-9, "{e}");
        assert!(e > WALK.1, "a GTC estimate covers the part that rests");
    }

    #[test]
    fn the_two_arms_agree_when_the_book_covers_the_size() {
        let full = (1000.0, 1380.0);
        let ioc = expected_amount(TradeAnchor::Pay, true, full, WANT, LIMIT);
        let gtc = expected_amount(TradeAnchor::Pay, false, full, WANT, LIMIT);
        assert!((ioc - gtc).abs() < 1e-9);
    }

    #[test]
    fn a_walk_past_the_typed_size_never_reduces_the_estimate() {
        let deep = (1200.0, 1650.0);
        let e = expected_amount(TradeAnchor::Pay, false, deep, WANT, LIMIT);
        assert!((e - 1650.0).abs() < 1e-9, "{e}");
    }

    #[test]
    fn the_cushion_only_ever_loosens_the_bound() {
        let floor = 1.3714;
        for touch in [1.3714, 1.3800, 1.5000, 0.0] {
            let b = trade_cushioned(floor, touch, None);
            assert!(b > 0.0 && b < floor, "touch {touch}: {b} not under {floor}");
        }
        assert_eq!(trade_cushioned(0.0, 1.0, None), 0.0);
    }

    #[test]
    fn a_steeper_book_earns_a_wider_cushion() {
        let floor = 1.0;
        let flat = trade_cushioned(floor, 1.0001, None);
        let steep = trade_cushioned(floor, 1.05, None);
        assert!(steep < flat, "steep {steep} should sit lower than flat {flat}");
        let absurd = trade_cushioned(floor, 100.0, None);
        assert!(absurd >= floor * (1.0 - CUSHION_MAX) * 0.999_999, "{absurd}");
    }

    #[test]
    fn delta_min_clears_one_tick() {
        assert!((trade_delta_min(Some(5)) - 1e-4).abs() < 1e-12);
        assert!((trade_delta_min(Some(6)) - 1e-5).abs() < 1e-12);
        assert!((trade_delta_min(None) - 1e-7).abs() < 1e-15);
        let flat_default = trade_cushioned(1.0, 1.0, None);
        let flat_bbrl = trade_cushioned(1.0, 1.0, Some(5));
        assert!(flat_bbrl < flat_default, "{flat_bbrl} !< {flat_default}");
    }

    #[test]
    fn a_finished_order_keeps_the_market_the_right_way_round() {
        for side in [TradeSide::Sell, TradeSide::Buy] {
            let mut state = AppState::default();
            orient_trade_pair(&mut state, side, "XRP", "RLUSD");
            state.trade_side_chosen = true;
            state.trade_amount = "1".to_string();

            clear_trade_form_after_order(&mut state);

            assert_eq!(trade_market_pair(&state), ("XRP", "RLUSD"), "after a {side:?}");
            let (base, quote) = trade_market_pair(&state);
            let fresh = trade_default_side(base, quote);
            assert_eq!(state.trade_side, fresh, "after a {side:?}");
            assert!(!state.trade_side_chosen, "after a {side:?}");
            assert_eq!(state.trade_amount, "", "after a {side:?}");
        }
    }
}
