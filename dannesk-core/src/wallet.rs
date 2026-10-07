use crate::channel::{CHANNEL, WSCommand};
use crate::bridge::json_storage;
use serde_json::{Value, json};
use tokio::sync::mpsc;

/// How an imported wallet's key is protected at rest. Chosen on the import
/// screen; drives both the entry UI and the storage backend.
///   • `Standard` — passphrase-encrypted file on disk.
///   • `Cold`     — nothing persisted; the key lives only for this session.
///
/// There is deliberately no hardware tier. A file works on 100% of devices; no
/// enclave does, and none of them (TPM 2.0, Apple SE, Android StrongBox) can
/// run secp256k1 inside the boundary anyway — so every one could only wrap a
/// blob we then unwrap into ordinary RAM, for the cost of a per-platform
/// backend. Revisit only if enclaves gain in-boundary support for our curves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportMode {
    #[default]
    Standard,
    Cold,
}

/// One watched address of the BTC wallet: `m/{purpose}'/0'/0'/{chain}/{index}`,
/// the purpose being the wallet's `script_type` (84' for the bc1q default).
/// Record 0 is ALWAYS #0 (`0/0`) — the wallet's permanent identity, what the
/// `bitcoin_wallet` channel carries and what change returns to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtcAddressRecord {
    pub chain: u32,
    pub index: u32,
    pub address: String,
}

/// The wallet's address list out of btc.json, v1-tolerant: a legacy file
/// carries a single `address` field and reads as `[{0, 0, address}]`. Empty =
/// no BTC wallet on this device. Callers treat the list as the wallet's whole
/// address universe — membership checks, signing key derivation, and
/// subscription all iterate it.
pub fn btc_address_records() -> Vec<BtcAddressRecord> {
    let Ok(jsn) = json_storage::read_json::<Value>("btc.json") else {
        return Vec::new();
    };
    records_from(&jsn)
}

fn records_from(jsn: &Value) -> Vec<BtcAddressRecord> {
    if let Some(arr) = jsn.get("addresses").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|r| {
                Some(BtcAddressRecord {
                    chain: r.get("chain")?.as_u64()? as u32,
                    index: r.get("index")?.as_u64()? as u32,
                    address: r.get("address")?.as_str()?.to_string(),
                })
            })
            .collect();
    }
    // v1: single `address` field = #0 and nothing else.
    jsn.get("address")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|a| {
            vec![BtcAddressRecord {
                chain: 0,
                index: 0,
                address: a.to_string(),
            }]
        })
        .unwrap_or_default()
}

/// Rewrite a v1 btc.json into the v2 (`addresses` array) shape in place,
/// preserving every other field. Idempotent; v2 files pass through untouched.
/// Never touches derivation — the address bytes on disk are the address bytes
/// written back.
fn migrate_btc_json(jsn: &Value) {
    if jsn.get("addresses").is_some() {
        return;
    }
    let records = records_from(jsn);
    if records.is_empty() {
        return;
    }
    let _ = json_storage::update_json("btc.json", |data: &mut Value| {
        if let Some(obj) = data.as_object_mut() {
            let addr = obj.remove("address");
            obj.insert(
                "addresses".to_string(),
                json!([{ "chain": 0, "index": 0, "address": addr.as_ref().and_then(|a| a.as_str()).unwrap_or_default() }]),
            );
            obj.insert("next_receive_index".to_string(), json!(1));
        }
    });
}

pub fn load_wallets(commands_tx: mpsc::Sender<WSCommand>) {
    // Load XRP wallet from xrp.json
      if let Ok(path) = json_storage::get_config_path("xrp.json")
    && path.exists()
        && let Ok(json) = json_storage::read_json::<Value>("xrp.json") {


                let address = json
                    .get("address")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let private_key_deleted = json
                    .get("private_key_deleted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let key_mode = crate::channel::KeyMode::from_method(
                    json.get("method").and_then(|v| v.as_str()).unwrap_or(""),
                );

                // Update XRP wallet channel with initial data
                if !address.is_empty() {
                    let _ =
                        CHANNEL
                            .wallet_balance_tx
                            .send((0.0, Some(address.clone()), private_key_deleted, key_mode));

                    // Send get_cached_balance command
                    let command = WSCommand {
                        command: "get_cached_balance".to_string(),
                        wallet: Some(address.clone()),
                        ..Default::default()
                    };
                    let _ = commands_tx.try_send(command);
                }
    }

    // Load Bitcoin wallet from btc.json
    if let Ok(path) = json_storage::get_config_path("btc.json")
        && path.exists()
  && let Ok(json) = json_storage::read_json::<Value>("btc.json") {

                // v1 file (single `address`) → v2 (`addresses` array), silently
                // and in place. Existing wallets must not break: record 0 keeps
                // the exact address bytes the v1 file held.
                migrate_btc_json(&json);
                let records = records_from(&json);

                let private_key_deleted = json
                    .get("private_key_deleted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let key_mode = crate::channel::KeyMode::from_method(
                    json.get("method").and_then(|v| v.as_str()).unwrap_or(""),
                );

                // Update BTC wallet channel with initial data. The identity is
                // record 0 (#0); the balance stays the aggregate over every
                // watched address and fills in as the per-address replies land.
                if let Some(primary) = records.first().map(|r| r.address.clone()) {
                    let _ =
                        CHANNEL
                            .bitcoin_wallet_tx
                            .send((0.0, Some(primary), private_key_deleted, key_mode));

                    // ONE ask for the whole wallet: the command layer turns an
                    // ask for the primary into the xpub fetch
                    // (`getbitcoincachedbalance::fetch_payload`), and the reply
                    // sends the live list. Only a wallet with no account key
                    // yet (pre-HD, until the backfill) asks per address — and
                    // it has the one.
                    let has_key = json
                        .get("account_xpub")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| !s.is_empty());
                    for record in records.iter().take(if has_key { 1 } else { records.len() }) {
                        let command = WSCommand {
                            command: "get_bitcoin_cached_balance".to_string(),
                            wallet: Some(record.address.clone()),
                            ..Default::default()
                        };
                        let _ = commands_tx.try_send(command);
                    }
                }
    }

    // Both files read: whatever wallet exists is in its channel now.
    CHANNEL.loaded_tx.send_if_modified(|l| !std::mem::replace(&mut l.wallets, true));
}
