use dannesk_btc_codec::bip39::Mnemonic;
use iced::Task;
use rand::Rng;
use crate::controller::message::{Message, PlainField, SecureField};
use crate::controller::app_state::{
    AppState, BtcFeeTier, BtcView, EnableInputMode, ImportMode, SendAnchor,
};
use crate::ui::managebtc::btcsend::{available_btc, fits, floor_sats, resolved_fee};
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
            // The grid opens empty: generation is the user's own `Generate ›`
            // click, so the moment the words appear is theirs to pick.
            reset_setup_form(state);
            state.btc_view = BtcView::Create;
        }
        Message::BtcGenerateMnemonic => {
            // One roll per flow: the link is gone from the screen once words
            // exist, and a stray second message must not silently replace a
            // phrase that may already be written down.
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
                // Marked secret so clipboard managers keep it out of history.
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
            // The one-screen flow has no step navigation in front of the
            // bridge, so the whole gate the button draws itself dead under is
            // re-checked here — Enter in any field fires this too. A generated
            // phrase is valid by construction, so Create's gate is the count,
            // the 25th-word answer, and the key.
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

            // Move the locked secrets out of AppState (no unlocked copy); the
            // form clears on completion either way.
            let mnemonic = state.create_mnemonic.take();
            let bip39 = state.btc_bip39_input.take_trimmed();
            // Freeze the gauge before the key it measures leaves.
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
                // `No` means no: a word typed under `Yes` and then disowned
                // must not survive to derive the wallet.
                state.btc_bip39_input.clear();
                state.btc_bip39_reveal = false;
            }
        }
        Message::BtcScriptTypeChosen(t) => {
            state.btc_script_type = t;
        }
        Message::BtcImportModeChanged(mode) => {
            // Clear the shared secret buffer on a switch: Cold stores nothing, so
            // a passphrase typed under Standard must not survive the change and
            // silently encrypt a wallet the user asked not to have one.
            if state.btc_import_mode != mode {
                state.btc_encryption_input.clear();
                state.btc_encryption_reveal = false;
                state.btc_import_mode = mode;
            }
        }
        Message::BtcImportSubmitClicked => {
            // The one-screen flow has no step navigation in front of the
            // bridge, so the whole gate the button draws itself dead under is
            // re-checked here — Enter in any field fires this too.
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

            // Move the locked secrets out of AppState (no unlocked copy); fields
            // are left empty, so a failed import clears the form.
            let mnemonic = state.btc_seed_text.take();
            let bip39 = state.btc_bip39_input.take_trimmed();
            // Freeze the gauge before the key it measures leaves.
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
            // The same row again is a close, not a re-open: the row is a
            // toggle, and one open at a time stays the rule. Keyed by txid,
            // not position — the list re-sorts as rows settle.
            state.btc_tx_selected =
                if state.btc_tx_selected.as_deref() == Some(txid.as_str()) { None } else { Some(txid) };
        }
        Message::BtcCopyTxid(txid) => {
            return iced::clipboard::write(txid).discard();
        }
        Message::BtcBackClicked => {
            // Import/create are one screen: back is leaving the flow, and
            // leaving the flow wipes the buffers.
            close_setup(state);
        }
        Message::BtcDeleteKey => {
            // The key-management page STAYS OPEN and cuts to its cold face the
            // moment the channel confirms — closing here dropped the user on
            // the balance screen at the instant something irreversible
            // happened, which reads as a crash rather than a result.
            state.btc_key_mgmt_restoring = false;
            let (_, wallet_address, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
            let Some(address) = wallet_address else { return Task::none(); };
            // `btc_key_purge_requested` guards the re-tap through the async gap —
            // the button draws disabled until the channel reports the key gone.
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
            // The page stays up, button dead, until the bridge reports back;
            // `BtcWalletRemoved` then routes to Menu, which — the address gone
            // from the channel — draws the wallet gate. A definite result
            // state, never emptiness.
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
            state.btc_view = BtcView::Menu;
        }
        Message::BtcCopyAddress => {
            // Copies what the page DISPLAYS — the rotating receive address
            // when rotation is active, #0 otherwise.
            let addr = state
                .btc_receive_address
                .clone()
                .or_else(|| CHANNEL.bitcoin_wallet_rx.borrow().1.clone());
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
            state.btc_pool_copied = None;
        }
        Message::BtcCopyPoolAddress(address) => {
            state.btc_pool_copied = Some(address.clone());
            return Task::batch([
                iced::clipboard::write(address).discard(),
                Task::perform(
                    async { tokio::time::sleep(std::time::Duration::from_millis(1500)).await },
                    |_| Message::BtcCopyAddressFeedback,
                ),
            ]);
        }
        Message::BtcTogglePoolFace => {
            state.btc_pool_face = !state.btc_pool_face;
            state.btc_pool_refusal = None;
        }
        Message::BtcSelectReceiveAddress(address) => {
            // The user picked which address the QR shows. Nothing is derived,
            // nothing is written — the pool already holds it.
            state.btc_receive_address = Some(address);
            state.btc_pool_face = false;
        }
        Message::BtcGenerateReceiveAddress => {
            match crate::bridge::btc_receive_rotation::generate_receive_address() {
                Ok(address) => {
                    state.btc_receive_pool = crate::bridge::btc_receive_rotation::receive_pool();
                    // Show what they just made — that is why they pressed it.
                    state.btc_receive_address = Some(address);
                    state.btc_pool_refusal = None;
                }
                // The pane NAMES the reason; a dark button that does not say
                // what it is waiting on reads as broken.
                Err(why) => state.btc_pool_refusal = Some(why),
            }
        }
        Message::BtcRemoveReceiveAddress(address) => {
            crate::bridge::btc_receive_rotation::remove_receive_address(&address);
            state.btc_receive_pool = crate::bridge::btc_receive_rotation::receive_pool();
            if state.btc_receive_address.as_deref() == Some(address.as_str()) {
                state.btc_receive_address = state.btc_receive_pool.last().cloned();
            }
            state.btc_pool_refusal = None;
        }
        Message::BtcCopyMasterAddress => {
            // #0 from the channel — the master, whatever the receive pane is
            // offering. Some people send to the master on purpose.
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
        Message::BtcSendFeeTierSelected(tier) => {
            state.btc_send_fee_tier = tier;
            state.btc_send_error = None;
        }
        Message::BtcSendContinueClicked => {
            // `Sign transaction` on the one-screen compose: everything the old
            // steps 1 and 2 checked, at once — the SAME facts the view lights
            // the CTA on. If the two could disagree, the screen would offer an
            // exit and then refuse to take it. Passing opens the sign stack.
            if state.btc_send_step == 1 {
                {
                    let addr = state.btc_send_recipient.trim().to_string();
                    if addr.is_empty() {
                        state.btc_send_error = Some("enter the address you're paying".to_string());
                        return Task::none();
                    }
                    if !crate::ui::managebtc::btcsend::is_valid(&addr) {
                        state.btc_send_error =
                            Some("that isn't a bitcoin address this wallet can pay".to_string());
                        return Task::none();
                    }
                    // Paying yourself burns a fee to move money nowhere.
                    let (_, own, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
                    if own.as_deref().is_some_and(|own| crate::ui::managebtc::btcsend::same_address(own, &addr)) {
                        state.btc_send_error =
                            Some("that is this wallet's own address".to_string());
                        return Task::none();
                    }
                    let amount = match state.btc_send_amount.trim().parse::<f64>() {
                        Ok(v) if v > 0.0 => v,
                        Ok(_) => {
                            state.btc_send_error = Some("enter an amount above zero".to_string());
                            return Task::none();
                        }
                        Err(_) => {
                            state.btc_send_error = Some("that isn't a number".to_string());
                            return Task::none();
                        }
                    };
                    // Fee is resolved BEFORE the balance check, because the balance check
                    // needs it. One resolver, shared with the screen that drew the number
                    // — a second copy here would eventually total differently from what
                    // the user was shown.
                    let Some(fee) = resolved_fee(state) else {
                        state.btc_send_error = Some("pick a fee, or set one yourself".to_string());
                        return Task::none();
                    };
                    // The floor is propagation, not policy: below 1 sat/vB most of the
                    // network won't relay it, so it would sit in our own mempool reaching
                    // nobody. There is no ceiling and no second opinion above this line —
                    // a slow fee is a choice, and a bumpable one.
                    // The node's own live `mempoolminfee`, not a constant of
                    // ours: below it bitcoind refuses the transaction, so the
                    // broadcast is a round trip that cannot succeed. Above it,
                    // how fast it confirms is the sender's call.
                    let floor = floor_sats(state);
                    if fee < floor {
                        state.btc_send_error =
                            Some(format!("below the {floor} sat relay minimum, the node will refuse it"));
                        return Task::none();
                    }
                    // Amount AND fee against the balance, not the amount alone.
                    //
                    // This used to test `amount > btc_balance` and stop there, so a send of
                    // the full balance cleared every client check and died inside
                    // `select_utxos` ("needed N satoshis, available M") — after auth, i.e.
                    // after the passphrase was taken and `clear_btc_send_form` had already
                    // wiped the form. The user lost their input to an error about satoshis
                    // they never typed. Inputs must cover amount + fee, so that is what the
                    // gate asks, and MAX now reports what is actually sendable.
                    //
                    // Note the unit mismatch this has to bridge: the balance and `amount`
                    // are BTC, `fee` is absolute satoshis (NOT sat/vB — `construct_transaction`
                    // subtracts it flat). The mobile twin carries the same fix.
                    //
                    // Checked against the ELIGIBLE set's sum — the coins signing
                    // will actually select from — not the hero's confirmed-only
                    // balance. The two disagree exactly while money moves, and
                    // this gate exists to predict `select_utxos`.
                    let btc_balance = available_btc();
                    let fee_btc = fee as f64 / 100_000_000.0;
                    if !fits(amount, fee) {
                        state.btc_send_error = Some(format!(
                            "amount plus fee is more than this wallet holds — at most {:.8}",
                            (btc_balance - fee_btc).max(0.0)
                        ));
                        return Task::none();
                    }
                    state.btc_send_recipient = addr;
                    state.btc_send_amount = format!("{amount:.8}");
                    // The fee the review pane shows under the stack and the
                    // bridge signs is the resolved one, written down here so a
                    // tier selection can't be re-read against a node frame
                    // that moved between the review someone read and the
                    // broadcast.
                    state.btc_send_fee = fee.to_string();
                    state.btc_send_error = None;
                    state.btc_send_step = 2;
                }
            }
        }
        Message::BtcSendSubmitClicked => {
            // No guard needed against a second press: the secrets are taken
            // below, which empties the buffers the button's own gate reads, so
            // it disables itself on the first click. The gate asks one question
            // and the answer is gone.
            //
            // Neither exit here draws on the screen. Every failure in this flow
            // is reported by the activity log — including these two, which open
            // their own because they return before the bridge does.
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
            let recipient = state.btc_send_recipient.clone();
            let amount = state.btc_send_amount.clone();
            let fee = state.btc_send_fee.clone();
            // The SECRETS are taken and zeroized here, as they always were —
            // that is the part that must happen at the moment of use, and it is
            // also what disarms the button. The composition (address, amount,
            // fee) is NOT taken: it is not a secret, keeping it costs nothing,
            // and destroying it was only ever how this screen prevented a
            // second broadcast. Nothing is in flight — if the key doesn't
            // decrypt, nothing was ever sent — so the modal simply stays put
            // with empty credential wells, and retyping re-arms it. The
            // teardown belongs to the one irreversible moment, which is
            // dispatch. See `Message::BtcSendDispatched`.
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
            // Only says the command reached the ws layer's queue. Whether it
            // becomes a transaction is `BtcSendDispatched`'s business, and
            // whether it doesn't is the activity log's. Nothing to do.
        }
        Message::BtcSendFailed(_) => {
            // The command never reached the ws queue, so nothing was sent. The
            // bridge already failed its `init` step, which is the whole report;
            // the screen stays exactly where it is and says nothing.
        }

        // The one irreversible moment. See [`crate::channel::BtcSendDispatches`]
        // for why nothing signals the other direction.
        Message::BtcSendDispatched => {
            // Torn down HERE rather than on a response, because a response may
            // never come and a live `broadcast to network ›` over a transaction
            // that may already be in a mempool is the one thing this screen must
            // never show. The fee bump dispatches on the same signal — one send
            // path, one teardown — and leaves the transactions panel open so
            // the replacement's row is the thing seen next.
            clear_btc_send_form(state);
            clear_btc_bump_form(state);
            state.btc_view = BtcView::Menu;
        }

        // ── Fee bump ─────────────────────────────────────────────────────────
        Message::BtcBumpClicked(txid) => {
            // Arm the stack over the transactions panel. Everything it prices
            // is already in hand — the row's record carries the body and the
            // node frame the policy numbers — so nothing is asked of anyone.
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
            // The fee is resolved at the press, from the same planner the
            // stack drew its number with — a named tier is a live quote off
            // the node frame, and this is the moment it becomes the number
            // that gets signed. No number, no dispatch.
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
            // Secrets taken — which disarms the button — the fee choice kept,
            // exactly as the send stack does: a key that doesn't decrypt costs
            // a retype, not the selection.
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
            // Same contract as the send twins: reaching the queue says nothing
            // about the transaction, and a failure before it never left the
            // device. The activity log carries both; the stack stays put.
        }
        Message::BtcReimportSpinnerTick => {
            // 7° per 16ms frame ≈ 0.82s per turn — the mock's ~0.85s spinner.
            state.btc_reimport_spinner_angle = (state.btc_reimport_spinner_angle + 7.0) % 360.0;
        }
        Message::BtcReimportConfirm => {
            // The restore action's own gate, re-checked here because Enter on
            // the key field fires this too: 24 words with a valid checksum and
            // a key at the import floor. Whether the phrase opens THIS wallet
            // is arbitrated downstream, by the address check in the bridge.
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
            // Freeze the gauge before the key it measures leaves.
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
            // Pop the whole restore stack back to the key-management page — the
            // channel now reports the key present, so the page renders its
            // Standard face. The quick cut IS the feedback.
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
            // Entering or leaving the restore face wipes whatever was typed. A
            // phrase sitting in a buffer behind a closed face is a secret the
            // user believes they have already put away.
            clear_btc_reimport_form(state);
            state.btc_key_mgmt_restoring = !state.btc_key_mgmt_restoring;
        }
        _ => {}
    }
    Task::none()
}

/// The receive pane's rotating address — the receive page's "on open"
/// rotation, for a pane that is never opened because it is always up.
///
/// Re-derived when the pushed UTXO set changed (a payment landed on the
/// offered address, which is what makes it used; or a coin was spent) and
/// on every switch to the tab (`force`). Otherwise one hash compare, so the
/// Sync path never reads `btc.json` for nothing. With no wallet the pool is
/// empty and the pane shows the channel's #0, as before.
pub fn refresh_receive_address(state: &mut AppState, force: bool) {
    let fp = utxo_fingerprint();
    let moved = fp != state.btc_utxo_fingerprint;
    if !force && !moved {
        return;
    }
    state.btc_utxo_fingerprint = fp;
    // The pool is a list on disk — read it, do not infer it. The shown address
    // is whichever row the user picked, kept if it is still in the pool, else
    // the most recently generated one.
    state.btc_receive_pool = crate::bridge::btc_receive_rotation::receive_pool();
    let held = state
        .btc_receive_address
        .as_ref()
        .filter(|a| state.btc_receive_pool.iter().any(|p| p == *a))
        .cloned();
    state.btc_receive_address = held.or_else(|| state.btc_receive_pool.last().cloned());
    // The coin set moved (heights are in the fingerprint): a paid address may
    // have CONFIRMED, which is when it leaves the live list — never at the
    // mempool hit. The list is rebuilt from what is unconfirmed now and sent
    // whole; an address still pending is simply still on it.
    if moved {
        crate::bridge::btc_receive_rotation::send_live_list();
    }
}

/// The UTXO union as one number: which coins, at which heights. Amounts are
/// implied by the outpoints; the owning address changes with them.
fn utxo_fingerprint() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let set = CHANNEL.btc_utxos_rx.borrow();
    set.0.hash(&mut h);
    for u in &set.1 {
        u.txid.hash(&mut h);
        u.vout.hash(&mut h);
        u.height.hash(&mut h);
    }
    h.finish()
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
    state.btc_reimport_spinner_angle = 0.0;
}

/// Empty every buffer the three setup steps write to, and put every choice they
/// make back to its default. Says nothing about the step or the view, so it
/// serves both ends of the flow — opening one and closing one.
///
/// **Every entry and every exit goes through here**, which is the point. The
/// clears used to be written out at each site, and each site had drifted to a
/// slightly different list: entering import cleared nothing at all, backing out
/// of create left the pasted phrase in `btc_seed_text`, and a finished import
/// kept whatever the create flow had generated. That drift is dangerous in one
/// specific way — `btc_word25` and `btc_import_mode` reset to their defaults on
/// the way in, so the radios read `No` and `On this Device` no matter what the
/// buffers behind them hold. A leftover 25th word is then invisible on the step
/// that owns it and still reaches the derivation: a wallet at an address the
/// user did not ask for, with nothing on screen that could have warned them.
///
/// The gauge is released here too — the same two occasions its doc names.
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

/// What a failed import or create leaves behind: nothing, and the menu.
///
/// **There is no retry.** The secrets were moved out of `AppState` before the
/// bridge ran, so by the time a failure arrives the buffers are already empty —
/// this closes the flow to match, rather than leaving the user on step 2 with a
/// blank key field and a phrase silently gone. Re-importing means typing the 24
/// words again, which is the honest cost of never holding them anywhere they
/// could be retried from.
///
/// It draws no message: the bridge opened an `ActivityLogState` on submit and
/// failed the active step, so the reason is already on screen, and dismissing
/// that log lands the user here on the import/create menu.
fn close_setup(state: &mut AppState) {
    reset_setup_form(state);
    state.btc_view = BtcView::Menu;
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
    let _ = crate::channel::CHANNEL.activity_tx.send(Some(log));
    close_setup(state);
}

/// A send that could not even be handed to the bridge — no wallet in the
/// channel, or no command channel at all.
///
/// Neither is the offline case, and it is worth being clear about that:
/// `CRYPTO_COMMANDS_TX` is a `OnceLock` filled when the ws task starts, so it
/// answers "has this app finished booting", never "is the socket up". Whether
/// there is a link to send over is checked in the ws command itself, against
/// `btc_ws_status_rx`, a hair before the transaction goes out. Both of these
/// are effectively unreachable from a screen you can only get to with a wallet
/// loaded — they are kept because they are real `Option`s, not because anyone
/// expects to see them.
///
/// It opens its own [`ActivityLogState`] for the same reason [`abandon_setup`]
/// does: every other failure in this flow is narrated by the log the bridge
/// creates on submit, and these two return before that exists. The signing card
/// itself draws nothing. It never does — the log is where this app reports what
/// the backend did, and a second copy of the same sentence under the key field
/// only teaches the eye that the card is a place errors live.
///
/// Unlike [`abandon_setup`] it does **not** close the flow, and takes no state
/// at all. Nothing was sent — the command never even reached the queue — so by
/// the same rule that keeps the modal open on a wrong key, the composition
/// stands. The secrets were not taken either (this returns before that), so the
/// button is still armed and pressing it again once the link is back is the
/// whole retry.
fn abandon_send() {
    let mut log = crate::channel::ActivityLogState::new(
        "Send bitcoin",
        &[("connect", "Contacting the network")],
    );
    log.fail("connect", "No connection to the network — nothing was sent.".to_string());
    let _ = crate::channel::CHANNEL.activity_tx.send(Some(log));
}

/// Follow-up work after a non-secret send field was edited.
///
/// The BTC and fiat amounts are two views of one number — the fiat side in
/// whatever base currency Settings chose — so each edit writes the
/// other — but only the one that wasn't typed into, or the caret would jump
/// mid-word as its own field was reformatted underneath it.
pub fn after_plain_edit(state: &mut AppState, field: PlainField) {
    state.btc_send_error = None;
    match field {
        // The typed field is the anchor; the other is derived from it now and
        // on every rate tick after (`resync_send_twin`).
        PlainField::BtcSendAmount => {
            state.btc_send_anchor = SendAnchor::Amount;
            sync_fiat_from_amount(state);
        }
        PlainField::BtcSendFiat => {
            state.btc_send_anchor = SendAnchor::Fiat;
            sync_amount_from_fiat(state);
        }
        // A hand-typed fee only counts while `custom` is the selection — but
        // typing into the field IS choosing custom, so the row selects itself
        // rather than making the user click the word first.
        PlainField::BtcSendFee => state.btc_send_fee_tier = BtcFeeTier::Custom,
        PlainField::BtcBumpFee => state.btc_bump_fee_tier = BtcFeeTier::Custom,
        PlainField::BtcSendRecipient => {}
        // Routed to `xrp::after_plain_edit` before this is reached.
        _ => {}
    }
}

fn sync_fiat_from_amount(state: &mut AppState) {
    let rate = crate::utils::price::cross("BTC", state.base_currency.code());
    state.btc_send_fiat_amount = match state.btc_send_amount.trim().parse::<f64>() {
        Ok(amount) => crate::utils::fiat_amount(amount * rate),
        Err(_) => String::new(),
    };
}

/// Eight places, padded — the shape the review step writes the amount in, so
/// a fiat-anchored amount does not re-render on the first tick after it.
fn sync_amount_from_fiat(state: &mut AppState) {
    let rate = crate::utils::price::cross("BTC", state.base_currency.code());
    state.btc_send_amount = match state.btc_send_fiat_amount.trim().parse::<f64>() {
        Ok(fiat) if rate > 0.0 => format!("{:.8}", fiat / rate),
        _ => String::new(),
    };
}

/// The XRP form's twin (`xrp::resync_send_twin`): re-derive the non-anchor
/// amount at the live rate on every `Sync`, touching nothing else, and leave
/// both alone while BTC is unpriced. See `SendAnchor`.
pub fn resync_send_twin(state: &mut AppState) {
    if crate::utils::price::cross("BTC", state.base_currency.code()) <= 0.0 {
        return;
    }
    match state.btc_send_anchor {
        SendAnchor::Amount => sync_fiat_from_amount(state),
        SendAnchor::Fiat => sync_amount_from_fiat(state),
    }
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
    state.btc_send_fee_tier = BtcFeeTier::default();
    state.btc_send_error = None;
    state.btc_send_passphrase.clear();
    state.btc_send_passphrase_reveal = false;
    state.btc_send_bip39.clear();
    state.btc_send_bip39_reveal = false;
    state.btc_send_seed_text.clear();
    state.btc_send_seed_reveal = false;
}
