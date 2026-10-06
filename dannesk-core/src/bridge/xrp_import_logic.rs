
use crate::channel::{CHANNEL, ActivityLogState, WSCommand};
use crate::wallet::ImportMode;
use crate::encrypt::encrypt_data;
use crate::ws::commands::import_wallet::{PendingXrpImport, set_pending_xrp};
use bip39::{Language, Mnemonic};
use bitcoin::bip32::{DerivationPath, Xpriv};
use bitcoin::secp256k1::Secp256k1;
use std::str::FromStr;
use tokio::sync::mpsc::Sender;
use zeroize::Zeroize;

use crate::secure::{SecureBytes, SecureString};
use sha2::{Digest, Sha256};
use ripemd::Ripemd160;

/// What `process` prepares on the worker thread for `import_wallet` to persist
/// once the relay confirms. For Cold the encrypted fields are all empty.
struct Prepared {
    address: String,
    encrypted_phrase: String,
    salt: String,
    iv: String,
    method: String,
}

pub struct XRPImportLogic;

impl XRPImportLogic {
    pub async fn process(
        mnemonic_phrase: SecureString,
        bip39_pass: SecureString,
        encryption_pass: SecureString,
        mode: ImportMode,
        ws_tx: Sender<WSCommand>,
    ) -> Result<(), String> {
        let mut log = ActivityLogState::new(
            "Import XRP wallet",
            &[
                ("derive",  "Deriving keys"),
                ("connect", "Subscribing wallet to network"),
                ("verify",  "Awaiting response"),
                ("save",    "Import successful"),
            ],
        );
        log.start("derive");
        let _ = CHANNEL.activity_tx.send(Some(log.clone()));

        // Move the locked secrets straight into the worker thread — no clone,
        // no unlocked copy. They zeroize + unlock when the closure returns.
        let crypto_result = tokio::task::spawn_blocking(
            move || -> Result<Prepared, String> {
                let mnemonic = Mnemonic::parse_in(Language::English, mnemonic_phrase.as_str())
                    .map_err(|e| format!("Invalid mnemonic: {}", e))?;

                let mut seed_arr = mnemonic.to_seed(bip39_pass.as_str());
                // Hold the raw seed in mlocked memory; wipe the transient stack array.
                let seed = SecureBytes::new(seed_arr.to_vec());
                seed_arr.zeroize();
                let secp = Secp256k1::new();

                let xpriv = Xpriv::new_master(bitcoin::Network::Bitcoin, seed.as_bytes())
                    .map_err(|e| format!("Failed to create master key: {}", e))?;

                let path = DerivationPath::from_str("m/44'/144'/0'/0/0")
                    .map_err(|_| "Invalid derivation path".to_string())?;

                let child_xpriv = xpriv
                    .derive_priv(&secp, &path)
                    .map_err(|e| format!("Derivation failed: {}", e))?;

                let public_key = child_xpriv.to_priv().public_key(&secp);
                let pk_bytes = public_key.inner.serialize();

                let sha_hash = Sha256::digest(pk_bytes);
                let rip_hash = Ripemd160::digest(sha_hash);

                let mut account_id = [0u8; 21];
                account_id[0] = 0x00;
                account_id[1..].copy_from_slice(&rip_hash);

                let alphabet = bs58::Alphabet::new(b"rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz").unwrap();
                let address = bs58::encode(&account_id)
                    .with_alphabet(&alphabet)
                    .with_check()
                    .into_string();

                // Per-mode key protection. The address above is derived for
                // both; only what we persist differs.
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

                Ok(Prepared { address, encrypted_phrase, salt, iv, method })
            },
        )
        .await;

        match crypto_result {
            Ok(Ok(prepared)) => {
                log.finish("derive");
                log.start("connect");
                let _ = CHANNEL.activity_tx.send(Some(log.clone()));

                let address = prepared.address.clone();
                set_pending_xrp(PendingXrpImport {
                    address: prepared.address,
                    encrypted_phrase: prepared.encrypted_phrase,
                    salt: prepared.salt,
                    iv: prepared.iv,
                    method: prepared.method,
                });

                let _ = ws_tx.try_send(WSCommand {
                    command: "import_wallet".to_string(),
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
            _ => {
                let msg = "Internal thread error".to_string();
                log.fail("derive", msg.clone());
                let _ = CHANNEL.activity_tx.send(Some(log));
                Err(msg)
            }
        }
    }
}
