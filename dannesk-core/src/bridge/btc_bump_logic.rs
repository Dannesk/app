//! The fee-bump bridge: opens the activity log and moves the credential into
//! a `bitcoin_bump_transaction` command. The twin of `btc_send_logic`, with
//! the original's txid riding `replaces` in place of a recipient and amount.

use crate::channel::{ActivityLogState, CHANNEL, WSCommand};
use crate::secure::SecureString;
use tokio::sync::mpsc::Sender;

pub struct BtcBumpParams {
    pub mode: String,
    pub passphrase: SecureString,
    pub mnemonic: SecureString,
    pub bip39_pass: SecureString,
    /// The mempool txid being replaced.
    pub txid: String,
    /// The committed fee, absolute satoshis.
    pub fee: String,
    pub wallet_address: String,
    pub ws_tx: Sender<WSCommand>,
}

pub struct BtcBumpLogic;

impl BtcBumpLogic {
    pub async fn process(params: BtcBumpParams) -> Result<(), String> {
        let BtcBumpParams { mode, passphrase, mnemonic, bip39_pass, txid, fee, wallet_address, ws_tx } =
            params;

        let mut log = ActivityLogState::begin(
            "Modify fee",
            &[
                ("init", "Initializing"),
                ("auth", "Authenticating"),
                ("build", "Rebuilding transaction"),
                ("broadcast", "Broadcasting to network"),
                ("confirm", "Awaiting response"),
            ],
            crate::channel::BTC_NODE,
        )?;

        let bip39_opt = if bip39_pass.is_empty() { None } else { Some(bip39_pass) };
        let (passphrase, seed) = match mode.as_str() {
            "passphrase" => ((!passphrase.is_empty()).then_some(passphrase), None),
            "seed" => (None, (!mnemonic.as_str().trim().is_empty()).then_some(mnemonic)),
            _ => (None, None),
        };

        let cmd = WSCommand {
            command: "bitcoin_bump_transaction".to_string(),
            wallet: Some(wallet_address),
            passphrase,
            seed,
            bip39: bip39_opt,
            fee: Some(fee),
            tx_type: Some("BTC".to_string()),
            wallet_type: Some("BTC".to_string()),
            replaces: Some(txid),
            ..Default::default()
        };

        ws_tx.try_send(cmd).map_err(|e| {
            let msg = format!("Dispatch failed: {}", e);
            log.fail("init", msg.clone());
            let _ = CHANNEL.activity_tx.send(Some(log));
            msg
        })
    }
}
