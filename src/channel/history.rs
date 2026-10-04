//! Paged history — the app's side of `load 20 more ›` (2026-10-04).
//!
//! Both relays keep 100 rows per wallet and send the newest 20 of each list
//! at startup; the rest is asked for a page at a time (`get_history` on the
//! XRP relay, `get_bitcoin_history` on the Bitcoin relay). Every reply that
//! carries rows also carries the facts here — how many settled rows the
//! server holds of the list and whether older history is known to exist —
//! and the app pages until it holds as many as the server does. The offset it
//! sends is the number of settled rows it already holds, so a row that lands
//! live between two pages shifts the window by one and costs a repeat the
//! map already dedups, never a gap.
//!
//! The status is this client's own: `Fetching` from the click until the reply,
//! the unsent report (link down) or the timeout, whichever is first; `Failed`
//! until `retry ›` is pressed. A late reply or timeout for an earlier request
//! is told apart by `seq`.

use serde_json::Value;

/// Which list a page is for — the two XRP panes draw two kinds off one wire,
/// Bitcoin has one list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HistoryList {
    XrpTransactions,
    XrpOrders,
    BtcTransactions,
}

impl HistoryList {
    /// The relay's word for the list, on the XRP wire.
    pub fn kind(self) -> &'static str {
        match self {
            HistoryList::XrpTransactions => "transactions",
            HistoryList::XrpOrders => "orders",
            HistoryList::BtcTransactions => "transactions",
        }
    }

    /// The facts field in a reply that counts this list.
    pub fn total_field(self) -> &'static str {
        match self {
            HistoryList::XrpTransactions | HistoryList::BtcTransactions => "history_transactions",
            HistoryList::XrpOrders => "history_orders",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageStatus {
    #[default]
    Idle,
    Fetching,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HistoryPage {
    pub status: PageStatus,
    /// Settled rows of this list the server holds. `None` until a reply has
    /// said — a relay from before paging never does — and no end row is drawn
    /// until then.
    pub total: Option<usize>,
    /// Older rows than the server holds are known to exist: its list is at
    /// its depth, or (XRP) the import saw more on the node than it took.
    pub capped: bool,
    /// Ticks per request, so a reply or timeout for an earlier one is ignored.
    pub seq: u32,
}

impl HistoryPage {
    /// A reply that carries the facts landed — a page of any list, or the
    /// startup reply. Fields the reply does not carry leave the facts as they
    /// were. The request, if one is in flight, is still in flight: only
    /// [`Self::done`] ends it, and only the page reply calls that.
    pub fn facts(&mut self, list: HistoryList, reply: &Value) {
        if let Some(n) = reply.get(list.total_field()).and_then(|v| v.as_u64()) {
            self.total = Some(n as usize);
        }
        if let Some(c) = reply.get("history_capped").and_then(|v| v.as_bool()) {
            self.capped = c;
        }
    }

    /// This list's page landed: the request is over.
    pub fn done(&mut self) {
        self.status = PageStatus::Idle;
    }

    /// A page was asked for. Returns the request's `seq`.
    pub fn start(&mut self) -> u32 {
        self.seq = self.seq.wrapping_add(1);
        self.status = PageStatus::Fetching;
        self.seq
    }

    /// The request `seq` could not be served (unsent, or timed out) — unless
    /// it already was, or a newer one is in flight.
    pub fn fail(&mut self, seq: u32) {
        if self.status == PageStatus::Fetching && self.seq == seq {
            self.status = PageStatus::Failed;
        }
    }

    /// Whether any request of this list could still be waiting: the unsent
    /// report names the list but not the `seq`.
    pub fn fail_any(&mut self) {
        if self.status == PageStatus::Fetching {
            self.status = PageStatus::Failed;
        }
    }

    /// More settled rows than the `held` ones exist on the server.
    pub fn more_than(&self, held: usize) -> bool {
        self.total.is_some_and(|t| held < t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_reply_sets_the_facts_and_its_own_page_ends_the_request() {
        let mut p = HistoryPage::default();
        let seq = p.start();
        assert_eq!(p.status, PageStatus::Fetching);
        let reply = json!({ "history_orders": 7, "history_transactions": 40, "history_capped": true });
        // The startup reply, or another list's page: facts only.
        p.facts(HistoryList::XrpOrders, &reply);
        assert_eq!(p.total, Some(7));
        assert!(p.capped);
        assert_eq!(p.status, PageStatus::Fetching, "still waiting for this list's page");
        p.done();
        assert_eq!(p.status, PageStatus::Idle);
        // A late failure for the request that already landed changes nothing.
        p.fail(seq);
        assert_eq!(p.status, PageStatus::Idle);
    }

    /// A relay from before paging says nothing; the page stays unknown and
    /// no end row is ever drawn for it.
    #[test]
    fn an_old_relay_leaves_the_page_unknown() {
        let mut p = HistoryPage::default();
        p.facts(HistoryList::BtcTransactions, &json!({ "balance": "0" }));
        assert_eq!(p.total, None);
        assert!(!p.more_than(0));
    }

    #[test]
    fn a_failure_names_the_request_it_is_for() {
        let mut p = HistoryPage::default();
        let first = p.start();
        let second = p.start();
        p.fail(first);
        assert_eq!(p.status, PageStatus::Fetching, "the first request was superseded");
        p.fail(second);
        assert_eq!(p.status, PageStatus::Failed);
        p.facts(HistoryList::XrpTransactions, &json!({ "history_transactions": 3 }));
        p.done();
        assert_eq!(p.status, PageStatus::Idle);
        assert!(p.more_than(2));
        assert!(!p.more_than(3));
    }
}
