use crate::channel::WSCommand;
use crate::ws::commands::{offer_cancel, offer_create, payment, trustset};
use crate::ws::commands::wallet_auth::Bip44Wallet;

pub async fn construct_blob(
    wallet_obj: &Bip44Wallet,
    cmd: &WSCommand,
    tx_type: &str,
    sequence: u32,
    fee: String,
    last_ledger_sequence: u32,
) -> Result<String, String> {
    let tx_blob = match tx_type {
        "payment"      => payment::construct_blob(wallet_obj, cmd, sequence, fee, last_ledger_sequence).await,
        "trustset"     => trustset::construct_blob(wallet_obj, cmd, sequence, fee, last_ledger_sequence).await,
        "offer_create"  => offer_create::construct_blob(wallet_obj, cmd, sequence, fee, last_ledger_sequence).await,
        "offer_cancel"  => offer_cancel::construct_blob(wallet_obj, cmd, sequence, fee, last_ledger_sequence).await,
        _               => return Err(format!("Unknown transaction type: {}", tx_type)),
    }?;

    Ok(tx_blob)
}
