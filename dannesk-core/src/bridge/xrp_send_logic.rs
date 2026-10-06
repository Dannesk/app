use crate::channel::{CHANNEL, ActivityLogState, WSCommand};
use crate::secure::SecureString;
use tokio::sync::mpsc::Sender;

pub struct XRPSendLogic;

pub struct SendParams {
    pub mode: String,
    pub passphrase: SecureString,
    pub mnemonic: SecureString,
    pub bip39_pass: SecureString,
    pub recipient: String,
    pub amount: String,
    /// Manually-entered destination tag (r-address path). Ignored when the
    /// recipient is an X-address, which carries its own tag.
    pub destination_tag: Option<u32>,
    pub wallet_address: String,
    pub asset: String,
    pub ws_tx: Sender<WSCommand>,
}

impl XRPSendLogic {
    pub async fn process(params: SendParams) -> Result<(), String> {
        let SendParams {
            mode,
            passphrase,
            mnemonic,
            bip39_pass,
            recipient,
            amount,
            destination_tag,
            wallet_address,
            asset,
            ws_tx,
        } = params;

        let mut log = ActivityLogState::begin(
            "Send transaction",
            &[
                ("init",      "Initializing"),
                ("auth",      "Authenticating"),
                ("build",     "Constructing transaction"),
                ("broadcast", "Broadcasting to network"),
                ("confirm",   "Awaiting response"),
            ],
            "relay:xrp",
        )?;

        // Move the locked secrets into the command — no clone, no unlocked copy.
        let bip39_opt = if bip39_pass.is_empty() {
            None
        } else {
            Some(bip39_pass)
        };
        // writes it into the same SecureString slot).
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
            recipient: Some(recipient),
            amount: Some(amount),
            passphrase,
            seed,
            bip39: bip39_opt,
            trustline_limit: None,
            fee: None,
            tx_type: Some("payment".to_string()),
            taker_pays: None,
            taker_gets: None,
            flags: None,
            wallet_type: Some(asset),
            offer_sequence: None,
            destination_tag,
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
