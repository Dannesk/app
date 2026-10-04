use crate::channel::{CHANNEL, ActivityLogState, TransactionState, WSCommand};
use crate::bridge::json_storage::{self, get_config_path, remove_json, write_json};
use crate::encrypt::encrypt_data;
use bip39::{Language, Mnemonic};
use bitcoin::bip32::{DerivationPath, Xpriv};
use bitcoin::secp256k1::Secp256k1;
use ripemd::Ripemd160;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;
use zeroize::Zeroize;

use crate::secure::SecureString;

pub struct WalletOperations;

impl WalletOperations {
    /// Deletes only the encrypted private key file (xrp_encrypt.json), leaving
    /// the wallet watch-only.
    pub async fn delete_key(wallet_address: String) {
        if let Ok(path) = get_config_path("xrp_encrypt.json") {
            if path.exists() {
                let _ = remove_json("xrp_encrypt.json");
            }
        }

        let _ = json_storage::update_json("xrp.json", |data: &mut serde_json::Value| {
            if let Some(obj) = data.as_object_mut() {
                obj.insert("private_key_deleted".to_string(), serde_json::Value::Bool(true));
            }
        });

        let (current_balance, _, _, current_mode) = *CHANNEL.wallet_balance_rx.borrow();
        let _ = CHANNEL.wallet_balance_tx.send((current_balance, Some(wallet_address), true, current_mode));
    }

    /// Re-imports the key: derives the address, verifies it matches the stored
    /// wallet, then re-encrypts the seed under the given passphrase and writes
    /// xrp_encrypt.json + updates xrp.json's `method`. No backend call — the
    /// wallet is still subscribed (relay/Redis retains its data).
    ///
    /// Always Standard (passphrase/Argon2). There is no mode to choose: Cold
    /// would mean restoring a key and storing nothing, which is a contradiction.
    pub async fn reimport_key(
        mnemonic_phrase: SecureString,
        bip39_pass: SecureString,
        encryption_pass: SecureString,
    ) -> Result<(), String> {
        let (_, wallet_address, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
        let Some(expected_address) = wallet_address else {
            return Err("ERR: NO_WALLET_FOUND".to_string());
        };

        // Move the locked secrets into the worker thread — no clone, no copy.
        // The address is verified INSIDE the closure, before anything is
        // encrypted, so a mismatched phrase can never reach the disk.
        let expected = expected_address.clone();
        let result = tokio::task::spawn_blocking(
            move || -> Result<(String, String, String, String), String> {
                let mnemonic = Mnemonic::parse_in(Language::English, mnemonic_phrase.as_str())
                    .map_err(|e| format!("Invalid recovery phrase: {}", e))?;

                let mut seed = mnemonic.to_seed(bip39_pass.as_str());
                let secp = Secp256k1::new();

                let xpriv = Xpriv::new_master(bitcoin::Network::Bitcoin, &seed)
                    .map_err(|e| { seed.zeroize(); format!("Key derivation failed: {}", e) })?;
                seed.zeroize();

                let path = DerivationPath::from_str("m/44'/144'/0'/0/0")
                    .map_err(|_| "Invalid derivation path".to_string())?;
                let child = xpriv.derive_priv(&secp, &path)
                    .map_err(|e| format!("Derivation failed: {}", e))?;

                let pk_bytes = child.to_priv().public_key(&secp).inner.serialize();
                let rip_hash = Ripemd160::digest(Sha256::digest(pk_bytes));
                let mut account_id = [0u8; 21];
                account_id[0] = 0x00;
                account_id[1..].copy_from_slice(&rip_hash);

                let alphabet = bs58::Alphabet::new(b"rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz").unwrap();
                let derived_address = bs58::encode(&account_id)
                    .with_alphabet(&alphabet)
                    .with_check()
                    .into_string();

                if derived_address != expected {
                    return Err("ERR: PHRASE_ADDRESS_MISMATCH — this phrase does not match the stored wallet.".to_string());
                }

                let (enc, salt, iv) = encrypt_data(encryption_pass.as_str(), mnemonic_phrase.as_str())
                    .map_err(|e| format!("Encryption failed: {}", e))?;
                Ok((enc, salt, iv, "standard".to_string()))
            },
        ).await;

        let (enc, salt, iv, method) = match result {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err("Internal thread error".to_string()),
        };

        #[derive(Serialize)]
        struct EncryptedWalletData {
            encrypted_phrase: String,
            salt: String,
            iv: String,
        }

        if let Err(e) = write_json("xrp_encrypt.json", &EncryptedWalletData {
            encrypted_phrase: enc,
            salt,
            iv,
        }) {
            return Err(format!("FS Error: {}", e));
        }

        // Refresh both flags: clear watch-only AND record the storage method, so
        // a wallet restored after a purge isn't left with a stale `method`.
        let _ = json_storage::update_json("xrp.json", |data: &mut serde_json::Value| {
            if let Some(obj) = data.as_object_mut() {
                obj.insert("private_key_deleted".to_string(), serde_json::Value::Bool(false));
                obj.insert("method".to_string(), serde_json::Value::String(method.clone()));
            }
        });

        let (current_balance, _, _, _) = *CHANNEL.wallet_balance_rx.borrow();
        let _ = CHANNEL.wallet_balance_tx.send((current_balance, Some(expected_address), false, crate::channel::KeyMode::from_method(&method)));

        Ok(())
    }

    /// Fully removes the wallet — deletes encrypted key, removes metadata JSON, notifies backend
    pub async fn remove_wallet(wallet_address: String, ws_tx: Sender<WSCommand>) {
        let mut log = ActivityLogState::new(
            "Remove XRP wallet",
            &[
                ("keys",   "Remove keys"),
                ("wallet", "Remove wallet"),
                ("notify", "Unsubscribing wallet"),
            ],
        );
        log.start("keys");
        let _ = CHANNEL.activity_tx.send(Some(log.clone()));

        // 1. Delete the key material.
        // Unconditional: `remove_json` is idempotent and takes the `.tmp` sibling
        // with it. Guarding on the file existing would skip that cleanup in the
        // one case that needs it — a crashed write leaves a tmp full of
        // ciphertext with no file beside it.
        let _ = remove_json("xrp_encrypt.json");

        log.finish("keys");
        log.start("wallet");
        let _ = CHANNEL.activity_tx.send(Some(log.clone()));

        // 2. Delete the wallet metadata (xrp.json)
        if let Ok(path) = get_config_path("xrp.json")
            && path.exists()
            && let Err(e) = remove_json("xrp.json")
        {
            log.fail("wallet", format!("Error removing XRP wallet file: {}", e));
            let _ = CHANNEL.activity_tx.send(Some(log));
            return;
        }

        log.finish("wallet");
        log.start("notify");
        let _ = CHANNEL.activity_tx.send(Some(log.clone()));

        sleep(Duration::from_millis(1000)).await;

        // 3. Notify backend — server is fire-and-forget, so close the step here
        let _ = ws_tx.try_send(WSCommand {
            command: "delete_wallet".to_string(),
            wallet: Some(wallet_address.clone()),
            ..Default::default()
        });

        log.finish("notify");
        let _ = CHANNEL.activity_tx.send(Some(log.clone()));

        // 4. Reset UI/Channels — rows and the paging with them.
        let _ = CHANNEL.transactions_tx.send(TransactionState::default());
        let _ = CHANNEL.wallet_balance_tx.send((0.0, None, false, crate::channel::KeyMode::Standard));
        CHANNEL.clear_tokens();
        // The files are gone, so what they said about a stored 25th word has to
        // go too. Left set, it would outlive the wallet it described and tell
        // the next one's signing screens to hide a field for a phrase that was
        // never stored.
    }
}
