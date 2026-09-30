//! The Bitcoin wallet's LIVE list (2026-09-20): the few addresses worth a live
//! push — the master, the receive and change addresses on offer, and a paid
//! one until it confirms — sent WHOLE, replacing the last list. The list is
//! built by `bridge::btc_receive_rotation::send_live_list`; this only puts it
//! on the wire. The relay sends no reply: the pushes that follow are the answer.

use crate::channel::WSCommand;
use crate::ws::CRYPTO_OUTGOING_TX;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

pub async fn execute(_bitcoin_current_wallet: String, cmd: WSCommand) -> Result<(), String> {
    let (Some(wallet), Some(addresses)) = (cmd.wallet, cmd.scan) else {
        return Err("Missing wallet or address list".to_string());
    };
    let msg_json = json!({
        "command": "subscribe_bitcoin_addresses",
        "wallet": wallet,
        "addresses": addresses,
    });
    if let Some(tx) = CRYPTO_OUTGOING_TX.get() {
        let _ = tx.send(Message::text(msg_json.to_string())).await;
    }
    Ok(())
}
