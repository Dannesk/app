use crate::channel::WSCommand;
use dannesk_xrpl_codec::Field;
use crate::ws::commands::{offer_cancel, offer_create, payment, trustset};
use crate::ws::commands::wallet_auth::Bip44Wallet;
use bitcoin::secp256k1::{Message, Secp256k1};

pub async fn construct_blob(
    wallet_obj: &Bip44Wallet,
    cmd: &WSCommand,
    tx_type: &str,
    sequence: u32,
    fee: u64,
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

/// Sign an XRPL transaction with the wallet's key and return the blob to
/// submit, as uppercase hex. `fields` is the whole transaction except
/// SigningPubKey and TxnSignature, which are added here: this is the one place
/// an XRPL transaction is signed.
pub fn sign(wallet_obj: &Bip44Wallet, mut fields: Vec<Field>) -> Result<String, String> {
    // The compressed 33-byte public key is part of what gets signed.
    fields.push(dannesk_xrpl_codec::signing_pub_key(&wallet_obj.public_key.serialize()));
    let digest = dannesk_xrpl_codec::signing_hash(&fields)?;

    // sign_ecdsa gives the low-S (canonical) signature the ledger requires.
    let sig = Secp256k1::new().sign_ecdsa(&Message::from_digest(digest), &wallet_obj.secret_key);
    fields.push(dannesk_xrpl_codec::txn_signature(&sig.serialize_der()));

    Ok(hex::encode_upper(dannesk_xrpl_codec::encode(&fields)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dannesk_xrpl_codec::TransactionType;
    use bitcoin::secp256k1::{ecdsa::Signature, PublicKey, SecretKey};

    #[test]
    fn the_signature_covers_the_key_and_verifies() {
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[0x11; 32]).unwrap();
        let wallet = Bip44Wallet {
            address: "rLSn6Z3T8uCxbcd1oxwfGQN1Fdn5CyGujK".to_string(),
            secret_key,
            public_key: PublicKey::from_secret_key(&secp, &secret_key),
        };
        let fields = || {
            vec![
                dannesk_xrpl_codec::transaction_type(TransactionType::OfferCancel),
                dannesk_xrpl_codec::account(&wallet.address).unwrap(),
                dannesk_xrpl_codec::fee(12).unwrap(),
                dannesk_xrpl_codec::sequence(1),
                dannesk_xrpl_codec::offer_sequence(7),
            ]
        };
        let blob = hex::decode(sign(&wallet, fields()).unwrap()).unwrap();

        // SigningPubKey (0x73, 33 bytes) is followed by TxnSignature (0x74,
        // length, DER): pull the signature out of the blob.
        let key = wallet.public_key.serialize();
        let after_key = blob
            .windows(35)
            .position(|w| w[..2] == [0x73, 0x21] && w[2..] == key)
            .expect("SigningPubKey in the blob")
            + 35;
        assert_eq!(blob[after_key], 0x74);
        let len = blob[after_key + 1] as usize;
        let sig = Signature::from_der(&blob[after_key + 2..after_key + 2 + len]).unwrap();

        // It verifies over the signing hash of the fields with the key in them,
        // and it is already low-S.
        let mut with_key = fields();
        with_key.push(dannesk_xrpl_codec::signing_pub_key(&key));
        let digest = dannesk_xrpl_codec::signing_hash(&with_key).unwrap();
        assert!(secp.verify_ecdsa(&Message::from_digest(digest), &sig, &wallet.public_key).is_ok());
        let mut low_s = sig;
        low_s.normalize_s();
        assert_eq!(low_s, sig);

        // And the blob is exactly those fields plus that signature.
        with_key.push(dannesk_xrpl_codec::txn_signature(&sig.serialize_der()));
        assert_eq!(dannesk_xrpl_codec::encode(&with_key).unwrap(), blob);
    }
}
