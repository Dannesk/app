use crate::channel::{CHANNEL, ActivityLogState, WSCommand};
use tokio::sync::mpsc::Sender;
use crate::secure::SecureString;

pub struct CancelLogic;

impl CancelLogic {
    pub async fn process(
        mode: String,
        passphrase: SecureString,
        mnemonic: SecureString,
        bip39_pass: SecureString,
        offer_sequence: u32,
        wallet_address: String,
        ws_tx: Sender<WSCommand>,
    ) {
        let Ok(mut log) = ActivityLogState::begin(
            "Cancel order",
            &[
                ("init",      "Initializing"),
                ("auth",      "Authenticating"),
                ("build",     "Constructing transaction"),
                ("broadcast", "Broadcasting to network"),
                ("confirm",   "Awaiting response"),

            ],
            "relay:xrp",
        ) else {
            return;
        };

        // Move the locked secrets into the command — no clone, no unlocked copy.
        let bip39_opt = if bip39_pass.is_empty() {
            None
        } else {
            Some(bip39_pass)
        };
        let (passphrase, seed) = match mode.as_str() {
            "passphrase" => (
                if passphrase.is_empty() { None } else { Some(passphrase) },
                None,
            ),
            "seed" => (
                None,
                if mnemonic.as_str().trim().is_empty() { None } else { Some(mnemonic) },
            ),
                        _ => (None, None),
        };

        let cmd = WSCommand {
            command: "submit_transaction".to_string(),
            wallet: Some(wallet_address),
            tx_type: Some("offer_cancel".to_string()),
            offer_sequence: Some(offer_sequence),
            passphrase,
            seed,
            bip39: bip39_opt,
            ..Default::default()
        };

        if let Err(e) = ws_tx.try_send(cmd) {
            log.fail("init", format!("Dispatch failed: {}", e));
            let _ = CHANNEL.activity_tx.send(Some(log));
        }
    }
}
