use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::history::{HistoryList, HistoryPage};

/// The account's own ledger facts the send path must know BEFORE signing,
/// pushed by the relay with every balance (meta `AccountRoot.FinalFields` on
/// each validated tx, `account_info` on import, the cached hash on connect).
///
/// `sequence` is the NEXT sequence this account's transaction must carry. It
/// moves only when one of OUR transactions validates — an incoming payment
/// never touches it — and our sends are serialised by the activity log, so no
/// in-flight arithmetic lives here. The same seed signing from another device
/// costs one `tefPAST_SEQ`; the relay refreshes this on that tx's validation
/// and the retry just works.
///
/// `owner_count × reserve_inc + reserve_base` is the chain-exact reserve.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct XrpAccount {
    pub sequence: Option<u32>,
    pub owner_count: Option<u32>,
}

/// Our XRPL node's own state and the fee market, measured by the relay off its
/// one xrpld socket and pushed as ONE frame to every client (`xrp_node_stats`).
/// Twin of [`super::BtcNodeStats`]: every field is `Option`, a field we cannot
/// measure stays `None` and draws `—`, and the relay clears the whole frame
/// when it loses the node.
///
/// Drops everywhere. `open_ledger_fee` is what a transaction must pay to enter
/// the ledger being built right now (base × load factor, rounded up) — THIS is
/// the signing fee, not `fee_base`; under escalation the base fee gets queued.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XrpNodeStats {
    pub ledger_index: Option<u64>,
    pub fee_base: Option<u64>,
    pub open_ledger_fee: Option<u64>,
    pub reserve_base: Option<u64>,
    pub reserve_inc: Option<u64>,
    pub txn_count: Option<u64>,
    pub peers: Option<u64>,
    /// `full`/`proposing`/`validating` ⟹ serving the validated ledger.
    pub synced: Option<bool>,
    pub state: Option<String>,
}

/// Which half of the XRP Transactions modal is showing. The split is
/// UNRESOLVED vs RESOLVED, exactly as BTC's `BtcTxTab`: `Open` holds GTC
/// offers still resting on the book — the only unresolved thing on XRPL —
/// and `Settled` everything finished: successes, failures, cancelled offers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TxTab {
    Open,
    #[default]
    Settled,
}

/// A transaction's terminal state, as the relay reads it off validated
/// metadata.
///
/// `Partial` and `Killed` exist because an immediate-or-cancel order has
/// neither of the two states the other four describe. It does not rest, so it
/// is never `Pending`; the ledger accepted it, so it is never `Failed`; and
/// calling a 5% fill `Success` is the untruth this enum was widened to stop.
/// Both are SETTLED — nothing about them is still in flight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TransactionStatus {
    Success,
    /// Some of the order traded and the rest is gone: the defining outcome of
    /// an IOC over a book that could not cover the whole size.
    Partial,
    /// None of the order traded and none of it rests — a fill-or-kill the book
    /// could not satisfy, or an IOC that crossed nothing. The fee was still
    /// charged.
    Killed,
    Failed,
    Pending,
    Cancelled,
}

/// The quote an order was signed against, alive from the moment Broadcast is
/// pressed until the ledger answers.
///
/// One slot, not a map: the signing surface takes one order at a time, exactly
/// as the activity log holds one flow at a time. It exists because the numbers
/// below are otherwise unrecoverable — the bound is computed inside a single
/// controller message and would be gone before the ledger ever replies, which
/// makes "the bound was never the reason it failed" a claim nothing can check.
///
/// `last_ledger` is also what makes an expiry knowable: past that index the
/// order cannot be included in any ledger, whatever the network is doing, and
/// that is a terminal fact rather than the 30-second watchdog's honest doubt.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingTrade {
    /// The engine pair, pay → receive (not the trader-facing market pair).
    pub pair: String,
    /// `"pay"` or `"receive"` — which side the user typed. The bound is a
    /// floor on one and a ceiling on the other, and side never decides it.
    pub anchor: String,
    /// The rate that was signed, receive-per-pay.
    pub bound: f64,
    /// The walked VWAP for this size at quote time — what we expected, as
    /// against `bound`, which is what we guaranteed. `None` under a typed
    /// limit, where the user owns the price and we claim no expectation:
    /// echoing the bound back as an expectation would make the two agree by
    /// construction and quietly poison the cushion measurement.
    pub expected_vwap: Option<f64>,
    /// The ledger index the walk was taken from.
    pub ledger_index: u64,
    /// `LastLedgerSequence` on the signed blob. `None` until the blob is
    /// built — the ticket knows the quote, the signing path knows the bounds.
    pub last_ledger: Option<u32>,
    /// Canonical transaction id, derived from the signed blob. `None` until
    /// the blob exists. It is what joins the quote to the outcome, and it is
    /// known before the order is even sent.
    pub hash: Option<String>,
    /// The account sequence the blob was signed with.
    ///
    /// This is what makes an expiry PROVABLE rather than inferred. A sequence
    /// is consumed by exactly one transaction; if the ledger has passed the
    /// blob's `LastLedgerSequence` and the account's next sequence is still
    /// this one, then nothing ever spent it and the order provably never
    /// entered a ledger. If it has moved on, something did land, and no
    /// expiry may be claimed however loud the silence is.
    pub sequence: Option<u32>,
    /// The relay's correlation id for this submission, echoed back on
    /// `submit_transaction_response`. It is the ONLY field in that reply
    /// identifying which order it answers — responses are routed by command
    /// string alone — so an outcome that does not match this one is not this
    /// order's outcome and must not be written onto its record.
    pub tx_id: Option<String>,
    pub pay: f64,
    pub receive: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TransactionState {
    pub transactions: HashMap<String, TransactionData>,
    /// The `transactions` pane's paging — every kind but an offer.
    pub tx_page: HistoryPage,
    /// The `orders` pane's paging — settled offers; a resting one is never
    /// paged, the startup reply carries every one of them.
    pub orders_page: HistoryPage,
}

impl TransactionState {
    /// Whether a record is an order (an `OfferCreate`) — the split the two
    /// panes draw, and the relay's own rule for which list a row is in.
    pub fn is_order(t: &TransactionData) -> bool {
        matches!(t.order_type.as_str(), "offercreate" | "offer_create")
    }

    /// Settled rows of `list` this client holds — the `offset` a page asks
    /// from, and what the end row compares against the server's total.
    /// Counted off the records, not the rows drawn: the server counts records.
    pub fn held(&self, list: HistoryList) -> usize {
        self.transactions
            .values()
            .filter(|t| match list {
                HistoryList::XrpTransactions => !Self::is_order(t),
                HistoryList::XrpOrders => Self::is_order(t) && t.status != TransactionStatus::Pending,
                HistoryList::BtcTransactions => false,
            })
            .count()
    }

    pub fn page(&self, list: HistoryList) -> &HistoryPage {
        match list {
            HistoryList::XrpOrders => &self.orders_page,
            _ => &self.tx_page,
        }
    }

    pub fn page_mut(&mut self, list: HistoryList) -> &mut HistoryPage {
        match list {
            HistoryList::XrpOrders => &mut self.orders_page,
            _ => &mut self.tx_page,
        }
    }

    /// A reply with rows landed: merge them (a repeat rewrites itself) and
    /// take the facts for both lists — every history-bearing reply carries
    /// both counts. `page` names the list whose request this reply answers,
    /// if it is a page; the startup reply answers none.
    pub fn apply_reply(&mut self, rows: Vec<TransactionData>, reply: &serde_json::Value, page: Option<HistoryList>) {
        for tx in rows {
            self.transactions.insert(tx.tx_id.clone(), tx);
        }
        self.tx_page.facts(HistoryList::XrpTransactions, reply);
        self.orders_page.facts(HistoryList::XrpOrders, reply);
        if let Some(list) = page {
            self.page_mut(list).done();
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Deserialize)]
pub struct TransactionData {
    pub tx_id: String,
    pub status: TransactionStatus,
    pub execution_price: String,
    pub order_type: String,
    pub timestamp: String,
    /// What was ASKED for — `TakerPays` on an offer, the delivered amount on a
    /// payment. It is never overwritten with what is left or what filled;
    /// those have their own fields below.
    pub amount: String,
    pub currency: String,
    pub fee: String,
    pub flags: Option<String>,
    pub receiver: String,
    pub sender: String,
    pub sequence: Option<u32>,
    /// Payments only: the routing tag the sender set, if any. `None` on a
    /// row written before the relay recorded it (a re-import fills it in).
    pub destination_tag: Option<u32>,
    /// Offers only. The PAY side of the request (`TakerGets`). [`Self::amount`]
    /// carries the receive side; on a pay-anchored (`tfSell`) order — the app's
    /// default — this is the side the user typed, and therefore the side a
    /// shortfall has to be stated against.
    pub pay_amount: Option<String>,
    pub pay_currency: Option<String>,
    /// Offers only. What actually traded, in the pay asset. `None` on a wire
    /// that predates the field, and cleared deliberately when a status was
    /// inferred rather than measured (see the relay's import backfill).
    pub filled: Option<String>,
    pub filled_currency: Option<String>,
    /// What actually arrived, in the receive asset.
    pub received: Option<String>,
    pub received_currency: Option<String>,
    /// Still outstanding on a resting offer, in the receive asset. Zero once
    /// the offer is off the book.
    pub remaining: Option<String>,
    pub remaining_currency: Option<String>,
    /// Filled ÷ requested on the anchored side, `0.0..=1.0`.
    pub coverage: Option<f64>,
    /// The rate the fill actually came in at. Distinct from
    /// [`Self::execution_price`], which is the rate that was asked for — on a
    /// partial fill the two are different numbers. Both are oriented by the
    /// relay (`outcome::rate`): token per XRP whenever one leg is XRP, so a buy
    /// and a sell of the same market show the same number as the ticket;
    /// receive per pay on a token/token pair. The panel adds the words.
    pub fill_price: Option<String>,
}




#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Trade {
    pub step: u8,
    pub base_asset: Option<String>,
    pub quote_asset: Option<String>,
    pub amount: Option<String>,
    pub limit_price: Option<String>,
    pub fee_percentage: f64,
    pub flags: Option<Vec<String>>,
    pub error: Option<String>,
    pub asset: String,
}

