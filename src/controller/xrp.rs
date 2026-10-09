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
            // The grid opens empty: generation is the user's own `Generate ›`
            // click, so the moment the words appear is theirs to pick.
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
            return iced::clipboard::write(hash).discard();
        }
        Message::BackClicked => match state.xrp_view {
            XrpView::Menu => {
                // Back from inline token enable/detail → return to XRP tab
                clear_enable_form(state);
                state.xrp_token_tab = XrpTokenTab::Xrp;
            }
            _ => {
                // Import/create are one screen now: back is leaving the flow,
                // and leaving the flow wipes the buffers.
                reset_setup_form(state);
                state.xrp_view = XrpView::Menu;
            }
        },
        Message::GenerateMnemonic => {
            // One roll per flow: the link is gone from the screen once words
            // exist, and a stray second message must not silently replace a
            // phrase that may already be written down.
            if state.create_mnemonic.as_str().split_whitespace().count() == 0 {
                use bip39::{Language, Mnemonic};
                use rand::RngExt;
                let mut entropy = [0u8; 32];
                rand::rng().fill(&mut entropy);
                let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy).unwrap();
                state.create_mnemonic = SecureString::new(mnemonic.to_string());
            }
        }
        Message::Word25Chosen(yes) => {
            state.xrp_word25 = yes;
            if !yes {
                // `No` means no: a word typed under `Yes` and then disowned
                // must not survive to derive the wallet.
                state.xrp_bip39_input.clear();
                state.xrp_bip39_reveal = false;
            }
        }
        Message::ImportModeChanged(mode) => {
            // Clear the shared secret buffer on a switch: Cold stores nothing, so
            // a passphrase typed under Standard must not survive the change and
            // silently encrypt a wallet the user asked not to have one.
            if state.xrp_import_mode != mode {
                state.xrp_encryption_input.clear();
                state.xrp_encryption_reveal = false;
                state.xrp_import_mode = mode;
            }
        }
        Message::ImportSubmitClicked => {
            // The one-screen flow has no step navigation in front of the
            // bridge, so the whole gate the button draws itself dead under is
            // re-checked here — Enter in any field fires this too.
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

            // Move the locked secrets out of AppState (no unlocked copy); fields
            // are left empty, so a failed import clears the form.
            let mnemonic = state.xrp_seed_text.take();
            let bip39 = state.xrp_bip39_input.take_trimmed();
            // Freeze the gauge before the key it measures leaves.
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
            // The key-management page STAYS OPEN and cuts to its cold face the
            // moment the channel confirms — closing here dropped the user on
            // the balance screen at the instant something irreversible
            // happened, which reads as a crash rather than a result.
            state.key_mgmt_restoring = false;
            let (_, wallet_address, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
            let Some(address) = wallet_address else { return Task::none(); };
            // `key_purge_requested` guards the re-tap through the async gap —
            // the button draws disabled until the channel reports the key gone.
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
            // The page stays up, button dead, until the bridge reports back;
            // `XrpWalletRemoved` then routes to Menu, which — the address gone
            // from the channel — draws the wallet gate. A definite result
            // state, never emptiness.
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
            state.xrp_view = XrpView::Menu;
            // The tag was this wallet's: its file went with the wallet, so
            // only the buffers are left to drop.
            state.receive_tag.clear();
            clear_receive_form(state);
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
                // Marked secret so clipboard managers keep it out of history.
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
            // The send form's derived amount follows the new asset's rate at
            // once, not at the next tick.
            resync_send_twin(state);
            // Picking swaps the send pane's face back to the form.
            state.send_asset_picker_open = false;
            clear_enable_form(state);
            // The amount typed stays; its base-currency twin re-crosses on
            // the new asset's rate.
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
            // Digits only reach the draft, so the one way to be wrong is to
            // outrun a u32 — and the button is dark for that. Belt and braces.
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
        Message::SendAssetPickerToggled => {
            state.send_asset_picker_open = !state.send_asset_picker_open;
        }
        Message::EnableTokenClicked(code) => {
            // An `enable` link on the Tokens page → arm the asset (the sign
            // stack and the downstream WS command both read it off
            // `xrp_token_tab`) and float the stack. The page STAYS open: the
            // stack is an overlay on it, not a screen that replaces it, so the
            // row being enabled sits behind the scrim where it can be read.
            state.xrp_token_tab = XrpTokenTab::Token(code);
            state.show_enable = true;
            clear_enable_form(state);
        }
        Message::EnableDismissed => {
            // Dismissing the stack drops the credential and leaves the page
            // exactly as it was — the send v3 rule: the surface survives, the
            // secret does not.
            state.show_enable = false;
            clear_enable_form(state);
        }
        Message::EnableSubmitClicked => {
            // Gated by the submit button; correctness is arbitrated downstream.
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
            // Close the stack; the new trustline lands via the channel and the
            // balance total and the Tokens page behind it pick it up — the row
            // moves from `available` into the held group on its own.
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
            // A `disable` link on a held row → the tokens pane becomes the
            // removal form for that line. Live only at an exact zero balance
            // (the view's gate), because the ledger accepts a limit-0
            // TrustSet on any balance and only DELETES the line at zero.
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
            // The gate the view drew: the balance the relay last wrote, to
            // the ledger's last digit, is exactly zero. Checked again here so
            // a balance that landed between the click and the sign cannot
            // send a TrustSet that would lower the limit and refund nothing.
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
            // Back to the list; the relay reports the deletion on the
            // trustline channel and the row leaves the held group on its own.
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
            // Same re-check as import's: no navigation guards the bridge now.
            // A generated phrase is valid by construction, so Create's gate is
            // the count, the 25th-word answer, and the key.
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

            // Move the locked secrets out of AppState (no unlocked copy); the
            // form clears on completion either way.
            let mnemonic = state.create_mnemonic.take();
            let bip39 = state.xrp_bip39_input.take_trimmed();
            // Freeze the gauge before the key it measures leaves.
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
            // Everything the reserve and the fee leave behind. The fee is the
            // open-ledger fee the send signs with; a token has no fee of its
            // own (the XRP fee is paid from the XRP balance, not the token).
            state.send_amount = match state.xrp_token_tab {
                XrpTokenTab::Xrp => {
                    let max = xrp_available().map_or(0.0, |s| (s - xrp_fee()).max(0.0));
                    crate::utils::format_token_amount(max, 6)
                }
                // The WHOLE balance, to the ledger's last digit: an issued
                // amount carries up to 15 significant digits, which an f64
                // round-trips exactly, and `{}` prints the shortest decimal
                // that does so — the relay's string back again, never a
                // truncation. Six places here left a tail that could keep
                // the line alive after "send everything".
                XrpTokenTab::Token(code) => exact_amount(CHANNEL.token(code).0),
            };
            after_plain_edit(state, PlainField::XrpSendAmount);
        }
        Message::SendSubmitClicked => {
            // Gated by the submit button; correctness is the blockchain's arbiter
            // (or wallet_auth on decrypt), surfaced via the activity log.
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
            // Move the locked secrets out of AppState (no unlocked copy); the
            // form clears immediately on submit either way.
            let passphrase = state.send_passphrase.take_trimmed();
            let mnemonic   = state.send_seed_text.take();
            let bip39      = state.send_bip39.take_trimmed();
            let recipient  = state.send_recipient.clone();
            let amount     = state.send_amount.clone();
            // Manual tag is meaningful only for the r-address path; construct_blob
            // ignores it when the recipient is an X-address.
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
            // Back to the compose screen (the form was cleared on dispatch).
            // `send_error` is not drawn there — this surface reports nothing,
            // the activity log does — so this is only the landing.
            state.send_error = Some(e);
            state.send_step = 1;
        }
        Message::SendContinueClicked => {
            // `Sign transaction` on the one-screen compose: everything the old
            // steps 1 and 2 checked, at once — the SAME facts the view lights
            // the CTA on. Passing opens the sign stack (step 2).
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
            // Paying yourself burns a fee to move money nowhere.
            let (_, own, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
            if own.as_deref() == Some(resolved.classic.as_str()) {
                state.send_error = Some("ERR: SELF_SEND".to_string());
                return Task::none();
            }
            // A manually-entered tag only applies to a plain r-address and
            // must fit in a u32; an X-address carries its own tag.
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
                // The engine's pay/receive pair flips; the ticket's amount
                // (base) and total (quote) stay where they are, so the strings
                // swap along with the assets and the anchor follows the
                // number the user typed. The price is quote-per-base on both
                // sides, so it stays as typed.
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
            // The field's unit is the anchor in the ticket's words: the
            // side you type in is the exact side. Flipping it promotes the
            // derived twin to the typed one — its formatted string stands as
            // what was typed, exactly as Coinbase converts in place — and the
            // old typed side becomes the twin on the next Sync.
            state.trade_anchor = match state.trade_anchor {
                TradeAnchor::Pay => TradeAnchor::Receive,
                TradeAnchor::Receive => TradeAnchor::Pay,
            };
            state.trade_error = None;
            refresh_trade_twin(state);
        }
        Message::TradeMaxClicked => {
            // The ticket's pay side is `trade_pay_asset` whichever way the
            // ticket is facing. XRP: what `trade_fits` will accept — the
            // available less the fee and, if this order can rest, the
            // reserve it would lock. A token: the whole balance as the
            // ledger holds it; the derived side then follows on Sync.
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
            // The ticket's pair, placed into the engine's orientation by the
            // current side. A new market starts a new order: amounts, price
            // and mode all reset, the anchor back on the amount field — which
            // is the engine's pay side under Sell and its receive side under
            // Buy.
            let side = trade_default_side(&base, &quote);
            state.trade_side_chosen = false;
            orient_trade_pair(state, side, &base, &quote);
            state.trade_amount = String::new();
            state.trade_receive = String::new();
            state.trade_limit_price = String::new();
            // Back to the ticket's opening contract, not to no contract:
            // see `AppState::trade_contract`.
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
            // Under Limit a clicked level is a chosen price. Under Market
            // the book is inert (spec §14.6): the click is GATED on the
            // mode, never allowed to change it.
            if trade_is_limit(state) {
                state.trade_limit_price = val;
                state.trade_error = None;
            }
        }
        Message::TradeContinueClicked => {
            // One screen plus a stack: 1 = the ticket, 2 = the sign stack over
            // it. Leaving the ticket is where the order is checked and its
            // bound frozen.
            if state.trade_step == 1 {
                // The asset picker restricts *which* asset you can pay (only held,
                // trustlined assets show), but not *how much* — gate the amount
                // against the available balance here so an over-typed order can't
                // proceed to sign only to fail on-ledger (tecUNFUNDED_OFFER).
                // A Market order leaves step 1 only with the ledger's answer
                // in hand — the review shows that answer, and there is no
                // other honest number to show.
                // The contract is the user's and stays theirs: a Market
                // ticket the ledger can't be asked about waits, it is never
                // rewritten as a Limit ticket.
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
                // The live limit, checked here so a priceless ticket can't
                // leave step 1 — but never frozen: the price on the review
                // and sign steps keeps following the book, and submit reads
                // it again at the moment of signing.
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
                // Nothing is struck here. The Market quote stays LIVE on the
                // sign stack; the price is chosen when Broadcast is pressed.
                state.trade_error = None;
                    // Step 2 is the sign stack over the ticket — there is no
                // review step; the ticket's own stats are the review.
                state.trade_step = 2;
            }
        }
        Message::TradeSubmitClicked => {
            // Gated by the submit button; correctness is arbitrated downstream.
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
            // The price is chosen HERE, at the press, off the live book — the
            // engine's walked floor with its cushion, as the sign step was
            // showing at that instant. IOC: whatever still crosses when it
            // lands fills, the rest drops; it never fills worse than this.
            // Checked BEFORE the credentials are taken: a priceless ticket
            // must not eat them.
            let limit = trade_effective_limit(state);
            if limit <= 0.0 {
                state.trade_error = Some(trade_no_price(state).to_string());
                return Task::none();
            }
            let terms = trade_terms(state, limit);
            // Re-checked HERE, not just at the step-1 Continue: balance and
            // owner count move while the ticket is open, and the broadcast path
            // used to guard `limit <= 0.0` and nothing else.
            if !trade_fits(state, terms.pay) {
                state.trade_error = Some(
                    format!("ERR: INSUFFICIENT_FUNDS // MAX: {:.6}", trade_available(state).unwrap_or(0.0))
                );
                return Task::none();
            }

            // Hold on to what this order was signed against. The bound is
            // computed right here and nowhere else, and without this it is
            // gone before the ledger has a chance to disagree with it — see
            // `channel::PendingTrade`. The signing path completes it with the
            // blob's ledger bounds and persists the pair.
            let walk = trade_book_quote(state);
            let _ = CHANNEL.pending_trade_tx.send(Some(crate::channel::PendingTrade {
                // The ENGINE pair, pay → receive. Not the trader-facing market
                // pair: a measurement joined on the wrong orientation compares
                // a rate with its own reciprocal.
                pair: format!("{}/{}", state.trade_pay_asset, state.trade_receive_asset),
                anchor: match state.trade_anchor {
                    TradeAnchor::Pay => "pay",
                    TradeAnchor::Receive => "receive",
                }
                .to_string(),
                bound: limit,
                // Only under a quote we actually walked. A typed limit has no
                // expectation attached to it.
                expected_vwap: if trade_is_market(state) { walk.as_ref().map(|q| q.vwap) } else { None },
                // The ledger the walk was taken from, falling back to the last
                // validated index the app has — never zero silently.
                // The ledger the walk was taken from — the book frame's own
                // index, which is what the bound was actually derived against.
                // Falls back to the last validated index the app has, never
                // zero silently.
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
            // A whole-balance pay is signed with the ledger's own digits
            // (see `trade_terms`); `amount_string` would round it back to
            // six places on the way to the blob.
            let pay_amount = if trade_pays_whole_balance(state) { exact_amount(terms.pay) } else { amount_string(terms.pay) };
            let receive_amount = amount_string(terms.receive);
            let mut flags = trade_tif(state);
            // "Pay" means pay: sell the whole TakerGets at the ratio or
            // better. Without the flag the ledger stops once TakerPays is
            // received, which is the `Receive` anchor's meaning, not this one's.
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
        // DISPATCHED — not filled, not even submitted to the ledger yet. This
        // fires when the async hand-off to the socket task returns; what the
        // ledger did with the order arrives later, on the relay's own reply,
        // and lands in the activity log (see `ws::commands::submit_transaction`).
        //
        // The form is emptied HERE and not a line earlier. It used to be
        // cleared before `Task::perform` was even called, which meant any
        // outcome that did come back was rendered over a blank ticket — the
        // amounts it was talking about had already been thrown away.
        Message::TradeCompleted => {
            // The pair survives the clear: the next order on the grid's
            // ticket is almost always on the same market, and re-picking it
            // after every trade is a chore. Everything else is a fresh order.
            clear_trade_form_after_order(state);
            state.xrp_view = XrpView::Menu;
            trade_grid_init(state);
        }
        Message::CancelOrderClicked(seq) => {
            // Arm the offer; the cancel card floats as a modal over the history
            // list (no page swap), keyed by `cancel_offer_sequence`.
            clear_cancel_form(state);
            state.cancel_offer_sequence = Some(seq);
        }
        Message::CancelDismissed => {
            clear_cancel_form(state);
        }
        Message::CancelSubmitClicked => {
            // Inputs are gated by the submit button (disabled until valid), so no
            // passphrase / word-count validation is needed here.
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
            // The cancel modal already closed when the form cleared on submit;
            // stay on the history list so the offer's new status is visible.
        }
        Message::ReimportNext => {
            match state.reimport_step {
                // Phrase → 25th word, on the same gate the Continue button
                // draws itself dead under — checked here too because Enter in
                // the phrase field fires this. Whether the phrase opens THIS
                // wallet is still arbitrated downstream, at submit.
                1 if state.reimport_seed_text.as_str().split_whitespace().count() == 24
                    && crate::ui::components::wallet_setup::mnemonic_checksum_ok(
                        state.reimport_seed_text.as_str(),
                    ) =>
                {
                    state.reimport_error = None;
                    state.reimport_step = 2;
                }
                // 25th word → encryption key. `Yes` with nothing typed is
                // someone who forgot, and silently proceeding as `No` is the
                // one thing that derives the wrong wallet.
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
            // One step back, keeping entries so the user can review rather than
            // re-type. Never below step 1: the back chevron there fires
            // `KeyMgmtRestoreToggled` instead, which leaves the flow AND wipes
            // the buffers.
            state.reimport_error = None;
            state.reimport_step = state.reimport_step.saturating_sub(1).max(1);
        }
        Message::ReimportWord25Chosen(yes) => {
            state.reimport_word25 = yes;
            if !yes {
                // `No` means no: a word typed under `Yes` and then disowned
                // must not survive to derive the wallet.
                state.reimport_bip39.clear();
                state.reimport_bip39_reveal = false;
            }
        }
        Message::ReimportSpinnerTick => {
            // 7° per 16ms frame ≈ 0.82s per turn — the mock's ~0.85s spinner.
            state.reimport_spinner_angle = (state.reimport_spinner_angle + 7.0) % 360.0;
        }
        Message::ReimportConfirm => {
            // The restore action's own gate, re-checked here because Enter on
            // the key field fires this too: 24 words with a valid checksum and
            // a key at the import floor. Whether the phrase opens THIS wallet
            // is arbitrated downstream, by the address check in the bridge.
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
            // Freeze the gauge before the key it measures leaves.
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
            // Pop the whole restore stack back to the key-management page — the
            // channel now reports the key present, so the page renders its
            // Standard face. The quick cut IS the feedback.
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
            // Entering or leaving the restore face wipes whatever was typed. A
            // phrase sitting in a buffer behind a closed face is a secret the
            // user believes they have already put away.
            clear_reimport_form(state);
            state.key_mgmt_restoring = !state.key_mgmt_restoring;
            // The form always opens on its first step. `reimport_step` is the
            // restore step the old wizard already kept; the panel reads it and
            // `ReimportNext`/`ReimportBack` still drive it.
            state.reimport_step = 1;
        }
        _ => {}
    }
    Task::none()
}

/// The Kraken fiat index for the selected pair, in quote-per-base. DISPLAY
/// ONLY — a depeg reference line and fiat estimates. It never prices, anchors,
/// or gates an order: the on-ledger book is the only executable price, and on
/// a genuine depeg the book is right and the index is the liar. 1.0 for a
/// same-asset pair, 0.0 when a rate isn't available yet.
pub fn trade_index_rate(state: &AppState) -> f64 {
    let (mb, mq) = trade_market_pair(state);
    let b = crate::utils::tokens::rate_key(mb).to_string();
    let q = crate::utils::tokens::rate_key(mq).to_string();
    if b.is_empty() || q.is_empty() {
        return 0.0;
    }
    if b == q { 1.0 } else { crate::utils::price::cross(&b, &q) }
}

/// The ledger's answer to the ticket as it stands, as a [`Quote`] in the
/// engine's pay/receive orientation — or `None` when the pair has no book we
/// can price against, when the frame has fallen behind (see [`trade_frame_ok`])
/// or when the walk finds no depth at all.
///
/// Before an amount is typed there is nothing to walk: the ladder's touch
/// stands in, with nothing filled, so the price line has a number.
/// Once an amount is typed the numbers are the pair's own book, CLOB levels and
/// pool consumed together in `utils::orderbook::walk` — one clock, the rates
/// frame, no node round trip (spec §12).
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

/// Whether a book frame read at `ledger` is close enough to the live ledger to
/// price an order from. rates replays its last stash the instant a client
/// subscribes, and on a pair nobody has held recently that stash can be far
/// older than a ledger — with no gate, the first quote on a cold pair is priced
/// off a stale book and nothing on screen says so.
///
/// The frame's index is the OPEN ledger and ours is the VALIDATED one, so the
/// frame legitimately runs AHEAD: only falling behind is a fault. Zero on
/// either side means we cannot judge, and an unjudgeable frame is not a
/// licence to sign against it.
fn trade_frame_ok(frame: u64) -> bool {
    let live = CHANNEL.xrp_node_rx.borrow().ledger_index.unwrap_or(0);
    if frame == 0 || live == 0 {
        return false;
    }
    frame + FRAME_STALE_LEDGERS >= live
}

/// How far behind the validated ledger a book frame may fall and still price an
/// order. Two ledgers is ~6-10 s: enough to ride out one missed doorbell and
/// the fetch behind it, far short of the minutes a cold pair's stash can carry.
const FRAME_STALE_LEDGERS: u64 = 2;

/// Whether the Market contract EXISTS for this pair — the static half of
/// [`trade_quotable`], and what lights the Market segment. XRP-leg pairs
/// only (§3.4): a token/token pair has no single book and no single pool,
/// and its synthetic composition is wrong by percent-scale, so it is
/// Limit-only. An issuer that has frozen or gated its line withholds it
/// too; unknown (no frame yet) is permissive, as everywhere in this file.
///
/// Nothing dynamic lives here — not the frame's age, not the book's
/// measured depth. Those change per ledger, and a segment that follows them
/// flickers (user, 2026-09-15: three flips in a session, a stutter each);
/// they darken the BUTTON and are named on the ticket's `Market` row
/// ([`trade_market_verdict`]) instead.
pub fn trade_market_offered(state: &AppState) -> bool {
    let (pay, recv) = (state.trade_pay_asset.as_str(), state.trade_receive_asset.as_str());
    if pay.is_empty() || recv.is_empty() || (pay != "XRP" && recv != "XRP") {
        return false;
    }
    !crate::utils::orderbook::oriented(pay, recv).is_some_and(|(b, _)| trade_issuer_restricted(&b))
}

/// Whether the ticket can be priced by us RIGHT NOW — the engine's question.
/// [`trade_market_offered`], and a book that resolves, has a touch and is
/// current; the fiat cross is NOT a fallback, because an index price is a
/// reference and not something anyone can trade at.
///
/// **Measured depth no longer gates this (user, 2026-09-16).** It used to —
/// `liquidity::tradeable`, the windowed classification, stood in front of
/// the walk and withheld Market on any book that read `illiquid`. That is a
/// statistic deciding for the user what the walk itself can measure exactly
/// for their size, and it turned the Market segment into a thing that
/// enables and disables itself on the book's mood. The rule now: **warn,
/// never decide.** The walk runs on any XRP-leg book; what it finds — how
/// much of the size it covers, how far the bound sits from the mid — is
/// printed on the ticket in amber where it should give pause, and the only
/// holds left are the ones where there is no number to sign at all
/// ([`trade_bound_sane`]).
pub fn trade_quotable(state: &AppState) -> bool {
    if !trade_market_offered(state) {
        return false;
    }
    let (pay, recv) = (state.trade_pay_asset.as_str(), state.trade_receive_asset.as_str());
    crate::utils::orderbook::oriented(pay, recv)
        .is_some_and(|(b, _)| crate::utils::orderbook::touch(&b).is_some() && trade_frame_ok(b.ledger))
}

/// The oriented book's touch for this ticket — the numerator of the cushion's
/// slope term. `0.0` when there is no book, which collapses the slope to zero
/// and leaves the cushion at `delta_min`.
fn trade_touch(state: &AppState) -> f64 {
    crate::utils::orderbook::oriented(&state.trade_pay_asset, &state.trade_receive_asset)
        .and_then(|(b, _)| crate::utils::orderbook::touch(&b))
        .unwrap_or(0.0)
}

/// The pair's effective TickSize, as rates read it off the issuer.
fn trade_tick_size(state: &AppState) -> Option<u32> {
    crate::utils::orderbook::oriented(&state.trade_pay_asset, &state.trade_receive_asset)
        .and_then(|(b, _)| b.tick_size)
}

/// Whether the token's issuer has put the order out of reach before it is even
/// priced: `lsfRequireAuth` (the line must be authorised) or `lsfGlobalFreeze`
/// (every line frozen). This is what replaces the per-order `tec` pre-flight
/// that went with the node dry run — the one class of failure no amount of
/// price checking catches (spec §12).
///
/// **Unknown is permissive.** rates asks once per connection, so `None` means
/// the answer has not landed yet, and a market must not black out because a
/// reply is in flight. Only a known `true` gates. The cost of being wrong this
/// way is one fee, which is the trade §12 explicitly accepted.
pub fn trade_issuer_restricted(book: &crate::channel::OrderBook) -> bool {
    book.require_auth == Some(true) || book.global_freeze == Some(true)
}

/// The amount on the typed side, as a number.
fn trade_typed_amount(state: &AppState) -> f64 {
    let s = match state.trade_anchor {
        TradeAnchor::Pay => &state.trade_amount,
        TradeAnchor::Receive => &state.trade_receive,
    };
    s.trim().parse::<f64>().unwrap_or(0.0)
}

/// The server books to hold: **every** XRP leg in the registry, held from the
/// moment an XRP wallet exists and dropped the moment it stops existing. The
/// wallet is the only gate — not the picked pair, not the open tab.
///
/// This deliberately reverses the earlier per-pair opt-in ("books are the
/// bandwidth"). Subscribing at pick time made the pane jitter (user,
/// 2026-09-09): rates replays its last stash the instant a client subscribes,
/// and on a pair nobody has held recently that stash is old
/// (`utils::orderbook`), so picking a market drew an empty book, then a stale
/// one, then the live frame a ledger later. Pre-warmed, the pane is current
/// before it is ever looked at. The cost is the registry's books running for
/// every XRP install; that was weighed and accepted.
///
/// Empty with no wallet — which is what makes removal drop everything.
fn wanted_books() -> Vec<String> {
    if !xrp_wallet_exists() {
        return Vec::new();
    }
    crate::utils::tokens::TOKENS
        .iter()
        .map(|t| format!("XRP/{}", t.code))
        .collect()
}

/// Whether an XRP wallet is imported on this device — the grid's book gate.
fn xrp_wallet_exists() -> bool {
    CHANNEL.wallet_balance_rx.borrow().1.is_some()
}

/// The pane grid's ticket pane has no `ShowTrade` to open it. It opens on
/// NO market (user, 2026-09-09: a default pair would promote one stablecoin
/// over another — the pane is the search until one is picked), on the
/// compose step every Sync refreshes the twin on. Called after every grid
/// message and every Sync on the grid.
///
/// With no wallet it still reconciles before returning: `wanted_books` is
/// empty there, so this is what releases the books when the wallet goes away.
/// Remove wallet has no unsubscribe of its own — it clears the channel and
/// routes to Menu — and neither does erase-all, so the drop has to happen on
/// the next Sync or not at all.
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

/// Keep the rates socket's book subscriptions equal to `wanted_books`:
/// subscribe what's newly wanted, drop what no longer is. A reconciler, not
/// an event — nothing subscribes on import and nothing unsubscribes on
/// removal; both are just a want-set that changed by the next Sync. The
/// socket task remembers the set across reconnects, which is also why a leak
/// here outlives the connection: `ws::socket` replays the held set on every
/// book-link rise.
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

/// The two amounts an order is made of, at a given limit.
#[derive(Debug, Clone, Copy)]
pub struct TradeTerms {
    /// TakerGets — what is sold. Exact under `Pay`; a `≤` ceiling under
    /// `Receive`, rounded **up** so the signed ratio stays at or under the
    /// limit and the whole walk remains crossable.
    pub pay: f64,
    /// TakerPays — what is received. Exact under `Receive`; a `≥` floor under
    /// `Pay`, rounded **down** so the promise is never more than the ledger
    /// guarantees.
    pub receive: f64,
}

/// Places the ledger is asked for. Six is a drop of XRP and well inside a
/// token's fifteen significant digits at any amount this wallet moves.
pub const SIGN_PLACES: usize = 6;

/// Resolve the typed side and derive the other from `limit`. The view passes
/// the live limit, and so do review and submit — nothing is frozen,
/// so what is shown at review is what is signed. Both numbers come back at
/// signing precision — the view rounds them further for display, never the
/// other way round.
pub fn trade_terms(state: &AppState, limit: f64) -> TradeTerms {
    use crate::utils::orderbook::{ceil_to, floor_to};
    match state.trade_anchor {
        TradeAnchor::Pay => {
            let typed = state.trade_amount.trim().parse::<f64>().unwrap_or(0.0);
            // The whole balance goes as the ledger holds it: flooring
            // 0.004127260319475 to six places would sell 0.004127 and leave
            // a second tail behind the first.
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

/// Whether the pay field holds the pay asset's entire balance — `max ›`, or
/// the same digits typed by hand. Derived from the field, never a flag: the
/// string IS the balance, and a keystroke that changes it changes the
/// answer. Only a token can say yes; an XRP max is already at the drop and
/// the floor is the identity on it.
pub fn trade_pays_whole_balance(state: &AppState) -> bool {
    let pay = state.trade_pay_asset.as_str();
    if pay == "XRP" || state.trade_anchor != TradeAnchor::Pay {
        return false;
    }
    let balance = CHANNEL.token(pay).0;
    balance > 0.0 && state.trade_amount.trim().parse::<f64>().ok() == Some(balance)
}

/// Places shown for a *typed* amount: XRP to the drop, every stablecoin to
/// the cent.
pub fn trade_decimals(code: &str) -> usize {
    if code == "XRP" { 6 } else { 2 }
}

/// Places shown for a *derived* amount — a ceiling or a floor computed from
/// the limit. Four for a stablecoin, not two: a Market ceiling of 1.461106
/// rounded up to the cent read as 1.47 on a 1.4538 fill (2026-08-27), a full
/// cent of the 0.5% pad's own size, and the digits that were thrown away
/// were the ones that said so.
pub fn trade_derived_decimals(code: &str) -> usize {
    if code == "XRP" { 6 } else { 4 }
}

/// What the derived side will most likely come to: the depth walk's own
/// numbers — each crossed level at its maker's price — with any uncovered
/// remainder priced at `limit`. `None` when there is nothing to expect apart
/// from the limit itself: a Limit order, or a book that can't quote. The
/// signed number is the limit-derived one from [`trade_terms`]; this is
/// what the ticket headlines for a Market order, the way a desk shows an
/// estimated total. With no tolerance the two agree whenever the amount fits
/// the top level; they differ only on a deep walk, where the signed limit is
/// the deepest level and the estimate the VWAP above it.
pub fn trade_expected_price(state: &AppState, limit: f64) -> Option<f64> {
    let x = trade_expected(state, limit)?;
    let (pay, recv) = if trade_fill_now(state) {
        // The fill's own ratio: dividing by the typed size would price the
        // part that never fills.
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

/// See [`trade_expected_price`] for the same estimate as a price.
pub fn trade_expected(state: &AppState, limit: f64) -> Option<f64> {
    if !trade_is_market(state) || limit <= 0.0 {
        return None;
    }
    let q = trade_book_quote(state)?;
    // The walk is what fills now, at this size, on this frame. Only a GTC
    // order gets its remainder priced at the limit: under IOC the rest is
    // dropped and under FOK it kills the order, so for those the estimate is
    // the fill itself — not "as if the rest filled at the floor".
    let terms = trade_terms(state, limit);
    Some(expected_amount(
        state.trade_anchor,
        trade_fill_now(state),
        (q.filled, q.receive),
        (terms.pay, terms.receive),
        limit,
    ))
}

/// The estimate's arithmetic, without the ticket around it.
///
/// `walk` is what the depth walk says crosses right now — `(filled, receive)`.
/// `want` is what the ticket asked for. Split out and pure because the
/// invariant it carries is the one an always-IOC market contract stands on:
/// **when nothing can rest, the estimate is the walk and never more than the
/// walk.** An estimate above what the book will actually deliver is a promise
/// the ledger has already declined to make, and on a depth-short ticket it is
/// the exact number a user would size their order against.
///
/// The other arm is deliberately allowed to exceed the walk: a GTC order's
/// uncovered remainder does not vanish, it rests, and pricing it at the limit
/// is what it will fill at if it fills. The difference between the two arms is
/// the TIF and nothing else.
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
        // IOC and FOK: the walk already IS everything that fills. The rest is
        // dropped (IOC) or kills the order (FOK) — never "as if it filled at
        // the floor".
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

/// IOC or FOK: the order fills now or not at all; nothing rests. Reads the
/// DERIVED tif, so a Market ticket is always fill-now however the tier row was
/// last left.
fn trade_fill_now(state: &AppState) -> bool {
    !trade_can_rest(state)
}

/// Keep the derived side's field in step with the typed side — called on
/// every `Sync` while the order step is open, so a Market Order's twin
/// follows the book tick by tick and a Limit Order's follows the typed
/// price. Only the *derived* field is ever written: typing into it flips
/// the anchor first (`after_plain_edit`), after which it is the source and
/// the other field is the one rewritten. Rounded for display the way its
/// bound points (`≥` down, `≤` up); the signed numbers come from
/// [`trade_terms`] off the typed side and never read this text.
pub fn refresh_trade_twin(state: &mut AppState) {
    use crate::utils::orderbook::{ceil_to, floor_to};
    let limit = trade_effective_limit(state);
    let terms = trade_terms(state, limit);
    // A Market order's twin shows the book's estimate, not the ceiling —
    // the ceiling is the review's footnote. A Limit order's is the limit's.
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

/// An amount as the ledger wants it written: at most [`SIGN_PLACES`], no
/// trailing zeros, no dangling point.
fn amount_string(x: f64) -> String {
    let raw = format!("{:.*}", SIGN_PLACES, x);
    raw.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The order's time-in-force, **derived** — never read back off the tier row.
///
/// Market is always immediate-or-cancel: we own the bound (§1), and a bound we
/// chose must not leave a remainder resting at the worst rate of its own sweep.
/// Limit is the user's genuine choice (§2), so it is whatever the tier row says.
///
/// Derivation, not defaulting, is what closes the carryover bug: `trade_flags`
/// survives both `TradeMarketSelected` and `TradePairSelected`, so a ticket set
/// to Limit+FOK and switched to Market used to sign a **FOK market order** —
/// exactly the combination §3 calls unsafe. Reading the row only under Limit
/// makes that unreachable rather than merely reset.
pub fn trade_tif(state: &AppState) -> Vec<String> {
    if trade_is_market(state) {
        vec!["tfImmediateOrCancel".to_string()]
    } else {
        state.trade_flags.clone()
    }
}

/// Whether this ticket is under the Market contract AND the book can be
/// walked: the engine's question. A Market ticket over a book that cannot
/// quote is not a Limit ticket — it is a Market ticket with no price, and
/// nothing signs until the book comes back or the user picks Limit.
pub fn trade_is_market(state: &AppState) -> bool {
    state.trade_contract == Some(TradeContract::Market) && trade_quotable(state)
}

/// Whether the user picked Limit. Only their click says so; the book never
/// does (2026-09-15).
pub fn trade_is_limit(state: &AppState) -> bool {
    state.trade_contract == Some(TradeContract::Limit)
}

/// Whether the order can leave a resting remainder — the only case that owes an
/// extra owner reserve. IOC drops its remainder and FOK kills the order, so
/// neither ever rests; GTC does.
pub fn trade_can_rest(state: &AppState) -> bool {
    !trade_tif(state).iter().any(|f| f == "tfImmediateOrCancel" || f == "tfFillOrKill")
}

/// How much of the walked touch-to-floor gap becomes cushion. The slope
/// self-scales across book shapes (§3.3): a steep book means a small
/// displacement costs a lot, so it earns more room; a flat book needs almost
/// none.
///
/// **UNMEASURED, and knowingly so.** §3.3 settles the *shape* and leaves the
/// size open, and §14.5's drift-to-short-fill measurement needs realized fills
/// we do not capture yet — so this cannot be calibrated today rather than
/// merely not having been. Half the gap is the deliberate starting point: it
/// tolerates a counterparty taking about half your depth before the fill
/// truncates, and the cost of being under is one fee plus a re-quote, not a
/// burned gas fee.
const CUSHION_ALPHA: f64 = 0.5;

/// Hard ceiling on the cushion. Past this we would be authorising a genuinely
/// bad fill rather than absorbing drift, and the honest answer is to let the
/// order miss and re-quote.
const CUSHION_MAX: f64 = 0.01;

/// The floor under the cushion, relative. Two things set it:
///
/// - **One tick at the pair's effective TickSize.** `Quality::round()` is a
///   mantissa CEILING applied before the crossing threshold, so the bound the
///   engine enforces is deterministically stricter than what we walked; a
///   cushion thinner than one tick is eaten by that rounding alone. A quality
///   kept to `n` significant digits has relative granularity up to `10^-(n-1)`.
///   Live values 2026-08-31: XSGD 6, BBRL 5, the other three unset.
/// - **1e-7 regardless**, below which a cushion buys nothing: the engine
///   already accepts a realized quality up to 1e-7 under the limit.
fn trade_delta_min(tick_size: Option<u32>) -> f64 {
    let by_tick = tick_size
        .filter(|n| *n >= 1)
        .map(|n| 10f64.powi(-((n as i32) - 1)))
        .unwrap_or(0.0);
    by_tick.max(1e-7)
}

/// The signed bound: the walked floor, moved to the loose side by the cushion.
///
/// **Direction is the same for both anchors.** The limit is always
/// receive-per-pay in the engine's orientation, and `trade_terms` uses it as
/// `receive = pay × limit` (Pay anchor) or `pay = receive / limit` (Receive
/// anchor) — so a LOWER limit accepts less receive in one case and pays more in
/// the other. Both are the loosening direction, so both subtract.
fn trade_cushioned(floor: f64, touch: f64, tick_size: Option<u32>) -> f64 {
    if floor <= 0.0 {
        return 0.0;
    }
    let slope = if touch > 0.0 { ((touch - floor) / touch).max(0.0) } else { 0.0 };
    let d = (CUSHION_ALPHA * slope).clamp(trade_delta_min(tick_size), CUSHION_MAX);
    // Down, and rounded down again: the bound is a `>=` promise and the ledger
    // keeps nine significant digits.
    crate::utils::orderbook::floor_sig(floor * (1.0 - d), 9)
}

/// How far (percent) a Market bound may sit from the market's windowed mid
/// before the ticket calls it out. The cushion is at most 1%, a deep walk
/// on a thin book a few more; a hole's bound is ten-fold off. The ledger of
/// a one-maker cycle where the asks are junk would otherwise sign a bound
/// of 40 AUDD per XRP against a fair 2 (measured 2026-09-15).
pub const BOUND_GUARD_PCT: f64 = 5.0;

/// Where a walked bound sits against the market of the last ~20 ledgers:
/// signed percent from the windowed mid, in the trader's orientation
/// (quote per base), so a sell's deep walk reads negative and a buy's
/// positive. `None` when there is no mid to compare with, or no bound.
pub fn trade_bound_vs_mid(state: &AppState, bound: f64) -> Option<f64> {
    let (base, quote) = trade_market_pair(state);
    let m = crate::utils::liquidity::market(base, quote)?;
    if m.mid <= 0.0 || bound <= 0.0 {
        return None;
    }
    Some((trade_market_price(state, bound) - m.mid) / m.mid * 100.0)
}

/// Whether a walked bound is a price at all — the one hold left on a
/// Market ticket besides having no frame.
///
/// Two things put a bound far from the mid, and they get opposite
/// treatment (user, 2026-09-16: *warn them, never decide for them*):
///
/// - **A deep walk on a still book.** The size eats 6% of a thin book, and
///   6% below mid IS the price for that size. Signed if the user wants it;
///   the ticket's `Market` row prints the number in amber and the button
///   stays lit. Blocking this was the paternalism — someone dumping into a
///   dust book, or converting the last 0.004 of a line to zero it, was
///   refused a trade the ledger would have honoured exactly as shown.
/// - **A hole on an unstable book.** The one maker is gone THIS ledger and
///   a junk ask is the whole book; the bound is ten-fold off and comes back
///   next ledger. That number is an artifact, not a price — signing it is
///   signing garbage, and refusing it is not blocking a trade, it is
///   refusing a number the market of the last twenty ledgers has never
///   seen. Held for the ledger; the row says so.
///
/// `stable` — holes ≤ 1 in the window — is what tells them apart, and it is
/// already measured. No mid to compare with is a refusal, not a licence.
pub fn trade_bound_sane(state: &AppState, bound: f64) -> bool {
    let (base, quote) = trade_market_pair(state);
    let Some(m) = crate::utils::liquidity::market(base, quote) else { return false };
    match trade_bound_vs_mid(state, bound) {
        Some(d) => d.abs() <= BOUND_GUARD_PCT || m.stable,
        None => false,
    }
}

/// The cushioned bound for a walk — the number a Market ticket signs,
/// before [`trade_bound_sane`] has its say. One place, so the button's
/// price and the `Market` row's readout are the same arithmetic.
fn trade_market_bound(state: &AppState, q: &crate::utils::orderbook::Quote) -> f64 {
    trade_cushioned(q.floor, trade_touch(state), trade_tick_size(state))
}

/// The Market contract's standing for this ticket, as the `Market` stats
/// row prints it — every reason the button is dark, and every number that
/// should give pause while it is lit, in one place so the row and the
/// button cannot disagree (both read [`trade_effective_limit`]'s inputs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarketVerdict {
    /// No XRP leg (or a restricted issuer): the contract does not exist for
    /// this pair. The segment is dead and this is what says why.
    LimitOnly,
    /// bookd has not summarised the pair yet — no windowed mid to judge a
    /// bound against.
    Measuring,
    /// No book frame current enough to walk (`trade_frame_ok`). Transient
    /// by construction since 2026-09-16: bookd re-reads every book every
    /// ledger, so this is the second or two after a pair is picked, or a
    /// bookd that is genuinely behind — never a quiet book.
    NoFrame,
    /// Nothing typed: nothing to walk yet.
    Idle,
    /// A book with nothing on the side this order takes.
    NoDepth,
    /// The walk for the typed size. `coverage_pct` is how much of it the
    /// book fills (an IOC drops the rest); `bound_pct` is where the signed
    /// bound sits against the windowed mid, signed, trader's orientation;
    /// `held` is [`trade_bound_sane`] refusing it — the hole.
    Walked { coverage_pct: f64, bound_pct: f64, held: bool },
}

/// See [`MarketVerdict`]. The ticket prints it under Market, and under
/// Limit only the `LimitOnly` case — a Market-contract fault on a Limit
/// ticket reads as a Limit fault, and the price there is the user's.
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

/// How much of `want` a walk that reached `got` covers, as a percent
/// clamped to `0..=100` — the walk can overshoot by a rounding hair on the
/// last level and that is still a full fill.
pub(crate) fn coverage_pct(got: f64, want: f64) -> f64 {
    if want <= 0.0 {
        return 0.0;
    }
    (got / want * 100.0).clamp(0.0, 100.0)
}

/// Why a Market ticket has no price right now, for the error line: a walk
/// that exists but fails the bound guard — a hole on an unstable book — is
/// `OFF_MARKET`, everything else (no frame, no contract) is `NO_PRICE`.
fn trade_no_price(state: &AppState) -> &'static str {
    if state.trade_contract == Some(TradeContract::Market) && trade_book_quote(state).is_some() {
        "ERR: OFF_MARKET // the book has a hole this ledger, wait one"
    } else {
        "ERR: NO_PRICE"
    }
}

/// The limit price the order will sign with, as the form stands right now.
///
/// **Limit Order** — the typed price, verbatim.
///
/// **Market Order** — the *floor* of the walk ([`trade_book_quote`]): the
/// deepest rate the fill reaches across the CLOB and the pool, which is the
/// touch itself whenever the amount fits inside it. Not the mid: a sell posted
/// at mid crosses nothing and rests inside the spread waiting for a buyer, the
/// opposite of what "Market" promises. Crossing executes at each maker's own
/// price, so the realized fill is the walked average or better — the floor is
/// the guarantee, never the expectation.
///
/// Zero while the ledger hasn't answered, or answered that the order would
/// fail. The form shows the ticket as still asking, and the gate refuses to
/// leave step 1 without an answer or a typed limit.
///
/// Live on every step, including sign: what the screen shows at the moment
/// `Broadcast` is pressed is what is signed — no snapshot, no tolerance. A
/// book that moves after that fails the order (FOK), part-fills it (IOC) or
/// rests it (GTC); it can never fill worse than the number that was shown.
/// The user's contract (2026-08-27): "they want a failure, not a sudden
/// change in price." The wire's prices are rounded away from the taker on
/// the rates server so a limit at the shown price always crosses a book
/// that hasn't moved.
pub fn trade_effective_limit(state: &AppState) -> f64 {
    match state.trade_contract {
        // No contract at all — unreachable while the ticket opens on Limit,
        // and priced at nothing if it ever became reachable again.
        None => 0.0,
        Some(TradeContract::Market) => match trade_book_quote(state).filter(|_| trade_quotable(state)) {
            // The sign button is where a hole is stopped: a bound the
            // windowed mid does not recognise is no price at all.
            Some(q) => {
                let b = trade_market_bound(state, &q);
                if trade_bound_sane(state, b) { b } else { 0.0 }
            }
            // Market with no answer yet, or over a book that stopped being
            // walkable: no price — never a price typed under Limit earlier
            // and left behind.
            None => 0.0,
        },
        // Limit: the typed price, in the trader's orientation; the engine's
        // is the inverse on the buy side.
        Some(TradeContract::Limit) => {
            trade_internal_price(state, state.trade_limit_price.trim().parse::<f64>().unwrap_or(0.0))
        }
    }
}

/// The market pair as the ticket shows it: `(base, quote)` — the asset being
/// bought or sold, and what it is priced in. `Sell` pays the base, `Buy` pays
/// the quote; the engine's `trade_pay_asset` / `trade_receive_asset` are the
/// pay / receive pair underneath.
/// settings.json key for the XRP grid's pair — `{"base": "XRP", "quote": "RLUSD"}`
/// in the trader's orientation, the one the top bar prints.
const PAIR_KEY: &str = "xrp_pair";

/// Write the held pair (trader's orientation) beside the grid layout. One
/// click, one write — no settle window: a pick is already a decision.
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

/// No pair, as on first run: the engine's two assets, the amounts they
/// priced, the search, and the saved key. The books stay — they are gated on
/// the wallet, not the pair.
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

/// Put the saved pair back into the engine's orientation under the side the
/// app opens on. Only a market the registry can make is honoured — a token
/// that left the registry since the pair was saved leaves the grid on no
/// pair, the same as first run. Nothing else in the form is touched.
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

/// The side a fresh ticket opens on: the one the wallet can pay for. Holds
/// the quote → Buy, the convention; only the base → Sell; both or neither →
/// Buy. A wallet holding XRP and no RLUSD has exactly one trade on
/// XRP/RLUSD and it is a Sell — "buy my first token" on an XRP-base pair IS
/// a Sell, which is what the 09-05 "opens on Buy" rule meant and did the
/// opposite of (user, 2026-09-16).
pub fn trade_default_side(base: &str, quote: &str) -> TradeSide {
    if trade_can_pay(quote) {
        TradeSide::Buy
    } else if trade_can_pay(base) {
        TradeSide::Sell
    } else {
        TradeSide::Buy
    }
}

/// Whether the wallet holds any of `asset` to pay with: XRP past the fee,
/// a token above zero. Unknown balances read as nothing.
fn trade_can_pay(asset: &str) -> bool {
    if asset == "XRP" {
        xrp_available().is_some_and(|a| a > xrp_fee())
    } else {
        CHANNEL.token(asset).0 > 0.0
    }
}

/// Put the ticket's `(base, quote)` into the engine's orientation under
/// `side`, with the anchor on the base amount — the one way the side is
/// ever set alongside the pair. Setting `trade_side` on its own was the
/// bug: `clear_trade_inputs` reset it to Buy without swapping the pay /
/// receive pair, so closing the ticket pane on Sell reopened it on the
/// pair backwards (found 2026-09-16).
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

/// An untouched ticket follows the wallet: no click on it yet, nothing
/// typed, and the wallet can pay for the other side — flip. Runs on Sync so
/// a balance that lands a second after the app opens (the store push) is
/// honoured on the pair `restore_trade_pair` put back before it arrived.
/// The first click or keystroke ends it.
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

/// An engine price (receive per pay) in the trader's orientation (quote per
/// base). Identity on the sell side; the inverse on the buy side, where the
/// engine's "receive" is the base.
pub fn trade_market_price(state: &AppState, internal: f64) -> f64 {
    match state.trade_side {
        TradeSide::Sell => internal,
        TradeSide::Buy => if internal > 0.0 { 1.0 / internal } else { 0.0 },
    }
}

/// The other way: a trader's price into the engine's orientation. The same
/// inversion — it is its own inverse — kept as its own name so a call site
/// says which way it is going.
pub fn trade_internal_price(state: &AppState, market: f64) -> f64 {
    trade_market_price(state, market)
}

/// What the pay asset can spend: XRP less every reserve (owner reserve scales
/// with open trustlines and offers), a token in full.
pub fn trade_available(state: &AppState) -> Option<f64> {
    if state.trade_pay_asset == "XRP" {
        xrp_available()
    } else {
        Some(CHANNEL.token(state.trade_pay_asset.as_str()).0)
    }
}

/// Whether `amount` of the pay asset can actually be posted: XRP has to cover
/// the amount plus the network fee, a token only the amount (the fee is paid
/// in XRP either way, but a token order's XRP fee is a rounding error against
/// the reserve that already gates it). `max ›` lands exactly on this line.
/// Whether the wallet can actually afford this order — the amount, the fee, and
/// the owner reserve a RESTING order locks up.
///
/// Two holes this closes. The reserve was never counted at all, so a GTC that
/// rests could be signed with no headroom for the +1 owner count it creates
/// (`tecINSUF_RESERVE_OFFER`); it is charged only when the order can rest,
/// because IOC and FOK never leave one. And a token-pay order never checked XRP
/// headroom at all — it can hold plenty of the token and still not afford the
/// fee — which its own twin `send_fits` gets right sixty lines below.
pub fn trade_fits(state: &AppState, amount: f64) -> bool {
    let Some(r) = xrp_reserve() else { return false };
    let xrp_owed = xrp_fee() + if trade_can_rest(state) { r.per_object } else { 0.0 };
    if state.trade_pay_asset == "XRP" {
        amount + xrp_owed <= r.available
    } else {
        amount <= CHANNEL.token(state.trade_pay_asset.as_str()).0 && xrp_owed <= r.available
    }
}

/// Has the order in flight provably run out of ledgers?
///
/// Every blob is signed with a `LastLedgerSequence`, and once the validated
/// index passes it the transaction can never be included — by any validator,
/// under any network conditions. Paired with an unspent sequence (see the
/// proof below) that is a *terminal* fact, and the only negative outcome the
/// app can establish on its own: every other one arrives as an answer, and an
/// expiry is precisely the case where no answer comes.
///
/// **This is not the order's lifetime.** `LAST_LEDGER_OFFSET` is 20 ledgers,
/// about eighty seconds, and it bounds how long the signed blob stays eligible
/// for INCLUSION — nothing more. An IOC or FOK resolves inside the ledger it
/// applies in, a few seconds after signing; a GTC that is included then rests
/// indefinitely. Normal orders never come near this path. It exists for the
/// one case where the app would otherwise have to shrug: sent, and nothing
/// ever came back.
///
/// Until this existed the surface's last word on such an order was the
/// watchdog's "it may still complete" — true, weaker than what
/// was knowable, and permanent. The user was left unable to tell an order that
/// vanished from one that might still land, at the exact moment they need to
/// decide whether to re-send.
///
/// Nothing here re-sends anything. It states the outcome, records it beside
/// the quote, and releases the slot.
///
/// Takes no `AppState`: every fact it needs — the order in flight, the live
/// ledger index, the log — is on a channel, and the update it publishes comes
/// back through `ActivityChanged` like any other.
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
    // Strictly greater: `LastLedgerSequence` is the last ledger the
    // transaction MAY be included in, so equality is still live.
    if now <= u64::from(last_ledger) {
        return;
    }

    // ── The proof ────────────────────────────────────────────────────────────
    //
    // A passed ledger bound alone is NOT evidence the order died. It says our
    // answer did not arrive; the transaction may have been included and the
    // reply lost — a relay restart, a dropped socket. That case matters most
    // for a GTC, which on inclusion rests on the book indefinitely: telling
    // someone "expired, nothing was traded" about an order that is live and
    // holding their funds would be a far worse answer than the watchdog's
    // honest doubt, which is what stands if we say nothing here.
    //
    // A sequence is spent by exactly one transaction. If the account's next
    // sequence is STILL the one this blob was signed with, then nothing ever
    // spent it, and the order provably never entered a ledger. If it has moved
    // on, something landed and no expiry may be claimed.
    //
    // Note what is deliberately NOT done when the sequence has moved: the slot
    // is left armed. The transaction is in a ledger, so its answer can still
    // arrive on a reconnect and settle the record properly. Clearing here
    // would throw that away to tidy up.
    if CHANNEL.xrp_account_rx.borrow().sequence != Some(signed_seq) {
        return;
    }

    // The RECORD is written whatever is on screen. The ledger passed the bound;
    // that is true regardless of whether anyone is looking, and §14.5 needs the
    // row either way.
    if let Some(hash) = pending.hash.as_deref() {
        crate::bridge::order_record::settle(hash, "expired", None, None);
    }
    let _ = CHANNEL.pending_trade_tx.send(None);

    // The MESSAGE goes only into the order's own log, and this check is not
    // decorative. The watchdog stalls the step long before the ~80s
    // `LastLedgerSequence` horizon, the user reads it and clicks Done (there is
    // no auto-dismiss), and the slot was still armed. Without the title gate the
    // next flow to open a log — a payment, a trustline, an import — would have
    // its `init` step rewritten to "The order expired", because
    // `restate_failure` targets the first Active or Stalled step and on a fresh
    // flow that is the first one. Telling someone their payment expired because
    // an unrelated order did is the worst sentence this surface could produce.
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
        // Published rather than written straight into `state`, so the channel
        // stays the single copy: the update comes back as `ActivityChanged`,
        // and a terminal log is what retires the core's watchdog.
        let _ = CHANNEL.activity_tx.send(log);
    }
}

pub(crate) fn clear_trade_form(state: &mut AppState) {
    sync_trade_books(state);
    state.trade_pay_asset = String::new();
    state.trade_receive_asset = String::new();
    clear_trade_inputs(state);
}

/// [`clear_trade_form`] once an order is handed off, keeping the market: the
/// `(base, quote)` survives, put back under the side a fresh ticket opens on.
///
/// What survives is the market, never the raw pay / receive pair. This used
/// to save the pair, clear, and write it back, but the clear re-sides the
/// ticket, and the old pair under the new side reads backwards: every Sell on
/// XRP/RLUSD came back as RLUSD/XRP, on success and failure alike (found
/// 2026-10-04, shipped in 0.1.0 and 0.1.1). The same mistake as setting the
/// side alone, see [`orient_trade_pair`].
fn clear_trade_form_after_order(state: &mut AppState) {
    let (base, quote) = trade_market_pair(state);
    let (base, quote) = (base.to_string(), quote.to_string());
    clear_trade_form(state);
    orient_trade_pair(state, trade_default_side(&base, &quote), &base, &quote);
}

/// The ticket's own inputs and credential — everything [`clear_trade_form`]
/// empties **except the pair**, and without touching the book subscriptions.
///
/// The split exists because closing the ticket pane is not the same act as
/// finishing a trade (2026-09-12). The pair is a persisted, top-bar-level
/// choice with its own chip; `Reset` is deliberately the one way back to no
/// pair, via [`clear_trade_pair`]. Closing a pane must not quietly unset it,
/// and must not fire `UnsubscribeBook` at the rates socket — a view action has
/// no business moving a subscription.
pub(crate) fn clear_trade_inputs(state: &mut AppState) {
    state.trade_step = 0;
    state.trade_amount = String::new();
    state.trade_limit_price = String::new();
    state.trade_flags = Vec::new();
    state.trade_contract = Some(TradeContract::Limit);
    // The pair stays (see above), so it is re-oriented, never just re-sided.
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

/// What a signature can reach in XRP right now: the balance less every
/// reserve. Owner reserve scales with the number of open trustlines — count
/// every held line in the registry, not a fixed subset — and open offers.
pub fn xrp_reserve() -> Option<crate::utils::reserves::Reserve> {
    let total = CHANNEL.wallet_balance_rx.borrow().0;
    let account = *CHANNEL.xrp_account_rx.borrow();
    let node = CHANNEL.xrp_node_rx.borrow();
    crate::utils::reserves::chain_reserve(total, CHANNEL.xrp_exists(), account, &node)
}

/// What the account can actually spend: balance less the reserve the LEDGER
/// states, not one we compute from the token registry.
///
/// `None` until the node frame and the account's `owner_count` have both
/// landed, and every gate below closes for that window. The registry
/// arithmetic this replaced counted only the trustlines we know about and the
/// offers we can see as Pending — it could not see an escrow, a check, a
/// ticket, or an offer placed from another client on the same seed, so it
/// always UNDERcounted owned objects and always OVERstated the available
/// balance. The cost of that landed on `max ›`, which would round up to a
/// figure the ledger
/// then refused with `tecINSUFFICIENT_RESERVE` — after burning the fee.
pub fn xrp_available() -> Option<f64> {
    xrp_reserve().map(|r| r.available)
}

/// The open-ledger fee in XRP — what the send signs with. Zero until the
/// first node frame lands, which only ever makes the gate *looser* for the
/// seconds before the fee is known.
pub fn xrp_fee() -> f64 {
    CHANNEL.xrp_node_rx.borrow().open_ledger_fee.unwrap_or(0) as f64 / 1e6
}

/// Whether `amount` of the chosen asset can actually be sent: the amount plus
/// the fee out of the available XRP balance, or the token balance with the fee still
/// payable from XRP. `max ›` lands exactly on this line; a gate that ignored
/// the fee would light the button on a payment the ledger then refuses.
pub fn send_fits(state: &AppState, amount: f64) -> bool {
    // No reserve yet ⇒ no verdict. The button stays dark for the ledger or two
    // it takes the node frame to land, which is the safe direction.
    let Some(available) = xrp_available() else { return false };
    match state.xrp_token_tab {
        XrpTokenTab::Xrp => amount + xrp_fee() <= available,
        XrpTokenTab::Token(code) => amount <= CHANNEL.token(code).0 && xrp_fee() <= available,
    }
}

/// The per-field follow-up after a plain grid edit on the XRP send form: the
/// XRP cross **in the chosen base currency**, kept in both directions and only
/// while the asset is XRP — a token has no fiat twin on this screen, because
/// it already is one.
/// The chosen send asset in the base currency — XRP's cross, or the token's
/// own (`rate_key`, so RLUSD reads Kraken's rate, not 1). The fiat box crosses
/// on this whatever is being sent (user, 2026-09-15): someone asked for 20
/// euros wants to see it in their own currency, and even RLUSD against USD is
/// not always one to one.
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

/// Re-derive the send form's non-anchor amount at the live rate. Run on every
/// `Sync` and on an asset switch: the anchor is what the user typed, the other
/// field is a readout of it, and a readout that lags the chart beside it is
/// what a user notices. Nothing else moves — not the error line, not the
/// anchor — and an unpriced asset leaves both as they were rather than
/// blanking a field against a rate of zero. See `SendAnchor`.
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
        // The typed field is the anchor; the other is derived from it now and
        // on every rate tick after (`resync_send_twin`).
        PlainField::XrpSendAmount => {
            state.send_anchor = SendAnchor::Amount;
            derive_send_fiat(state, rate);
        }
        PlainField::XrpSendFiat => {
            state.send_anchor = SendAnchor::Fiat;
            derive_send_amount(state, rate);
        }
        // The trade card's two fields: nothing derives from them on edit — the
        // receive amount and the output lines are computed in the view — but
        // a keystroke retires whatever the gate last said.
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
        // The picker's query filters in the view; nothing to derive.
        PlainField::TradePairQuery => {}
        _ => {}
    }
}

/// A pane's three tag slots — the tag as set, whether its form is up, and
/// the form's draft — so the three tag messages are written once.
fn tag_slots(state: &mut AppState, pane: TagPane) -> (&mut String, &mut bool, &mut String) {
    match pane {
        TagPane::Send => (&mut state.send_destination_tag, &mut state.send_tag_editing, &mut state.send_tag_draft),
        TagPane::Receive => (&mut state.receive_tag, &mut state.receive_tag_editing, &mut state.receive_tag_draft),
    }
}

/// What the receive pane shows and copies: the classic address, or — with a
/// tag set — the same account as an X-address with that tag baked in, so
/// the two are one string a payer cannot separate. The view and the copy
/// handler both go through here so the QR, the line under it and the
/// clipboard can never disagree.
pub(crate) fn receive_address(state: &AppState, classic: &str) -> String {
    tag_of(&state.receive_tag)
        .and_then(|tag| dannesk_xrpl_codec::xaddress::encode(classic, Some(tag)))
        .unwrap_or_else(|| classic.to_string())
}

/// A tag buffer as a number: `None` when empty or too large for the ledger.
pub(crate) fn tag_of(buf: &str) -> Option<u32> {
    buf.trim().parse::<u32>().ok()
}

/// True when a tag draft holds something that is not a valid tag.
pub(crate) fn tag_wrong(buf: &str) -> bool {
    !buf.trim().is_empty() && tag_of(buf).is_none()
}

/// The receive pane's form buffers — NOT the tag itself, which is the
/// wallet's and outlives the pane (pane close, 2026-09-15).
pub(crate) fn clear_receive_form(state: &mut AppState) {
    state.receive_tag_draft = String::new();
    state.receive_tag_editing = false;
    state.xrp_copy_feedback = false;
}

/// xrp.json key for the receive tag — a bare number beside `address`. The
/// wallet's file, not settings.json (user, 2026-09-15): the tag is the
/// wallet's, and remove wallet deletes the file, so nothing has to remember
/// to clear it.
const RECEIVE_TAG_KEY: &str = "receive_tag";

/// Write the receive tag into the wallet's metadata; no tag = no key.
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

/// Put the saved receive tag back. No wallet file, no tag; anything but a
/// u32 is ignored.
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

/// What a completed or failed import/create leaves behind: nothing, and the
/// menu. The four flows all end here, which is the point — success and failure
/// clear the same fields, so neither can quietly keep something the other drops.
///
/// **There is no retry on failure.** The secrets were moved out of `AppState`
/// before the bridge ran, so by the time a failure arrives the buffers are
/// already empty; this closes the flow to match, rather than leaving the user on
/// step 2 with a blank key field and a phrase silently gone. Re-importing means
/// typing the 24 words again, which is the honest cost of never holding them
/// anywhere they could be retried from.
///
/// A failure draws no message here: the bridge opened an `ActivityLogState` on
/// submit and failed the active step, so the reason is already on screen, and
/// dismissing that log lands the user on this menu.
fn close_setup(state: &mut AppState) {
    reset_setup_form(state);
    state.xrp_view = XrpView::Menu;
}

/// Empty every buffer the three setup steps write to, and put every choice they
/// make back to its default. Says nothing about the step or the view, so it
/// serves both ends of the flow — opening one and closing one.
///
/// **Every entry and every exit goes through here**, which is the point. The
/// clears used to be written out at each site, and each site had drifted to a
/// slightly different list: entering import cleared nothing at all, backing out
/// of the flow left the storage mode where it was, and the create arm cleared
/// the 25th word the import arm kept. That drift is dangerous in one specific
/// way — `xrp_word25` and `xrp_import_mode` reset to their defaults on the way
/// in, so the radios read `No` and `On this Device` no matter what the buffers
/// behind them hold. A leftover 25th word is then invisible on the step that
/// owns it and still reaches the derivation: a wallet at an address the user
/// did not ask for, with nothing on screen that could have warned them.
///
/// The gauge is released here too — the same two occasions its doc names.
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

/// The one failure the bridge cannot report, because it happens before the
/// bridge is reached: no command channel, so nothing was ever sent.
///
/// Every other failure opens its log inside `process`. This path returns first,
/// so it has to open its own — otherwise it would be the single exit that closes
/// the flow with no explanation at all. One step, failed immediately: there is
/// no progress to show, only an outcome.
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
    state.reimport_spinner_angle = 0.0;
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

/// The disable face's buffers. Buffers only: the face itself
/// (`disable_token`) is the caller's to drop, the way `show_enable` is.
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

    /// A walk that reaches a rounding hair past the size is a full fill;
    /// one that reaches a third of it says so; nothing wanted is nothing.
    #[test]
    fn coverage_is_clamped_to_a_percent() {
        assert_eq!(coverage_pct(1000.000001, 1000.0), 100.0);
        assert!((coverage_pct(333.0, 1000.0) - 33.3).abs() < 0.01);
        assert_eq!(coverage_pct(0.0, 1000.0), 0.0);
        assert_eq!(coverage_pct(5.0, 0.0), 0.0);
    }

    /// The send-max string is the ledger's own digits: every significant
    /// digit the wire carried, and no invented ones.
    #[test]
    fn exact_amount_round_trips_the_ledger_string() {
        for s in ["0", "1", "10.5", "10.1234567", "0.000000789", "123456789012345", "0.00000000000001", "1000000.005"] {
            assert_eq!(exact_amount(s.parse::<f64>().unwrap()), s, "{s}");
        }
    }

    /// A ticket for 1,000 XRP over a book that only covers 600 of it at an
    /// average of 1.40, against a limit of 1.3714.
    const WALK: (f64, f64) = (600.0, 840.0);
    const WANT: (f64, f64) = (1000.0, 1371.4);
    const LIMIT: f64 = 1.3714;

    /// The invariant of an always-IOC contract: the estimate is what the walk
    /// says fills, full stop. Extrapolating the uncovered 400 XRP at the limit
    /// would headline 1,388.56 for an order the book will answer with 840 —
    /// a number the ledger has already declined to produce.
    #[test]
    fn a_depth_short_ticket_never_estimates_above_the_walk() {
        let e = expected_amount(TradeAnchor::Pay, true, WALK, WANT, LIMIT);
        assert!((e - WALK.1).abs() < 1e-9, "{e}");
        assert!(e <= WALK.1 + 1e-9, "an IOC estimate may never exceed the walk");
    }

    /// Same book, receive anchor: the estimate is what the walk actually
    /// spends, not the full size the ticket asked to spend.
    #[test]
    fn the_receive_anchor_estimate_is_also_the_walk_under_a_fill_now_tif() {
        let e = expected_amount(TradeAnchor::Receive, true, WALK, WANT, LIMIT);
        assert!((e - WALK.0).abs() < 1e-9, "{e}");
        assert!(e < WANT.0, "the ticket asked to spend more than the book takes");
    }

    /// The other arm, and why the two must stay apart: a GTC remainder does
    /// not disappear, it rests, so pricing it at the limit is correct. This
    /// test exists so that "estimate == walk" is never generalised into a rule
    /// that also silently rewrites what a resting order is worth.
    #[test]
    fn a_resting_remainder_is_priced_at_the_limit_and_that_is_deliberate() {
        let e = expected_amount(TradeAnchor::Pay, false, WALK, WANT, LIMIT);
        assert!((e - (840.0 + 400.0 * LIMIT)).abs() < 1e-9, "{e}");
        assert!(e > WALK.1, "a GTC estimate covers the part that rests");
    }

    /// A book that covers the whole size gives the same answer either way —
    /// there is no remainder for the TIF to disagree about.
    #[test]
    fn the_two_arms_agree_when_the_book_covers_the_size() {
        let full = (1000.0, 1380.0);
        let ioc = expected_amount(TradeAnchor::Pay, true, full, WANT, LIMIT);
        let gtc = expected_amount(TradeAnchor::Pay, false, full, WANT, LIMIT);
        assert!((ioc - gtc).abs() < 1e-9);
    }

    /// A walk deeper than the ticket asked for must not subtract from the
    /// estimate — `max(0.0)` is load-bearing, not defensive dressing.
    #[test]
    fn a_walk_past_the_typed_size_never_reduces_the_estimate() {
        let deep = (1200.0, 1650.0);
        let e = expected_amount(TradeAnchor::Pay, false, deep, WANT, LIMIT);
        assert!((e - 1650.0).abs() < 1e-9, "{e}");
    }

    /// The cushion moves the bound DOWN and only down: a bound above the walked
    /// floor would refuse its own walk. Both anchors subtract, because the limit
    /// is receive-per-pay either way and lowering it is the loosening direction
    /// in both (`receive = pay x limit`, `pay = receive / limit`).
    #[test]
    fn the_cushion_only_ever_loosens_the_bound() {
        let floor = 1.3714;
        for touch in [1.3714, 1.3800, 1.5000, 0.0] {
            let b = trade_cushioned(floor, touch, None);
            assert!(b > 0.0 && b < floor, "touch {touch}: {b} not under {floor}");
        }
        assert_eq!(trade_cushioned(0.0, 1.0, None), 0.0);
    }

    /// Slope self-scales: a steeper book (bigger touch-to-floor gap) earns more
    /// room, a flat book almost none. This is the whole reason it is not a flat
    /// percentage.
    #[test]
    fn a_steeper_book_earns_a_wider_cushion() {
        let floor = 1.0;
        let flat = trade_cushioned(floor, 1.0001, None);
        let steep = trade_cushioned(floor, 1.05, None);
        assert!(steep < flat, "steep {steep} should sit lower than flat {flat}");
        // And never past the ceiling, however steep.
        let absurd = trade_cushioned(floor, 100.0, None);
        assert!(absurd >= floor * (1.0 - CUSHION_MAX) * 0.999_999, "{absurd}");
    }

    /// delta_min must clear one tick at the pair's TickSize, or the engine's own
    /// mantissa-ceiling rounding eats the whole cushion. Live values 2026-08-31:
    /// XSGD 6, BBRL 5, the other three unset.
    #[test]
    fn delta_min_clears_one_tick() {
        assert!((trade_delta_min(Some(5)) - 1e-4).abs() < 1e-12);
        assert!((trade_delta_min(Some(6)) - 1e-5).abs() < 1e-12);
        // Unset issuers fall back to the engine's own 1e-7 tolerance.
        assert!((trade_delta_min(None) - 1e-7).abs() < 1e-15);
        // A coarse tick actually widens the cushion on an otherwise flat book.
        let flat_default = trade_cushioned(1.0, 1.0, None);
        let flat_bbrl = trade_cushioned(1.0, 1.0, Some(5));
        assert!(flat_bbrl < flat_default, "{flat_bbrl} !< {flat_default}");
    }

    /// An order handed off on either side leaves the ticket on the same
    /// market, oriented for the next order: XRP/RLUSD stays XRP/RLUSD. After
    /// a Sell it used to come back as RLUSD/XRP.
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
