use crate::channel::WSCommand;

pub fn validate_inputs(
    cmd: &WSCommand,
    current_wallet: &mut String,
) -> Result<(String, String, String), String> {
    static FAILED: &str = "Error: Transaction failed";

    let tx_type = match &cmd.tx_type {
        Some(tx_type) => tx_type.clone(),
        None => return Err(FAILED.to_string()),
    };

    let wallet = match &cmd.wallet {
        Some(wallet) => wallet.clone(),
        None => return Err(FAILED.to_string()),
    };

    if cmd.passphrase.is_none() && cmd.seed.is_none() {
        return Err("Error: Must provide a passphrase, seed, or PIN".to_string());
    }
    if !current_wallet.is_empty() && *current_wallet != wallet {
        return Err(FAILED.to_string());
    }
    *current_wallet = wallet.clone();

    Ok((tx_type, wallet, String::new()))
}
