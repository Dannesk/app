use crate::channel::{CHANNEL, WSCommand};
use crate::ws::CRYPTO_OUTGOING_TX;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

pub async fn execute(
    _current_wallet: String,
    cmd: WSCommand,
) -> Result<(), String> {
    if let Some(wallet) = &cmd.wallet {
        let msg_json = json!({ "command": "get_cached_balance", "wallet": wallet });
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
            let data: Value =
                serde_json::from_str(&text).map_err(|e| format!("Failed to parse JSON: {}", e))?;

            if data.get("command").and_then(|c| c.as_str()) != Some("get_cached_balance") {
                return Ok(());
            }

            if let Some(error) = data.get("error").and_then(|e| e.as_str()) {
                return Err(format!("Server error: {}", error));
            }

            let wallet = data
                .get("wallet")
                .and_then(|w| w.as_str())
                .ok_or_else(|| "Missing wallet field".to_string())?;

            let balance_xrp = data
                .get("balance")
                .and_then(|b| b.as_str())
                .and_then(|b| b.parse::<f64>().ok())
                .map(|b| b / 1_000_000.0)
                .unwrap_or(0.0);

            let transactions_data = if let Some(tx_value) = data.get("transactions") {
                if tx_value.is_null() {
                    Vec::new()
                } else if let Some(tx_array) = tx_value.as_array() {
                    // ONE parser for this shape — see `get_transaction::parse_tx`.
                    // This site used to carry an inline copy of it, including
                    // its own status table, and a status the copy did not know
                    // silently dropped the row instead of failing loudly.
                    tx_array
                        .iter()
                        .filter_map(super::get_transaction::parse_tx)
                        .collect()
                } else {
                    return Err("Invalid transaction format".to_string());
                }
            } else {
                Vec::new()
            };

            let (_, _, private_key_deleted, key_mode) = *CHANNEL.wallet_balance_rx.borrow();
            let _ = CHANNEL.wallet_balance_tx.send((balance_xrp, Some(wallet.to_string()), private_key_deleted, key_mode));
            // exists: true/false is the node's own answer (account_info /
            // stream meta, cached relay-side); null or absent means a legacy
            // hash or old relay — infer from the balance at write time, and the
            // next reseed writes the real answer.
            let exists = data
                .get("exists")
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| crate::utils::reserves::is_activated(balance_xrp));
            CHANNEL.set_xrp_exists(exists);
            CHANNEL.apply_xrp_account(&data);
            // One pass over the registry: each token parses its own balance,
            // has-trustline flag and limit from its named JSON fields. Adding a
            // token needs no edit here.
            for token in crate::utils::tokens::TOKENS {
                let balance = data
                    .get(token.balance_field)
                    .and_then(|v| v.as_str())
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(0.0);
                let has = data
                    .get(token.has_field)
                    .and_then(|h| h.as_bool())
                    .unwrap_or(false);
                let limit = data
                    .get(token.trustline_limit_field)
                    .and_then(|l| l.as_str())
                    .and_then(|l| l.parse::<f64>().ok());
                CHANNEL.set_token(token.code, (balance, has, limit));
            }

            // Rows merge, never replace, and the history facts — what
            // `load 20 more ›` can page to — ride along even when the reply
            // carries no rows.
            CHANNEL.transactions_tx.send_modify(|state| state.apply_reply(transactions_data, &data, None));

            // The balance, the account and the tokens are all in.
            CHANNEL.loaded_tx.send_if_modified(|l| !std::mem::replace(&mut l.xrp, true));

            Ok(())
        }
        _ => Err("Non-text message received".to_string()),
    }
}
