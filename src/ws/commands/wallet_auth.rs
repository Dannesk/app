use crate::decrypt::decrypt_data;
use crate::bridge::json_storage::read_json;
use bip39::{Language, Mnemonic};
use bitcoin::bip32::{DerivationPath, Xpriv};
use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey};
use ripemd::Ripemd160;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::str::FromStr;
use zeroize::Zeroize;

use crate::secure::{SecureBytes, SecureString};

#[derive(Deserialize)]
struct EncryptedWalletData {
    encrypted_phrase: String,
    #[serde(default)]
    salt: String,
    iv: String,
}

#[derive(Clone)]
pub struct Bip44Wallet {
    pub address: String,
    pub secret_key: SecretKey,
    pub public_key: PublicKey,
}

pub fn authenticate_wallet(
    passphrase: Option<SecureString>,
    seed: Option<SecureString>,
    bip39: Option<SecureString>,
    wallet_address: &str,
) -> Result<Bip44Wallet, String> {
    // The blob holds the bare mnemonic and nothing else — the stored-25th-word
    // feature is gone, so there is no payload document to parse. The 25th word
    // is always the typed one.
    let mnemonic_text: SecureString = match (passphrase, seed) {
        // Seed-mode: the typed phrase IS the mnemonic — move it in, no copy.
        (None, Some(s)) => s,
        // Standard: passphrase → Argon2id → AES-GCM decrypt the stored blob.
        (Some(p), None) => {
            let stored_data: EncryptedWalletData = read_json("xrp_encrypt.json")
                .map_err(|e| format!("Error: XRP credentials not found: {}", e))?;

            decrypt_data(
                p.as_str(),
                &stored_data.encrypted_phrase,
                &stored_data.salt,
                &stored_data.iv,
            )
            .map_err(|_| "Error: Decryption failed".to_string())?
        }
        _ => return Err("Error: Must provide exactly one of passphrase or seed".to_string()),
    };

    let mnemonic = Mnemonic::parse_in(Language::English, mnemonic_text.as_str())
        .map_err(|_| "Error: Invalid mnemonic".to_string())?;

    let seed_passphrase = bip39.as_ref().map(|s| s.as_str()).unwrap_or("");
    let mut seed_arr = mnemonic.to_seed(seed_passphrase);
    // Hold the raw seed in mlocked memory; wipe the transient stack array.
    let bip39_seed = SecureBytes::new(seed_arr.to_vec());
    seed_arr.zeroize();

    let secp = Secp256k1::new();

    // `bip39_seed` is zeroized + unlocked when it drops at end of scope.
    let xpriv = Xpriv::new_master(bitcoin::Network::Bitcoin, bip39_seed.as_bytes())
        .map_err(|e| format!("Error: Failed to create master key: {}", e))?;

    let path = DerivationPath::from_str("m/44'/144'/0'/0/0")
        .map_err(|_| "Error: Invalid derivation path".to_string())?;

    let child_xpriv = xpriv
        .derive_priv(&secp, &path)
        .map_err(|e| format!("Error: Derivation failed: {}", e))?;

    let secret_key = child_xpriv.private_key;
    let public_key = child_xpriv.to_priv().public_key(&secp).inner;

    // Prove the key we just built belongs to the wallet we were asked to open.
    //
    // Without this the function returns whatever address it was *handed* beside
    // a key derived from whatever the user typed, and nothing checks that the
    // two are related. A wrong 25th word is the ordinary way to reach that
    // state: it is optional, it is never stored, and `to_seed` accepts any
    // string at all — so an omitted or mistyped word silently derives a
    // different, perfectly valid wallet. The mismatch then surfaces as a
    // rejected submission from the ledger, long after the credential prompt,
    // in an error that says nothing about the word that caused it.
    //
    // `reimport_key` has always checked this (bridge/xrp_wallet_operations.rs);
    // the signing path never did. Same derivation, same alphabet, same
    // comparison, so the two cannot disagree about what "this wallet" means.
    let account_id = {
        let rip_hash = Ripemd160::digest(Sha256::digest(public_key.serialize()));
        let mut id = [0u8; 21];
        id[0] = 0x00;
        id[1..].copy_from_slice(&rip_hash);
        id
    };
    let alphabet = bs58::Alphabet::new(b"rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz")
        .map_err(|_| "Error: Invalid address alphabet".to_string())?;
    let derived_address = bs58::encode(&account_id)
        .with_alphabet(&alphabet)
        .with_check()
        .into_string();
    if !wallet_address.is_empty() && derived_address != wallet_address {
        return Err(
            "Error: that credential opens a different wallet — check your 25th word".to_string(),
        );
    }

    Ok(Bip44Wallet {
        // The derived one, not the argument: after the check they are equal,
        // and returning what we proved keeps the struct self-consistent.
        address: derived_address,
        secret_key,
        public_key,
    })
}
