//! `get_history` — one page of the XRP wallet's history, the `load 20 more ›`
//! at the end of the transactions or orders pane (2026-10-04). A store read
//! on the relay, never a node call. The request names the list (`kind`) and
//! how many settled rows of it the app already holds (`offset`); the reply
//! carries the next page in the `get_cached_balance` row shape plus the
//! history facts, which [`crate::channel::TransactionState::apply_reply`]
//! takes. Rows merge into the map: a repeat at the window's edge (a row
//! landed live since the last page) rewrites itself.

use crate::channel::{CHANNEL, HistoryList, WSCommand};
use crate::ws::CRYPTO_OUTGOING_TX;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

pub async fn execute(_current_wallet: String, cmd: WSCommand) -> Result<(), String> {
    let wallet = cmd.wallet.ok_or("Missing wallet parameter")?;
    let kind = cmd.history_kind.ok_or("Missing history kind")?;
    let msg_json = json!({
        "command": "get_history",
        "wallet": wallet,
        "kind": kind,
        "offset": cmd.history_offset.unwrap_or(0),
    });
    if let Some(tx) = CRYPTO_OUTGOING_TX.get() {
        let _ = tx.send(Message::text(msg_json.to_string())).await;
    }
    Ok(())
}

pub async fn process_response(message: Message, _current_wallet: &str) -> Result<(), String> {
    let Message::Text(text) = message else {
        return Err("Non-text message received".to_string());
    };
    let data: Value = serde_json::from_str(&text).map_err(|e| format!("Failed to parse JSON: {}", e))?;
    if data.get("command").and_then(|c| c.as_str()) != Some("get_history") {
        return Ok(());
    }
    // Data about the address the app holds, or nothing — the rule
    // `balances::process_response` states.
    let wallet = data.get("wallet").and_then(|w| w.as_str()).ok_or("Missing wallet field")?;
    if CHANNEL.wallet_balance_rx.borrow().1.as_deref() != Some(wallet) {
        return Ok(());
    }
    let list = match data.get("kind").and_then(|k| k.as_str()) {
        Some("transactions") => HistoryList::XrpTransactions,
        Some("orders") => HistoryList::XrpOrders,
        _ => return Err("Missing history kind".to_string()),
    };
    // The relay answers a request it could not serve with `error` and no
    // rows; the page then reads failed, and `retry ›` asks again.
    if let Some(error) = data.get("error").and_then(|e| e.as_str()) {
        CHANNEL.transactions_tx.send_modify(|state| state.page_mut(list).fail_any());
        return Err(format!("Server error: {}", error));
    }
    let rows: Vec<_> = data
        .get("transactions")
        .and_then(|t| t.as_array())
        .into_iter()
        .flatten()
        .filter_map(super::get_transaction::parse_tx)
        .collect();
    CHANNEL.transactions_tx.send_modify(|state| state.apply_reply(rows, &data, Some(list)));
    Ok(())
}
