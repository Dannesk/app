use crate::channel::{CHANNEL, WSCommand};
use crate::bridge::json_storage::{read_bytes, remove_json, write_bytes, write_json};
use crate::ws::CRYPTO_OUTGOING_TX;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
use tokio_tungstenite::tungstenite::Message;
use zeroize::{Zeroize, ZeroizeOnDrop};

// ── Pending create state ──────────────────────────────────────────────────────
// Self-contained twin of the import path's pending store. A freshly created
// wallet has nothing on-chain to fetch, so create never touches import_wallet.
// Holds the encrypted key material between bridge (derive) and process_response
// (write to disk). ZeroizeOnDrop wipes the heap buffers on drop.

#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct PendingXrpCreate {
    pub address: String,
    pub encrypted_phrase: String,
    pub salt: String,
    pub iv: String,
    /// "standard" | "cold" — decides what gets written.
    pub method: String,
}

static PENDING_XRP_CREATE: OnceLock<Mutex<Option<PendingXrpCreate>>> = OnceLock::new();

fn pending_store() -> &'static Mutex<Option<PendingXrpCreate>> {
    PENDING_XRP_CREATE.get_or_init(|| Mutex::new(None))
}

pub(crate) fn set_pending_xrp(data: PendingXrpCreate) {
    *pending_store().lock().unwrap() = Some(data);
}

fn take_pending_xrp() -> Option<PendingXrpCreate> {
    pending_store().lock().unwrap().take()
}

/// Drop a pending create on failure. The prepared secrets zeroize on drop
/// (`ZeroizeOnDrop`), so this is the whole cleanup.
fn cancel_pending() {
    let _ = take_pending_xrp();
}

// ── Helpers ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct EncryptedWalletData {
    encrypted_phrase: String,
    salt: String,
    iv: String,
}

fn fail_log(msg: &str) {
    let mut log_opt = CHANNEL.activity_tx.borrow().clone();
    if let Some(ref mut log) = log_opt {
        log.fail_active(msg.to_string());
        let _ = CHANNEL.activity_tx.send(log_opt.clone());
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

pub async fn execute(_current_wallet: String, cmd: WSCommand) -> Result<(), String> {
    static FAILED: &str = "Error: Wallet creation failed";

    let wallet = match cmd.wallet {
        Some(w) => w,
        None => {
            cancel_pending();
            fail_log(FAILED);
            return Err(FAILED.to_string());
        }
    };

    let msg_json = json!({"command": "create_wallet", "wallet": wallet});

    if let Some(tx) = CRYPTO_OUTGOING_TX.get() {
        if tx.send(Message::text(msg_json.to_string())).await.is_err() {
            cancel_pending();
            fail_log(FAILED);
            return Err(FAILED.to_string());
        }
    }

    // WS sent — advance connect → verify
    let mut log_opt = CHANNEL.activity_tx.borrow().clone();
    if let Some(ref mut log) = log_opt {
        log.finish("connect");
        log.start("verify");
        let _ = CHANNEL.activity_tx.send(log_opt.clone());
    }

    Ok(())
}

pub async fn process_response(message: Message, _current_wallet: &str) -> Result<(), String> {
    static FAILED: &str = "Error: Wallet creation failed";
    match message {
        Message::Text(text) => {
            let data: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    cancel_pending();
                    fail_log(&format!("{}: parse error", FAILED));
                    return Err(format!("Failed to parse JSON: {}", e));
                }
            };

            if let Some(wallet) = data.get("wallet").and_then(|w| w.as_str()) {
                let balance_xrp = data
                    .get("balance")
                    .and_then(|b| b.as_str())
                    .and_then(|b| b.parse::<f64>().ok())
                    .map(|b| b / 1_000_000.0)
                    .unwrap_or(0.0);

                // Relay acknowledged the subscription — advance to save step.
                let mut log_opt = CHANNEL.activity_tx.borrow().clone();
                if let Some(ref mut log) = log_opt {
                    log.finish("verify");
                    log.start("save");
                    let _ = CHANNEL.activity_tx.send(log_opt.clone());
                }

                let pending = match take_pending_xrp() {
                    Some(p) => p,
                    None => {
                        fail_log(FAILED);
                        return Ok(());
                    }
                };

                let is_cold = pending.method == "cold";

                // Whatever key file is on disk right now is about to be
                // replaced or deleted, and the metadata write below can still
                // fail after that. Hold its bytes so the failure can put them
                // back: rolling back by *deleting* the encrypt file — which is
                // what this used to do — throws away the previous wallet's only
                // key copy along with the half-finished create.
                let prior_key = read_bytes("xrp_encrypt.json").ok().flatten();

                if is_cold {
                    // Watch-only: nothing about the key is stored; clear any stale file.
                    let _ = remove_json("xrp_encrypt.json");
                } else {
                    let wallet_data = EncryptedWalletData {
                        encrypted_phrase: pending.encrypted_phrase.clone(),
                        salt: pending.salt.clone(),
                        iv: pending.iv.clone(),
                    };

                    if let Err(e) = write_json("xrp_encrypt.json", &wallet_data) {
                        let msg = format!("FS Error: {}", e);
                        fail_log(&msg);
                        return Err(msg);
                    }
                }

                let wallet_metadata = json!({
                    "address": wallet,
                    "private_key_deleted": is_cold,
                    "method": pending.method,
                });

                if let Err(e) = write_json("xrp.json", &wallet_metadata) {
                    match prior_key {
                        Some(bytes) => { let _ = write_bytes("xrp_encrypt.json", &bytes); }
                        None => { let _ = remove_json("xrp_encrypt.json"); }
                    }
                    let msg = format!("Metadata Error: {}", e);
                    fail_log(&msg);
                    return Err(msg);
                }
                let _ = CHANNEL.wallet_balance_tx.send((
                    balance_xrp,
                    Some(wallet.to_string()),
                    is_cold,
                    crate::channel::KeyMode::from_method(&pending.method),
                ));

                // A freshly created account provably has no AccountRoot yet.
                // Written explicitly (not left to the map default) so a prior
                // wallet's existence entry can't leak into this one.
                CHANNEL.set_xrp_exists(
                    data.get("exists").and_then(|v| v.as_bool()).unwrap_or(false),
                );

                // A new account holds no trustlines — reset every token to its
                // empty state so no stale balances leak from a prior wallet.
                for token in crate::utils::tokens::TOKENS {
                    CHANNEL.set_token(token.code, (0.0, false, None));
                }

                let mut log_opt = CHANNEL.activity_tx.borrow().clone();
                if let Some(ref mut log) = log_opt {
                    log.finish("save");
                    let _ = CHANNEL.activity_tx.send(log_opt.clone());
                }
            } else {
                cancel_pending();
                fail_log(FAILED);
            }
        }
        _ => {
            cancel_pending();
            fail_log(FAILED);
        }
    }
    Ok(())
}
