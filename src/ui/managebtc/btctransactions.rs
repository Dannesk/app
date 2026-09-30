//! The BTC transactions **row model** — everything that has happened to
//! this wallet, resolved from the channel into rows for the `transactions`
//! pane ([`crate::ui::managebtc::panes::transactions`]). The pane draws;
//! this file decides what each row says.
//!
//! The generating rule is that **a transaction is in exactly one set** —
//! unresolved or settled — and both share one row grammar, so a transaction
//! that resolves keeps its shape and changes its status word. It is the same
//! record gaining fields, and it should look like it.
//!
//! The rows lead with the txid because every BTC transaction is the same kind
//! of thing; XRP puts its type word in that column instead. No `· this wallet`
//! tag on either party: the row's sign and its state column already carry the
//! direction, and the tag restated it.
//!
//! ## Where the facts come from
//!
//! Everything is live. txid, signed amount, fee, direction and the two
//! addresses ride the per-wallet channel; the tip that confirmations count
//! against comes off `CHANNEL.btc_node_rx` — the SAME frame the dashboard's
//! telemetry bar reads, never a second copy, because the two sit one click
//! apart and disagreeing by a refresh would discredit both. Confirmations are
//! derived here rather than carried on the wire: `tip - height + 1`, so the
//! count cannot be stale in the way a transmitted count would be one block
//! after it was sent.
//!
//! [`in_flight`] is the dashboard's pending line, computed from these same
//! resolved rows — one pass, so the line and the rows cannot disagree.
//!
//! Four traps in that data, all handled below:
//!
//! 1. **Direction must come from `sender_addresses`.** `receiver_addresses`
//!    excludes our own address, so on an incoming transaction we are simply
//!    absent from it and it cannot decide anything.
//! 2. **`receiver_addresses` is not "who I paid" on an incoming transaction.**
//!    It holds the payer's *other* outputs — usually their change address.
//!    Rendering it as the destination would show a stranger's change address to
//!    someone who was just paid, so `to` is synthesised from our own address
//!    instead, which is the truth and is something we already know.
//! 3. **Address lists arrive unordered** (they come off a `HashSet` in the
//!    relay), so neither may be indexed blind — see [`party`].
//! 4. **`fees` is `"0"` when unknown**, which happens when `getmempoolentry`
//!    missed. A real transaction cannot pay zero and sit in a mempool, so `"0"`
//!    reads as *unknown* and renders `—`. Confirmation repairs it: the block
//!    carries a measured fee.

use iced::Color;
use chrono::{Datelike, Local, TimeZone};
use crate::controller::message::Message;
use crate::channel::CHANNEL;
use crate::channel::btc::{BitcoinTransactionStatus, BtcTransactionData};
use crate::ui::components::tx_panel::{Line, Seg, Tail};
use crate::utils::add_commas;
use crate::utils::theme::CompactPalette;

/// Rendered in place of any value that cannot currently be measured.
const NA: &str = "—";

// What this app can honestly say about how far back history goes has two
// halves, because since the diff-heal (2026-08-18) there are two truths.
//
// The receive that FUNDED each still-unspent coin IS recovered: the relay's
// fetch-through diff finds coins the stored record can't explain and reads
// the full transaction back off our own node, which works for as long as the
// node still holds the block (a pruned node keeps roughly half a year).
// Everything else — spends from before the wallet was added, receives whose
// coins are already spent away, anything past the prune window — has nothing
// to be read from, so that half of the record still starts at watch time.
// The old single line ("history starts when this wallet was added") became a
// lie the day the heal shipped: an imported wallet now opens with settled
// rows that predate it. Neither half is said in the app — the landing-page
// docs explain both.

/// One transaction, resolved from the channel into exactly what the card draws.
/// Built once per render so a summary row and its expanded detail cannot
/// disagree about the same transaction.
pub(crate) struct Tx {
    pub(crate) txid: String,
    /// Positive when the wallet is receiving.
    signed_btc: f64,
    /// Fee in satoshis, or `None` when nobody could measure it (see module docs).
    fee_sats: Option<u64>,
    pub(crate) incoming: bool,
    pub(crate) confirmed: bool,
    /// Left the mempool without being mined. Files as SETTLED — it is as
    /// finished as a mined transaction, it just finished the other way.
    dropped: bool,
    /// Superseded by a fee bump of ours. Files as SETTLED too, but it is not
    /// a failure: the payment went on under `replaced_by`.
    replaced: bool,
    replaced_by: Option<String>,
    /// What the replacement pays, read off ITS record when we hold one — the
    /// number the person chose on the bump stack.
    replacement_fee: Option<u64>,
    /// For a REPLACEMENT: the txid it superseded, and what that one offered.
    /// Set by [`fold_replaced`], which is also what hides the original.
    replaces: Option<String>,
    original_fee: Option<u64>,
    /// Epoch seconds the relay first saw it, or `None` if the stamp is unusable.
    first_seen: Option<u64>,
    /// Epoch seconds of the carrying block. `None` while pending.
    confirmed_at: Option<u64>,
    /// Epoch seconds bitcoind reported it gone from the mempool.
    dropped_at: Option<u64>,
    block_height: Option<u32>,
    from: Option<Party>,
    to: Option<Party>,
}

/// One side of a transaction as the panel names it.
struct Party {
    addr: String,
    /// How many further addresses were on this side and are not shown. A
    /// transaction with several payers has no single "from", and quietly
    /// picking one would be a lie of omission.
    extra: usize,
}

/// Choose the address to show from an unordered list.
///
/// Ours wins when present — on an outgoing transaction that is the answer the
/// user is checking. Otherwise the first, carrying a count of the rest. The
/// lists come off a `HashSet` in the relay, so `first()` is arbitrary and the
/// `+N` is what keeps that honest rather than hidden.
fn party(list: &[String], own: &str) -> Option<Party> {
    if list.iter().any(|a| a == own) {
        return Some(Party { addr: own.to_string(), extra: list.len() - 1 });
    }
    let first = list.first()?;
    Some(Party { addr: first.clone(), extra: list.len() - 1 })
}

impl Tx {
    fn from(tx: &BtcTransactionData, own: &str) -> Self {
        // Direction from the input side only — see the module docs.
        let incoming = !tx.sender_addresses.iter().any(|a| a == own);
        let amount = tx.amount.parse::<f64>().unwrap_or(0.0);

        // On an incoming transaction the destination is us, and we know that
        // without being told: `receiver_addresses` holds the payer's other
        // outputs, not ours.
        let (from, to) = if incoming {
            (
                party(&tx.sender_addresses, own),
                Some(Party { addr: own.to_string(), extra: 0 }),
            )
        } else {
            (party(&tx.sender_addresses, own), party(&tx.receiver_addresses, own))
        };

        Self {
            txid: tx.txid.clone(),
            signed_btc: if incoming { amount } else { -amount },
            fee_sats: tx.fees.parse::<u64>().ok().filter(|f| *f > 0),
            incoming,
            confirmed: matches!(tx.status, BitcoinTransactionStatus::Success),
            dropped: matches!(
                tx.status,
                BitcoinTransactionStatus::Dropped
                    | BitcoinTransactionStatus::Failed
                    | BitcoinTransactionStatus::Cancelled
            ),
            replaced: matches!(tx.status, BitcoinTransactionStatus::Replaced),
            replaced_by: tx.replaced_by.clone(),
            replacement_fee: None,
            replaces: None,
            original_fee: None,
            first_seen: tx.timestamp.parse::<u64>().ok().filter(|t| *t > 0),
            confirmed_at: tx.confirmed_at.as_deref().and_then(|s| s.parse().ok()).filter(|t| *t > 0),
            dropped_at: tx.dropped_at.as_deref().and_then(|s| s.parse().ok()).filter(|t| *t > 0),
            block_height: tx.block_height.as_deref().and_then(|s| s.parse().ok()).filter(|h| *h > 0),
            from,
            to,
        }
    }

    /// Still in the mempool, waiting on a block. Everything else has
    /// finished, one way or the other.
    pub(crate) fn unresolved(&self) -> bool {
        !self.confirmed && !self.dropped && !self.replaced
    }

    /// The instant this row sorts and reads by: when it resolved if it has,
    /// when we first saw it if it hasn't. A drop resolves too, so it sorts
    /// among the settled by when it failed, not by when it was sent.
    pub(crate) fn at(&self) -> Option<u64> {
        self.confirmed_at.or(self.dropped_at).or(self.first_seen)
    }

    /// `a3f9…c21d` — 4 and 4.
    pub(crate) fn short_txid(&self) -> String {
        elide(&self.txid, 4, 4)
    }

    pub(crate) fn amount_str(&self) -> String {
        format!("{:+.8}", self.signed_btc)
    }

    /// The status column.
    ///
    /// A settled row reports the OUTCOME — `success` or `rejected` — because
    /// those are the only two things that can have happened to it. `confirmed`
    /// was borrowed from the successful case and left the failed one wearing a
    /// word that quietly means the opposite.
    ///
    /// `rejected` over `failed`: the network declined to include it, which is
    /// true of all four ways this happens (replaced, evicted, conflicted,
    /// expired). `failed` reads as though the app broke.
    ///
    /// A waiting row has no outcome yet, so it names its direction. It used to
    /// print the fee here instead; the fee is on the detail line one click
    /// away, and a column that means "direction" on one row and "cost" on the
    /// next is a column that means nothing.
    pub(crate) fn state_label(&self) -> &'static str {
        if self.confirmed {
            "success"
        } else if self.replaced {
            // Superseded by our own bump: an outcome, but not a verdict on the
            // payment, which is still on its way under the new txid.
            "replaced"
        } else if self.dropped {
            "rejected"
        } else if self.incoming {
            "incoming"
        } else {
            "outgoing"
        }
    }

    /// Colour only where an OUTCOME is the subject: a settled row is green
    /// or red. A waiting row names a direction, which is not a severity, so
    /// both directions read `dim`.
    pub(crate) fn state_color(&self, p: &'static CompactPalette) -> Color {
        if self.dropped {
            p.red
        } else if self.confirmed {
            p.green
        } else {
            // Waiting rows AND replaced rows: neither is a severity.
            p.dim
        }
    }
}

/// Everything a row needs that is the same for every row. Passed as one value
/// so a row builder's signature stays about the row.
pub(crate) struct Ctx {
    pub(crate) btc_rate: f64,
    pub(crate) ccy: &'static str,
    pub(crate) tip: Option<u64>,
    pub(crate) p: &'static CompactPalette,
}

/// Both sets — unresolved, settled — each newest-first. A transaction is in
/// exactly one of them.
pub(crate) fn split(own: &str) -> (Vec<Tx>, Vec<Tx>) {
    let txs = CHANNEL.btc_transactions_rx.borrow();
    let mut all = fold_replaced(txs.transactions.values().map(|t| Tx::from(t, own)).collect());
    // Timestamps are decimal epoch-second strings of differing lengths, so they
    // must be compared as numbers, not lexically.
    all.sort_by_key(|t| std::cmp::Reverse(t.at().unwrap_or(0)));
    // The split is UNRESOLVED vs RESOLVED, not pending vs confirmed.
    //
    // A drop is a hard, permanent failure — the transaction is as finished as a
    // mined one, it just finished the other way. Filing it as resolved gives
    // the failure the long-term record it needs.
    all.into_iter().partition(|t| t.unresolved())
}

/// One payment, one row.
///
/// A fee bump is a NEW transaction on the chain, so the record holds two —
/// but the wallet made one payment, and the row that matters is the live one.
/// Once the replacement's record is on hand, the original folds away and the
/// replacement's row carries the history instead: what it was bumped from,
/// and which txid it superseded. The original stays visible in exactly one
/// case — its replacement dropped — because then it is the one that can still
/// confirm (both were valid; miners could have taken either), and a row
/// reading `replaced` beside a `rejected` replacement is the honest picture.
/// An original whose replacement we hold no record of (a bump signed on
/// another device, a trimmed history) also stays, saying what it can.
fn fold_replaced(mut all: Vec<Tx>) -> Vec<Tx> {
    // (original txid, original fee) → replacement txid, for every fold.
    let folds: Vec<(String, Option<u64>, String)> = all
        .iter()
        .filter(|t| t.replaced)
        .filter_map(|t| {
            let by = t.replaced_by.clone()?;
            let live = all.iter().find(|r| r.txid == by)?;
            (!live.dropped).then(|| (t.txid.clone(), t.fee_sats, by))
        })
        .collect();
    for (orig, orig_fee, by) in &folds {
        if let Some(r) = all.iter_mut().find(|r| r.txid == *by) {
            r.replaces = Some(orig.clone());
            r.original_fee = *orig_fee;
        }
    }
    all.retain(|t| !folds.iter().any(|(orig, _, _)| *orig == t.txid));
    // An original that stays visible still names what it pays now, when the
    // replacement's record is on hand.
    let fees: Vec<(String, Option<u64>)> = all.iter().map(|t| (t.txid.clone(), t.fee_sats)).collect();
    for t in all.iter_mut() {
        if t.replaced {
            t.replacement_fee = t
                .replaced_by
                .as_deref()
                .and_then(|by| fees.iter().find(|(id, _)| id == by))
                .and_then(|(_, f)| *f);
        }
    }
    all
}

// ── Rows ─────────────────────────────────────────────────────────────────────

/// A resolved row reads the wall clock it settled at — confirmed or dropped,
/// both are finished. Only a row still waiting reads an age, because that is
/// the only number still moving.
pub(crate) fn time_label(tx: &Tx, now: u64) -> String {
    if !tx.unresolved() {
        tx.at().map_or(NA.to_string(), stamp)
    } else {
        match tx.first_seen {
            // Saturating: the stamp is the RELAY's clock, so a workstation
            // running behind it would otherwise underflow into a huge age.
            Some(t) => age(now.saturating_sub(t)),
            None => NA.to_string(),
        }
    }
}

/// The detail lines under an expanded row. Settled and pending share the middle
/// of the list and differ only at the ends, which is what makes a transaction's
/// detail recognisable as it settles.
pub(crate) fn detail_lines(tx: &Tx, c: &Ctx) -> Vec<Line> {
    let p = c.p;
    let mut rows: Vec<Line> = Vec::new();

    if tx.confirmed {
        rows.push((
            "block",
            match (tx.block_height, tx.block_height.and_then(|h| confirmations(h, c.tip))) {
                (Some(h), Some(n)) => vec![
                    Seg::mono(add_commas(h as i64), p.text),
                    Seg::mono(" \u{b7} ", p.faint),
                    Seg::mono(format!("{n} confirmation{}", if n == 1 { "" } else { "s" }), p.text),
                ],
                (Some(h), None) => vec![Seg::mono(add_commas(h as i64), p.text)],
                _ => vec![Seg::mono(NA, p.muted)],
            },
        ));
    }

    // Fee in satoshis is what the user chose in btcsend, so it is the unit
    // shown here; sat/vB appears only on the telemetry bar and in the send flow.
    rows.push((
        "fee",
        if tx.incoming {
            vec![Seg::word("incoming — the sender paid it", p.muted)]
        } else if tx.dropped || tx.replaced {
            // What it OFFERED, not what it cost. A dropped transaction never
            // confirmed, so nothing was paid — rendering this in fee-amber
            // beside a fiat figure would bill the user for a payment that did
            // not happen, on the one screen they came to check exactly that.
            match tx.fee_sats {
                Some(f) => vec![Seg::mono(format!("{f} sats"), p.muted), Seg::word(" \u{b7} never paid", p.faint)],
                None => vec![Seg::mono(NA, p.muted)],
            }
        } else {
            match tx.fee_sats {
                Some(f) => {
                    let mut r = vec![Seg::mono(format!("{f} sats"), p.amber)];
                    if c.btc_rate > 0.0 {
                        r.push(Seg::mono(" \u{b7} ", p.faint));
                        r.push(Seg::mono(format!("{} {}", fiat(f as f64 / 1e8 * c.btc_rate), c.ccy), p.muted));
                    }
                    // A bump: what the payment offered before this row took
                    // over. The original's row is folded; this is its trace.
                    if let Some(was) = tx.original_fee.filter(|_| tx.replaces.is_some()) {
                        r.push(Seg::word(" \u{b7} bumped from ", p.faint));
                        r.push(Seg::mono(format!("{was} sats"), p.muted));
                    }
                    r
                }
                // Nobody could measure it — say so rather than render a zero fee.
                None => vec![Seg::mono(NA, p.muted)],
            }
        },
    ));

    for (name, side) in [("from", &tx.from), ("to", &tx.to)] {
        rows.push((
            name,
            match side {
                Some(party) => {
                    let mut r = vec![Seg::mono(elide_addr(&party.addr), p.text)];
                    if party.extra > 0 {
                        r.push(Seg::mono(format!(" +{}", party.extra), p.muted));
                    }
                    r
                }
                None => vec![Seg::mono(NA, p.muted)],
            },
        ));
    }

    if let Some(orig) = &tx.replaces {
        rows.push(("replaces", vec![Seg::mono(elide(orig, 4, 4), p.text)]));
    }

    if tx.replaced {
        let mut r = match tx.replacement_fee {
            Some(f) => vec![Seg::word("replaced at ", p.muted), Seg::mono(format!("{f} sats"), p.text)],
            None => vec![Seg::word("replaced at a higher fee", p.muted)],
        };
        if let Some(by) = &tx.replaced_by {
            r.push(Seg::word(" \u{b7} now ", p.faint));
            r.push(Seg::mono(elide(by, 4, 4), p.text));
        }
        rows.push(("status", r));
    } else if tx.dropped {
        rows.push(("status", vec![Seg::word("left the mempool without being mined", p.red)]));
        // Stated flatly and without a remedy. We know it left the mempool; we
        // do NOT know why, and the four reasons want opposite advice — a
        // replacement is already in flight, an eviction wants a resend. The
        // balance on the screen behind this is the arbiter for which happened.
        rows.push(("note", vec![Seg::word("this transaction will not confirm", p.muted)]));
    }

    rows
}

/// The line that closes an expanded transaction. Settled rows offer the txid;
/// a pending outgoing row offers the fee bump — `modify fee ›` arms the
/// pane's `modify fee` face (`btcbump::lead`). A pending INCOMING row cannot:
/// the sender owns the fee, and that is said plainly and without the `›`.
pub(crate) fn tail(tx: &Tx, p: &'static CompactPalette) -> Tail {
    if tx.confirmed || tx.dropped || tx.replaced {
        return Tail::Link { label: "copy txid", color: p.dim, msg: Message::BtcCopyTxid(tx.txid.clone()) };
    }
    if tx.incoming {
        Tail::Prose { s: "incoming — the sender sets the fee", color: p.muted }
    } else {
        Tail::Link { label: "modify fee", color: p.dim, msg: Message::BtcBumpClicked(tx.txid.clone()) }
    }
}

/// `tip - height + 1`, or `None` when the node frame cannot currently say.
/// A tip behind the block is clock-free nonsense (a reorg mid-render, or a
/// frame from before the block landed) and answers nothing rather than zero.
fn confirmations(height: u32, tip: Option<u64>) -> Option<u64> {
    let tip = tip?;
    (tip >= height as u64).then(|| tip - height as u64 + 1)
}

// ── Formatters ───────────────────────────────────────────────────────────────

/// `bc1qyfxmsj…amh06f67` — 10 and 8, so **every** address type lands on 19
/// columns: 42 for native SegWit, 62 for Taproot, 34 for the legacy forms.
/// Fixed at both ends so `from` and `to` land on the same width.
fn elide_addr(a: &str) -> String {
    elide(a, 10, 8)
}

fn elide(s: &str, head: usize, tail: usize) -> String {
    let n = s.chars().count();
    if n <= head + tail + 1 {
        return s.to_string();
    }
    let h: String = s.chars().take(head).collect();
    let t: String = s.chars().skip(n - tail).collect();
    format!("{h}…{t}")
}

/// `9m ago` under an hour, `1.2h ago` above — a pending transaction that has
/// been waiting a day is the interesting case.
fn age(secs: u64) -> String {
    if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{:.1}h ago", secs as f64 / 3600.0)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// `08-15 09:44` in LOCAL time, and `2025-11-02` for a different year — a year
/// that needs saying is worth more than a minute that doesn't.
fn stamp(secs: u64) -> String {
    let Some(dt) = Local.timestamp_opt(secs as i64, 0).single() else {
        return NA.to_string();
    };
    if dt.year() == Local::now().year() {
        dt.format("%m-%d %H:%M").to_string()
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}

/// A fee in the display currency, figure only — the code follows it on the
/// line. Sub-cent rounds up: a fee costing a fraction of a cent is cheap, not
/// free, and `0.00` would say the wrong one.
fn fiat(v: f64) -> String {
    format!("{:.2}", v.max(0.01))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::managebtc::panes::transactions::{CELL, TIME_W};
    use crate::utils::theme;

    fn tx(
        txid: &str,
        amount: &str,
        fees: &str,
        ts: u64,
        senders: &[&str],
        receivers: &[&str],
        confirmed: Option<(u32, u64)>,
    ) -> BtcTransactionData {
        BtcTransactionData {
            txid: txid.to_string(),
            status: if confirmed.is_some() {
                BitcoinTransactionStatus::Success
            } else {
                BitcoinTransactionStatus::Pending
            },
            amount: amount.to_string(),
            fees: fees.to_string(),
            receiver_addresses: receivers.iter().map(|s| s.to_string()).collect(),
            sender_addresses: senders.iter().map(|s| s.to_string()).collect(),
            timestamp: ts.to_string(),
            confirmed_at: confirmed.map(|(_, t)| t.to_string()),
            dropped_at: None,
            block_height: confirmed.map(|(h, _)| h.to_string()),
            replaced_by: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            vsize: None,
        }
    }

    const OWN: &str = "bc1qyfxmsjaaaaaaaaaaaaaaaaaaaaaaaaaaamh06f67";
    const THEM: &str = "bc1qu5g2twbbbbbbbbbbbbbbbbbbbbbbbbbbzkcum7szm";

    const P: &CompactPalette = &theme::COMPACT_OBSIDIAN;

    fn ctx() -> Ctx {
        Ctx { btc_rate: 60_000.0, ccy: "USD", tip: Some(999_999), p: P }
    }

    fn flat(lines: &[Line]) -> String {
        lines
            .iter()
            .flat_map(|(l, runs)| std::iter::once(l.to_string()).chain(runs.iter().map(|seg| seg.s.clone())))
            .collect()
    }

    #[test]
    fn direction_comes_from_the_sender_side() {
        let out = Tx::from(&tx("a", "0.00004120", "56", 100, &[OWN], &[THEM], None), OWN);
        assert!(!out.incoming);
        assert!(out.amount_str().starts_with('-'));

        // Incoming: we are not among the inputs. `receiver_addresses` is empty
        // here on purpose — the relay strips our own address from it.
        let inc = Tx::from(&tx("b", "0.00002500", "0", 100, &[THEM], &[], None), OWN);
        assert!(inc.incoming);
        assert!(inc.amount_str().starts_with('+'));
    }

    /// The destination of an incoming transaction is US, and it must be read
    /// off what we already know rather than off `receiver_addresses` — which
    /// on an incoming transaction holds the PAYER's change address.
    #[test]
    fn an_incoming_transaction_is_addressed_to_this_wallet() {
        let payers_change = "bc1qtheirchangeaddressxxxxxxxxxxxxxxxxxxxxxx";
        let inc = Tx::from(
            &tx("b", "0.00002500", "0", 100, &[THEM], &[payers_change], None),
            OWN,
        );
        let to = inc.to.as_ref().expect("an incoming tx always has a destination: us");
        assert_eq!(to.addr, OWN, "the destination is this wallet's own address");
        assert!(!to.addr.contains("theirchange"), "never the payer's change");

        let from = inc.from.as_ref().expect("the payer is on the frame");
        assert_eq!(from.addr, THEM);

        // Ours is named by its address and nothing else — the `+` on the row
        // already says which side is ours.
        let d = flat(&detail_lines(&inc, &ctx()));
        assert!(d.contains(&elide_addr(OWN)), "{d}");
        assert!(!d.contains("wallet"), "{d}");
    }

    /// Address lists arrive off a `HashSet`, so order is arbitrary. Ours must
    /// win on the sender side however the set happened to iterate.
    #[test]
    fn our_address_wins_an_unordered_sender_list() {
        for senders in [vec![OWN, THEM], vec![THEM, OWN]] {
            let t = Tx::from(&tx("a", "0.1", "56", 100, &senders, &[THEM], None), OWN);
            let from = t.from.unwrap();
            assert_eq!(from.addr, OWN, "ours must win regardless of iteration order");
            assert_eq!(from.extra, 1, "and must say how many it is standing in for");
        }
    }

    /// indexd defaults a missing fee to "0". A transaction cannot pay zero and
    /// sit in a mempool, so that must read as unknown, never as a real fee.
    #[test]
    fn zero_fee_is_unknown_not_free() {
        let unknown = Tx::from(&tx("a", "0.0001", "0", 100, &[OWN], &[THEM], None), OWN);
        assert_eq!(unknown.fee_sats, None);
        assert!(flat(&detail_lines(&unknown, &ctx())).contains(NA));

        let known = Tx::from(&tx("a", "0.0001", "56", 100, &[OWN], &[THEM], None), OWN);
        assert_eq!(known.fee_sats, Some(56));
        let d = flat(&detail_lines(&known, &ctx()));
        assert!(d.contains("56 sats"));
        // The fiat figure carries the display currency's code, never a glyph.
        assert!(d.contains("0.03 USD"), "{d}");
        assert!(!d.contains('$'), "{d}");
    }

    /// A confirmed row reads the time it SETTLED, not the time we first saw
    /// it. The two differ by however long the transaction waited, and the old
    /// wire carried only the first — a three-hour wait rendered three hours
    /// wrong.
    #[test]
    fn a_confirmed_row_reads_the_block_time_not_first_seen() {
        let seen_at = 1_786_750_000;
        let mined_at = 1_786_761_453;
        let t = Tx::from(
            &tx("a", "0.021", "210", seen_at, &[THEM], &[], Some((962_488, mined_at))),
            OWN,
        );
        assert!(t.confirmed);
        assert_eq!(t.first_seen, Some(seen_at), "the wait is still on the record");
        assert_eq!(t.at(), Some(mined_at), "but the row reads the block");
        assert_eq!(t.block_height, Some(962_488));
    }

    /// A drop is a hard, permanent failure — as finished as a mined block, just
    /// finished the other way. It files as SETTLED, not as pending: pending
    /// means "still waiting on a block", and a transaction that will never
    /// see one is not waiting for anything.
    #[test]
    fn a_dropped_transaction_files_as_settled_not_pending() {
        let mut raw = tx("a", "0.001", "300", 100, &[OWN], &[THEM], None);
        raw.status = BitcoinTransactionStatus::Dropped;
        raw.dropped_at = Some("500".to_string());
        let t = Tx::from(&raw, OWN);

        assert!(t.dropped);
        assert!(!t.confirmed, "it must never be filed as a success");
        assert!(!t.unresolved(), "but it IS resolved — that is the whole point");
        assert_eq!(t.state_label(), "rejected");
        assert_eq!(t.state_color(P), P.red);
        // It sorts among the settled by when it FAILED, not when it was sent.
        assert_eq!(t.at(), Some(500));

        let (pending, settled): (Vec<Tx>, Vec<Tx>) =
            vec![t].into_iter().partition(|t| t.unresolved());
        assert!(pending.is_empty(), "pending holds only what is waiting");
        assert_eq!(settled.len(), 1);
    }

    /// A dropped transaction never paid its fee. Rendering one as a cost would
    /// bill the user for a payment that did not happen, on the one screen they
    /// opened to find out whether it did.
    #[test]
    fn a_dropped_fee_is_shown_as_never_paid() {
        let mut raw = tx("a", "0.001", "300", 100, &[OWN], &[THEM], None);
        raw.status = BitcoinTransactionStatus::Dropped;
        let t = Tx::from(&raw, OWN);

        let s = flat(&detail_lines(&t, &ctx()));
        assert!(s.contains("never paid"), "a dropped fee must say it was not paid");
        assert!(!s.contains("USD"), "and must not be priced in fiat: {s}");
        assert!(s.contains("will not confirm"));
        // Nothing to wait for, so no block-time expectation is offered — and
        // `avg block` is on the telemetry bar behind the card either way.
        assert!(!s.contains("avg block"));
    }

    /// Every status the relay can write must round-trip through the wire
    /// parser. One missing arm silently deletes those records on reload.
    #[test]
    fn every_relay_status_survives_the_wire() {
        use crate::ws::commands::get_btc_transaction::parse_btc_tx;
        for (wire, want) in [
            ("pending", BitcoinTransactionStatus::Pending),
            ("confirmed", BitcoinTransactionStatus::Success),
            ("dropped", BitcoinTransactionStatus::Dropped),
            ("replaced", BitcoinTransactionStatus::Replaced),
        ] {
            let v = serde_json::json!({
                "txid": "aa", "status": wire, "amount": "0.001", "fees": "300",
                "timestamp": "100", "sender_addresses": [OWN], "receiver_addresses": [THEM],
            });
            let parsed = parse_btc_tx(&v)
                .unwrap_or_else(|| panic!("status {wire:?} was discarded by the parser"));
            assert_eq!(parsed.status, want);
        }
    }

    /// A settled row reports its outcome and a pending row its direction.
    /// A failure wearing `confirmed` says the opposite of what happened.
    #[test]
    fn the_state_column_means_outcome_or_direction() {
        let won = Tx::from(
            &tx("a", "0.021", "210", 100, &[THEM], &[], Some((962_488, 1_786_761_453))),
            OWN,
        );
        assert_eq!(won.state_label(), "success");
        assert_eq!(won.state_color(P), P.green);

        let mut raw = tx("b", "0.001", "300", 100, &[OWN], &[THEM], None);
        raw.status = BitcoinTransactionStatus::Dropped;
        let lost = Tx::from(&raw, OWN);
        assert_eq!(lost.state_label(), "rejected");
        assert_eq!(lost.state_color(P), P.red);

        // Waiting rows name their direction — never a fee, which is a cost and
        // belongs on the detail line. A direction is not a severity, so
        // neither one is coloured.
        let out = Tx::from(&tx("c", "0.001", "300", 100, &[OWN], &[THEM], None), OWN);
        let inc = Tx::from(&tx("d", "0.001", "0", 100, &[THEM], &[], None), OWN);
        assert_eq!(out.state_label(), "outgoing");
        assert_eq!(inc.state_label(), "incoming");
        assert_eq!(out.state_color(P), P.dim);
        assert_eq!(inc.state_color(P), P.dim);
    }

    /// A row our own bump superseded is settled, says so in a neutral ink,
    /// and names the replacement. It is the one settled outcome that is not
    /// a verdict on the payment.
    #[test]
    fn a_replaced_row_is_settled_but_not_a_failure() {
        let mut raw = tx("b", "0.001", "300", 100, &[OWN], &[THEM], None);
        raw.status = BitcoinTransactionStatus::Replaced;
        raw.replaced_by = Some("c0ffee0000000000000000000000000000000000000000000000000000001234".into());
        raw.dropped_at = Some("150".into());
        let t = Tx::from(&raw, OWN);
        assert!(!t.unresolved());
        assert_eq!(t.state_label(), "replaced");
        assert_eq!(t.state_color(P), P.dim);
        assert_eq!(t.at(), Some(150));
        let lines = detail_lines(&t, &ctx());
        assert!(flat(&lines).contains("never paid"));
        assert!(flat(&lines).contains("c0ff\u{2026}1234"));
        assert!(flat(&lines).contains("replaced at a higher fee"));
        // With the replacement's record in hand, the row says what it pays.
        let mut t = t;
        t.replacement_fee = Some(300);
        assert!(flat(&detail_lines(&t, &ctx())).contains("replaced at 300 sats"));
        assert!(matches!(tail(&t, P), Tail::Link { label: "copy txid", .. }));
    }

    /// One payment, one row: the original folds into its replacement, which
    /// then carries the bump's history. The original comes back only if the
    /// replacement dropped — then it is the one that can still confirm.
    #[test]
    fn a_replaced_original_folds_into_its_live_replacement() {
        let mut orig = tx("729a", "0.00012505", "28", 100, &[OWN], &[THEM], None);
        orig.status = BitcoinTransactionStatus::Replaced;
        orig.replaced_by = Some("4c25".into());
        orig.dropped_at = Some("150".into());
        let bump = tx("4c25", "0.00012505", "300", 160, &[OWN], &[THEM], None);

        let rows = fold_replaced(vec![Tx::from(&orig, OWN), Tx::from(&bump, OWN)]);
        assert_eq!(rows.len(), 1, "two txids, one payment, one row");
        let live = &rows[0];
        assert_eq!(live.txid, "4c25");
        assert_eq!(live.replaces.as_deref(), Some("729a"));
        assert_eq!(live.original_fee, Some(28));
        let text = flat(&detail_lines(live, &ctx()));
        assert!(text.contains("300 sats"), "{text}");
        assert!(text.contains("bumped from 28 sats"), "{text}");
        assert!(text.contains("729a"), "{text}");

        // Confirmed replacement: still one row, now a success.
        let mined = tx("4c25", "0.00012505", "300", 160, &[OWN], &[THEM], Some((965_706, 200)));
        let rows = fold_replaced(vec![Tx::from(&orig, OWN), Tx::from(&mined, OWN)]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state_label(), "success");
        assert!(flat(&detail_lines(&rows[0], &ctx())).contains("bumped from 28 sats"));

        // Replacement dropped: the original is the live one again, and says
        // what it was replaced at.
        let mut dead = tx("4c25", "0.00012505", "300", 160, &[OWN], &[THEM], None);
        dead.status = BitcoinTransactionStatus::Dropped;
        let rows = fold_replaced(vec![Tx::from(&orig, OWN), Tx::from(&dead, OWN)]);
        assert_eq!(rows.len(), 2);
        let o = rows.iter().find(|t| t.txid == "729a").unwrap();
        assert_eq!(o.replacement_fee, Some(300));

        // No record of the replacement at all: the original stays, unfolded.
        let rows = fold_replaced(vec![Tx::from(&orig, OWN)]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state_label(), "replaced");
    }

    /// Only a pending OUTGOING row can bump its fee.
    #[test]
    fn the_bump_link_is_offered_exactly_to_pending_outgoing_rows() {
        let out = Tx::from(&tx("c", "0.001", "300", 100, &[OWN], &[THEM], None), OWN);
        assert!(matches!(tail(&out, P), Tail::Link { label: "modify fee", msg: Message::BtcBumpClicked(_), .. }));
        let inc = Tx::from(&tx("d", "0.001", "0", 100, &[THEM], &[], None), OWN);
        assert!(matches!(tail(&inc, P), Tail::Prose { .. }));
        let won = Tx::from(&tx("a", "0.021", "210", 100, &[OWN], &[THEM], Some((1, 2))), OWN);
        assert!(matches!(tail(&won, P), Tail::Link { label: "copy txid", .. }));
    }

    #[test]
    fn confirmations_count_from_the_live_tip() {
        assert_eq!(confirmations(962_488, Some(962_493)), Some(6));
        assert_eq!(confirmations(962_488, Some(962_488)), Some(1), "in the tip block");
        // No node frame, or a tip behind the block: answer nothing, never zero.
        assert_eq!(confirmations(962_488, None), None);
        assert_eq!(confirmations(962_488, Some(962_487)), None);
    }

    /// Pending and settled are disjoint and each is newest-first. Overlap is
    /// the exact failure the merge was built to remove.
    #[test]
    fn the_two_sets_are_disjoint_and_newest_first() {
        let mut all = vec![
            Tx::from(&tx("old", "0.1", "10", 100, &[OWN], &[THEM], Some((1, 150))), OWN),
            Tx::from(&tx("new", "0.1", "10", 200, &[OWN], &[THEM], Some((2, 250))), OWN),
            Tx::from(&tx("pend", "0.1", "10", 300, &[OWN], &[THEM], None), OWN),
        ];
        all.sort_by_key(|t| std::cmp::Reverse(t.at().unwrap_or(0)));
        let (pending, settled): (Vec<Tx>, Vec<Tx>) = all.into_iter().partition(|t| t.unresolved());

        assert_eq!(pending.len(), 1);
        assert_eq!(settled.len(), 2);
        assert_eq!(settled[0].txid, "new", "newest settled first");
        assert_eq!(settled[1].txid, "old");
    }

    #[test]
    fn addresses_elide_to_one_fixed_width() {
        // Native SegWit, Taproot and legacy must all land on the same columns,
        // so `from` and `to` read as one aligned pair.
        for a in [
            "bc1qyfxmsjaaaaaaaaaaaaaaaaaaaaaaaaaaamh06f67",
            "bc1pmzfrwwndsqmk5yh69yjr5lfgfg4ev8c0tsc06e2ahcm7cq0v6nrsgvdvgd",
            "1NbFKXag72gY1EuNEaQxBu2rstfo9JEB5n",
            "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy",
        ] {
            assert_eq!(elide_addr(a).chars().count(), 19, "{a}");
        }
        // Something already short is passed through, not padded or sliced.
        assert_eq!(elide_addr("bc1qshort"), "bc1qshort");
    }

    /// The time column is the narrowest fixed column, and pending and settled
    /// rows put a different shape in it. Neither may outgrow the pane's.
    #[test]
    fn age_and_stamp_fit_the_time_column() {
        // 66px at 9.5px mono (0.6 em advance) is 11 characters.
        const BUDGET: usize = (TIME_W / (CELL * 0.6)) as usize;
        for s in [0, 59, 3599, 3600, 86_399, 86_400, 8_640_000] {
            assert!(age(s).chars().count() <= BUDGET, "age({s}) over budget");
        }
        assert_eq!(age(540), "9m ago");
        assert_eq!(age(4320), "1.2h ago");
        assert_eq!(age(200_000), "2d ago");
        for t in [1_786_761_453u64, 1_600_000_000, 1] {
            assert!(stamp(t).chars().count() <= BUDGET, "stamp({t}) over budget");
        }
    }

    #[test]
    fn fiat_never_reads_as_free() {
        assert_eq!(fiat(0.0009), "0.01");
        assert_eq!(fiat(0.13), "0.13");
    }
}
