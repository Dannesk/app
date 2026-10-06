use crate::channel::{CHANNEL, TransactionData, TransactionStatus, WSCommand};
use crate::ws::CRYPTO_OUTGOING_TX;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

pub async fn execute(
    _current_wallet: String,
    cmd: WSCommand,
) -> Result<(), String> {
    if let Some(wallet) = &cmd.wallet {
        let msg_json = json!({ "command": "get_transaction", "wallet": wallet });
        if let Some(tx) = CRYPTO_OUTGOING_TX.get() {
            let _ = tx.send(Message::text(msg_json.to_string())).await;
        }
        Ok(())
    } else {
        Err("Missing wallet parameter".to_string())
    }
}

pub async fn process_response(message: Message, _current_wallet: &str) -> Result<(), String> {
    match message {
        Message::Text(text) => {
            let data: Value = serde_json::from_str(&text).map_err(|e| format!("Failed to parse JSON: {}", e))?;

            let command = data.get("command").and_then(|c| c.as_str());
            if command != Some("get_transaction") {
                return Ok(());
            }

            // Data about the address the app holds, or nothing — the rule
            // `balances::process_response` states. A row for any other
            // address must not land in this wallet's history.
            let wallet = data
                .get("wallet")
                .and_then(|w| w.as_str())
                .ok_or("Missing wallet field")?;
            if CHANNEL.wallet_balance_rx.borrow().1.as_deref() != Some(wallet) {
                return Ok(());
            }

            let transactions_data = if let Some(tx) = data.get("transaction") {
                if tx.is_null() {
                    Vec::new()
                } else {
                    if let Some(tx_data) = parse_tx(tx) {
                        vec![tx_data]
                    } else {
                        Vec::new()
                    }
                }
            } else {
                return Err("Missing transaction field".to_string());
            };

            if !transactions_data.is_empty() {
                CHANNEL.transactions_tx.send_modify(|state| {
                    for tx_data in transactions_data {
                        state.transactions.insert(tx_data.tx_id.clone(), tx_data);
                    }
                });
            }
            Ok(())
        }
        _ => Err("Non-text message received".to_string()),
    }
}

/// One wire transaction into the history model.
///
/// **The ONE parser for this shape.** The cached-balance reseed used to carry
/// its own inline copy of the status table; two tables that have to agree is a
/// drift hazard, and the drift would be invisible — an unrecognised status
/// makes this return `None`, which does not render an error, it makes the row
/// disappear from history. Add a status in exactly one place.
pub(crate) fn parse_tx(tx: &Value) -> Option<TransactionData> {
    let tx_id = tx.get("hash").and_then(|h| h.as_str())?.to_string();
    let status = match tx.get("status").and_then(|s| s.as_str()) {
        Some("success") => TransactionStatus::Success,
        Some("partial") => TransactionStatus::Partial,
        Some("killed") => TransactionStatus::Killed,
        Some("failed") => TransactionStatus::Failed,
        Some("pending") => TransactionStatus::Pending,
        Some("cancelled") => TransactionStatus::Cancelled,
        _ => return None,
    };
    let s = |k: &str| tx.get(k).and_then(|v| v.as_str()).map(str::to_string);
    Some(TransactionData {
        tx_id,
        status,
        execution_price: tx.get("price").and_then(|p| p.as_str()).unwrap_or("0").to_string(),
        order_type: tx.get("tx_type").and_then(|t| t.as_str()).unwrap_or_default().to_string().to_lowercase(),
        timestamp: tx.get("timestamp").and_then(|t| t.as_str()).unwrap_or_default().to_string(),
        amount: tx.get("amount").and_then(|a| a.as_str()).unwrap_or("0").to_string(),
        currency: tx.get("currency").and_then(|c| c.as_str()).unwrap_or_default().to_string(),
        fee: tx.get("fee").and_then(|f| f.as_str()).unwrap_or_default().to_string(),
        flags: tx.get("flags").and_then(|f| f.as_str()).map(|s| s.to_string()),
        receiver: tx.get("receiver").and_then(|r| r.as_str()).unwrap_or_default().to_string(),
        sender: tx.get("sender").and_then(|s| s.as_str()).unwrap_or_default().to_string(),
        sequence: tx.get("sequence").and_then(|s| s.as_u64()).map(|s| s as u32),
        destination_tag: tx.get("destination_tag").and_then(|t| t.as_u64()).and_then(|t| u32::try_from(t).ok()),
        pay_amount: s("pay_amount"),
        pay_currency: s("pay_currency"),
        filled: s("filled"),
        filled_currency: s("filled_currency"),
        received: s("received"),
        received_currency: s("received_currency"),
        remaining: s("remaining"),
        remaining_currency: s("remaining_currency"),
        coverage: tx.get("coverage").and_then(|v| v.as_f64()),
        fill_price: s("fill_price"),
    })
}
