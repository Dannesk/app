//! `btc_utxos` — the relay pushing one address's whole UTXO set.
//!
//! Push-only, like `btc_balance`: the relay sends it on every wallet event,
//! every subscribe, and every stream reconnect, so the client never asks. The
//! same array also rides the cached-balance reply (see
//! `getbitcoincachedbalance`), which parses it with [`parse_utxos`] so the two
//! paths cannot drift.
//!
//! HD: the wallet watches several addresses of one seed, each pushed as its
//! own set, and the channel carries their UNION with every coin tagged by its
//! owning address. [`merge_btc_utxos`] is the ONLY writer of that union — it
//! replaces exactly the pushed address's slice, recomputes the aggregate
//! balance (the union's confirmed coins summed — the relay's own balance
//! definition), and drops pushes for addresses that are not in btc.json (a
//! stale subscription from a replaced wallet must not leak coins into the new
//! one's spendable set).

use crate::channel::{BtcUtxo, CHANNEL};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

pub async fn execute(
    _bitcoin_current_wallet: String,
    _cmd: crate::channel::WSCommand,
) -> Result<(), String> {
    // No client-initiated execution — the set is pushed.
    Ok(())
}

/// The wire array → typed set, every coin tagged with the owning address the
/// frame named. Entries that fail to parse are dropped rather than failing the
/// frame: a short set self-heals on the next push, a rejected frame leaves the
/// previous (staler) set in place.
pub fn parse_utxos(v: &Value, owner: &str) -> Vec<BtcUtxo> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|u| {
            Some(BtcUtxo {
                txid: u["txid"].as_str()?.to_string(),
                vout: u["vout"].as_u64()? as u32,
                sats: u["sats"].as_u64()?,
                height: u["height"].as_u64()?,
                address: owner.to_string(),
            })
        })
        .collect()
}

/// Fold one address's freshly-pushed set into the union and recompute the
/// aggregate balance. The channel's wallet tag is pinned to the PRIMARY (#0)
/// — the identity every consumer checks against — regardless of which member
/// address the push was for.
pub fn merge_btc_utxos(owner: &str, set: Vec<BtcUtxo>) {
    let records = crate::wallet::btc_address_records();
    if !records.iter().any(|r| r.address == owner) {
        return;
    }
    let primary = records[0].address.clone();
    CHANNEL.btc_utxos_tx.send_modify(|(set_wallet, union)| {
        // A change of primary means a different wallet's union is in the
        // channel — start over rather than mixing two wallets' coins.
        if set_wallet.as_deref() != Some(primary.as_str()) {
            union.clear();
            *set_wallet = Some(primary.clone());
        }
        union.retain(|u| u.address != owner);
        union.extend(set);
    });
    recompute_btc_balance();
}

/// The whole wallet's coins at once — the whole-wallet frame
/// (`getbitcoincachedbalance::apply_whole_wallet`). REPLACES the union: a
/// member the frame did not list holds nothing now. Coins of an address
/// btc.json does not record are dropped, as in [`merge_btc_utxos`] — a coin
/// nobody can sign for is not shown.
pub fn replace_btc_utxos(primary: &str, set: Vec<BtcUtxo>) {
    let records = crate::wallet::btc_address_records();
    if records.first().map(|r| r.address.as_str()) != Some(primary) {
        return;
    }
    let set: Vec<BtcUtxo> = set
        .into_iter()
        .filter(|u| records.iter().any(|r| r.address == u.address))
        .collect();
    CHANNEL.btc_utxos_tx.send_modify(|(set_wallet, union)| {
        *set_wallet = Some(primary.to_string());
        *union = set;
    });
    recompute_btc_balance();
}

/// Aggregate balance = the union's confirmed coins summed — the same
/// definition the relay uses per address (`balance` = set summed), applied
/// across the wallet. The identity and key-state fields are left alone.
pub fn recompute_btc_balance() {
    let sats: u64 = CHANNEL
        .btc_utxos_rx
        .borrow()
        .1
        .iter()
        .filter(|u| u.height > 0)
        .map(|u| u.sats)
        .sum();
    CHANNEL.bitcoin_wallet_tx.send_modify(|state| {
        state.0 = sats as f64 / 100_000_000.0;
    });
}

pub async fn process_response(
    message: Message,
    _bitcoin_current_wallet: &str,
) -> Result<(), String> {
    let Message::Text(text) = message else {
        return Err("Non-text message received".to_string());
    };
    let data: Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse JSON: {}", e))?;
    if data.get("command").and_then(|c| c.as_str()) != Some("btc_utxos") {
        return Ok(());
    }
    let wallet = data
        .get("wallet")
        .and_then(|w| w.as_str())
        .ok_or("Missing wallet field")?;
    merge_btc_utxos(wallet, parse_utxos(&data["utxos"], wallet));
    Ok(())
}
