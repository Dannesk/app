use crate::channel::{CHANNEL, ActivityLogState, WSCommand};
use tokio::sync::mpsc::Sender;
use crate::secure::SecureString;

/// The title the order flow raises its activity log under.
///
/// Shared rather than repeated because it is load-bearing outside this file:
/// `xrp::trade_expiry_check` uses it to be sure the log it is about to rewrite
/// belongs to the order it is talking about, and not to whatever flow the user
/// happens to have opened since.
pub const LOG_TITLE: &str = "Place order";

pub struct TradeLogic;

impl TradeLogic {
    #[allow(clippy::too_many_arguments)]
    pub async fn process(
        mode: String,
        passphrase: SecureString,
        mnemonic: SecureString,
        bip39_pass: SecureString,
        base_asset: String,
        quote_asset: String,
        pay_amount: String,
        receive_amount: String,
        flags: Vec<String>,
        wallet_address: String,
        ws_tx: Sender<WSCommand>,
        _last_view: Option<()>,
    ) {
        let Ok(mut log) = ActivityLogState::begin(
            LOG_TITLE,
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

        // TakerGets = what the taker takes from us = what we sell = base_asset (e.g. XRP)
        // TakerPays = what the taker pays us = what we receive = quote_asset (e.g. RLUSD)
        // Both arrive already rounded the way their bound points (see
        // `controller::xrp::trade_terms`); nothing is derived here.
        let taker_gets = Some((pay_amount, base_asset));
        let taker_pays = Some((receive_amount, quote_asset));

        let cmd = WSCommand {
            command: "submit_transaction".to_string(),
            wallet: Some(wallet_address),
            recipient: None,
            amount: None,
            passphrase,
            seed,
            bip39: bip39_opt,
            trustline_limit: None,
            fee: None,
            tx_type: Some("offer_create".to_string()),
            taker_pays,
            taker_gets,
            flags: Some(flags),
            wallet_type: None,
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
            // Nothing was sent, so nothing is in flight. Leaving the quote
            // parked would arm the expiry watch for an order that does not
            // exist. See `ws::commands::submit_transaction::release_pending`.
            let _ = CHANNEL.pending_trade_tx.send(None);
        }
    }
}
