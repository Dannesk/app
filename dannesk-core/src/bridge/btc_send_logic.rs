use crate::channel::{CHANNEL, ActivityLogState, WSCommand};
use tokio::sync::mpsc::Sender;
use crate::secure::SecureString;

pub struct BtcSendLogic;

pub struct BtcSendParams {
    pub mode: String,
    pub passphrase: SecureString,
    pub mnemonic: SecureString,
    pub bip39_pass: SecureString,
    pub recipient: String,
    pub amount: String,
    pub fee: String,
    pub wallet_address: String,
    pub asset: String,
    pub ws_tx: Sender<WSCommand>,
}

impl BtcSendLogic {
    pub async fn process(params: BtcSendParams) -> Result<(), String> {
        let BtcSendParams {
            mode,
            passphrase,
            mnemonic,
            bip39_pass,
            recipient,
            amount,
            fee,
            wallet_address,
            asset,
            ws_tx,
        } = params;

        let mut log = ActivityLogState::begin(
            "Send bitcoin",
            &[
                // No "Fetching UTXOs" step any more: the set is pushed by the
                // relay and signing builds from it directly (auth → build →
                // broadcast), so there is no round trip to narrate.
                ("init",      "Initializing"),
                ("auth",      "Authenticating"),
                ("build",     "Constructing transaction"),
                ("broadcast", "Broadcasting to network"),
                ("confirm",   "Awaiting response"),
            ],
            crate::channel::BTC_NODE,
        )?;

        // Move the locked secrets into the command — no clone, no unlocked copy.
        let bip39_opt = if bip39_pass.is_empty() {
            None
        } else {
            Some(bip39_pass)
        };
        let (passphrase, seed) = match mode.as_str() {
            "passphrase" => {
                let p = if passphrase.is_empty() { None } else { Some(passphrase) };
                (p, None)
            }
            "seed" => {
                let s = if mnemonic.as_str().trim().is_empty() { None } else { Some(mnemonic) };
                (None, s)
            }
            _ => (None, None),
        };

        let cmd = WSCommand {
            command: "bitcoin_submit_transaction".to_string(),
            wallet: Some(wallet_address),
            recipient: Some(recipient),
            amount: Some(amount),
            passphrase,
            seed,
            bip39: bip39_opt,
            trustline_limit: None,
            fee: Some(fee),
            tx_type: Some("BTC".to_string()),
            taker_pays: None,
            taker_gets: None,
            flags: None,
            wallet_type: Some(asset),
            offer_sequence: None,
            destination_tag: None,
            scan: None,
            replaces: None,
            history_kind: None,
            history_offset: None,
        };

        ws_tx.try_send(cmd).map_err(|e| {
            let msg = format!("Dispatch failed: {}", e);
            log.fail("init", msg.clone());
            let _ = CHANNEL.activity_tx.send(Some(log));
            msg
        })
    }
}
