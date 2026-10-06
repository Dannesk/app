//! What each order was signed against, kept so the promise can be checked.
//!
//! A Market order signs a *bound* — the worst price we will accept on the
//! user's behalf — and the contract around it says that if the bound is ever
//! the reason a trade failed, we were wrong. That claim is only falsifiable if
//! the number we chose is written down beside what the ledger actually did
//! with it. Without this file the claim is unfalsifiable **by construction**,
//! not merely unmeasured: the bound exists for one instant inside a controller
//! message and is then gone.
//!
//! So one line per submitted order: the quote it was built from (bound,
//! expected VWAP, the ledger the walk was done on, pair and anchor) and the
//! canonical transaction hash, which is the join key against the ledger and
//! against our own history. The realized outcome is written into the same line
//! when the relay answers, so the comparison is a local read rather than a
//! chain crawl.
//!
//! **Nothing secret goes in here.** Amounts, rates and a public transaction
//! hash — the same facts an explorer shows anyone who has the address. It is
//! written through `json_storage`, so it inherits the atomic replace and the
//! owner-only mode, but it is not key material and losing it costs a
//! measurement, not money.

use serde::{Deserialize, Serialize};

use crate::bridge::json_storage;
use crate::channel::PendingTrade;

const FILE: &str = "orders.json";

/// How many orders are kept. Old enough to cover a measurement window,
/// bounded so an unattended wallet cannot grow a file without limit. The
/// newest is first.
const CAP: usize = 200;

/// One submitted order: what was promised, and what came back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderRecord {
    /// Canonical transaction hash — the join key. Computed from the signed
    /// blob, so it exists before the ledger has seen the order.
    pub hash: String,
    /// The engine pair, pay → receive.
    pub pair: String,
    /// `"pay"` or `"receive"` — which side the user typed, and therefore which
    /// side the bound is a floor or a ceiling on.
    pub anchor: String,
    /// The rate that was signed, receive-per-pay.
    pub bound: f64,
    /// The walked VWAP for this size at quote time. What we expected; `bound`
    /// is what we guaranteed, and the gap between them is the cushion. `None`
    /// under a typed limit — see [`PendingTrade::expected_vwap`].
    pub expected_vwap: Option<f64>,
    /// The ledger index the walk was done on. A quote is only as good as the
    /// ledger it was taken from, and drift is measured from here.
    pub ledger_index: u64,
    /// `LastLedgerSequence` on the signed blob — past this the order can no
    /// longer be included, whatever else happens.
    pub last_ledger: Option<u32>,
    pub pay: f64,
    pub receive: f64,
    /// Epoch seconds at submission. Local clock, for ordering only.
    pub submitted_at: u64,
    /// What the ledger did, once it says: `success` · `partial` · `killed` ·
    /// `pending` · `failed` · `expired`. `None` while still unanswered.
    pub status: Option<String>,
    /// Realized amounts, in the same units as `pay` / `receive`.
    pub filled: Option<f64>,
    pub received: Option<f64>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Every record on disk, newest first. An unreadable or absent file is an
/// empty history — this is a measurement aid, and refusing to trade because a
/// log file is corrupt would be the wrong trade-off by a wide margin.
pub fn all() -> Vec<OrderRecord> {
    json_storage::read_json::<Vec<OrderRecord>>(FILE).unwrap_or_default()
}

fn save(records: &[OrderRecord]) {
    let _ = json_storage::write_json(FILE, &records.to_vec());
}

/// Write the quote half: what this order was signed against, keyed by its
/// canonical hash. Called once, at submission, before the ledger has answered.
pub fn record(hash: &str, quote: &PendingTrade) {
    let mut records = all();
    records.retain(|r| r.hash != hash);
    records.insert(
        0,
        OrderRecord {
            hash: hash.to_string(),
            pair: quote.pair.clone(),
            anchor: quote.anchor.clone(),
            bound: quote.bound,
            expected_vwap: quote.expected_vwap,
            ledger_index: quote.ledger_index,
            last_ledger: quote.last_ledger,
            pay: quote.pay,
            receive: quote.receive,
            submitted_at: now_secs(),
            status: None,
            filled: None,
            received: None,
        },
    );
    records.truncate(CAP);
    save(&records);
}

/// Write the outcome half onto the record this order already has. A no-op when
/// the hash is unknown — an order submitted by another install, or one whose
/// record has aged past [`CAP`], is not a reason to invent a row.
pub fn settle(hash: &str, status: &str, filled: Option<f64>, received: Option<f64>) {
    let mut records = all();
    let Some(rec) = records.iter_mut().find(|r| r.hash == hash) else {
        return;
    };
    rec.status = Some(status.to_string());
    rec.filled = filled;
    rec.received = received;
    save(&records);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These tests all read and write the one real `orders.json`, so they are
    /// serialized against each other. Cargo runs tests in parallel by default
    /// and two of them interleaving on the same file is a lost write, not a
    /// flake worth retrying.
    static FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn quote() -> PendingTrade {
        PendingTrade {
            pair: "XRP/RLUSD".into(),
            anchor: "pay".into(),
            bound: 1.3714,
            expected_vwap: Some(1.3731),
            ledger_index: 106_606_128,
            last_ledger: Some(106_606_148),
            sequence: Some(42),
            hash: Some("HASH_A".into()),
            tx_id: Some("uuid-a".into()),
            pay: 1000.0,
            receive: 1371.4,
        }
    }

    /// The record is the join between what we promised and what happened. A
    /// quote written without an outcome, then completed by one, has to end up
    /// as ONE row — the whole point is that the two halves meet.
    #[test]
    fn a_quote_and_its_outcome_are_one_row() {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = json_storage::remove_json(FILE);
        record("HASH_A", &quote());
        settle("HASH_A", "partial", Some(612.0), Some(839.21));

        let rows = all();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status.as_deref(), Some("partial"));
        assert!((rows[0].bound - 1.3714).abs() < 1e-9, "the promise survives the settle");
        assert!((rows[0].filled.unwrap() - 612.0).abs() < 1e-9);
        let _ = json_storage::remove_json(FILE);
    }

    /// An outcome for an order we have no quote for must not fabricate one:
    /// a row with no bound would measure a number against no counterparty,
    /// which is exactly the hole this file exists to close.
    #[test]
    fn an_unknown_hash_does_not_invent_a_row() {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = json_storage::remove_json(FILE);
        settle("HASH_NOBODY", "success", Some(1.0), Some(1.0));
        assert!(all().is_empty());
        let _ = json_storage::remove_json(FILE);
    }

    /// Newest first, and bounded — an unattended wallet cannot grow this file
    /// without limit.
    #[test]
    fn history_is_newest_first_and_capped() {
        let _guard = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = json_storage::remove_json(FILE);
        for i in 0..(CAP + 5) {
            record(&format!("HASH_{i}"), &quote());
        }
        let rows = all();
        assert_eq!(rows.len(), CAP);
        assert_eq!(rows[0].hash, format!("HASH_{}", CAP + 4));
        let _ = json_storage::remove_json(FILE);
    }
}
