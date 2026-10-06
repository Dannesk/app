// ws/commands/bitcoin_import_wallet.rs

use crate::channel::{CHANNEL, WSCommand};
use crate::bridge::json_storage::{read_bytes, read_json, remove_json, write_bytes, write_json};
use crate::ws::CRYPTO_OUTGOING_TX;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::{Mutex, OnceLock};
use tokio_tungstenite::tungstenite::Message;
use zeroize::{Zeroize, ZeroizeOnDrop};

// ── Pending import state ──────────────────────────────────────────────────────

#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct PendingBtcImport {
    pub address: String,
    pub encrypted_phrase: String,
    pub salt: String,
    pub iv: String,
    /// "standard" | "cold" — decides what gets written.
    pub method: String,
    /// Neutered `m/{purpose}'/0'/0'` xpub — persisted to btc.json for
    /// receive rotation (address derivation without the seed). Reveals
    /// addresses, never keys.
    pub account_xpub: String,
    /// The address type chosen at import/create — persisted as
    /// `script_type` so rotation and member derivation walk the same purpose.
    #[zeroize(skip)]
    pub script_type: crate::btc_script_type::BtcScriptType,
}

static PENDING_BTC: OnceLock<Mutex<Option<PendingBtcImport>>> = OnceLock::new();

fn pending_store() -> &'static Mutex<Option<PendingBtcImport>> {
    PENDING_BTC.get_or_init(|| Mutex::new(None))
}

pub(crate) fn set_pending_btc(data: PendingBtcImport) {
    *pending_store().lock().unwrap() = Some(data);
}

fn take_pending_btc() -> Option<PendingBtcImport> {
    pending_store().lock().unwrap().take()
}

/// Drop a pending import on failure. The prepared secrets zeroize on drop
/// (`ZeroizeOnDrop`), so this is the whole cleanup.
fn cancel_pending() {
    let _ = take_pending_btc();
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

pub async fn execute(
    _bitcoin_current_wallet: String,
    cmd: WSCommand,
) -> Result<(), String> {
    static FAILED: &str = "Error: Bitcoin wallet import failed";

    let wallet = match cmd.wallet {
        Some(w) => w,
        None => {
            cancel_pending();
            fail_log(FAILED);
            return Err(FAILED.to_string());
        }
    };

    // The account xpub rides the import (2026-09-20, the Trezor/Blockbook
    // shape): the Bitcoin relay derives both chains from it, walks them gap-20
    // against its used-script set and answers for the whole wallet — in place
    // of this client deriving a 2 × 1000 window and sending it as addresses.
    // Create sends it too: a fresh mnemonic finds nothing, and the relay still
    // learns the wallet's membership, which is what tells change from money
    // sent. Read off the pending import — btc.json does not exist yet.
    let key = pending_store()
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| (p.account_xpub.clone(), p.script_type.tag()));
    let Some((xpub, script_type)) = key else {
        fail_log(FAILED);
        return Err(FAILED.to_string());
    };
    let msg_json = json!({
        "command": "import_bitcoin_wallet",
        "wallet": wallet,
        "xpub": xpub,
        "script_type": script_type,
    });

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

pub async fn process_response(
    message: Message,
    _bitcoin_current_wallet: &str,
) -> Result<(), String> {
    static FAILED: &str = "Error: Bitcoin wallet import failed";
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

            // REFUSAL — must be checked before the wallet branch below.
            //
            // The relay publishes this when Bitcoin can't be served (indexd not
            // live). It is addressed with a `wallet` field like a success, but
            // carries no `balance` — and the branch below reads a missing
            // balance as 0.0, so falling through would write btc.json and
            // btc_encrypt.json and register a healthy-looking zero-balance
            // wallet for an import that never happened.
            if let Some(err) = data.get("error").and_then(|e| e.as_str()) {
                cancel_pending();
                fail_log(&format!("Error: {}", err));
                return Ok(());
            }

            // A success WITHOUT `used` is a relay that predates the xpub import
            // and answered for #0 alone: saving that would import a long-used
            // wallet as one address, its other coins invisible. Refused.
            if data.get("wallet").is_some() && data.get("used").is_none() {
                cancel_pending();
                fail_log("Error: Bitcoin service is out of date — try again shortly");
                return Ok(());
            }

            if let Some(wallet) = data.get("wallet").and_then(|w| w.as_str()) {
                let balance_btc = data
                    .get("balance")
                    .and_then(|b| b.as_str())
                    .and_then(|b| b.parse::<f64>().ok())
                    .map(|b| b / 100_000_000.0)
                    .unwrap_or(0.0);

                // On-chain verified — advance to save step
                let mut log_opt = CHANNEL.activity_tx.borrow().clone();
                if let Some(ref mut log) = log_opt {
                    log.finish("verify");
                    log.start("save");
                    let _ = CHANNEL.activity_tx.send(log_opt.clone());
                }

                let pending = match take_pending_btc() {
                    Some(p) => p,
                    None => {
                        fail_log(FAILED);
                        return Ok(());
                    }
                };

                // The reply's `wallet` is an address to MATCH, never an
                // identity to adopt: the relay echoes the #0 this device
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
                let prior_key = read_bytes("btc_encrypt.json").ok().flatten();

                if is_cold {
                    // Watch-only: nothing about the key is stored; clear any stale file.
                    let _ = remove_json("btc_encrypt.json");
                } else {
                    let wallet_data = EncryptedWalletData {
                        encrypted_phrase: pending.encrypted_phrase.clone(),
                        salt: pending.salt.clone(),
                        iv: pending.iv.clone(),
                    };

                    if let Err(e) = write_json("btc_encrypt.json", &wallet_data) {
                        let msg = format!("FS Error: {}", e);
                        fail_log(&msg);
                        return Err(msg);
                    }
                }

                // v2 metadata: #0 first (the identity — record order is a
                // contract), then every USED address the relay's walk found,
                // ordered by (chain, index). Each is checked against OUR
                // derivation from the account xpub first: the relay derives
                // the same chains from the same key, but btc.json is what
                // signing trusts, and an address this device cannot reproduce
                // would be a coin we could display but never sign for.
                let mut records = vec![json!({ "chain": 0, "index": 0, "address": wallet })];
                let mut members: Vec<(u32, u32, String)> = Vec::new();
                for m in data.get("used").and_then(|u| u.as_array()).into_iter().flatten() {
                    let (Some(addr), Some(chain), Some(index)) = (
                        m.get("address").and_then(|a| a.as_str()),
                        m.get("chain").and_then(|c| c.as_u64()),
                        m.get("index").and_then(|i| i.as_u64()),
                    ) else {
                        continue;
                    };
                    if addr == wallet {
                        continue;
                    }
                    let ours = crate::bridge::btc_receive_rotation::derive_member(
                        &pending.account_xpub,
                        pending.script_type,
                        chain as u32,
                        index as u32,
                    );
                    if ours.as_deref() != Some(addr) {
                        // The key file above is already the new wallet's: put
                        // back what was there, as the metadata failure does.
                        match &prior_key {
                            Some(bytes) => { let _ = write_bytes("btc_encrypt.json", bytes); }
                            None => { let _ = remove_json("btc_encrypt.json"); }
                        }
                        fail_log("Error: the server named an address this wallet does not derive");
                        return Err("Import named an address outside the wallet's derivation".to_string());
                    }
                    members.push((chain as u32, index as u32, addr.to_string()));
                }
                members.sort();
                members.dedup();
                for (chain, index, addr) in &members {
                    records.push(json!({ "chain": chain, "index": index, "address": addr }));
                }
                let next_receive_index = records
                    .iter()
                    .filter(|r| r["chain"].as_u64() == Some(0))
                    .filter_map(|r| r["index"].as_u64())
                    .max()
                    .unwrap_or(0)
                    + 1;
                let next_change_index = records
                    .iter()
                    .filter(|r| r["chain"].as_u64() == Some(1))
                    .filter_map(|r| r["index"].as_u64())
                    .map(|i| i + 1)
                    .max()
                    .unwrap_or(0);
                // `write_json` TRUNCATES — this literal is the whole file, so
                // anything not named here is destroyed. The receive pool has to
                // be carried across a same-device re-import explicitly, or
                // addresses the user has already handed to payers would drop
                // out of the pool and off the live list. A different device has
                // nothing to carry and starts with one, which is the same
                // accepted reset as the counters.
                let prior = read_json::<Value>("btc.json").ok().filter(|j| {
                    j.get("account_xpub").and_then(|v| v.as_str()) == Some(pending.account_xpub.as_str())
                });
                let carried_pool = prior
                    .as_ref()
                    .and_then(|j| j.get("receive_pool").cloned())
                    .unwrap_or(Value::Null);
                let carried_high = prior
                    .as_ref()
                    .and_then(|j| j.get("receive_high_water").and_then(|v| v.as_u64()));
                // Seed one pool member so the post-import live list is
                // master + 1. It was master ALONE before 2026-09-20: the
                // import set the counter to an index with no record, so
                // `send_live_list` found nothing and the first receive address
                // only appeared on the first BTC tab switch.
                let seeded = crate::bridge::btc_receive_rotation::derive_member(
                    &pending.account_xpub,
                    pending.script_type,
                    0,
                    next_receive_index as u32,
                );
                let pool = match carried_pool {
                    Value::Array(a) if !a.is_empty() => Value::Array(a),
                    _ => match &seeded {
                        Some(address) => {
                            if !records.iter().any(|r| r["address"].as_str() == Some(address.as_str())) {
                                records.push(json!({
                                    "chain": 0,
                                    "index": next_receive_index,
                                    "address": address,
                                }));
                            }
                            json!([address])
                        }
                        None => json!([]),
                    },
                };
                let high_water = carried_high
                    .unwrap_or(next_receive_index + seeded.is_some() as u64)
                    .max(next_receive_index);

                let wallet_metadata = json!({
                    "addresses": records,
                    "next_receive_index": next_receive_index,
                    "next_change_index": next_change_index,
                    "receive_pool": pool,
                    "receive_high_water": high_water,
                    "account_xpub": pending.account_xpub,
                    "script_type": pending.script_type.tag(),
                    "private_key_deleted": is_cold,
                    "method": pending.method,
                });

                if let Err(e) = write_json("btc.json", &wallet_metadata) {
                    match prior_key {
                        Some(bytes) => { let _ = write_bytes("btc_encrypt.json", &bytes); }
                        None => { let _ = remove_json("btc_encrypt.json"); }
                    }
                    let msg = format!("Metadata Error: {}", e);
                    fail_log(&msg);
                    return Err(msg);
                }
                let _ = CHANNEL.bitcoin_wallet_tx.send((balance_btc, Some(wallet.to_string()), is_cold, crate::channel::KeyMode::from_method(&pending.method)));

                // Everything else the frame carries — every funded address's
                // UTXO set (signing works immediately), the history rows, and
                // the live list going out — is applied exactly as an app open
                // applies it. AFTER the btc.json write above: the union only
                // accepts coins of recorded addresses.
                if let Err(e) = crate::ws::commands::getbitcoincachedbalance::apply_whole_wallet(&data) {
                    fail_log(&format!("Error: {}", e));
                    return Err(e);
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
