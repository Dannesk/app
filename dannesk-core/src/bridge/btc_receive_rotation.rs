//! Address rotation — the Electrum discipline on both chains: offer the first
//! UNUSED address, advance only once it has been used. Because advancing
//! requires use, the wallet never accumulates a gap of unused addresses,
//! which is what keeps our seeds restorable in any gap-20 scanner (and inside
//! our own 0..1000 import window) forever.
//!
//! Two consumers, and since 2026-09-20 they no longer share a walk:
//! - receive (`chain 0`): a POOL the user curates. [`generate_receive_address`]
//!   adds one, [`remove_receive_address`] drops one, capped at
//!   [`MAX_RECEIVE_POOL`]. Nothing about it is inferred — the pool is a list on
//!   disk, so the pane can show several addresses at once and hand them to
//!   different payers. The old auto-walk could only ever produce ONE unused
//!   address, because advancing required a payment to arrive first.
//! - change (`chain 1`): the signer calls [`ensure_change_address`] per send
//!   and routes change there instead of consolidating on #0. This one KEEPS
//!   the walk — change is never user-visible and must rotate by itself.
//!
//! Derivation here is from the NEUTERED account xpub stored in btc.json —
//! addresses without the seed, which is what makes rotation work on cold
//! wallets and without a credential prompt. The xpub sits at the wallet's
//! own purpose (`script_type`, 2026-09-14) and its children are encoded in
//! that type — a taproot wallet rotates through bc1p addresses. A wallet
//! whose btc.json predates the xpub (imported before the HD work) returns
//! `None` and both callers fall back to #0, exactly the old behavior; the
//! xpub backfills on the next signing auth or key restore.
//!
//! The pool is bounded by COUNT, never by how many members look unused:
//! "used" is read from the UTXO union, which is empty until the whole-wallet
//! reply lands, so a cap on unused members would refuse a generate that should
//! have been allowed. A count of rows on disk cannot be wrong.
//!
//! "Used" is judged from what this client can see: the live UTXO union and
//! the transaction list. Since 2026-09-17 the import itself records every
//! address indexd's used-script set reports as ever used, so the counters
//! start past a used-then-emptied address too (BTC-HD-SPEC.md §10) — the
//! old soft spot of re-offering one after a reimport is closed at the source.

use crate::bridge::json_storage;
use crate::channel::CHANNEL;
use bitcoin::bip32::{ChildNumber, Xpub};
use bitcoin::secp256k1::Secp256k1;
use serde_json::{Value, json};
use std::str::FromStr;

/// How many addresses the receive pool may hold, beside the master. The user
/// curates it, so this is a count of ROWS ON DISK — never of "unused" members.
pub const MAX_RECEIVE_POOL: usize = 5;

/// indexd's gap limit, mirrored (`indexd/src/relay/xpub.rs` `GAP`). The two
/// crates share no code, so a change there has to be made here as well.
const GAP: u32 = 20;

/// Why a generate was refused. The pane must NAME the reason — a dark button
/// that does not say what it is waiting on reads as broken (ICED.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolRefusal {
    /// [`MAX_RECEIVE_POOL`] reached: one has to be removed first.
    Full,
    /// Another address would land past what a gap-20 restore can reach, so one
    /// of the existing ones has to be PAID first. Being used is the only thing
    /// that moves the window forward — see [`highest_reachable_index`].
    OutOfGap,
    /// No wallet, no account key yet, or btc.json could not be written.
    Unavailable,
}

/// The receive pool as the user has curated it.
pub fn receive_pool() -> Vec<String> {
    json_storage::read_json::<Value>("btc.json")
        .ok()
        .map(|jsn| receive_pool_of(&jsn))
        .unwrap_or_default()
}

/// The pool out of an already-read btc.json. A file written before the pool
/// existed falls back to the single address the old walk had on offer, so the
/// first open after an upgrade shows exactly what it showed before; the first
/// generate or remove writes the real field.
fn receive_pool_of(jsn: &Value) -> Vec<String> {
    if let Some(arr) = jsn.get("receive_pool").and_then(|v| v.as_array()) {
        return arr.iter().filter_map(|v| v.as_str().map(String::from)).collect();
    }
    let records = crate::wallet::btc_address_records();
    jsn.get("next_receive_index")
        .and_then(|v| v.as_u64())
        .and_then(|i| records.iter().find(|r| r.chain == 0 && r.index == i as u32))
        .map(|r| vec![r.address.clone()])
        .unwrap_or_default()
}

/// The next chain-0 index to derive. Monotonic: it counts what has been
/// ISSUED, not what is held, so removing a pool member never hands its index
/// out again.
fn high_water_of(jsn: &Value) -> u32 {
    jsn.get("receive_high_water")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or_else(|| {
            crate::wallet::btc_address_records()
                .iter()
                .filter(|r| r.chain == 0)
                .map(|r| r.index + 1)
                .max()
                .unwrap_or(1)
        })
}

/// The highest chain-0 index indexd's walk will still look at.
///
/// Mirrors `relay/xpub.rs`: the walk opens with `end = GAP` and only grows it
/// on a USED hit (`end = end.max(m.index + 1 + GAP)`). So with nothing used it
/// reaches `0..=GAP-1`, and past a used index `u` it reaches `0..=u+GAP`.
/// Deriving beyond that is not a soft failure — the whole-wallet fetch would
/// never ask about the address, and `replace_btc_utxos` REPLACES the union, so
/// a coin on it would show once and then vanish on the next open, unspendable.
fn highest_reachable_index(jsn: &Value) -> u32 {
    match jsn.get("last_used_receive_index").and_then(|v| v.as_u64()) {
        Some(used) => used as u32 + GAP,
        None => GAP - 1,
    }
}

/// Issue another receive address into the pool and subscribe it.
pub fn generate_receive_address() -> Result<String, PoolRefusal> {
    let Ok(jsn) = json_storage::read_json::<Value>("btc.json") else {
        return Err(PoolRefusal::Unavailable);
    };
    let Some(xpub) = jsn.get("account_xpub").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
    else {
        return Err(PoolRefusal::Unavailable);
    };
    if receive_pool_of(&jsn).len() >= MAX_RECEIVE_POOL {
        return Err(PoolRefusal::Full);
    }
    let index = high_water_of(&jsn);
    if index > highest_reachable_index(&jsn) {
        return Err(PoolRefusal::OutOfGap);
    }
    let script_type = jsn
        .get("script_type")
        .and_then(|v| v.as_str())
        .and_then(crate::btc_script_type::BtcScriptType::from_tag)
        .unwrap_or_default();
    let Some(address) = derive_member(xpub, script_type, 0, index) else {
        return Err(PoolRefusal::Unavailable);
    };

    // Persist BEFORE handing it out: an address on screen that btc.json does
    // not record is a payment the signer could never spend.
    let recorded = address.clone();
    if json_storage::update_json("btc.json", move |data: &mut Value| {
        let Some(obj) = data.as_object_mut() else { return };
        let known = obj
            .get("addresses")
            .and_then(|a| a.as_array())
            .map(|a| a.iter().any(|r| r["address"].as_str() == Some(recorded.as_str())))
            .unwrap_or(false);
        if !known && let Some(arr) = obj.get_mut("addresses").and_then(|a| a.as_array_mut()) {
            arr.push(json!({ "chain": 0, "index": index, "address": recorded }));
        }
        let mut pool: Vec<String> = obj
            .get("receive_pool")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        if !pool.iter().any(|a| a == &recorded) {
            pool.push(recorded.clone());
        }
        obj.insert("receive_pool".to_string(), json!(pool));
        obj.insert("receive_high_water".to_string(), json!(index + 1));
    })
    .is_err()
    {
        return Err(PoolRefusal::Unavailable);
    }
    // Watch it from the moment it can be paid.
    send_live_list();
    Ok(address)
}

/// Drop one address from the pool and stop watching it.
///
/// The `addresses[]` RECORD stays: the address may already hold a coin, and the
/// signer needs its derivation to spend it. Only pool membership goes, and with
/// it the subscription — the shorter live list IS the unsubscribe
/// (`indexd/src/relay/websocket/bitcoin_subscribe.rs` diffs the list it holds
/// against the one sent and issues `UnsubscribeBitcoinAddress` per address that
/// left). An address still holding an UNCONFIRMED coin keeps being re-added by
/// the height-0 loop in `send_live_list` until it confirms, so removal is
/// immediate on the pane and deferred on the wire, with no extra code.
pub fn remove_receive_address(address: &str) {
    let target = address.to_string();
    let _ = json_storage::update_json("btc.json", move |data: &mut Value| {
        let Some(obj) = data.as_object_mut() else { return };
        let pool: Vec<String> = obj
            .get("receive_pool")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .filter(|a| a != &target)
                    .collect()
            })
            .unwrap_or_default();
        obj.insert("receive_pool".to_string(), json!(pool));
    });
    send_live_list();
}

/// Where the next send's change goes. Same walk on the change chain; the
/// spent-to address becomes "used" the moment the send lands in the union, so
/// every send gets a fresh change address. `None` ⟹ the signer falls back to
/// #0 (legacy wallet, pre-xpub) — the Phase A/B behavior.
pub fn ensure_change_address() -> Option<String> {
    ensure_rotating_address(1, "next_change_index")
}

/// CHANGE only since 2026-09-20 — receive is the pool above.
///
/// Ensure the offered change address exists in btc.json, is on the live list,
/// and is unused, advancing the counter past any used ones. The counter means
/// "the index currently offered" and persists on every advance. Change is never
/// user-visible and must rotate by itself, so this keeps the original walk: the
/// spent-to address becomes used the moment the send lands in the union, so
/// every send gets a fresh one.
fn ensure_rotating_address(chain: u32, counter_key: &'static str) -> Option<String> {
    let jsn = json_storage::read_json::<Value>("btc.json").ok()?;
    let xpub_str = jsn.get("account_xpub").and_then(|v| v.as_str()).filter(|s| !s.is_empty())?;
    let xpub = Xpub::from_str(xpub_str).ok()?;
    let script_type = jsn
        .get("script_type")
        .and_then(|v| v.as_str())
        .and_then(crate::btc_script_type::BtcScriptType::from_tag)
        .unwrap_or_default();
    let secp = Secp256k1::verification_only();

    let mut records = crate::wallet::btc_address_records();
    if records.is_empty() {
        return None;
    }
    // Counter fallback when the field is absent (older v2 file): first index
    // past what the import scan found on the change chain, which starts at 0.
    let fallback = records
        .iter()
        .filter(|r| r.chain == chain)
        .map(|r| r.index + 1)
        .max()
        .unwrap_or(0);
    let mut index = jsn
        .get(counter_key)
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(fallback);

    // Bounded walk: each iteration either returns the offered address or
    // advances past a used one. The bound is loop safety, not policy — with
    // advance-only-on-use the gap never exceeds one fresh address per call.
    for _ in 0..21 {
        let offered = match records.iter().find(|r| r.chain == chain && r.index == index) {
            Some(r) => r.address.clone(),
            None => {
                let child = xpub
                    .derive_pub(
                        &secp,
                        &[
                            ChildNumber::from_normal_idx(chain).ok()?,
                            ChildNumber::from_normal_idx(index).ok()?,
                        ],
                    )
                    .ok()?;
                let address = script_type.address(&secp, &child.to_pub()).to_string();
                // Persist BEFORE handing it out: an address on screen — or in
                // a change output — that btc.json doesn't know is a payment
                // the signer could never spend. The same write advances the
                // counter to the offered index.
                let addr = address.clone();
                if json_storage::update_json("btc.json", move |data: &mut Value| {
                    if let Some(obj) = data.as_object_mut() {
                        if let Some(arr) = obj.get_mut("addresses").and_then(|a| a.as_array_mut()) {
                            arr.push(json!({ "chain": chain, "index": index, "address": addr }));
                        }
                        obj.insert(counter_key.to_string(), json!(index));
                    }
                })
                .is_err()
                {
                    return None;
                }
                records.push(crate::wallet::BtcAddressRecord {
                    chain,
                    index,
                    address: address.clone(),
                });
                // Watch it from the moment it can be paid: the live list is
                // sent again, now holding this address, BEFORE it is handed
                // out — so the payment (or our own change) is seen in the
                // mempool.
                send_live_list();
                address
            }
        };
        if !address_used(&offered) {
            // Re-persist the counter when the walk advanced past used
            // records that already existed (no derivation happened above).
            if jsn.get(counter_key).and_then(|v| v.as_u64()) != Some(index as u64) {
                let _ = json_storage::update_json("btc.json", move |data: &mut Value| {
                    if let Some(obj) = data.as_object_mut() {
                        obj.insert(counter_key.to_string(), json!(index));
                    }
                });
                // A different address is on offer now, so the list changed.
                send_live_list();
            }
            return Some(offered);
        }
        index += 1;
    }
    None
}

/// One member address from the account xpub: `{chain}/{index}` in the wallet's
/// own address type. The same derivation the rotation walk uses — and the
/// check every address the SERVER names must pass before it is written to
/// btc.json (it derives the same chains from the same key; an address this
/// cannot reproduce is one the signer could never spend from).
pub fn derive_member(
    account_xpub: &str,
    script_type: crate::btc_script_type::BtcScriptType,
    chain: u32,
    index: u32,
) -> Option<String> {
    let xpub = Xpub::from_str(account_xpub).ok()?;
    let secp = Secp256k1::verification_only();
    let child = xpub
        .derive_pub(
            &secp,
            &[
                ChildNumber::from_normal_idx(chain).ok()?,
                ChildNumber::from_normal_idx(index).ok()?,
            ],
        )
        .ok()?;
    Some(script_type.address(&secp, &child.to_pub()).to_string())
}

/// Send the wallet's LIVE list — whole, replacing the last one (the Bitcoin
/// relay's `subscribe_bitcoin_addresses`, Blockbook's `subscribeAddresses`):
///
/// 1. the master (#0) — leaves only with the wallet;
/// 2. the receive and the change address currently on offer;
/// 3. every member holding a coin, confirmed or pending: an address that was
///    on offer and has been paid, our own pending change, and a used address a
///    coin still sits on. Every coin that can change gets a push this way, and
///    a consumed coin's mark comes from the tracker that saw the spend
///    (2026-10-06: a used address's spent coin sat in the set all session,
///    because nothing ever pushed for that address).
///
/// Nothing else is ever watched: an emptied used address is fetched with the
/// wallet (`get_bitcoin_cached_balance` carrying the account xpub), never
/// watched.
/// Sent after every whole-wallet reply (import, open, link rise), on every
/// pool change, on every change rotation, and when the UTXO set changes.
/// A wallet with no account key yet has no list: it is one address on the
/// per-address path.
///
/// Since 2026-09-20 the chain-0 half is the POOL, not one walked address.
/// Shrinking this list is how an address is unsubscribed — the server diffs
/// what it holds against what arrives and drops the difference.
pub fn send_live_list() {
    let Ok(jsn) = json_storage::read_json::<Value>("btc.json") else { return };
    if jsn.get("account_xpub").and_then(|v| v.as_str()).is_none_or(str::is_empty) {
        return;
    }
    let records = crate::wallet::btc_address_records();
    let Some(primary) = records.first().map(|r| r.address.clone()) else { return };

    let mut list = vec![primary.clone()];
    // The receive POOL, exactly as the user curates it — no inference.
    for address in receive_pool_of(&jsn) {
        if !list.contains(&address) {
            list.push(address);
        }
    }
    // Change is still automatic and still walks: the address the next send
    // will pay itself, which must be watched before the build.
    let on_offer = jsn
        .get("next_change_index")
        .and_then(|v| v.as_u64())
        .and_then(|i| records.iter().find(|r| r.chain == 1 && r.index == i as u32));
    if let Some(r) = on_offer
        && !list.contains(&r.address)
    {
        list.push(r.address.clone());
    }
    for u in CHANNEL.btc_utxos_rx.borrow().1.iter() {
        if records.iter().any(|r| r.address == u.address) && !list.contains(&u.address) {
            list.push(u.address.clone());
        }
    }

    if let Some(tx) = crate::ws::CRYPTO_COMMANDS_TX.get() {
        let _ = tx.try_send(crate::channel::WSCommand {
            command: "subscribe_bitcoin_addresses".to_string(),
            wallet: Some(primary),
            scan: Some(list),
            ..Default::default()
        });
    }
}

/// Has this address ever been paid, as far as this client can see? Any coin
/// in the union (any height — a mempool receive already counts as use) or any
/// transaction that lists it as a receiver.
fn address_used(address: &str) -> bool {
    if CHANNEL
        .btc_utxos_rx
        .borrow()
        .1
        .iter()
        .any(|u| u.address == address)
    {
        return true;
    }
    CHANNEL
        .btc_transactions_rx
        .borrow()
        .transactions
        .values()
        .any(|tx| tx.receiver_addresses.iter().any(|a| a == address))
}
