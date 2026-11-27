use iced::Task;
use crate::controller::message::{Message, PlainField, SecureField, TagPane};
use crate::controller::app_state::{AppState, EnableInputMode, ReceiveEdit, ReceiveFlash, SendAnchor, XrpTokenTab, XrpView, TradeKind, TradeSide, TradeTif, TradeView};
use crate::controller::panes::PaneKind;
use crate::secure::SecureString;
use crate::channel::CHANNEL;
use crate::bridge::xrp_import_logic::XRPImportLogic;
use crate::bridge::xrp_create_logic::XRPCreateLogic;
use crate::bridge::cancel_logic::CancelLogic;
use crate::bridge::trade_logic::TradeLogic;

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
        Message::XrpTxSelected(hash) => {
            state.xrp_tx_selected = Some(hash);
        }
        Message::XrpTxBack => {
            state.xrp_tx_selected = None;
        }
        Message::XrpCopyDetail(full) => {
            state.xrp_detail_copied = Some(full.clone());
            return Task::batch([
                iced::clipboard::write(full).discard(),
                Task::perform(
                    async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                    |_| Message::XrpCopyDetailFeedback,
                ),
            ]);
        }
        Message::XrpCopyDetailFeedback => {
            state.xrp_detail_copied = None;
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
            state.xrp_word25 = Some(yes);
            if !yes {
                state.xrp_bip39_input.clear();
                state.xrp_bip39_reveal = false;
            }
        }
        Message::ImportModeChanged(mode) => {
            if state.xrp_import_mode != Some(mode) {
                state.xrp_encryption_input.clear();
                state.xrp_encryption_reveal = false;
                state.xrp_import_mode = Some(mode);
            }
        }
        Message::ImportSubmitClicked => {
            if !crate::ui::components::wallet_setup::can_finish(
                crate::ui::components::wallet_setup::SetupFlow::Import,
                &state.xrp_seed_text,
                false,
                state.xrp_word25,
                &state.xrp_bip39_input,
                state.xrp_import_mode,
                &state.xrp_encryption_input,
            ) {
                return Task::none();
            }
            let Some(mode) = state.xrp_import_mode else {
                return Task::none();
            };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                abandon_setup(state, "Import XRP wallet");
                return Task::none();
            };

            let mnemonic = state.xrp_seed_text.take();
            let bip39 = state.xrp_bip39_input.take_trimmed();
            state.hold_entropy(SecureField::XrpEncryption);
            let encryption = state.xrp_encryption_input.take_trimmed();

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
                return Task::batch([iced::clipboard::write(address).discard(), receive_flash(state, ReceiveFlash::Copied)]);
            }
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
            if !state.send_amount.trim().is_empty() {
                after_plain_edit(state, PlainField::XrpSendAmount);
            }
        }
        Message::TagEdit(TagPane::Send) => {
            if send_to_xaddress(state) {
                return Task::none();
            }
            unlock_send(state);
            state.send_asset_picker_open = false;
            state.send_tag_editing = true;
            state.request_focus(PlainField::XrpSendTag);
        }
        Message::SendTagLeft => {
            state.send_tag_editing = false;
        }
        Message::SendTagCleared => {
            state.send_destination_tag.clear();
            state.send_tag_editing = true;
            state.request_focus(PlainField::XrpSendTag);
            after_plain_edit(state, PlainField::XrpSendTag);
        }
        Message::TagEdit(TagPane::Receive) => {
            close_receive_editor(state);
            state.receive_tag_draft = state.receive_tag.clone();
            state.receive_edit = ReceiveEdit::Tag;
            state.request_focus(PlainField::XrpReceiveTag);
        }
        Message::ReceiveAmountEdit => {
            close_receive_editor(state);
            let (asset, amount) = state.receive_request.clone().unwrap_or_default();
            state.receive_asset = asset;
            state.receive_amount_draft = amount;
            state.receive_edit = ReceiveEdit::Amount;
            state.request_focus(PlainField::XrpReceiveAmount);
        }
        Message::ReceiveAssetListToggled => {
            state.receive_asset_list = !state.receive_asset_list;
            if !state.receive_asset_list {
                state.request_focus(PlainField::XrpReceiveAmount);
            }
        }
        Message::ReceiveAssetPicked(tab) => {
            state.receive_asset = tab;
            state.receive_asset_list = false;
            state.request_focus(PlainField::XrpReceiveAmount);
        }
        Message::ReceiveSet => match receive_set(state) {
            Some(ReceiveSet::Amount(amount)) => {
                state.receive_request = Some((state.receive_asset.clone(), amount));
                close_receive_editor(state);
            }
            Some(ReceiveSet::Tag(tag)) => {
                state.receive_tag = tag.to_string();
                persist_receive_tag(state);
                close_receive_editor(state);
            }
            None => {}
        },
        Message::ReceiveRemoved => {
            match state.receive_edit {
                ReceiveEdit::Amount => state.receive_request = None,
                ReceiveEdit::Tag => {
                    state.receive_tag.clear();
                    persist_receive_tag(state);
                }
                ReceiveEdit::None => return Task::none(),
            }
            close_receive_editor(state);
        }
        Message::ReceiveCancelled => close_receive_editor(state),
        Message::CopyReceiveTag => {
            if let Some(tag) = tag_of(&state.receive_tag) {
                return Task::batch([
                    iced::clipboard::write(tag.to_string()).discard(),
                    receive_flash(state, ReceiveFlash::TagCopied),
                ]);
            }
        }
        Message::ReceiveDownload => {
            let (_, addr, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
            if let Some(payload) = addr.map(|a| receive_payload(state, &a)).filter(|p| !p.is_empty()) {
                return save_code(payload, Message::ReceiveSaved);
            }
        }
        Message::ReceiveSaved(saved) => {
            return receive_flash(state, if saved { ReceiveFlash::Saved } else { ReceiveFlash::SaveFailed });
        }
        Message::ReceiveFlashElapsed(seq) => {
            if state.receive_flash.is_some_and(|(_, s)| s == seq) {
                state.receive_flash = None;
            }
        }
        Message::SendAssetPickerToggled => {
            state.send_asset_picker_open = !state.send_asset_picker_open;
            if state.send_asset_picker_open {
                unlock_send(state);
                state.send_tag_editing = false;
            }
        }
        Message::EnableTokenClicked(code) => {
            state.enable_token = Some(code);
            state.show_enable = true;
            clear_enable_form(state);
            state.enable_fee = CHANNEL.xrp_node_rx.borrow().open_ledger_fee;
        }
        Message::TokenDetailsClicked(code) => {
            state.token_details = Some(code);
        }
        Message::TokenDetailsDismissed => {
            state.token_details = None;
        }
        Message::CopyIssuerAddress(code) => {
            if let Some(t) = crate::utils::tokens::by_code(code) {
                state.xrp_issuer_copied = true;
                return Task::batch([
                    iced::clipboard::write(t.issuer.to_string()).discard(),
                    Task::perform(
                        async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                        |_| Message::CopyIssuerAddressFeedback,
                    ),
                ]);
            }
        }
        Message::CopyIssuerAddressFeedback => {
            state.xrp_issuer_copied = false;
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
            let asset = match state.enable_token {
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
            let fee = state.enable_fee;
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
                        fee,
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
            state.disable_fee = CHANNEL.xrp_node_rx.borrow().open_ledger_fee;
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
            let fee = state.disable_fee;
            clear_disable_form(state);
            state.disable_token = None;
            state.token_details = None;
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
                        "Remove trustline",
                        fee,
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
                state.create_checked,
                state.xrp_word25,
                &state.xrp_bip39_input,
                state.xrp_import_mode,
                &state.xrp_encryption_input,
            ) {
                return Task::none();
            }
            let Some(mode) = state.xrp_import_mode else {
                return Task::none();
            };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else {
                abandon_setup(state, "Create XRP wallet");
                return Task::none();
            };

            let mnemonic = state.create_mnemonic.take();
            let bip39 = state.xrp_bip39_input.take_trimmed();
            state.hold_entropy(SecureField::XrpEncryption);
            let encryption = state.xrp_encryption_input.take_trimmed();

            return Task::perform(
                async move { XRPCreateLogic::process(mnemonic, bip39, encryption, mode, ws_tx).await },
                |()| Message::CreateCompleted,
            );
        }
        Message::CreateCompleted => {
            close_setup(state);
        }
        Message::SendMaxClicked => {
            state.send_amount = match send_asset(state) {
                XrpTokenTab::Xrp => {
                    let max = xrp_available().map_or(0.0, |s| (s - xrp_fee()).max(0.0));
                    crate::utils::format_token_amount(max, 6)
                }
                XrpTokenTab::Token(code) => CHANNEL.token(code).3,
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
            let fee_xrp = state.send_fee.map_or(0.0, |d| d as f64 / 1e6);
            let fits = state.send_amount.trim().parse::<f64>().ok().is_some_and(|a| send_fits_for(&state.send_tab, a, fee_xrp));
            if state.send_step != 3 || state.send_fee.is_none() || !fits {
                return Task::none();
            }
            let asset = match state.send_tab {
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
            let fee = state.send_fee;
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
                            fee,
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
            state.send_step = 0;
        }
        Message::SendContinueClicked => {
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
            state.send_amount = match send_asset(state) {
                XrpTokenTab::Token(code) if state.send_amount.trim() == CHANNEL.token(code).3 => CHANNEL.token(code).3,
                XrpTokenTab::Token(_) => crate::utils::format_token_amount(crate::utils::orderbook::floor_to(amount, 6), 6),
                XrpTokenTab::Xrp => crate::utils::format_token_amount(amount, 6),
            };
            state.send_error = None;
            state.send_fee = CHANNEL.xrp_node_rx.borrow().open_ledger_fee;
            state.send_tab = send_asset(state);
            state.send_tag_editing = false;
            state.send_step = 2;
        }
        Message::SendAnchorToggled => {
            state.send_anchor = match state.send_anchor {
                SendAnchor::Amount => SendAnchor::Fiat,
                SendAnchor::Fiat => SendAnchor::Amount,
            };
        }
        Message::SendRecipientCleared => {
            unlock_send(state);
            state.send_recipient.clear();
            state.send_destination_tag.clear();
            state.send_tag_editing = false;
            state.send_error = None;
        }
        Message::SendReviewCancelled => {
            unlock_send(state);
        }
        Message::SendReviewContinued => {
            if state.send_step == 2 {
                state.send_step = 3;
            }
        }
        Message::TradeSideSet(side) => {
            if state.trade_side != Some(side) {
                unlock_trade(state);
                state.trade_side = Some(side);
            }
            push_trade_ticket(state);
        }
        Message::TradePairSwapped => {
            if !trade_locked(state) {
                std::mem::swap(&mut state.trade_base, &mut state.trade_quote);
                state.trade_amount.clear();
                state.trade_limit_price.clear();
                state.trade_figure = PlainField::TradeAmount;
                persist_trade_pair(state);
                push_trade_ticket(state);
            }
        }
        Message::TradeMaxClicked => {
            let max = answering_quote(state).and_then(|q| q.max_base).filter(|m| *m > 0.0);
            if let Some(m) = max {
                let base = state.trade_base.clone();
                state.trade_amount = if state.trade_side == Some(TradeSide::Sell) && base != "XRP" {
                    CHANNEL.token(&base).3
                } else {
                    crate::utils::format_token_amount(m, SIGN_PLACES)
                };
                push_trade_ticket(state);
            }
        }
        Message::TradeTifSet(tif) => {
            state.trade_tif = tif;
            push_trade_ticket(state);
        }
        Message::TradePairSelected(base, quote) => {
            state.trade_base = base;
            state.trade_quote = quote;
            reset_ticket(state);
            state.trade_pair_list = None;
            state.trade_pair_query.clear();
            persist_trade_pair(state);
            push_trade_ticket(state);
        }
        Message::TradeMarketSelected => {
            if pair_quote(state).is_some_and(|q| q.market_ok) && state.trade_mode != Some(TradeKind::Market) {
                unlock_trade(state);
                state.trade_mode = Some(TradeKind::Market);
                state.trade_limit_price.clear();
                state.trade_tif = TradeTif::Gtc;
                state.trade_figure = PlainField::TradeAmount;
                push_trade_ticket(state);
            }
        }
        Message::TradeLimitSelected => {
            if state.trade_mode != Some(TradeKind::Limit) {
                unlock_trade(state);
                state.trade_mode = Some(TradeKind::Limit);
                push_trade_ticket(state);
            }
        }
        Message::TradeReviewPressed => {
            let fee = CHANNEL.xrp_node_rx.borrow().open_ledger_fee;
            if state.trade_view == TradeView::Ticket
                && let Some(q) = answering_quote(state).filter(|q| review_ready(q, fee))
            {
                state.trade_review_fee = fee;
                state.trade_view = TradeView::Review(Box::new(q));
            }
        }
        Message::TradeReviewContinued => {
            if let TradeView::Review(q) = &state.trade_view {
                state.trade_view = TradeView::Sign(q.clone());
            }
        }
        Message::TradeReviewCancelled => {
            unlock_trade(state);
        }
        Message::TradeBookPriceClicked(val) => {
            if state.trade_mode == Some(TradeKind::Limit) && !trade_locked(state) {
                state.trade_limit_price = val;
                push_trade_ticket(state);
            }
        }
        Message::TradeSubmitClicked => {
            let TradeView::Sign(quote) = &state.trade_view else { return Task::none() };
            let q = (**quote).clone();
            let (_, wallet_address, key_deleted, key_mode) = CHANNEL.wallet_balance_rx.borrow().clone();
            let Some(address) = wallet_address else { return Task::none() };
            let Some(ws_tx) = crate::ws::CRYPTO_COMMANDS_TX.get().cloned() else { return Task::none() };
            let Some(fee) = state.trade_review_fee else { return Task::none() };
            let (Some(pay), Some(receive)) = (q.pay.clone(), q.receive.clone()) else { return Task::none() };
            let mode = match EnableInputMode::detect(key_deleted, key_mode) {
                EnableInputMode::Passphrase => "passphrase",
                EnableInputMode::Seed => "seed",
            }
            .to_string();

            let pay_f: f64 = pay.parse().unwrap_or(0.0);
            let receive_f: f64 = receive.parse().unwrap_or(0.0);
            let anchor_is_pay = q.ticket.anchor_is_pay();
            let selling = q.ticket.side == Some(TradeSide::Sell);
            let expected_vwap = (q.ticket.kind == Some(TradeKind::Market))
                .then(|| {
                    q.price_row
                        .map(|p| if selling { p } else if p > 0.0 { 1.0 / p } else { 0.0 })
                })
                .flatten()
                .filter(|v| *v > 0.0);
            let _ = CHANNEL.pending_trade_tx.send(Some(crate::channel::PendingTrade {
                pair: format!("{}/{}", q.pay_asset, q.receive_asset),
                anchor: if anchor_is_pay { "pay" } else { "receive" }.to_string(),
                bound: if pay_f > 0.0 { receive_f / pay_f } else { 0.0 },
                expected_vwap,
                ledger_index: if q.ledger > 0 {
                    q.ledger
                } else {
                    CHANNEL.xrp_node_rx.borrow().ledger_index.unwrap_or(0)
                },
                last_ledger: None,
                sequence: None,
                hash: None,
                tx_id: None,
                pay: pay_f,
                receive: receive_f,
            }));

            let passphrase = state.trade_passphrase.take_trimmed();
            let mnemonic   = state.trade_seed_text.take();
            let bip39      = state.trade_bip39.take_trimmed();
            clear_trade_inputs(state);
            return Task::perform(
                async move {
                    TradeLogic::process(
                        mode, passphrase, mnemonic, bip39,
                        q.pay_asset, q.receive_asset, pay, receive, q.flags,
                        address, Some(fee), ws_tx, None,
                    ).await
                },
                |_| Message::TradeCompleted,
            );
        }
        Message::TradeCompleted => {
            clear_trade_inputs(state);
        }
        Message::CancelOrderClicked(seq) => {
            clear_cancel_form(state);
            state.cancel_offer_sequence = Some(seq);
            state.cancel_fee = CHANNEL.xrp_node_rx.borrow().open_ledger_fee;
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
            let fee = state.cancel_fee;
            clear_cancel_form(state);
            state.xrp_tx_selected = None;
            return Task::perform(
                async move {
                    CancelLogic::process(
                        mode, passphrase, mnemonic, bip39,
                        offer_sequence, address, fee, ws_tx,
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

fn xrp_wallet_exists() -> bool {
    CHANNEL.wallet_balance_rx.borrow().1.is_some()
}

pub const SIGN_PLACES: usize = crate::utils::trade::SIGN_PLACES;

pub fn trade_decimals(code: &str) -> usize {
    if code == "XRP" { 6 } else { 2 }
}

pub fn current_ticket(state: &AppState) -> Option<crate::utils::trade::TradeTicket> {
    if !xrp_wallet_exists() {
        return None;
    }
    let (base, quote) = trade_market_pair(state);
    if base.is_empty() || quote.is_empty() {
        return None;
    }
    Some(crate::utils::trade::TradeTicket {
        base: base.to_string(),
        quote: quote.to_string(),
        side: state.trade_side,
        kind: state.trade_mode,
        amount: state.trade_amount.trim().to_string(),
        limit: typed_price(&state.trade_limit_price),
        tif: state.trade_tif,
    })
}

pub fn typed_price(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|p| *p > 0.0)
}

pub fn answering_quote(state: &AppState) -> Option<crate::utils::trade::TradeQuote> {
    let ticket = current_ticket(state)?;
    CHANNEL.trade_quote_rx.borrow().as_ref().filter(|q| q.ticket == ticket).cloned()
}

fn pair_quote(state: &AppState) -> Option<crate::utils::trade::TradeQuote> {
    CHANNEL
        .trade_quote_rx
        .borrow()
        .as_ref()
        .filter(|q| q.ticket.base == state.trade_base && q.ticket.quote == state.trade_quote)
        .cloned()
}

pub fn trade_locked(state: &AppState) -> bool {
    matches!(state.trade_view, TradeView::Review(_) | TradeView::Sign(_))
}

pub(crate) fn unlock_trade(state: &mut AppState) {
    if trade_locked(state) {
        state.trade_view = TradeView::Ticket;
    }
    state.trade_review_fee = None;
    clear_trade_secrets(state);
}

fn reset_ticket(state: &mut AppState) {
    state.trade_side = None;
    state.trade_mode = None;
    state.trade_amount.clear();
    state.trade_limit_price.clear();
    state.trade_figure = PlainField::TradeAmount;
    state.trade_tif = TradeTif::Gtc;
    state.trade_view = TradeView::Ticket;
    state.trade_review_fee = None;
    clear_trade_secrets(state);
}

pub fn push_trade_ticket(state: &AppState) {
    let ticket = current_ticket(state);
    CHANNEL.trade_ticket_tx.send_if_modified(|t| {
        if *t == ticket {
            false
        } else {
            *t = ticket;
            true
        }
    });
}

pub fn sync_trade(state: &mut AppState) {
    push_trade_ticket(state);
}

pub fn review_ready(q: &crate::utils::trade::TradeQuote, fee: Option<u64>) -> bool {
    let typed: f64 = q.ticket.amount.trim().parse().unwrap_or(0.0);
    q.lockable
        && fee.is_some()
        && typed > 0.0
        && q.max_base.is_some_and(|m| typed <= m + 1e-9)
        && trade_fee_xrp_ok(q)
}

pub fn trade_fee_xrp_ok(q: &crate::utils::trade::TradeQuote) -> bool {
    if q.pay_asset == "XRP" || q.pay_asset.is_empty() {
        return true;
    }
    let resting = q.ticket.kind == Some(TradeKind::Limit) && q.ticket.tif == TradeTif::Gtc;
    xrp_reserve().is_some_and(|r| r.available >= xrp_fee() + if resting { r.per_object } else { 0.0 })
}

pub(crate) fn clear_trade_secrets(state: &mut AppState) {
    state.trade_passphrase.clear();
    state.trade_passphrase_reveal = false;
    state.trade_bip39.clear();
    state.trade_bip39_reveal = false;
    state.trade_seed_text.clear();
    state.trade_seed_reveal = true;
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
    state.trade_base.clear();
    state.trade_quote.clear();
    reset_ticket(state);
    state.trade_pair_list = None;
    state.trade_pair_query.clear();
    let _ = crate::bridge::json_storage::update_json::<serde_json::Value>("settings.json", |json| {
        if let Some(obj) = json.as_object_mut() {
            obj.remove(PAIR_KEY);
        }
    });
}

pub fn ensure_trade_pair(state: &mut AppState) {
    {
        let (base, quote) = trade_market_pair(state);
        if !base.is_empty() && !quote.is_empty() {
            return;
        }
    }
    use crate::ui::managexrp::xrptrade::picker;
    let (rows, _) = picker::listing("");
    let (base, quote) = rows
        .into_iter()
        .next()
        .or_else(|| picker::all_markets().into_iter().next())
        .unwrap_or(("XRP", "RLUSD"));
    state.trade_base = base.to_string();
    state.trade_quote = quote.to_string();
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
    state.trade_base = base.to_string();
    state.trade_quote = quote.to_string();
}

pub fn trade_market_pair(state: &AppState) -> (&str, &str) {
    (state.trade_base.as_str(), state.trade_quote.as_str())
}

pub fn trade_pay_asset(state: &AppState) -> Option<&str> {
    match state.trade_side? {
        TradeSide::Sell => Some(state.trade_base.as_str()),
        TradeSide::Buy => Some(state.trade_quote.as_str()),
    }
}

pub fn trade_available(state: &AppState) -> Option<f64> {
    let pay = trade_pay_asset(state)?;
    if pay == "XRP" { xrp_available() } else { Some(CHANNEL.token(pay).0) }
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

pub(crate) fn clear_trade_inputs(state: &mut AppState) {
    reset_ticket(state);
    if state.trade_pair_list == Some(PaneKind::Ticket) {
        state.trade_pair_list = None;
        state.trade_pair_query.clear();
    }
    push_trade_ticket(state);
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
    send_fits_at(state, amount, xrp_fee())
}

pub fn send_fits_at(state: &AppState, amount: f64, fee: f64) -> bool {
    send_fits_for(&send_asset(state), amount, fee)
}

pub(crate) fn holds(code: &str) -> bool {
    let (balance, held, _, _) = CHANNEL.token(code);
    held && balance > 0.0
}

pub(crate) fn send_asset(state: &AppState) -> XrpTokenTab {
    match state.xrp_token_tab {
        XrpTokenTab::Token(code) if holds(code) => XrpTokenTab::Token(code),
        _ => XrpTokenTab::Xrp,
    }
}

pub fn send_fits_for(tab: &XrpTokenTab, amount: f64, fee: f64) -> bool {
    let Some(available) = xrp_available() else { return false };
    match *tab {
        XrpTokenTab::Xrp => amount + fee <= available,
        XrpTokenTab::Token(code) => amount <= CHANNEL.token(code).0 && fee <= available,
    }
}

pub fn send_rate(state: &AppState) -> f64 {
    let key = send_asset(state)
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
    if state.send_step >= 2 {
        return;
    }
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
    if matches!(field, PlainField::XrpReceiveAmount | PlainField::XrpReceiveTag) {
        return;
    }
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
        PlainField::XrpSendRecipient => {
            if state.send_step >= 2 {
                unlock_send(state);
            }
            if send_to_xaddress(state) {
                state.send_tag_editing = false;
                state.send_destination_tag.clear();
            }
        }
        PlainField::XrpSendTag if state.send_step >= 2 => unlock_send(state),
        PlainField::TradeAmount | PlainField::TradeLimit => {
            push_trade_ticket(state);
        }
        PlainField::TradePairQuery => {}
        _ => {}
    }
}

fn unlock_send(state: &mut AppState) {
    state.send_step = 0;
    state.send_fee = None;
    state.send_passphrase.clear();
    state.send_passphrase_reveal = false;
    state.send_bip39.clear();
    state.send_bip39_reveal = false;
    state.send_seed_text.clear();
    state.send_seed_reveal = true;
}

pub(crate) fn send_to_xaddress(state: &AppState) -> bool {
    dannesk_xrpl_codec::xaddress::resolve(state.send_recipient.trim()).is_some_and(|r| r.from_xaddress)
}

// With a tag set, the X-address: QR, address row and copy stay one string.
pub(crate) fn receive_address(state: &AppState, classic: &str) -> String {
    receive_tag_shown(state)
        .and_then(|tag| dannesk_xrpl_codec::xaddress::encode(classic, Some(tag)))
        .unwrap_or_else(|| classic.to_string())
}

pub(crate) fn receive_payload(state: &AppState, classic: &str) -> String {
    let request = state
        .receive_request
        .as_ref()
        .map(|(asset, amount)| crate::utils::payment_code::Request { amount, token: asset.code() });
    crate::utils::payment_code::xrp_payload(classic, receive_tag_shown(state), request).unwrap_or_default()
}

pub(crate) fn receive_tag_shown(state: &AppState) -> Option<u32> {
    tag_of(&state.receive_tag)
}

pub(crate) fn tag_of(buf: &str) -> Option<u32> {
    buf.trim().parse::<u32>().ok()
}

pub(crate) enum ReceiveSet {
    Amount(String),
    Tag(u32),
}

pub(crate) fn receive_set(state: &AppState) -> Option<ReceiveSet> {
    match state.receive_edit {
        ReceiveEdit::Amount => request_amount(&state.receive_amount_draft).map(ReceiveSet::Amount),
        ReceiveEdit::Tag => tag_of(&state.receive_tag_draft).map(ReceiveSet::Tag),
        ReceiveEdit::None => None,
    }
}

pub(crate) fn request_amount(draft: &str) -> Option<String> {
    let draft = draft.trim();
    draft.parse::<f64>().is_ok_and(|v| v > 0.0).then(|| draft.to_string())
}

fn close_receive_editor(state: &mut AppState) {
    state.receive_edit = ReceiveEdit::None;
    state.receive_asset_list = false;
    state.receive_amount_draft.clear();
    state.receive_tag_draft.clear();
}

fn receive_flash(state: &mut AppState, flash: ReceiveFlash) -> Task<Message> {
    let seq = next_flash(state);
    state.receive_flash = Some((flash, seq));
    flash_timer(seq, Message::ReceiveFlashElapsed)
}

pub(crate) fn next_flash(state: &mut AppState) -> u32 {
    state.receive_flash_seq = state.receive_flash_seq.wrapping_add(1);
    state.receive_flash_seq
}

pub(crate) fn flash_timer(seq: u32, done: fn(u32) -> Message) -> Task<Message> {
    Task::perform(async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await }, move |_| done(seq))
}

pub(crate) fn save_code(payload: String, done: fn(bool) -> Message) -> Task<Message> {
    Task::perform(
        async move {
            let ink = crate::ui::components::grid::QR_INK;
            tokio::task::spawn_blocking(move || crate::utils::qr::save_png(&payload, ink).is_some())
                .await
                .unwrap_or(false)
        },
        done,
    )
}

pub(crate) fn clear_receive_form(state: &mut AppState) {
    close_receive_editor(state);
    state.receive_request = None;
    state.receive_asset = XrpTokenTab::Xrp;
    state.receive_flash = None;
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
    state.xrp_token_tab = XrpTokenTab::Xrp;
    state.send_step = 0;
    state.send_fee = None;
    state.send_recipient = String::new();
    state.send_destination_tag = String::new();
    state.send_tag_editing = false;
    state.send_asset_picker_open = false;
    state.send_amount = String::new();
    state.send_fiat_amount = String::new();
    state.send_anchor = SendAnchor::Amount;
    state.send_passphrase.clear();
    state.send_passphrase_reveal = false;
    state.send_bip39.clear();
    state.send_bip39_reveal = false;
    state.send_seed_text.clear();
    state.send_seed_reveal = true;
    state.send_error = None;
}

pub(crate) fn clear_cancel_form(state: &mut AppState) {
    state.cancel_offer_sequence = None;
    state.cancel_fee = None;
    state.cancel_passphrase.clear();
    state.cancel_passphrase_reveal = false;
    state.cancel_bip39.clear();
    state.cancel_bip39_reveal = false;
    state.cancel_seed_text.clear();
    state.cancel_seed_reveal = true;
    state.cancel_error = None;
}

fn close_setup(state: &mut AppState) {
    reset_setup_form(state);
    state.xrp_view = XrpView::Menu;
}

fn reset_setup_form(state: &mut AppState) {
    state.release_entropy();
    state.xrp_seed_text.clear();
    state.xrp_seed_reveal = true;
    state.xrp_encryption_input.clear();
    state.xrp_encryption_reveal = false;
    state.xrp_bip39_input.clear();
    state.xrp_bip39_reveal = false;
    state.xrp_word25 = None;
    state.create_mnemonic.clear();
    state.create_seed_reveal = true;
    state.xrp_import_mode = None;
    state.xrp_copy_feedback = false;
    crate::controller::setup::reset(state, crate::controller::app_state::SetupChain::Xrp);
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
    state.reimport_seed_reveal = true;
    state.reimport_passphrase.clear();
    state.reimport_passphrase_reveal = false;
    state.reimport_bip39.clear();
    state.reimport_bip39_reveal = false;
    state.reimport_word25 = false;
    state.reimport_error = None;
    state.reimport_spinning = false;
}


pub(crate) fn clear_enable_form(state: &mut AppState) {
    state.enable_fee = None;
    state.enable_passphrase.clear();
    state.enable_passphrase_reveal = false;
    state.enable_bip39.clear();
    state.enable_bip39_reveal = false;
    state.enable_seed_text.clear();
    state.enable_seed_reveal = true;
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
    state.xrp_tx_selected = None;
}

pub(crate) fn clear_disable_form(state: &mut AppState) {
    state.disable_fee = None;
    state.disable_passphrase.clear();
    state.disable_passphrase_reveal = false;
    state.disable_bip39.clear();
    state.disable_bip39_reveal = false;
    state.disable_seed_text.clear();
    state.disable_seed_reveal = true;
    state.disable_error = None;
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::message::SecureOp;
    use crate::utils::trade::{TradeQuote, TradeTicket, Verdict};

    fn frozen_quote() -> TradeQuote {
        TradeQuote {
            ticket: TradeTicket {
                base: "XRP".to_string(),
                quote: "RLUSD".to_string(),
                side: Some(TradeSide::Sell),
                kind: Some(TradeKind::Limit),
                amount: "500".to_string(),
                limit: Some(1.3004),
                tif: TradeTif::Gtc,
            },
            market_ok: true,
            require_auth: false,
            global_freeze: false,
            bids: Vec::new(),
            asks: Vec::new(),
            book_mid: Some(1.2994),
            book_spread_pct: Some(0.11),
            ledger: 1,
            fresh: true,
            verdict: Verdict::Idle,
            price_row: None,
            counter: Some(650.2),
            route: None,
            pay_asset: "XRP".to_string(),
            receive_asset: "RLUSD".to_string(),
            pay: Some("500".to_string()),
            receive: Some("650.2".to_string()),
            flags: vec!["tfSell".to_string()],
            lockable: true,
            max_base: Some(8213.10219),
        }
    }

    fn on_pair(side: Option<TradeSide>, mode: Option<TradeKind>) -> AppState {
        let mut state = AppState::default();
        state.trade_base = "XRP".to_string();
        state.trade_quote = "RLUSD".to_string();
        state.trade_side = side;
        state.trade_mode = mode;
        state
    }

    #[test]
    fn a_finished_order_keeps_the_pair_and_picks_nothing() {
        let mut state = on_pair(Some(TradeSide::Sell), Some(TradeKind::Market));
        state.trade_amount = "1".to_string();

        clear_trade_inputs(&mut state);

        assert_eq!(trade_market_pair(&state), ("XRP", "RLUSD"));
        assert_eq!((state.trade_side, state.trade_mode), (None, None), "a side or kind survived");
        assert_eq!(state.trade_amount, "");
    }

    #[test]
    fn picking_a_pair_drops_the_review_lock_and_picks_nothing() {
        let mut state = on_pair(Some(TradeSide::Sell), Some(TradeKind::Market));
        state.trade_tif = TradeTif::Fok;
        state.trade_amount = "500".to_string();
        state.trade_view = TradeView::Review(Box::new(frozen_quote()));
        state.trade_review_fee = Some(12);
        state.trade_pair_list = Some(PaneKind::Depth);

        let _ = handle(&mut state, Message::TradePairSelected("RLUSD".to_string(), "EUROP".to_string()));

        assert_eq!(state.trade_view, TradeView::Ticket, "the lock survived a pair pick");
        assert_eq!(state.trade_review_fee, None);
        assert_eq!((state.trade_side, state.trade_mode), (None, None), "a pick preselected something");
        assert_eq!(state.trade_tif, TradeTif::Gtc);
        assert_eq!(state.trade_amount, "");
        assert_eq!(state.trade_pair_list, None);
        assert_eq!(trade_market_pair(&state), ("RLUSD", "EUROP"));
    }

    #[test]
    fn a_book_press_is_inert_while_the_order_is_locked() {
        let mut state = on_pair(Some(TradeSide::Sell), Some(TradeKind::Limit));
        state.trade_limit_price = "1.3004".to_string();
        for locked in [TradeView::Review(Box::new(frozen_quote())), TradeView::Sign(Box::new(frozen_quote()))] {
            state.trade_view = locked;
            let _ = handle(&mut state, Message::TradeBookPriceClicked("1.3016".to_string()));
            assert_eq!(state.trade_limit_price, "1.3004", "a level press reached a locked ticket");
            assert!(trade_locked(&state));
        }

        state.trade_view = TradeView::Ticket;
        let _ = handle(&mut state, Message::TradeBookPriceClicked("1.3016".to_string()));
        assert_eq!(state.trade_limit_price, "1.3016");
        assert_eq!(state.trade_view, TradeView::Ticket);
    }

    #[test]
    fn the_limit_segment_switches_the_mode_and_opens_nothing() {
        let mut state = on_pair(Some(TradeSide::Sell), Some(TradeKind::Market));

        let _ = handle(&mut state, Message::TradeLimitSelected);
        assert_eq!(state.trade_mode, Some(TradeKind::Limit));
        assert_eq!(state.trade_view, TradeView::Ticket);

        let _ = handle(&mut state, Message::TradeLimitSelected);
        assert_eq!(state.trade_mode, Some(TradeKind::Limit), "a second press changed the kind");
        assert_eq!(state.trade_view, TradeView::Ticket, "a second press opened something");
    }

    #[test]
    fn the_price_is_typed_into_the_ticket_and_lights_its_figure() {
        let mut state = on_pair(Some(TradeSide::Sell), Some(TradeKind::Limit));

        assert!(state.apply_plain_edit(PlainField::TradeLimit, SecureOp::Paste(0, "1.3050".to_string())));
        assert_eq!(state.trade_limit_price, "1.3050");

        assert_eq!(state.trade_figure, PlainField::TradeAmount, "the amount is lit by default");
        state.figure_pressed(PlainField::TradeLimit);
        assert_eq!(state.trade_figure, PlainField::TradeLimit);
        state.figure_pressed(PlainField::XrpSendAmount);
        assert_eq!(state.trade_figure, PlainField::TradeLimit, "a Send field moved the ticket's lit figure");
    }

    #[test]
    fn review_continues_to_sign_and_cancel_drops_the_lock() {
        let mut state = on_pair(Some(TradeSide::Sell), Some(TradeKind::Limit));
        state.trade_view = TradeView::Review(Box::new(frozen_quote()));
        state.trade_review_fee = Some(12);

        let _ = handle(&mut state, Message::TradeReviewContinued);
        assert!(matches!(state.trade_view, TradeView::Sign(_)), "{:?}", state.trade_view);

        let _ = handle(&mut state, Message::TradeReviewCancelled);
        assert_eq!(state.trade_view, TradeView::Ticket);
        assert_eq!(state.trade_review_fee, None);
    }

    #[test]
    fn a_new_side_or_kind_drops_the_lock_and_the_same_one_keeps_it() {
        let mut state = on_pair(Some(TradeSide::Sell), Some(TradeKind::Limit));
        state.trade_view = TradeView::Sign(Box::new(frozen_quote()));
        state.trade_review_fee = Some(12);

        let _ = handle(&mut state, Message::TradeSideSet(TradeSide::Sell));
        assert!(trade_locked(&state), "the same side dropped the lock");

        let _ = handle(&mut state, Message::TradeSideSet(TradeSide::Buy));
        assert_eq!(state.trade_view, TradeView::Ticket);
        assert_eq!(state.trade_review_fee, None);
        assert_eq!(state.trade_side, Some(TradeSide::Buy));

        state.trade_view = TradeView::Review(Box::new(frozen_quote()));
        let _ = handle(&mut state, Message::TradeLimitSelected);
        assert!(trade_locked(&state), "the same kind dropped the lock");

        state.trade_mode = Some(TradeKind::Market);
        let _ = handle(&mut state, Message::TradeLimitSelected);
        assert_eq!(state.trade_view, TradeView::Ticket, "a new kind kept the lock");
        assert_eq!(state.trade_mode, Some(TradeKind::Limit));
    }

    #[test]
    fn the_swap_reverses_the_pair_and_keeps_the_side_and_kind() {
        let mut state = on_pair(Some(TradeSide::Buy), Some(TradeKind::Limit));
        state.trade_amount = "100".to_string();
        state.trade_limit_price = "1.3004".to_string();
        state.trade_tif = TradeTif::Ioc;
        state.trade_figure = PlainField::TradeLimit;

        let _ = handle(&mut state, Message::TradePairSwapped);

        assert_eq!(state.trade_figure, PlainField::TradeAmount, "the cleared price stayed lit");
        assert_eq!(trade_market_pair(&state), ("RLUSD", "XRP"));
        assert_eq!(trade_pay_asset(&state), Some("XRP"), "a buy of the reversed pair pays XRP");
        assert_eq!((state.trade_side, state.trade_mode), (Some(TradeSide::Buy), Some(TradeKind::Limit)));
        assert_eq!(state.trade_tif, TradeTif::Ioc);
        assert_eq!(state.trade_amount, "", "an XRP figure survived into the RLUSD field");
        assert_eq!(state.trade_limit_price, "", "a price in RLUSD per XRP survived the reversal");

        let _ = handle(&mut state, Message::TradePairSwapped);
        assert_eq!(trade_market_pair(&state), ("XRP", "RLUSD"));

        state.trade_view = TradeView::Review(Box::new(frozen_quote()));
        let _ = handle(&mut state, Message::TradePairSwapped);
        assert_eq!(trade_market_pair(&state), ("XRP", "RLUSD"), "a locked ticket reversed");
    }

    #[test]
    fn a_tag_past_a_u32_is_held_but_never_set_or_reviewed() {
        let mut state = AppState::default();
        assert!(state.apply_plain_edit(PlainField::XrpReceiveTag, SecureOp::Paste(0, "4294967296".to_string())));
        assert_eq!(state.receive_tag_draft, "4294967296", "the field refused what it should draw red");
        assert_eq!(tag_of(&state.receive_tag_draft), None, "a tag past a u32 could be set");

        state.send_recipient = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De".to_string();
        assert!(state.apply_plain_edit(PlainField::XrpSendTag, SecureOp::Paste(0, "4294967296".to_string())));
        state.send_amount = "1".to_string();
        let _ = handle(&mut state, Message::SendContinueClicked);
        assert_eq!(state.send_error.as_deref(), Some("ERR: INVALID_DESTINATION_TAG"));
        assert_eq!(state.send_step, 0, "a tag past a u32 reached the review");
    }

    #[test]
    fn an_x_address_drops_a_typed_tag_for_good() {
        let classic = "rpcbQcdbjQrPvAVDzymF95vqht9M4wAzkc";
        let mut state = AppState::default();
        state.send_recipient = classic.to_string();
        let _ = handle(&mut state, Message::TagEdit(TagPane::Send));
        assert!(state.apply_plain_edit(PlainField::XrpSendTag, SecureOp::Paste(0, "77".to_string())));

        state.replace_plain_field(PlainField::XrpSendRecipient, "X7Yoo9fNvD5LCE49ScoNk3EKPL9wVee2mbUcQgFj8AXmcVt".to_string());
        after_plain_edit(&mut state, PlainField::XrpSendRecipient);
        assert_eq!(state.send_destination_tag, "", "a typed tag survived the X-address");
        assert!(!state.send_tag_editing);
        let _ = handle(&mut state, Message::TagEdit(TagPane::Send));
        assert!(!state.send_tag_editing, "an X-address tag opened for editing");

        state.replace_plain_field(PlainField::XrpSendRecipient, classic.to_string());
        after_plain_edit(&mut state, PlainField::XrpSendRecipient);
        assert_eq!(state.send_destination_tag, "", "the typed tag came back with the classic address");
    }

    #[test]
    fn clearing_the_tag_keeps_the_caret_in_the_slot() {
        let mut state = AppState::default();
        let _ = handle(&mut state, Message::TagEdit(TagPane::Send));
        assert!(state.apply_plain_edit(PlainField::XrpSendTag, SecureOp::Paste(0, "104829".to_string())));
        let _ = handle(&mut state, Message::SendTagLeft);
        let _ = handle(&mut state, Message::SendTagCleared);
        assert_eq!(state.send_destination_tag, "");
        assert!(state.send_tag_editing, "the press that clears left the slot");
        assert_eq!(state.focus, Some(PlainField::XrpSendTag));
    }
}
