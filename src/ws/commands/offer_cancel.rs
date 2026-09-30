// ws/commands/offer_cancel.rs
use crate::channel::WSCommand;
use crate::ws::commands::wallet_auth::Bip44Wallet;
use rippled_binary_codec::serialize::serialize_tx;

use bitcoin::secp256k1::{Message, Secp256k1};
use sha2::{Digest, Sha512};

pub async fn construct_blob(
    wallet_obj: &Bip44Wallet,
    cmd: &WSCommand,
    sequence: u32,
    fee: String,
    last_ledger_sequence: u32,
) -> Result<String, String> {
    let offer_sequence = cmd.offer_sequence.ok_or("Missing offer_sequence")?;
    let pub_key_hex = hex::encode(wallet_obj.public_key.serialize()).to_uppercase();

    let mut tx_json_val = serde_json::json!({
        "TransactionType": "OfferCancel",
        "Account": wallet_obj.address,
        "Fee": fee,
        "Sequence": sequence,
        "LastLedgerSequence": last_ledger_sequence,
        "OfferSequence": offer_sequence,
        "Flags": 0,
    });

    // 1. Inject SigningPubKey
    if let Some(obj) = tx_json_val.as_object_mut() {
        obj.insert("SigningPubKey".to_string(), serde_json::Value::String(pub_key_hex));
    }

    // 2. Hash the Binary (Unsigned)
    let unsigned_tx_json = serde_json::to_string(&tx_json_val).unwrap();
    let unsigned_hex = serialize_tx(unsigned_tx_json, false)
        .ok_or_else(|| "Binary encoding failed".to_string())?;
    let unsigned_bytes = hex::decode(&unsigned_hex).unwrap();

    // 3. Prefix + SHA-512/256 (STX\0)
    let mut payload = Vec::new();
    payload.extend_from_slice(&[0x53, 0x54, 0x58, 0x00]);
    payload.extend_from_slice(&unsigned_bytes);

    let mut hasher = Sha512::new();
    hasher.update(&payload);
    let hash = hasher.finalize();

    // 4. ECDSA Sign
    let secp = Secp256k1::new();
    let message = Message::from_digest_slice(&hash[0..32]).unwrap();
    let sig = secp.sign_ecdsa(&message, &wallet_obj.secret_key);
    let sig_hex = hex::encode(sig.serialize_der().as_ref()).to_uppercase();

    // 5. Inject TxnSignature
    if let Some(obj) = tx_json_val.as_object_mut() {
        obj.insert("TxnSignature".to_string(), serde_json::Value::String(sig_hex));
    }

    // 6. Final Blob
    let final_tx_json = serde_json::to_string(&tx_json_val).unwrap();
    let tx_blob = serialize_tx(final_tx_json, false)
        .ok_or_else(|| "Final encoding failed".to_string())?;

    Ok(tx_blob)
}
