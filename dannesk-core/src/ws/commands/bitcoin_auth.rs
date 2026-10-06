use crate::decrypt::decrypt_data;
use crate::bridge::json_storage::read_json;
use bip39::Mnemonic;
use bitcoin::bip32::{DerivationPath, Xpriv};
use bitcoin::key::{CompressedPublicKey, PrivateKey};
use bitcoin::network::Network;
use bitcoin::secp256k1::Secp256k1;
use bitcoin::Address;
use serde::Deserialize;
use std::str::FromStr;
use zeroize::{Zeroize, Zeroizing};

use crate::secure::{SecureBytes, SecureString};

/// The authenticated wallet: the primary (#0) identity plus one WIF per
/// watched address record. HD wallets hold coins on several derived addresses
/// of one seed; every input is signed with ITS owner's key, looked up here.
/// Keys live in `Zeroizing` and the map is private — they move out of this
/// module only as borrowed WIF strings at signing time.
#[derive(Debug)]
pub struct BitcoinWallet {
    /// #0 — the identity the flow validated against, and the change target.
    pub address: String,
    keys: std::collections::HashMap<String, Zeroizing<String>>,
}

impl BitcoinWallet {
    pub fn wif_for(&self, address: &str) -> Option<&str> {
        self.keys.get(address).map(|w| w.as_str())
    }

    /// A wallet from bare (address, WIF) pairs — the signer's tests build
    /// one key per script family without a seed or a btc.json.
    #[cfg(test)]
    pub(crate) fn for_test(address: String, keys: Vec<(String, String)>) -> Self {
        Self {
            address,
            keys: keys.into_iter().map(|(a, w)| (a, Zeroizing::new(w))).collect(),
        }
    }
}

#[derive(Deserialize)]
struct EncryptedWalletData {
    encrypted_phrase: String,
    #[serde(default)]
    salt: String,
    iv: String,
}

pub fn authenticate_wallet(
    passphrase: Option<SecureString>,
    seed: Option<SecureString>,
    bip39: Option<SecureString>,
    wallet_address: &str,
) -> Result<BitcoinWallet, String> {
    // The blob holds the bare mnemonic and nothing else — the stored-25th-word
    // feature is gone, so there is no payload document to parse. The 25th word
    // is always the typed one.
    let mnemonic_text: SecureString = match (passphrase, seed) {
        // Seed-mode: the typed phrase IS the mnemonic — move it in, no copy.
        (None, Some(s)) => s,
        // Standard: passphrase → Argon2id → AES-GCM decrypt the stored blob.
        (Some(p), None) => {
            let stored_data: EncryptedWalletData = read_json("btc_encrypt.json")
                .map_err(|e| format!("Error: Encrypted wallet file not found or corrupted: {}", e))?;

            decrypt_data(
                p.as_str(),
                &stored_data.encrypted_phrase,
                &stored_data.salt,
                &stored_data.iv,
            )
            .map_err(|e| format!("Error: Decryption failed: {}", e))?
        }
        _ => return Err("Error: Must provide exactly one of passphrase or seed".to_string()),
    };

    let mnemonic = Mnemonic::from_str(mnemonic_text.as_str())
        .map_err(|e| format!("Error: Invalid mnemonic: {}", e))?;

    let seed_passphrase = bip39.as_ref().map(|s| s.as_str()).unwrap_or("");
    let mut seed_arr = mnemonic.to_seed(seed_passphrase);
    // Hold the raw seed in mlocked memory; wipe the transient stack array.
    let seed_bytes = SecureBytes::new(seed_arr.to_vec());
    seed_arr.zeroize();

    let network = Network::Bitcoin;
    let secp = Secp256k1::new();

    let xpriv = Xpriv::new_master(network, seed_bytes.as_bytes())
        .map_err(|e| format!("Error: Failed to create master key: {}", e))?;

    let derivation_path = DerivationPath::from_str("m/84'/0'/0'/0/0").unwrap();
    let child_xpriv = xpriv
        .derive_priv(&secp, &derivation_path)
        .map_err(|e| format!("Error: Failed to derive private key: {}", e))?;

    let private_key = child_xpriv.to_priv();

    // Prove the key we just built belongs to the wallet we were asked to open.
    //
    // Without this the function returns whatever address it was *handed* beside
    // a key derived from whatever the user typed, and nothing checks that the
    // two are related. A wrong 25th word is the ordinary way to reach that
    // state: it is optional, it is never stored, and `to_seed` accepts any
    // string at all — so an omitted or mistyped word silently derives a
    // different, perfectly valid wallet. The mismatch then surfaces as a
    // rejected broadcast from the network, long after the credential prompt,
    // in an error that says nothing about the word that caused it.
    //
    // `reimport_key` has always checked this (bridge/btc_wallet_operations.rs);
    // the signing path never did. Same derivation, same comparison, so the two
    // cannot disagree about what "this wallet" means.
    let derived_address =
        Address::p2wpkh(&CompressedPublicKey(private_key.public_key(&secp).inner), network)
            .to_string();

    // The other three address types (2026-09-14): #0 at the STORED type's
    // purpose and encoding. Native keeps the frozen block above verbatim;
    // the comparison below then proves the stored type against the seed the
    // same way it proves the 25th word.
    let script_type = crate::btc_script_type::stored();
    let (private_key, derived_address) = match script_type {
        crate::btc_script_type::BtcScriptType::NativeSegwit => (private_key, derived_address),
        other => {
            let child = xpriv
                .derive_priv(&secp, &other.member_path(0, 0))
                .map_err(|e| format!("Error: Failed to derive private key: {}", e))?;
            let key = child.to_priv();
            let address = other.address(&secp, &CompressedPublicKey(key.public_key(&secp).inner)).to_string();
            (key, address)
        }
    };
    if !wallet_address.is_empty() && derived_address != wallet_address {
        return Err(
            "Error: that credential opens a different wallet — check your 25th word".to_string(),
        );
    }

    let private_key_wif = PrivateKey {
        compressed: true,
        network: bitcoin::network::NetworkKind::Main,
        inner: private_key.inner,
    }
    .to_wif();

    let mut keys = std::collections::HashMap::new();
    keys.insert(derived_address.clone(), Zeroizing::new(private_key_wif));

    // HD members: one key per btc.json record beyond #0, each derived at ITS
    // recorded (chain, index) and verified against the recorded address before
    // it may sign anything. A mismatch means btc.json is corrupt or from a
    // different seed — refusing outright beats signing an input with a key
    // the network will reject (or worse, one that happens to be valid for a
    // record we mislabeled). A NEW derivation site, self-contained on purpose;
    // the #0 path above is frozen and stays as it was.
    for record in crate::wallet::btc_address_records() {
        if record.chain == 0 && record.index == 0 {
            continue; // #0 handled above, against the caller's expectation
        }
        let member_path = script_type.member_path(record.chain, record.index);
        let member_xpriv = xpriv
            .derive_priv(&secp, &member_path)
            .map_err(|e| format!("Error: member key derivation failed: {}", e))?;
        let member_key = member_xpriv.to_priv();
        let member_address = script_type
            .address(&secp, &CompressedPublicKey(member_key.public_key(&secp).inner))
            .to_string();
        if member_address != record.address {
            return Err(format!(
                "Error: derived address for {}/{} does not match the stored wallet",
                record.chain, record.index
            ));
        }
        let member_wif = PrivateKey {
            compressed: true,
            network: bitcoin::network::NetworkKind::Main,
            inner: member_key.inner,
        }
        .to_wif();
        keys.insert(member_address, Zeroizing::new(member_wif));
    }

    // Backfill: a wallet imported before the HD work has no account xpub on
    // disk, and this is the one moment the seed is in hand anyway. With it,
    // the receive screen can rotate; without it, it falls back to #0. Written
    // only when absent — an existing value is never overwritten (the address
    // check above already proved this seed IS the stored wallet).
    let stored: Result<serde_json::Value, _> = read_json("btc.json");
    if let Ok(stored) = stored
        && stored.get("account_xpub").and_then(|v| v.as_str()).is_none_or(|s| s.is_empty())
    {
        let account_xpriv = xpriv
            .derive_priv(&secp, &script_type.account_path())
            .map_err(|e| format!("Error: account derivation failed: {}", e))?;
        let account_xpub = bitcoin::bip32::Xpub::from_priv(&secp, &account_xpriv).to_string();
        let _ = crate::bridge::json_storage::update_json("btc.json", |data: &mut serde_json::Value| {
            if let Some(obj) = data.as_object_mut() {
                obj.insert("account_xpub".to_string(), serde_json::Value::String(account_xpub.clone()));
            }
        });
    }

    Ok(BitcoinWallet {
        // The derived one, not the argument: after the check they are equal,
        // and returning what we proved keeps the struct self-consistent.
        address: derived_address,
        keys,
    })
}
