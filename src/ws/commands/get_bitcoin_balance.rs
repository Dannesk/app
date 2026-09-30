use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

pub async fn execute(
    _bitcoin_current_wallet: String,
    _cmd: crate::channel::WSCommand,
) -> Result<(), String> {
    // No client-initiated execution for btc_balance
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
            if command != Some("btc_balance") {
                return Ok(());
            }

            // HD: this frame carries ONE address's balance, but the channel
            // carries the wallet AGGREGATE — and the identity slot is the
            // primary (#0), which a member-address frame must never overwrite.
            // The paired `btc_utxos` frame (same relay pipeline, next message)
            // updates the union this recompute reads, so the number here is
            // only a trigger; the union is the arbiter.
            let _ = wallet;
            crate::ws::commands::get_btc_utxos::recompute_btc_balance();
        }
        _ => {
            return Err("Non-text message received".to_string());
        }
    }
    Ok(())
}
