use crate::channel::{CHANNEL, WSCommand};
use crate::bridge::json_storage::{read_bytes, remove_json, write_bytes, write_json};
use crate::ws::commands::get_transaction::parse_tx;
use crate::ws::CRYPTO_OUTGOING_TX;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
use tokio_tungstenite::tungstenite::Message;
use zeroize::{Zeroize, ZeroizeOnDrop};

// ── Pending import state ──────────────────────────────────────────────────────
// Holds in-memory encrypted data between bridge (derive) and process_response
// (write to disk). ZeroizeOnDrop ensures the heap buffers are wiped on drop.

#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct PendingXrpImport {
    pub address: String,
    pub encrypted_phrase: String,
    pub salt: String,
    pub iv: String,
    /// "standard" | "cold" — decides what gets written.
    pub method: String,
}

static PENDING_XRP: OnceLock<Mutex<Option<PendingXrpImport>>> = OnceLock::new();

fn pending_store() -> &'static Mutex<Option<PendingXrpImport>> {
    PENDING_XRP.get_or_init(|| Mutex::new(None))
}

pub(crate) fn set_pending_xrp(data: PendingXrpImport) {
    *pending_store().lock().unwrap() = Some(data);
}

fn take_pending_xrp() -> Option<PendingXrpImport> {
    pending_store().lock().unwrap().take()
}

/// Drop a pending import on failure. The prepared secrets zeroize on drop
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

fn parse_asset(
    data: &Value,
    has_key: &str,
    balance_key: &str,
    limit_key: &str,
) -> (f64, bool, Option<f64>) {
    let has = data.get(has_key).and_then(|h| h.as_bool()).unwrap_or(false);
    let balance = data
        .get(balance_key)
        .and_then(|b| b.as_str())
        .and_then(|b| b.parse::<f64>().ok())
        .unwrap_or(0.0);
    let limit = data
        .get(limit_key)
        .and_then(|l| l.as_str())
        .and_then(|l| l.parse::<f64>().ok());
    (balance, has, limit)
}

// ── Commands ──────────────────────────────────────────────────────────────────

pub async fn execute(
    _current_wallet: String,
    cmd: WSCommand,
) -> Result<(), String> {
    static FAILED: &str = "Error: Wallet import failed";

    let wallet = match cmd.wallet {
        Some(w) => w,
        None => {
            cancel_pending();
            fail_log(FAILED);
            return Err(FAILED.to_string());
        }
    };

    let msg_json = json!({"command": "import_wallet", "wallet": wallet});

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
    static FAILED: &str = "Error: Wallet import failed";
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

                // On-chain verified — advance to save step
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

                // The reply's `wallet` is an address to MATCH, never an
                // identity to adopt: the relay echoes the address this device
                // derived, so a reply naming any other is refused here, before
                // a single file is written.
                if wallet != pending.address.as_str() {
                    fail_log("Error: the server named an address this wallet does not derive");
                    return Err("Import reply named an address other than the one derived".to_string());
                }

                let is_cold = pending.method == "cold";

                // Whatever key file is on disk right now is about to be
                // replaced or deleted, and the metadata write below can still
                // fail after that. Hold its bytes so the failure can put them
                // back: rolling back by *deleting* the encrypt file — which is
                // what this used to do — throws away the previous wallet's only
                // key copy along with the half-finished import.
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
                let _ = CHANNEL.wallet_balance_tx.send((balance_xrp, Some(wallet.to_string()), is_cold, crate::channel::KeyMode::from_method(&pending.method)));
                // Node-asserted existence (actNotFound ⇒ false); null/absent
                // (query failed / old relay) falls back to the balance threshold.
                let exists = data
                    .get("exists")
                    .and_then(|v| v.as_bool())
                    .unwrap_or_else(|| crate::utils::reserves::is_activated(balance_xrp));
                CHANNEL.set_xrp_exists(exists);
                // Fresh account: drop anything a previous wallet left, then fold.
                CHANNEL.clear_xrp_account();
                CHANNEL.apply_xrp_account(&data);
                // Registry pass: parse each token's fields and seed the map.
                for token in crate::utils::tokens::TOKENS {
                    let (balance, has, limit) = parse_asset(
                        &data,
                        token.has_field,
                        token.balance_field,
                        token.trustline_limit_field,
                    );
                    CHANNEL.set_token(token.code, (balance, has, limit));
                }

                // The startup rows — the newest page of each list plus every
                // resting offer — and the history facts beside them.
                let rows: Vec<_> = data
                    .get("transactions")
                    .and_then(|t| t.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(parse_tx)
                    .collect();
                CHANNEL.transactions_tx.send_modify(|state| state.apply_reply(rows, &data, None));

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
