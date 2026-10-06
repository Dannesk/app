use crate::channel::{BtcUtxo, CHANNEL, WSCommand};
use crate::ws::commands::{
    bitcoin_auth, bitcoin_payment, bitcoin_transaction_sender, bitcoin_validation,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio_tungstenite::tungstenite::Message;

/// Broadcasts in flight, txid → (wallet, raw hex). Stashed at dispatch and
/// consumed by the relay's response: on success the signed transaction itself
/// tells us exactly which coins left and which change came back, so the local
/// UTXO set moves the MOMENT our own broadcast is accepted — back-to-back
/// sends must not race the relay's event round-trip and re-offer coins the
/// previous send just spent. The relay's own `btc_utxos` push lands seconds
/// later and re-derives the same answer from indexd; this is only the bridge
/// across that gap.
fn in_flight() -> &'static Mutex<HashMap<String, InFlight>> {
    static MAP: OnceLock<Mutex<HashMap<String, InFlight>>> = OnceLock::new();
    MAP.get_or_init(Default::default)
}

struct InFlight {
    wallet: String,
    tx_hex: String,
    /// A fee bump: the txid this one supersedes. Its outputs (our old change)
    /// leave the local set the moment the replacement is accepted.
    replaces: Option<String>,
}

pub(crate) fn stash_in_flight(wallet: &str, tx_hex: &str, replaces: Option<&str>) {
    use bitcoin::consensus::encode::deserialize_hex;
    if let Ok(tx) = deserialize_hex::<bitcoin::Transaction>(tx_hex) {
        in_flight().lock().unwrap().insert(
            tx.compute_txid().to_string(),
            InFlight {
                wallet: wallet.to_string(),
                tx_hex: tx_hex.to_string(),
                replaces: replaces.map(|s| s.to_string()),
            },
        );
    }
}

/// The broadcast was accepted: mark its inputs in the local set as consumed
/// by it (they stay: the chain still holds them, so the confirmed figure does
/// not move before the block, and marked they are never offered again), add
/// our own outputs at height 0, and write a minimal pending row so the
/// eligibility check recognises that change as OUR send (the relay's full
/// record upserts over the row within seconds — `or_insert` keeps whichever
/// arrived first).
fn apply_local_spend(txid: &str) {
    use bitcoin::consensus::encode::deserialize_hex;
    let Some(InFlight { wallet, tx_hex, replaces }) = in_flight().lock().unwrap().remove(txid) else {
        return;
    };
    let Ok(tx) = deserialize_hex::<bitcoin::Transaction>(&tx_hex) else {
        return;
    };
    let spent: Vec<(String, u32)> = tx
        .input
        .iter()
        .map(|i| (i.previous_output.txid.to_string(), i.previous_output.vout))
        .collect();
    // HD: "ours" is membership in the wallet's address list, and a returning
    // output is tagged with the member address that owns it — an untagged (or
    // primary-mistagged) coin would later sign with the wrong key.
    let records = crate::wallet::btc_address_records();
    let owner_of = |spk: &bitcoin::Script| -> Option<String> {
        bitcoin::Address::from_script(spk, bitcoin::Network::Bitcoin)
            .ok()
            .map(|a| a.to_string())
            .filter(|a| records.iter().any(|r| r.address == *a))
    };
    let ours = |spk: &bitcoin::Script| owner_of(spk).is_some();

    let mut spent_sats = 0u64;
    // The coins leaving, with value and owner — the body of the pending row
    // below, so this send can be bumped before the relay's record even lands.
    let mut spent_coins: Vec<crate::channel::BtcRbfInput> = Vec::new();
    CHANNEL.btc_utxos_tx.send_modify(|(set_wallet, set)| {
        if set_wallet.as_deref() != Some(wallet.as_str()) {
            return;
        }
        for u in set.iter_mut() {
            if spent.iter().any(|(t, v)| *t == u.txid && *v == u.vout) {
                spent_sats += u.sats;
                spent_coins.push(crate::channel::BtcRbfInput {
                    txid: u.txid.clone(),
                    vout: u.vout,
                    sats: u.sats,
                    address: Some(u.address.clone()),
                });
                // The relay's push carries the same mark moments later; a
                // replacement re-marks the original's inputs with its own txid.
                u.spent_by = Some(txid.to_string());
            }
        }
        // A replaced transaction's outputs are phantoms now — its change was
        // ours at height 0 and can never confirm.
        set.retain(|u| replaces.as_deref() != Some(u.txid.as_str()));
        for (vout, out) in tx.output.iter().enumerate() {
            // The relay's push for the change address may have landed first —
            // the node announces a transaction while the broadcast is still
            // answering — and then this coin is in the set already. A second
            // entry would count the change twice and offer one coin as two.
            let held = set.iter().any(|u| u.txid == txid && u.vout == vout as u32);
            if let Some(owner) = owner_of(&out.script_pubkey)
                && !held
            {
                set.push(BtcUtxo {
                    txid: txid.to_string(),
                    vout: vout as u32,
                    sats: out.value.to_sat(),
                    height: 0,
                    address: owner,
                    spent_by: None,
                });
            }
        }
    });
    // The union moved — the aggregate balance moves with it, immediately.
    crate::ws::commands::get_btc_utxos::recompute_btc_balance();

    let sent_sats: u64 = tx
        .output
        .iter()
        .filter(|o| !ours(&o.script_pubkey))
        .map(|o| o.value.to_sat())
        .sum();
    let fee_sats = spent_sats.saturating_sub(tx.output.iter().map(|o| o.value.to_sat()).sum());
    let receivers: Vec<String> = tx
        .output
        .iter()
        .filter(|o| !ours(&o.script_pubkey))
        .filter_map(|o| {
            bitcoin::Address::from_script(&o.script_pubkey, bitcoin::Network::Bitcoin)
                .ok()
                .map(|a| a.to_string())
        })
        .collect();
    // Only a complete body is worth carrying: every input must have been in
    // the local set — which an ordinary send and a replacement both satisfy
    // now that a consumed coin stays in the set, marked, until the block.
    let inputs = if spent_coins.len() == tx.input.len() { spent_coins } else { Vec::new() };
    let outputs: Vec<crate::channel::BtcRbfOutput> = tx
        .output
        .iter()
        .enumerate()
        .map(|(vout, o)| crate::channel::BtcRbfOutput {
            vout: vout as u32,
            sats: o.value.to_sat(),
            address: bitcoin::Address::from_script(&o.script_pubkey, bitcoin::Network::Bitcoin)
                .ok()
                .map(|a| a.to_string()),
            spk: Some(o.script_pubkey.to_hex_string()),
        })
        .collect();
    let vsize = Some(tx.vsize() as u64);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    CHANNEL.btc_transactions_tx.send_modify(|state| {
        state
            .transactions
            .entry(txid.to_string())
            .or_insert(crate::channel::BtcTransactionData {
                txid: txid.to_string(),
                status: crate::channel::BitcoinTransactionStatus::Pending,
                amount: format!("{:.8}", sent_sats as f64 / 1e8),
                fees: fee_sats.to_string(),
                receiver_addresses: receivers,
                sender_addresses: vec![wallet],
                timestamp: now,
                confirmed_at: None,
                dropped_at: None,
                block_height: None,
                replaced_by: None,
                inputs,
                outputs,
                vsize,
            });
    });
}

/// The socket task dropped this flow's transaction unsent — the link fell
/// between the check in `execute` and the pickup. Certain, so said plainly;
/// the in-flight stash goes with it, as for any other failure to leave.
pub fn report_unsent() {
    discard_in_flight(None);
    fail_log("No connection to the network — nothing was sent.");
}

/// Any terminal non-success outcome: the transaction is not in the mempool
/// (rejected), or we cannot know (unavailable) — in which case the event
/// stream corrects the set if it did land. Either way nothing local moves.
pub(crate) fn discard_in_flight(txid: Option<&str>) {
    let mut map = in_flight().lock().unwrap();
    match txid {
        Some(t) => {
            map.remove(t);
        }
        // The failure frame may not carry a txid; a stale entry only wastes a
        // map slot, but a failed flow is also the likeliest to be retried with
        // the SAME coins — clear everything rather than let an old stash apply.
        None => map.clear(),
    }
}

/// Tell the send screen a signed transaction has gone out.
///
/// Raised on exactly one occasion, and the asymmetry is the point. Every exit
/// before this one is LOCAL — input validation, argon2id (which runs offline on
/// a blocking thread, deliberately ahead of the network, precisely so a
/// mistyped key costs 0.4s rather than a round trip), coin selection, and the
/// build — so a send that ends there never left the device and the screen has
/// nothing to undo. It is the activity log's job to say what happened, on every
/// one of those paths and on this one.
pub(crate) fn dispatched() {
    CHANNEL.btc_send_dispatched_tx.send_modify(|n| *n = n.wrapping_add(1));
}

pub(crate) fn fail_log(msg: &str) {
    let mut log_opt = CHANNEL.activity_tx.borrow().clone();
    if let Some(ref mut log) = log_opt {
        log.fail_active(msg.to_string());
        let _ = CHANNEL.activity_tx.send(log_opt.clone());
    }
}

pub(crate) fn advance_log(done: &'static str, next: &'static str) {
    let mut log_opt = CHANNEL.activity_tx.borrow().clone();
    if let Some(ref mut log) = log_opt {
        log.finish(done);
        log.start(next);
        let _ = CHANNEL.activity_tx.send(log_opt.clone());
    }
}

pub(crate) fn finish_log(id: &'static str) {
    let mut log_opt = CHANNEL.activity_tx.borrow().clone();
    if let Some(ref mut log) = log_opt {
        log.finish(id);
        let _ = CHANNEL.activity_tx.send(log_opt.clone());
    }
}

pub async fn execute(
    bitcoin_current_wallet: String,
    mut cmd: WSCommand,
) -> Result<(), String> {
    let (tx_type, wallet, _passphrase) =
        bitcoin_validation::validate_inputs(&cmd, &bitcoin_current_wallet)
            .map_err(|e| {
                fail_log(&e);
                e
            })?;

    // The fee is carried in the command, not fetched — so checking it is part of
    // validating the inputs, and it stays under the `init` step with the rest.
    let fee = cmd
        .fee
        .as_ref()
        .ok_or_else(|| {
            fail_log("No transaction fee specified");
            "Error: No transaction fee specified".to_string()
        })?
        .to_string();

    if fee.parse::<u32>().is_err() {
        fail_log("Invalid transaction fee");
        return Err("Error: Invalid transaction fee format".to_string());
    }

    advance_log("init", "auth");

    // Authenticate BEFORE touching the network — the UTXO fetch used to run
    // first, which is backwards for the same reason as the XRP twin: nothing in
    // the auth path reads a UTXO, so a mistyped passphrase cost a full round
    // trip (up to the 20s timeout when the relay is unreachable) to discover.
    // Argon2id answers in ~0.4s, locally, offline. Cheap check first.
    //
    // Move the secrets out of `cmd` (no clone, no secret duplication); the
    // now-secretless `cmd` is still used below to build the blob.
    let passphrase = cmd.passphrase.take();
    let seed = cmd.seed.take();
    let bip39 = cmd.bip39.take();
    let wallet_clone = wallet.clone();
    let auth_result = tokio::task::spawn_blocking(move || {
        bitcoin_auth::authenticate_wallet(passphrase, seed, bip39, &wallet_clone)
    })
    .await
    .map_err(|e| format!("Internal thread error: {}", e));

    let wallet_obj = match auth_result {
        Ok(Ok(w)) => w,
        // A key that doesn't decrypt. Local, offline, ~0.4s, nothing has
        // touched the network — so the send screen keeps its composition and
        // simply waits for the key to be retyped. The log carries the reason;
        // the card says nothing.
        Ok(Err(e)) => {
            fail_log("Authentication failed");
            return Err(e);
        }
        Err(e) => {
            fail_log("Authentication failed");
            return Err(e);
        }
    };

    advance_log("auth", "build");

    // Signing is auth → build → broadcast now: the UTXO set was pushed by the
    // relay and sits in the watch channel, so the round trip that used to live
    // here (and its 20s worst case) is gone. The backstop for a stale set is
    // the node itself — `sendrawtransaction` rejects a spent input outright,
    // and nothing is ever lost to that rejection.
    let utxos = bitcoin_payment::eligible_utxos(&wallet).map_err(|e| {
        fail_log("No spendable coins in view");
        e
    })?;

    // Change rotation (chain 1): a fresh unused change address per send. The
    // walk persists + subscribes it BEFORE the build, so the returning coin is
    // watched and spendable. Legacy wallet without an xpub falls back to #0 —
    // the consolidating pre-HD behavior.
    let change_address = crate::bridge::btc_receive_rotation::ensure_change_address()
        .unwrap_or_else(|| wallet_obj.address.clone());

    let tx_hex = match bitcoin_payment::construct_transaction(
        &wallet_obj,
        &cmd,
        &tx_type,
        utxos,
        fee,
        &change_address,
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            fail_log("Failed to build transaction");
            return Err(e);
        }
    };

    advance_log("build", "broadcast");

    // Refuse to dispatch into a socket that isn't there.
    //
    // This is the app's only real liveness check, and it has to live HERE — as
    // late as possible, microseconds before the send. `CRYPTO_COMMANDS_TX`
    // being present says the ws task started, not that it is connected, and
    // `send_transaction` succeeds either way: it hands the frame to an mpsc,
    // and the crypto loop then drops it on the floor when `!is_connected`.
    // Without this check an offline send authenticates, builds, declares
    // itself dispatched, tears the composition down — and sends nothing, with
    // the log left sitting on "Broadcasting to network" forever.
    //
    // Checked after the build rather than before it so the window between the
    // check and the send is as small as it can be. Nothing has left the device
    // at this point, so this behaves exactly like a wrong key: the log says
    // why, the send screen keeps its composition, and pressing the button
    // again once the link is back is the whole retry.
    if !*CHANNEL.btc_ws_status_rx.borrow() {
        fail_log("No connection to the network — nothing was sent.");
        return Err("Error: no connection to the network".to_string());
    }

    // Stashed BEFORE dispatch so the success response — however fast — always
    // finds it. A response that never comes leaves a dead map entry, nothing
    // more.
    stash_in_flight(&wallet, &tx_hex, None);

    // The point of no return, announced BEFORE the send for the same reason the
    // stash is written before it: after this call nothing here can prove the
    // bytes didn't leave. The send screen tears its composition down on this
    // signal, not on a response — a response may never arrive.
    dispatched();

    if let Err(e) =
        bitcoin_transaction_sender::send_transaction(&wallet, &tx_type, tx_hex, None, Some(&change_address)).await
    {
        discard_in_flight(None);
        fail_log(&e);
        return Err(e);
    }

    advance_log("broadcast", "confirm");

    Ok(())
}

pub async fn process_response(
    message: Message,
    _bitcoin_current_wallet: &str,
) -> Result<(), String> {
    match message {
        Message::Text(text) => {
            let data: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => {
                    fail_log("Transaction failed");
                    return Err("Failed to parse response".to_string());
                }
            };

            let command = data.get("command").and_then(|c| c.as_str());

            // Server received the blob and is processing — no step change needed
            if command == Some("bitcoin_submit_transaction")
                && data.get("status").and_then(|s| s.as_str()) == Some("submitted")
            {
                return Ok(());
            }

            if command == Some("bitcoin_transaction_ack") {
                return Ok(());
            }

            if command != Some("submit_bitcoin_transaction_response") {
                return Ok(());
            }

            let result = match data.get("result") {
                Some(r) => r,
                None => {
                    fail_log("Transaction failed");
                    return Err("Missing result".to_string());
                }
            };

            if result.get("status").and_then(|s| s.as_str()) != Some("success") {
                discard_in_flight(result.get("txid").and_then(|t| t.as_str()));
                let msg = result
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Transaction failed")
                    .to_string();
                fail_log(&msg);
                return Err(msg);
            }

            // Our broadcast is in the node's mempool: move the local set NOW
            // rather than when the relay's event round-trip lands, so a
            // follow-up send cannot select the coins this one just spent.
            if let Some(txid) = result.get("txid").and_then(|t| t.as_str()) {
                apply_local_spend(txid);
            }

            finish_log("confirm");

            Ok(())
        }
        _ => Ok(()),
    }
}
