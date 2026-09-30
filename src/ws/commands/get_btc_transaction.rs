use crate::channel::{BitcoinTransactionStatus, BtcRbfInput, BtcRbfOutput, BtcTransactionData, CHANNEL};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

/// An optional string field: absent, JSON null and empty all mean "we don't
/// know", and none of them may become a rendered value.
fn str_field(tx: &Value, key: &str) -> Option<String> {
    tx.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

pub fn parse_btc_tx(tx: &Value) -> Option<BtcTransactionData> {
    let txid = tx.get("txid").and_then(|h| h.as_str()).unwrap_or_default().to_string();
    // NOTE: an unrecognised status returns `None` and the record is DROPPED on
    // the floor — silently, and on the cached-reload path too, so the symptom
    // is "my transaction disappeared when I reopened the app". Every status the
    // relay can write must appear here.
    let status = match tx.get("status").and_then(|s| s.as_str()) {
        Some("pending") => BitcoinTransactionStatus::Pending,
        Some("confirmed") | Some("success") => BitcoinTransactionStatus::Success,
        Some("dropped") => BitcoinTransactionStatus::Dropped,
        Some("failed") => BitcoinTransactionStatus::Failed,
        Some("cancelled") => BitcoinTransactionStatus::Cancelled,
        Some("replaced") => BitcoinTransactionStatus::Replaced,
        _ => return None,
    };
    let amount = tx.get("amount").and_then(|a| a.as_str()).unwrap_or("0").to_string();
    let fees = tx.get("fees").and_then(|f| f.as_str()).unwrap_or("0").to_string();
    let sender_addresses = tx
        .get("sender_addresses")
        .and_then(|s| s.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let receiver_addresses = tx
        .get("receiver_addresses")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let timestamp = tx.get("timestamp").and_then(|t| t.as_str()).unwrap_or_default().to_string();
    // Absent while pending, and absent is the honest answer — a zero height
    // or a confirmation time borrowed from the mempool would both read as
    // facts. `str_field` also treats an explicit JSON null as absent, which is
    // how the relay's cached-balance passthrough spells "not confirmed yet".
    let confirmed_at = str_field(tx, "confirmed_at");
    let dropped_at = str_field(tx, "dropped_at");
    let block_height = str_field(tx, "block_height");
    let replaced_by = str_field(tx, "replaced_by");
    // The body, when the record carries it (pending rows since 2026-09-06).
    // An input without an outpoint or value is useless to a bump, so it is
    // dropped rather than carried half-formed; the planner then refuses on
    // the count mismatch, and the fetch path answers instead.
    let inputs: Vec<BtcRbfInput> = tx["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| {
            Some(BtcRbfInput {
                txid: i["txid"].as_str()?.to_string(),
                vout: i["vout"].as_u64()? as u32,
                sats: i["sats"].as_u64()?,
                address: i["address"].as_str().map(|a| a.to_string()),
            })
        })
        .collect();
    let outputs: Vec<BtcRbfOutput> = tx["outputs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            Some(BtcRbfOutput {
                vout: o["vout"].as_u64()? as u32,
                sats: o["sats"].as_u64()?,
                address: o["address"].as_str().map(|a| a.to_string()),
                spk: o["spk"].as_str().map(|h| h.to_string()),
            })
        })
        .collect();
    let vsize = tx["vsize"].as_u64();

    Some(BtcTransactionData {
        txid,
        status,
        amount,
        fees,
        receiver_addresses,
        sender_addresses,
        timestamp,
        confirmed_at,
        dropped_at,
        block_height,
        replaced_by,
        inputs,
        outputs,
        vsize,
    })
}

pub async fn execute(
    _current_wallet: String,
    _cmd: crate::channel::WSCommand,
) -> Result<(), String> {
    Ok(())
}

pub async fn process_response(
    message: Message,
    _bitcoin_current_wallet: &str,
) -> Result<(), String> {
    match message {
        Message::Text(text) => {
            let data: Value =
                serde_json::from_str(&text).map_err(|e| format!("Failed to parse JSON: {}", e))?;

            let wallet = data
                .get("wallet")
                .and_then(|w| w.as_str())
                .ok_or("Missing wallet field")?;

            let command = data.get("command").and_then(|c| c.as_str());
            if command != Some("get_bitcoin_transaction") {
                return Ok(());
            }

            // Data about an address btc.json records, or nothing — the rule
            // `get_btc_utxos::merge_btc_utxos` applies to coins, applied to
            // rows. A stale subscription (a replaced wallet, or an import the
            // service answered for but the app never saved) must not put a
            // row into this wallet's history. The identity slot is never
            // written from a frame on either chain.
            if !crate::wallet::btc_address_records().iter().any(|r| r.address == wallet) {
                return Ok(());
            }

            let tx = match data.get("transaction") {
                Some(tx) if !tx.is_null() => tx,
                Some(_) => return Ok(()),
                None => return Err("Missing transaction field".to_string()),
            };

            if let Some(tx_data) = parse_btc_tx(tx) {
                CHANNEL.btc_transactions_tx.send_modify(|state| {
                    state.transactions.insert(tx_data.txid.clone(), tx_data);
                });
            } else {
                return Err("Invalid transaction status".to_string());
            }
        }
        _ => {
            return Err("Non-text message received".to_string());
        }
    }
    Ok(())
}
