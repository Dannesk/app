use crate::channel::{CHANNEL, WSCommand};
use crate::ws::CRYPTO_OUTGOING_TX;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

fn fail_log(msg: &str) {
    let mut log_opt = CHANNEL.activity_tx.borrow().clone();
    if let Some(ref mut log) = log_opt {
        log.fail_active(msg.to_string());
        let _ = CHANNEL.activity_tx.send(log_opt.clone());
    }
}

pub async fn execute(
    _current_wallet: String,
    cmd: WSCommand,
) -> Result<(), String> {
    static FAILED: &str = "Error: Wallet deletion failed";
    if let Some(wallet) = cmd.wallet {
        let msg_json = json!({"command": "delete_wallet", "wallet": wallet});

        if let Some(tx) = CRYPTO_OUTGOING_TX.get() {
            if tx.send(Message::text(msg_json.to_string())).await.is_err() {
                return Err(FAILED.to_string());
            }
        }
        Ok(())
    } else {
        Err(FAILED.to_string())
    }
}

pub async fn process_response(message: Message, _current_wallet: &str) -> Result<(), String> {
    static FAILED: &str = "Error: Wallet deletion failed";
    match message {
        Message::Text(text) => {
            let data: Value = serde_json::from_str(&text).map_err(|_| {
                fail_log(FAILED);
                FAILED.to_string()
            })?;

            if data.get("status").and_then(|s| s.as_str()) == Some("deleted") {
                let mut log_opt = CHANNEL.activity_tx.borrow().clone();
                if let Some(ref mut log) = log_opt {
                    log.finish("notify");
                    let _ = CHANNEL.activity_tx.send(log_opt.clone());
                }
            }
        }
        _ => {}
    }
    Ok(())
}
