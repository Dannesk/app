use crate::channel::{CHANNEL, WSCommand};
use crate::ws::CRYPTO_OUTGOING_TX;
use serde_json::{Value, json};
use std::collections::HashMap;
use tokio_tungstenite::tungstenite::Message;

/// The whole-wallet ask (2026-09-20, the Trezor/Blockbook shape): ONE message
/// carrying the account xpub, from which the Bitcoin relay derives both
/// chains itself, walks them gap-20 and answers for the whole wallet — in place
/// of one ask per recorded address. `None` when `wallet` is not this device's
/// primary or btc.json holds no account key yet (a pre-HD wallet, until the
/// backfill): the caller falls back to the per-address ask.
///
/// Shared by app open (`execute`) and the link-rise re-sync
/// (`RelayState::resync_btc_payloads`), so the two cannot drift.
pub fn fetch_payload(wallet: &str) -> Option<String> {
    let jsn = crate::bridge::json_storage::read_json::<Value>("btc.json").ok()?;
    let xpub = jsn.get("account_xpub").and_then(|v| v.as_str()).filter(|s| !s.is_empty())?;
    let primary = crate::wallet::btc_address_records().first()?.address.clone();
    if primary != wallet {
        return None;
    }
    let script_type = jsn
        .get("script_type")
        .and_then(|v| v.as_str())
        .and_then(crate::btc_script_type::BtcScriptType::from_tag)
        .unwrap_or_default();
    Some(
        json!({
            "command": "get_bitcoin_cached_balance",
            "wallet": primary,
            "xpub": xpub,
            "script_type": script_type.tag(),
        })
        .to_string(),
    )
}

pub async fn execute(
    bitcoin_current_wallet: String,
    cmd: WSCommand,
) -> Result<(), String> {
    if let Some(wallet) = &cmd.wallet {
        if let Some(payload) = fetch_payload(wallet) {
            if let Some(tx) = CRYPTO_OUTGOING_TX.get() {
                let _ = tx.send(Message::text(payload)).await;
            }
            return Ok(());
        }
        // `primary` is this wallet's #0 — the HD group the address belongs to.
        // The service used to infer it from the order of asks on the
        // connection; since the proxy hub (2026-09-14) that connection is
        // shared by every client, so the group is stated here instead.
        let primary = if bitcoin_current_wallet.is_empty() { wallet.as_str() } else { bitcoin_current_wallet.as_str() };
        let msg_json = json!({ "command": "get_bitcoin_cached_balance", "wallet": wallet, "primary": primary });
        if let Some(tx) = CRYPTO_OUTGOING_TX.get() {
            let _ = tx.send(Message::text(msg_json.to_string())).await;
        }
        Ok(())
    } else {
        Err("Missing wallet parameter".to_string())
    }
}

pub async fn process_response(
    message: Message,
    _bitcoin_current_wallet: &str,
) -> Result<(), String> {
    match message {
        Message::Text(text) => {
            let data: Value =
                serde_json::from_str(&text).map_err(|e| format!("Failed to parse JSON: {}", e))?;

            let command = data.get("command").and_then(|c| c.as_str());
            if command != Some("get_bitcoin_cached_balance") {
                return Ok(());
            }

            // The whole wallet in one frame (it carries `used`) — see below.
            if data.get("used").is_some() {
                return apply_whole_wallet(&data);
            }
            // A refused whole-wallet ask (index not live, key not recognised):
            // nothing to apply, and nothing held is touched — the next link
            // rise asks again.
            if data.get("error").is_some() {
                return Ok(());
            }

            if let Some(wallet) = data.get("wallet").and_then(|w| w.as_str()) {
                // HD: one reply per watched address. The frame's balance is
                // that ADDRESS's number — the channel's aggregate is the UTXO
                // union summed, recomputed inside the merge below, and the
                // identity slot (#0) is never touched from here.
                if data.get("balance").is_none() {
                    return Err("Missing balance field".to_string());
                }

                // Process transactions
                // One parser, shared with the live push — the cached read and
                // the runtime frame are the same record and must not drift.
                let mut transactions_map = HashMap::new();
                if let Some(transactions) = data.get("transactions").and_then(|t| t.as_array()) {
                    for tx in transactions {
                        if let Some(tx_data) = crate::ws::commands::get_btc_transaction::parse_btc_tx(tx) {
                            transactions_map.insert(tx_data.txid.clone(), tx_data);
                        }
                    }
                }

                // The cached UTXO set rides this reply so signing is possible
                // the moment the app opens; the relay's fetch-through refresh
                // re-pushes a fresh copy (`btc_utxos`) moments later either
                // way. Absent field (old relay) = leave whatever we hold.
                if data.get("utxos").is_some_and(|u| u.is_array()) {
                    crate::ws::commands::get_btc_utxos::merge_btc_utxos(
                        wallet,
                        crate::ws::commands::get_btc_utxos::parse_coins(&data, wallet),
                    );
                }

                // MERGE, never replace.
                //
                // This command answers on three different occasions and only
                // one of them carries a snapshot worth trusting wholesale. A
                // cache MISS or a first-subscribe-since-restart makes the relay
                // reseed from the index, and that reseed publishes under this
                // same command with `transactions: []` — so a wholesale replace
                // wiped the list every time the relay restarted, taking any
                // in-flight payment off the screen with it. It arrives second,
                // too (the reseed costs an indexd round trip), so it always won.
                //
                // Merging also protects the narrower race of a transaction
                // pushed between Redis being read and this frame being applied.
                // Nothing is ever removed here: Redis is a cache, not the
                // arbiter, and its forgetting something is not evidence the
                // chain did. The reply's history facts (what `load 20 more ›`
                // can page to) ride along even when it carries no rows.
                CHANNEL.btc_transactions_tx.send_modify(|state| {
                    state.apply_reply(transactions_map.into_values().collect(), &data, false);
                });

                Ok(())
            } else {
                Err("Missing wallet field".to_string())
            }
        }
        _ => Err("Non-text message received".to_string()),
    }
}

/// Apply a whole-wallet frame: the answer to [`fetch_payload`], and the body of
/// a successful import (`bitcoin_import_wallet` shares this for everything
/// after its key files are written).
///
/// 1. Every USED address the server names is checked against OUR derivation
///    from the account xpub before it is recorded — the server derives the
///    same chains from the same key, but btc.json is what signing trusts, so
///    nothing enters it that this device cannot reproduce. New ones are
///    appended (record 0 stays #0) and the rotation counters move past them, so
///    an address used from another device is never offered again.
/// 2. The UTXO union is REPLACED with the frame's sets: a member the frame
///    does not list holds nothing now, whatever it held before.
/// 3. History rows merge, never replace — as on the per-address path.
/// 4. The live list goes out: this frame is what follows an import, an open
///    and every link rise, and the session is bound to nothing until it does.
pub(crate) fn apply_whole_wallet(data: &Value) -> Result<(), String> {
    use crate::bridge::btc_receive_rotation::{derive_member, send_live_list};
    use crate::ws::commands::get_btc_utxos::{parse_coins, replace_btc_utxos};

    let wallet = data.get("wallet").and_then(|w| w.as_str()).ok_or("Missing wallet field")?;
    let records = crate::wallet::btc_address_records();
    if records.first().map(|r| r.address.as_str()) != Some(wallet) {
        // Not this device's wallet (removed or replaced since the ask).
        return Ok(());
    }
    let jsn = crate::bridge::json_storage::read_json::<Value>("btc.json")
        .map_err(|e| format!("btc.json unreadable: {}", e))?;
    let xpub = jsn.get("account_xpub").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let script_type = jsn
        .get("script_type")
        .and_then(|v| v.as_str())
        .and_then(crate::btc_script_type::BtcScriptType::from_tag)
        .unwrap_or_default();

    // The highest chain-0 index the server reports as USED, over the WHOLE
    // list — not just the ones new to us. It is the only thing that moves
    // indexd's gap window forward (`relay/xpub.rs`: `end = m.index + 1 + GAP`),
    // so `generate_receive_address` reads it to refuse an address the walk
    // could never reach again. Persisted because the walk runs on the server
    // and the app has no other way to know.
    let mut last_used_receive: Option<u32> = None;
    let mut fresh: Vec<(u32, u32, String)> = Vec::new();
    for m in data.get("used").and_then(|u| u.as_array()).into_iter().flatten() {
        let (Some(address), Some(chain), Some(index)) = (
            m.get("address").and_then(|a| a.as_str()),
            m.get("chain").and_then(|c| c.as_u64()),
            m.get("index").and_then(|i| i.as_u64()),
        ) else {
            continue;
        };
        if chain == 0 {
            last_used_receive = Some(last_used_receive.map_or(index as u32, |h| h.max(index as u32)));
        }
        if records.iter().any(|r| r.address == address) {
            continue;
        }
        if derive_member(&xpub, script_type, chain as u32, index as u32).as_deref() != Some(address) {
            return Err("The server named an address this wallet does not derive".to_string());
        }
        fresh.push((chain as u32, index as u32, address.to_string()));
    }
    // Write when there is something new to record OR when the used high-water
    // moved. The stamp matters on its own: a POOL member becoming used is not
    // "fresh" (we already hold its record), so it would never be written here
    // otherwise — and the gap guard would stay frozen at the import value.
    let stamp_moved = last_used_receive.is_some()
        && last_used_receive
            != jsn
                .get("last_used_receive_index")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
    if !fresh.is_empty() || stamp_moved {
        fresh.sort();
        fresh.dedup();
        crate::bridge::json_storage::update_json("btc.json", move |file: &mut Value| {
            let Some(obj) = file.as_object_mut() else { return };
            if let Some(high) = last_used_receive {
                let held = obj.get("last_used_receive_index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                // Monotonic: the server reports what it can see, and a reply
                // that saw less must never walk the guard backwards.
                if high > held || obj.get("last_used_receive_index").is_none() {
                    obj.insert("last_used_receive_index".to_string(), json!(high));
                }
            }
            for (chain, index, address) in &fresh {
                if let Some(arr) = obj.get_mut("addresses").and_then(|a| a.as_array_mut()) {
                    arr.push(json!({ "chain": chain, "index": index, "address": address }));
                }
                // CHANGE only. Chain 0 is the user-curated pool since
                // 2026-09-20: jumping a counter past a used index would drag
                // it over pool members the user has handed out and not yet
                // been paid on, dropping them off the pool and off the live
                // list. The pool is a list, not a cursor.
                if *chain == 1 {
                    let held = obj.get("next_change_index").and_then(|v| v.as_u64()).unwrap_or(0);
                    if held <= *index as u64 {
                        obj.insert("next_change_index".to_string(), json!(*index as u64 + 1));
                    }
                }
            }
        })
        .map_err(|e| format!("btc.json write failed: {}", e))?;
    }

    // After the write: the union only accepts coins of recorded addresses.
    let mut union = parse_coins(data, wallet);
    for f in data.get("funded").and_then(|f| f.as_array()).into_iter().flatten() {
        if let Some(addr) = f.get("address").and_then(|a| a.as_str()) {
            union.extend(parse_coins(f, addr));
        }
    }
    replace_btc_utxos(wallet, union);

    let rows: Vec<_> = data
        .get("transactions")
        .and_then(|t| t.as_array())
        .into_iter()
        .flatten()
        .filter_map(crate::ws::commands::get_btc_transaction::parse_btc_tx)
        .collect();
    // Rows merge, never replace; the history facts ride along either way.
    CHANNEL.btc_transactions_tx.send_modify(|state| {
        state.apply_reply(rows, data, false);
    });

    // The change address on offer is derived and recorded here, not at the
    // first send, so the live list watches it from the first frame. A wallet
    // reimported while its send pends would otherwise not see that send's
    // change until it confirmed and the next open (2026-10-06 review).
    let _ = crate::bridge::btc_receive_rotation::ensure_change_address();
    send_live_list();
    Ok(())
}
