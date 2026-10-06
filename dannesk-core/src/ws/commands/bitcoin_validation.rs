use crate::channel::WSCommand;

pub fn validate_inputs(
    cmd: &WSCommand,
    bitcoin_current_wallet: &str,
) -> Result<(String, String, String), String> {
    let tx_type = cmd
        .tx_type
        .as_ref()
        .ok_or_else(|| "Missing tx_type".to_string())?
        .to_string();

    let wallet = cmd
        .wallet
        .as_ref()
        .ok_or_else(|| "Missing wallet".to_string())?
        .to_string();

    if cmd.passphrase.is_none() && cmd.seed.is_none() {
        return Err("Error: Must provide a passphrase, seed, or PIN".to_string());
    }

    if !bitcoin_current_wallet.is_empty() && wallet != bitcoin_current_wallet {
        return Err(format!("Wallet mismatch: {} != {}", wallet, bitcoin_current_wallet));
    }

    if tx_type != "BTC" {
        return Err("Invalid transaction type for Bitcoin".to_string());
    }

    Ok((tx_type, wallet, String::new()))
}
