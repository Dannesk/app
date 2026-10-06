// src/bridge/btc_wallet_operations.rs

use crate::channel::{BtcTransactionState, CHANNEL, ActivityLogState, WSCommand};
use crate::bridge::json_storage::{self, get_config_path, remove_json, write_json};
use crate::encrypt::encrypt_data;
use bip39::{Language, Mnemonic};
use bitcoin::address::Address;
use bitcoin::bip32::{DerivationPath, Xpriv};
use bitcoin::secp256k1::Secp256k1;
use bitcoin::{CompressedPublicKey, Network};
use serde::Serialize;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;
use zeroize::Zeroize;

use crate::secure::SecureString;

pub struct BitcoinWalletOperations;

impl BitcoinWalletOperations {
    /// Deletes only the encrypted private key file (btc_encrypt.json), leaving
    /// the wallet watch-only.
    pub async fn delete_key(wallet_address: String) {
        if let Ok(path) = get_config_path("btc_encrypt.json") {
            if path.exists() {
                let _ = remove_json("btc_encrypt.json");
            }
        }

        let _ = json_storage::update_json("btc.json", |data: &mut serde_json::Value| {
            if let Some(obj) = data.as_object_mut() {
                obj.insert("private_key_deleted".to_string(), serde_json::Value::Bool(true));
            }
        });

        let (current_balance, _, _, current_mode) = *CHANNEL.bitcoin_wallet_rx.borrow();
        let _ = CHANNEL.bitcoin_wallet_tx.send((current_balance, Some(wallet_address), true, current_mode));
    }

    /// Re-imports the key: derives the address, verifies it matches the stored
    /// wallet, then re-encrypts the seed under the given passphrase and writes
    /// btc_encrypt.json + updates btc.json's `method`. No backend call — the
    /// wallet is still subscribed (relay/Redis retains its data).
    ///
    /// Always Standard (passphrase/Argon2). There is no mode to choose: Cold
    /// would mean restoring a key and storing nothing, which is a contradiction.
    pub async fn reimport_key(
        mnemonic_phrase: SecureString,
        bip39_pass: SecureString,
        encryption_pass: SecureString,
    ) -> Result<(), String> {
        let (_, wallet_address, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
        let Some(expected_address) = wallet_address else {
            return Err("ERR: NO_WALLET_FOUND".to_string());
        };

        // Move the locked secrets into the worker thread — no clone, no copy.
        // The address is verified INSIDE the closure, before anything is
        // encrypted, so a mismatched phrase can never reach the disk.
        let expected = expected_address.clone();
        let script_type = crate::btc_script_type::stored();
        let result = tokio::task::spawn_blocking(
            move || -> Result<(String, String, String, String, String), String> {
                let mnemonic = Mnemonic::parse_in(Language::English, mnemonic_phrase.as_str())
                    .map_err(|e| format!("Invalid recovery phrase: {}", e))?;

                let mut seed = mnemonic.to_seed(bip39_pass.as_str());
                let secp = Secp256k1::new();
                let network = Network::Bitcoin;

                let xpriv = Xpriv::new_master(network, &seed)
                    .map_err(|e| { seed.zeroize(); format!("Key derivation failed: {}", e) })?;
                seed.zeroize();

                let path = DerivationPath::from_str("m/84'/0'/0'/0/0")
                    .map_err(|_| "Invalid derivation path".to_string())?;
                let child = xpriv.derive_priv(&secp, &path)
                    .map_err(|e| format!("Derivation failed: {}", e))?;

                let public_key = child.to_priv().public_key(&secp);
                let compressed_pubkey = CompressedPublicKey(public_key.inner);
                let derived_address = Address::p2wpkh(&compressed_pubkey, network).to_string();

                // The other three address types (2026-09-14): #0 at the
                // stored type's purpose and encoding. Native keeps the frozen
                // block above verbatim.
                let derived_address = match script_type {
                    crate::btc_script_type::BtcScriptType::NativeSegwit => derived_address,
                    other => {
                        let child = xpriv
                            .derive_priv(&secp, &other.member_path(0, 0))
                            .map_err(|e| format!("Derivation failed: {}", e))?;
                        let pk = CompressedPublicKey(child.to_priv().public_key(&secp).inner);
                        other.address(&secp, &pk).to_string()
                    }
                };

                if derived_address != expected {
                    return Err("ERR: PHRASE_ADDRESS_MISMATCH — this phrase does not match the stored wallet.".to_string());
                }

                // Neutered account key for receive rotation — backfilled below
                // if btc.json predates the HD work. NEW site; the #0 block
                // above is frozen.
                let account_xpriv = xpriv
                    .derive_priv(&secp, &script_type.account_path())
                    .map_err(|e| format!("Account derivation failed: {}", e))?;
                let account_xpub =
                    bitcoin::bip32::Xpub::from_priv(&secp, &account_xpriv).to_string();

                let (enc, salt, iv) = encrypt_data(encryption_pass.as_str(), mnemonic_phrase.as_str())
                    .map_err(|e| format!("Encryption failed: {}", e))?;
                Ok((enc, salt, iv, "standard".to_string(), account_xpub))
            },
        ).await;

        let (enc, salt, iv, method, account_xpub) = match result {
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

        if let Err(e) = write_json("btc_encrypt.json", &EncryptedWalletData {
            encrypted_phrase: enc,
            salt,
            iv,
        }) {
            return Err(format!("FS Error: {}", e));
        }

        // Refresh both flags: clear watch-only AND record the storage method, so
        // a wallet restored after a purge isn't left with a stale `method`.
        let _ = json_storage::update_json("btc.json", |data: &mut serde_json::Value| {
            if let Some(obj) = data.as_object_mut() {
                obj.insert("private_key_deleted".to_string(), serde_json::Value::Bool(false));
                obj.insert("method".to_string(), serde_json::Value::String(method.clone()));
                // Backfill only — never overwrite an existing value (the
                // address check above proved this phrase IS the stored wallet).
                if obj.get("account_xpub").and_then(|v| v.as_str()).is_none_or(|s| s.is_empty()) {
                    obj.insert("account_xpub".to_string(), serde_json::Value::String(account_xpub.clone()));
                }
            }
        });

        let (current_balance, _, _, _) = *CHANNEL.bitcoin_wallet_rx.borrow();
        let _ = CHANNEL.bitcoin_wallet_tx.send((current_balance, Some(expected_address), false, crate::channel::KeyMode::from_method(&method)));

        Ok(())
    }

    /// Fully removes the wallet — deletes encrypted key, removes metadata JSON, notifies backend
    pub async fn remove_wallet(wallet_address: String, ws_tx: Sender<WSCommand>) {
        let mut log = ActivityLogState::new(
            "Remove Bitcoin wallet",
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
        let _ = remove_json("btc_encrypt.json");

        log.finish("keys");
        log.start("wallet");
        let _ = CHANNEL.activity_tx.send(Some(log.clone()));

        // 2. Delete the wallet metadata (btc.json)
        if let Ok(path) = get_config_path("btc.json")
            && path.exists()
            && let Err(e) = remove_json("btc.json")
        {
            log.fail("wallet", format!("Error removing wallet file: {}", e));
            let _ = CHANNEL.activity_tx.send(Some(log));
            return;
        }

        log.finish("wallet");
        log.start("notify");
        let _ = CHANNEL.activity_tx.send(Some(log.clone()));

        sleep(Duration::from_millis(1000)).await;

        // 3. Notify backend — server is fire-and-forget, so close the step here
        let _ = ws_tx.try_send(WSCommand {
            command: "delete_bitcoin_wallet".to_string(),
            wallet: Some(wallet_address.clone()),
            ..Default::default()
        });

        log.finish("notify");
        let _ = CHANNEL.activity_tx.send(Some(log.clone()));

        // 4. Reset UI/Channels — rows and the paging with them.
        let _ = CHANNEL.btc_transactions_tx.send(BtcTransactionState::default());
        // The coin set goes with the wallet: the next wallet's first frame would
        // replace it anyway, but nothing may sum a removed wallet's coins meanwhile.
        let _ = CHANNEL.btc_utxos_tx.send((None, Vec::new()));
        let _ = CHANNEL.bitcoin_wallet_tx.send((0.0, None, false, crate::channel::KeyMode::Standard));
        // The files are gone, so what they said about a stored 25th word has to
        // go too. Left set, it would outlive the wallet it described and tell
        // the next one's signing screens to hide a field for a phrase that was
        // never stored.
    }
}
