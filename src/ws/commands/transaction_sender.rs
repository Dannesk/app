use crate::ws::CRYPTO_OUTGOING_TX;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

/// Hand the signed blob to the relay. Returns the `tx_id` minted for it — the
/// correlation id the relay echoes back on `submit_transaction_response`, and
/// the only thing in the reply that says WHICH order it is about. Responses
/// are routed by command string alone (`ws::relay`), so without this a second
/// order in flight has its answer applied to the first.
pub async fn send_transaction(
    wallet: &str,
    tx_type: &str,
    tx_blob: String,
) -> Result<String, String> {
    let tx_id = Uuid::new_v4().to_string();
    let msg_json = json!({
        "command": "submit_transaction",
        "wallet": wallet,
        "tx_type": tx_type,
        "tx_id": tx_id,
        "signed_blob": json!({ "tx_blob": tx_blob })
    });

    if let Some(tx) = CRYPTO_OUTGOING_TX.get() {
        if tx.send(Message::text(msg_json.to_string())).await.is_err() {
            return Err("Failed to send transaction".to_string());
        }
    } else {
        return Err("Internal error: outgoing channel not initialized".to_string());
    }

    Ok(tx_id)
}
