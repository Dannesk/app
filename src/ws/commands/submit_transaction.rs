use crate::channel::{ActivityStepState, CHANNEL, WSCommand};
use crate::ws::commands::{transaction_builder, transaction_sender, validation, wallet_auth};
use serde_json::Value;
use sha2::{Digest, Sha512};
use tokio_tungstenite::tungstenite::Message;

fn fail_log(msg: &str) {
    let mut log_opt = CHANNEL.activity_tx.borrow().clone();
    if let Some(ref mut log) = log_opt {
        log.fail_active(msg.to_string());
        let _ = CHANNEL.activity_tx.send(log_opt.clone());
    }
}

fn advance_log(done: &'static str, next: &'static str) {
    let mut log_opt = CHANNEL.activity_tx.borrow().clone();
    if let Some(ref mut log) = log_opt {
        log.finish(done);
        log.start(next);
        let _ = CHANNEL.activity_tx.send(log_opt.clone());
    }
}

fn finish_log(id: &'static str) {
    let mut log_opt = CHANNEL.activity_tx.borrow().clone();
    if let Some(ref mut log) = log_opt {
        log.finish(id);
        let _ = CHANNEL.activity_tx.send(log_opt.clone());
    }
}

/// Land the last step AND state what the work actually did. Two facts, one
/// update: the step mark is our side of the wire, the note is the ledger's
/// answer, and they are published together so the surface never shows a
/// finished flow with its outcome still missing.
fn finish_log_noted(id: &'static str, note: String) {
    let mut log_opt = CHANNEL.activity_tx.borrow().clone();
    if let Some(ref mut log) = log_opt {
        log.finish(id);
        log.note(note);
        let _ = CHANNEL.activity_tx.send(log_opt.clone());
    }
}

/// Land `broadcast` and light `confirm` — once. The relay's `submitted` ack
/// is what normally does this; the terminal response calls it too, because an
/// ack lost to a dropped socket must not leave `broadcast` spinning under a
/// finished `confirm`. Idempotent: a `broadcast` already landed is left alone.
fn ensure_confirming() {
    let mut log_opt = CHANNEL.activity_tx.borrow().clone();
    if let Some(ref mut log) = log_opt {
        let broadcasting = log
            .steps
            .iter()
            .any(|s| s.id == "broadcast" && matches!(s.state, ActivityStepState::Active { .. }));
        if broadcasting {
            log.finish("broadcast");
            log.start("confirm");
            let _ = CHANNEL.activity_tx.send(log_opt.clone());
        }
    }
}

/// The socket task dropped this flow's blob unsent — the link fell between
/// the check in `execute` and the pickup. Certain, so said plainly, and the
/// order slot is released as for any other failure to leave the device.
pub fn report_unsent() {
    fail_log("No connection to the network — nothing was sent.");
    release_pending();
}

pub async fn execute(
    current_wallet: String,
    mut cmd: WSCommand,
) -> Result<(), String> {
    let mut wallet_copy = current_wallet;

    let (tx_type, wallet, _passphrase) = validation::validate_inputs(&cmd, &mut wallet_copy)
        .map_err(|e| {
            fail_log(&e);
            release_pending();
            e
        })?;

    advance_log("init", "auth");

    // Authenticate BEFORE touching the network.
    //
    // Nothing in the auth path reads anything the ledger fetch produces, so the
    // order was free — and the old one made the user pay a full round trip (up
    // to the 20s timeout when the relay is unreachable) to find out they had
    // mistyped their passphrase. Argon2id answers that in ~0.4s, locally, and it
    // works with no connection at all. The wrong-credential case is the common
    // failure here; the network is the expensive one. Do the cheap check first.
    //
    // Move the secrets out of `cmd` (no clone, no secret duplication); the
    // now-secretless `cmd` is still used below to build the blob.
    let passphrase = cmd.passphrase.take();
    let seed = cmd.seed.take();
    let bip39 = cmd.bip39.take();
    let wallet_clone = wallet.clone();
    let auth_result = tokio::task::spawn_blocking(move || {
        wallet_auth::authenticate_wallet(passphrase, seed, bip39, &wallet_clone)
    })
    .await
    .map_err(|e| format!("Internal thread error: {}", e));

    let wallet_obj = match auth_result {
        Ok(Ok(w)) => w,
        Ok(Err(e)) => {
            fail_log("Authentication failed");
            release_pending();
            return Err(e);
        }
        Err(e) => {
            fail_log("Authentication failed");
            release_pending();
            return Err(e);
        }
    };

    advance_log("auth", "build");

    // Signing is auth → build → broadcast: sequence, fee and ledger index were
    // pushed by the relay and sit in the watch channels, so the round trip that
    // used to live here (and its 20s worst case) is gone. See `ledger_env`.
    let env = ledger_env(&wallet).map_err(|e| {
        fail_log(&e);
        release_pending();
        e
    })?;

    let tx_blob = match transaction_builder::construct_blob(
        &wallet_obj,
        &cmd,
        &tx_type,
        env.sequence,
        env.fee,
        env.last_ledger_sequence,
    )
    .await
    {
        Ok(b) => b,
        Err(e) => {
            fail_log("Failed to build transaction");
            release_pending();
            return Err(e);
        }
    };

    advance_log("build", "broadcast");

    // Refuse to dispatch into a socket that isn't there — the Bitcoin flow's
    // check, which this one never had. `send_transaction` hands the frame to
    // an mpsc and succeeds either way; the socket task then drops it when the
    // link is down. Checked as late as possible so the window between here
    // and the send is as small as it can be; a drop inside that window is
    // reported by the socket task itself (`report_unsent`). Nothing has left
    // the device at this point, so this behaves exactly like a wrong key.
    if !*CHANNEL.relay_ws_status_rx.borrow() {
        fail_log("No connection to the network — nothing was sent.");
        release_pending();
        return Err("Error: no connection to the network".to_string());
    }

    let tx_id = match transaction_sender::send_transaction(&wallet, &tx_type, tx_blob.clone()).await {
        Ok(id) => id,
        Err(e) => {
            fail_log(&e);
            release_pending();
            return Err(e);
        }
    };

    // Pin the quote to the transaction it became — AFTER a successful send,
    // because an order that never left is not an order in flight.
    //
    // The canonical id is derivable from the signed blob — no round trip, and
    // it is the same id the ledger, the relay and every explorer will report —
    // so the order's promise and its outcome can be joined the moment it goes.
    // `last_ledger` comes from here too: the ticket knows the quote, only the
    // signing path knows the bounds the blob was built with, and past that
    // index the order can no longer be included in any ledger. `tx_id` is what
    // the relay will echo back, and the only way to tell whose answer it is.
    if tx_type == "offer_create"
        && let Some(hash) = transaction_hash(&tx_blob)
    {
        let mut pending = CHANNEL.pending_trade_rx.borrow().clone();
        if let Some(quote) = pending.as_mut() {
            quote.last_ledger = Some(env.last_ledger_sequence);
            quote.sequence = Some(env.sequence);
            quote.hash = Some(hash.clone());
            quote.tx_id = Some(tx_id);
            crate::bridge::order_record::record(&hash, quote);
            let _ = CHANNEL.pending_trade_tx.send(pending);
        }
    }

    // `broadcast` stays lit. It lands on the relay's `submitted` ack — the
    // node's word that it holds the blob — not on the mpsc handoff above,
    // which proves nothing about the wire. See `ensure_confirming`.

    Ok(())
}

/// Drop the order-in-flight slot.
///
/// Every path that abandons a submission has to do this. A quote left parked
/// there outlives the attempt it belongs to, and the next order's answer is
/// then matched against a stale promise — or, worse, `trade_expiry_check`
/// keeps watching a ledger bound for a transaction that was never sent.
fn release_pending() {
    if CHANNEL.pending_trade_rx.borrow().is_some() {
        let _ = CHANNEL.pending_trade_tx.send(None);
    }
}

pub async fn process_response(message: Message, _current_wallet: &str) -> Result<(), String> {
    match message {
        Message::Text(text) => {
            let data: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => {
                    fail_log("Transaction failed");
                    return Err("Failed to parse response".to_string());
                }
            };

            // The node holds the blob: the relay forwards its synchronous `submit`
            // verdict, and an accepted one is what `broadcast` lands on. A rejected
            // one arrives as a `submit_transaction_response` below, shaped like a
            // validated failure.
            if data.get("command").and_then(|c| c.as_str()) == Some("submit_transaction")
                && data.get("status").and_then(|s| s.as_str()) == Some("submitted")
            {
                ensure_confirming();
                return Ok(());
            }

            if data.get("command").and_then(|c| c.as_str()) == Some("transaction_ack") {
                return Ok(());
            }

            if data.get("command").and_then(|c| c.as_str()) != Some("submit_transaction_response") {
                return Ok(());
            }

            // Whatever the answer, the blob reached the network to get one — even
            // when the ack that would have said so was lost.
            ensure_confirming();

            let result = match data.get("result") {
                Some(r) => r,
                None => {
                    fail_log("Transaction failed");
                    return Err("Missing result".to_string());
                }
            };

            // An order's outcome is not its engine result.
            //
            // `tesSUCCESS` means the ledger APPLIED the transaction, and an
            // immediate-or-cancel order applies whether it filled all of the
            // size, part of it, or none of it. Before the relay reported a
            // real status this branch fell straight through to
            // `finish_log("confirm")` for all three, so a 39% fill and an
            // empty one both ended on a green tick and an emptied form.
            //
            // The gate below is the outcome, not the engine result — but only
            // when the relay actually sent one. Every other flow, and any
            // older relay, keeps the engine-result path underneath unchanged.
            if data.get("tx_type").and_then(|t| t.as_str()) == Some("offer_create")
                && let Some(status) = result.get("status").and_then(|s| s.as_str())
            {
                let sentence = outcome_sentence(status, result);
                settle_pending(status, result, data.get("tx_id").and_then(|v| v.as_str()));
                if status == "killed" || status == "failed" {
                    fail_log(&sentence);
                    return Err(sentence);
                }
                finish_log_noted("confirm", sentence);
                return Ok(());
            }

            let transaction_result = result.get("transaction_result").and_then(|t| t.as_str());
            if transaction_result != Some("tesSUCCESS") {
                // The node's own sentence when the relay forwarded one (an immediate
                // rejection: "This sequence number has already passed."); a validated
                // `tec` carries only its code.
                let msg = if transaction_result == Some("tecKILLED") {
                    "Order could not fill at that price".to_string()
                } else if let Some(said) = result.get("message").and_then(|m| m.as_str()) {
                    format!("Rejected by the network \u{2014} {}", said.trim_end_matches('.'))
                } else {
                    "Transaction failed".to_string()
                };
                fail_log(&msg);
                return Err(msg);
            }

            finish_log("confirm");

            Ok(())
        }
        _ => Ok(()),
    }
}

/// Close the loop on the order in flight: write the ledger's answer onto the
/// record that already holds what it was signed against, and release the slot.
///
/// The slot is released whatever the outcome — an order the ledger has
/// answered is not in flight any more, and leaving a stale quote behind would
/// let the expiry check fire against an index belonging to a finished order.
fn settle_pending(status: &str, result: &Value, tx_id: Option<&str>) {
    // The unrounded figure when the relay sends one (`filled_exact`), else the
    // four-place display string. The record exists to measure a cushion of a
    // few parts per million, which a number rounded at 1e-4 cannot resolve.
    let num = |k: &str| {
        result
            .get(format!("{k}_exact"))
            .and_then(|v| v.as_f64())
            .or_else(|| {
                result
                    .get(k)
                    .and_then(|v| v.as_str())
                    .and_then(|v| v.parse::<f64>().ok())
            })
    };

    // Whose answer is this?
    //
    // The relay publishes to the wallet's shared pubsub channel and the client
    // routes replies by command string alone, so an answer reaching this
    // function is not necessarily the answer to the order sitting in the slot.
    // Matching on `tx_id` is the whole of the correlation. Without it, two
    // orders in flight — or one signed in a second session on the same wallet —
    // and the first answer writes its outcome onto the second's record, then
    // empties the slot so the second is never settled and its expiry can never
    // fire either. A record joined to the wrong promise is worse than no record.
    let pending = CHANNEL.pending_trade_rx.borrow().clone();
    let Some(quote) = pending else { return };
    if quote.tx_id.as_deref() != tx_id || tx_id.is_none() {
        return;
    }

    if let Some(hash) = quote.hash.as_deref() {
        crate::bridge::order_record::settle(hash, status, num("filled"), num("received"));
    }
    // Answered, so no longer in flight — and the expiry watch stands down.
    let _ = CHANNEL.pending_trade_tx.send(None);
}

/// The canonical transaction id: SHA-512Half of the signed blob under the
/// `TXN\0` prefix (`0x54584E00`).
///
/// The ledger derives it exactly this way, so it is knowable the moment the
/// blob exists — no submission, no reply, nothing to wait for. That is what
/// makes it usable as the join key for an order whose outcome has not
/// happened yet.
///
/// Verified 2026-08-30 against mainnet ledger 106,635,488: the binary blob of
/// `004BA720…A906` hashes to its own id under this prefix.
fn transaction_hash(tx_blob_hex: &str) -> Option<String> {
    let bytes = hex::decode(tx_blob_hex).ok()?;
    let mut hasher = Sha512::new();
    hasher.update([0x54, 0x58, 0x4E, 0x00]);
    hasher.update(&bytes);
    Some(hex::encode(&hasher.finalize()[0..32]).to_uppercase())
}

/// An amount off the wire, trimmed for prose: the relay writes four places for
/// every asset, and "1000.0000 XRP" in a sentence reads like a machine.
fn trim(v: &Value, key: &str) -> String {
    let raw = v.get(key).and_then(|x| x.as_str()).unwrap_or("");
    if !raw.contains('.') {
        return raw.to_string();
    }
    raw.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn ccy<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("")
}

/// One sentence about what the order did, in the voice of a fact: the amount
/// in the market's base and the price, as a ticket says it. "Filled 1 XRP at
/// 1.4942 RLUSD." What came back is the one times the other, so it is not
/// said again (user, 2026-10-04: the arrow and "per XRP" were too much).
///
/// **A partial states what filled and what was asked.** One that states only
/// what filled invites the reading that the rest is still working; one that
/// states only what was asked for hides that anything traded at all. The
/// remainder of an immediate-or-cancel order does not rest, is not retried
/// and is not coming back, and this is the only place the app says so.
///
/// The "of" comparison is drawn against whichever side the order was ANCHORED
/// on — the relay sends `requested` with its own currency for exactly this, so
/// a pay-anchored and a receive-anchored order each state their shortfall in
/// the unit the user typed. Side never enters it.
fn outcome_sentence(status: &str, r: &Value) -> String {
    let (filled, fc) = (trim(r, "filled"), ccy(r, "filled_currency"));
    let (received, rc) = (trim(r, "received"), ccy(r, "received_currency"));
    let (asked, ac) = (trim(r, "requested"), ccy(r, "requested_currency"));
    // The amount is the base's: the XRP leg when there is one, whichever way
    // the market was crossed, what was paid otherwise. The relay orients the
    // rate to match (token per XRP, or receive per pay), so the price reads
    // in the other leg's unit.
    let (amount, unit, price_unit) = if rc == "XRP" {
        (received.as_str(), rc, fc)
    } else {
        (filled.as_str(), fc, rc)
    };
    let at = match r.get("fill_price").and_then(|x| x.as_str()) {
        Some(p) if !p.is_empty() => format!(" at {} {}", p, price_unit),
        _ => String::new(),
    };
    let traded = format!("{} {}{}", amount, unit, at);

    match status {
        "success" => format!("Filled {}.", traded),
        "partial" => {
            let of = if ac == unit {
                format!("Filled {} of {} {}{}", amount, asked, unit, at)
            } else if ac == fc {
                format!("Filled {} of {} {} \u{2014} {}", filled, asked, fc, traded)
            } else if ac == rc {
                format!("Received {} of {} {} \u{2014} {}", received, asked, rc, traded)
            } else {
                format!("Filled {}", traded)
            };
            format!("{}. The rest did not fill and is gone.", of)
        }
        // Nothing crossed. Said as a book fact rather than a fault: the
        // transaction was accepted, the price simply was not there.
        "killed" => "Nothing filled \u{2014} the book moved before the order landed. Only the network fee was charged.".to_string(),
        // The one status where an order is genuinely still live.
        "pending" if filled.parse::<f64>().unwrap_or(0.0) > 0.0 => {
            format!("Filled {}. The rest is resting on the book.", traded)
        }
        "pending" => "Nothing filled yet \u{2014} the order is resting on the book.".to_string(),
        // The node's own reason when the relay forwarded one (an immediate
        // rejection); a validated failure carries only its code.
        _ => match r.get("message").and_then(|m| m.as_str()) {
            Some(said) => format!(
                "The ledger rejected the order \u{2014} {}. Nothing moved.",
                said.trim_end_matches('.')
            ),
            None => "The ledger rejected the order \u{2014} nothing moved.".to_string(),
        },
    }
}

/// What the blob needs from the ledger, read from the watches — never fetched.
struct LedgerEnv {
    sequence: u32,
    /// Drops.
    fee: u64,
    last_ledger_sequence: u32,
}

/// Ledgers of validity granted to a transaction. ~4s each, so this is about
/// a minute: long enough to survive a slow ledger or a brief relay hiccup,
/// short enough that a queued (under-fee'd) transaction expires instead of
/// blocking this account's sequence indefinitely.
pub const LAST_LEDGER_OFFSET: u32 = 20;

fn ledger_env(wallet: &str) -> Result<LedgerEnv, String> {
    // The account watch is not wallet-tagged; the balance tuple is. Refuse to
    // sign with a sequence that might belong to a previous wallet.
    let wallet_loaded = CHANNEL.wallet_balance_rx.borrow().1.as_deref() == Some(wallet);
    let account = *CHANNEL.xrp_account_rx.borrow();
    let node = CHANNEL.xrp_node_rx.borrow().clone();

    let sequence = account
        .sequence
        .filter(|_| wallet_loaded)
        .ok_or_else(|| "Wallet data not loaded yet — try again shortly".to_string())?;
    // The node's open-ledger fee, exactly as reported — what it costs to get
    // into the ledger being built. No padding, no ceiling: the number on the
    // dashboard's telemetry bar is the number that gets signed.
    let fee = node
        .open_ledger_fee
        .ok_or_else(|| "Node data not loaded yet — try again shortly".to_string())?;
    let ledger_index = node
        .ledger_index
        .ok_or_else(|| "Node data not loaded yet — try again shortly".to_string())?;
    let last_ledger_sequence = u32::try_from(ledger_index)
        .map_err(|_| "Ledger index out of range".to_string())?
        .saturating_add(LAST_LEDGER_OFFSET);

    Ok(LedgerEnv { sequence, fee, last_ledger_sequence })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn order(filled: &str, fc: &str, received: &str, rc: &str, requested: &str, ac: &str) -> Value {
        json!({
            "filled": filled, "filled_currency": fc,
            "received": received, "received_currency": rc,
            "requested": requested, "requested_currency": ac,
            "fill_price": "1.4942",
        })
    }

    /// A fill is the base amount at the price: the same sentence for a Sell
    /// (XRP paid) and a Buy (XRP received).
    #[test]
    fn a_fill_is_the_base_amount_at_the_price() {
        let sell = order("1.0000", "XRP", "1.4942", "RLUSD", "1", "XRP");
        let buy = order("1.4942", "RLUSD", "1.0000", "XRP", "1", "XRP");
        assert_eq!(outcome_sentence("success", &sell), "Filled 1 XRP at 1.4942 RLUSD.");
        assert_eq!(outcome_sentence("success", &buy), "Filled 1 XRP at 1.4942 RLUSD.");
        let no_price = json!({"filled": "1", "filled_currency": "XRP", "received": "1.4942", "received_currency": "RLUSD"});
        assert_eq!(outcome_sentence("success", &no_price), "Filled 1 XRP.");
    }

    /// A partial says what filled against what was asked, in the unit typed,
    /// and that the rest is gone; a resting one says the rest is resting.
    #[test]
    fn a_partial_states_its_shortfall_in_the_unit_typed() {
        let on_xrp = order("0.5", "XRP", "0.7471", "RLUSD", "1", "XRP");
        assert_eq!(
            outcome_sentence("partial", &on_xrp),
            "Filled 0.5 of 1 XRP at 1.4942 RLUSD. The rest did not fill and is gone."
        );
        let on_total = order("0.5", "XRP", "0.7471", "RLUSD", "1.4942", "RLUSD");
        assert_eq!(
            outcome_sentence("partial", &on_total),
            "Received 0.7471 of 1.4942 RLUSD \u{2014} 0.5 XRP at 1.4942 RLUSD. The rest did not fill and is gone."
        );
        assert_eq!(
            outcome_sentence("pending", &on_xrp),
            "Filled 0.5 XRP at 1.4942 RLUSD. The rest is resting on the book."
        );
    }
}
