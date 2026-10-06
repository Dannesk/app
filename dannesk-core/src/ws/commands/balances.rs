use crate::channel::{CHANNEL, WSCommand};
use crate::ws::CRYPTO_OUTGOING_TX;
use crate::utils::tokens;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

/// Is this a live-balance command we handle? `xrp_balance` is the native asset;
/// every issued token is recognized by its registry `balance_cmd`.
fn is_balance_command(command: &str) -> bool {
    command == "xrp_balance" || tokens::by_balance_cmd(command).is_some()
}

// ====================== Main Logic ======================

pub async fn execute(
    _current_wallet: String,
    cmd: WSCommand,
) -> Result<(), String> {
    let wallet = cmd.wallet.as_ref().ok_or("Missing wallet parameter")?;

    if !is_balance_command(&cmd.command) {
        return Ok(());
    }

    let msg = json!({ "command": cmd.command, "wallet": wallet });

    if let Some(tx) = CRYPTO_OUTGOING_TX.get() {
        tx.send(Message::text(msg.to_string())).await.map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub async fn process_response(message: Message, _current_wallet: &str) -> Result<(), String> {
    let Message::Text(text) = message else {
        return Err("Non-text message received".to_string());
    };

    let data: Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    let wallet = data
        .get("wallet")
        .and_then(|w| w.as_str())
        .ok_or("Missing wallet field")?;

    let command_str = data
        .get("command")
        .and_then(|c| c.as_str())
        .ok_or("Missing command field")?;

    // A push is DATA about an address, never a wallet to adopt. The app's
    // wallet is what the key file put in the channel (app open, import or
    // create success) and nothing else; a push for any other address is
    // nothing. The one live case (2026-09-21): an import the relay subscribed
    // at the node and then never answered, so no file was written, followed
    // by a transaction on that address — writing the frame's `wallet` into
    // the channel, which this used to do, put a wallet with no key file on
    // the dashboard and hid import until restart.
    if CHANNEL.wallet_balance_rx.borrow().1.as_deref() != Some(wallet) {
        return Ok(());
    }

    // Native XRP: balance arrives in drops under "balance" and feeds the wallet
    // channel's balance slot — the identity slot is never written from a
    // frame. Issued tokens are looked up in the registry by command, parsed
    // from their own JSON field, and written to the token map by code.
    if command_str == "xrp_balance" {
        let raw = data
            .get("balance")
            .and_then(|v| v.as_str())
            .ok_or("Missing balance field")?;
        let balance = raw
            .parse::<f64>()
            .map_err(|_| format!("Invalid balance format: {}", raw))?
            / 1_000_000.0;
        CHANNEL.wallet_balance_tx.send_modify(|state| {
            state.0 = balance;
        });
        CHANNEL.apply_xrp_account(&data);
        // Chain-asserted existence rides the same message (relay sets it from
        // validated tx meta). An old relay omits the field — fall back to the
        // balance-threshold inference at write time.
        let exists = data
            .get("exists")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| crate::utils::reserves::is_activated(balance));
        CHANNEL.set_xrp_exists(exists);
    } else if let Some(token) = tokens::by_balance_cmd(command_str) {
        let raw = data
            .get(token.balance_field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("Missing {} field", token.balance_field))?;
        let balance = raw
            .parse::<f64>()
            .map_err(|_| format!("Invalid balance format: {}", raw))?;
        CHANNEL.set_token_balance(token.code, balance);
    }

    Ok(())
}
