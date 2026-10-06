// src/ui/managebtc/btccreate/btccreatelogic.rs

use crate::btc_script_type::BtcScriptType;
use crate::channel::{CHANNEL, ActivityLogState, WSCommand};
use crate::wallet::ImportMode;
use crate::encrypt::encrypt_data;
use crate::ws::commands::bitcoin_import_wallet::{PendingBtcImport, set_pending_btc};
use bip39::{Language, Mnemonic};
use bitcoin::address::Address;
use bitcoin::bip32::{DerivationPath, Xpriv};
use bitcoin::secp256k1::Secp256k1;
use bitcoin::{CompressedPublicKey, Network};
use std::str::FromStr;
use tokio::sync::mpsc::Sender;
use zeroize::Zeroize;

use crate::secure::{SecureBytes, SecureString};

pub struct BTCCreateLogic;

impl BTCCreateLogic {
    pub async fn process(
        mnemonic_phrase: SecureString,
        bip39_pass: SecureString,
        encryption_pass: SecureString,
        mode: ImportMode,
        script_type: BtcScriptType,
        ws_tx: Sender<WSCommand>,
    ) -> Result<(), String> {
        let mut log = ActivityLogState::new(
            "Create Bitcoin wallet",
            &[
                ("derive",  "Deriving keys"),
                ("connect", "Subscribing wallet to network"),
                ("verify",  "Awaiting response"),
                ("save",    "Your wallet has been created"),

            ],
        );
        log.start("derive");
        let _ = CHANNEL.activity_tx.send(Some(log.clone()));

        // Move the locked secrets straight into the worker thread — no clone,
        // no unlocked copy. They zeroize + unlock when the closure returns.
        let crypto_result = tokio::task::spawn_blocking(
            move || -> Result<(String, String, String, String, String, String), String> {
                let mnemonic = Mnemonic::parse_in(Language::English, mnemonic_phrase.as_str())
                    .map_err(|e| format!("Invalid generated mnemonic: {}", e))?;

                let mut seed_arr = mnemonic.to_seed(bip39_pass.as_str());
                // Hold the raw seed in mlocked memory; wipe the transient stack array.
                let seed = SecureBytes::new(seed_arr.to_vec());
                seed_arr.zeroize();
                let network = Network::Bitcoin;
                let secp = Secp256k1::new();

                let xpriv = Xpriv::new_master(network, seed.as_bytes())
                    .map_err(|e| format!("Failed to create master key: {}", e))?;

                let derivation_path = DerivationPath::from_str("m/84'/0'/0'/0/0")
                    .map_err(|_| "Invalid derivation path".to_string())?;

                let child_xpriv = xpriv
                    .derive_priv(&secp, &derivation_path)
                    .map_err(|e| format!("Derivation failed: {}", e))?;

                let public_key = child_xpriv.to_priv().public_key(&secp);
                let compressed_pubkey = CompressedPublicKey(public_key.inner);
                let address = Address::p2wpkh(&compressed_pubkey, network);

                // Taproot create (2026-09-14): #0 at ITS purpose and encoding.
                // Native keeps the frozen block above verbatim.
                let address = match script_type {
                    BtcScriptType::NativeSegwit => address,
                    other => {
                        let child = xpriv
                            .derive_priv(&secp, &other.member_path(0, 0))
                            .map_err(|e| format!("Derivation failed: {}", e))?;
                        let pk = CompressedPublicKey(child.to_priv().public_key(&secp).inner);
                        other.address(&secp, &pk)
                    }
                };

                // Neutered account key for receive rotation — a NEW derivation
                // site (the #0 block above is frozen); addresses only, no keys.
                let account_xpriv = xpriv
                    .derive_priv(&secp, &script_type.account_path())
                    .map_err(|e| format!("Account derivation failed: {}", e))?;
                let account_xpub =
                    bitcoin::bip32::Xpub::from_priv(&secp, &account_xpriv).to_string();

                // Per-mode key protection, mirroring import: the address above is
                // derived for both; only what we persist differs.
                let (encrypted_phrase, salt, iv, method) = match mode {
                    ImportMode::Standard => {
                        let (enc, salt, iv) = encrypt_data(encryption_pass.as_str(), mnemonic_phrase.as_str())
                            .map_err(|e| format!("Encryption failed: {}", e))?;
                        (enc, salt, iv, "standard".to_string())
                    }
                    ImportMode::Cold => {
                        // Watch-only — nothing about the key is persisted.
                        (String::new(), String::new(), String::new(), "cold".to_string())
                    }
                };

                Ok((address.to_string(), encrypted_phrase, salt, iv, method, account_xpub))
            },
        )
        .await;

        match crypto_result {
            Ok(Ok((address, encrypted, salt, iv, method, account_xpub))) => {
                log.finish("derive");
                log.start("connect");
                let _ = CHANNEL.activity_tx.send(Some(log.clone()));

                set_pending_btc(PendingBtcImport {
                    address: address.clone(),
                    encrypted_phrase: encrypted,
                    salt,
                    iv,
                    method,
                    // Create = fresh random mnemonic: the relay's walk finds
                    // nothing, so btc.json gets #0 alone.
                    account_xpub,
                    script_type,
                });

                let _ = ws_tx.try_send(WSCommand {
                    command: "import_bitcoin_wallet".to_string(),
                    wallet: Some(address),
                    ..Default::default()
                });

                Ok(())
            }
            Ok(Err(e)) => {
                log.fail("derive", e.clone());
                let _ = CHANNEL.activity_tx.send(Some(log));
                Err(e)
            }
            Err(e) => {
                let msg = format!("Internal Thread Error: {}", e);
                log.fail("derive", msg.clone());
                let _ = CHANNEL.activity_tx.send(Some(log));
                Err(msg)
            }
        }
    }
}
