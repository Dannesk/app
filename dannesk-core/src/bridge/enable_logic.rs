use crate::channel::{CHANNEL, ActivityLogState, WSCommand};
use tokio::sync::mpsc::Sender;
use crate::secure::SecureString;

/// The limit an `enable` sets. Nominal: what the line will hold is decided
/// by the issuer and the user's trades, and the ledger caps a balance at the
/// limit only on payments IN.
pub const ENABLE_LIMIT: &str = "1000000";
/// The limit a `disable` sets. A line at limit 0, balance 0 and default
/// flags is deleted by the ledger and its reserve refunded; at a nonzero
/// balance the same transaction succeeds and merely lowers the limit, which
/// is why the client gates the face on an exact zero.
pub const DISABLE_LIMIT: &str = "0";

pub struct TrustlineEnableLogic;

impl TrustlineEnableLogic {
    /// One bridge for both directions of a TrustSet: `limit` is
    /// [`ENABLE_LIMIT`] or [`DISABLE_LIMIT`], `title` names the activity log.
    #[allow(clippy::too_many_arguments)]
    pub async fn process(
        mode: String,
        passphrase: SecureString,
        mnemonic: SecureString,
        bip39_pass: SecureString,
        wallet_address: String,
        asset: String,
        limit: &'static str,
        title: &'static str,
        ws_tx: Sender<WSCommand>,
    ) {
        let Ok(mut log) = ActivityLogState::begin(
            title,
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
            recipient: None,
            amount: None,
            passphrase,
            seed,
            trustline_limit: Some(limit.to_string()),
            fee: None,
            tx_type: Some("trustset".to_string()),
            taker_pays: None,
            taker_gets: None,
            flags: None,
            wallet_type: Some(asset),
            bip39: bip39_opt,
            offer_sequence: None,
            destination_tag: None,
            scan: None,
            replaces: None,
            history_kind: None,
            history_offset: None,
        };

        if let Err(e) = ws_tx.try_send(cmd) {
            log.fail("init", format!("Dispatch failed: {}", e));
            let _ = CHANNEL.activity_tx.send(Some(log));
        }
    }
}
