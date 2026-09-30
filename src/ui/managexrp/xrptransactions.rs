//! The Transactions modal — everything that has happened to this account,
//! floated over the XRP dashboard. The panel itself is
//! [`crate::ui::components::tx_panel`], shared with BTC; this file resolves the
//! channel into rows and documents only what XRP does differently.
//!
//! ## `open`, not `mempool`
//!
//! XRPL has no mempool a user can watch: a transaction is validated in ~4s or
//! it never existed. The only unresolved thing on this ledger is a **GTC offer
//! still resting on the book** — placed, partly or wholly unfilled, waiting for
//! a counterparty that may never come. That is what the `open` tab holds, and
//! it is what makes the split the same rule as BTC's: a row is in exactly one
//! set, unresolved or resolved, and it migrates without changing shape when
//! the offer fills or is cancelled.
//!
//! ## The type is the row's second column
//!
//! BTC's rows lead with the txid because every BTC transaction is the same
//! kind of thing. An XRP account does four kinds — payment, offer, cancel,
//! trustline — and the kind is the fact the eye scans for, so it takes the
//! txid's column and the hash goes to the detail block.
//!
//! ## Parties are a payment's fact
//!
//! Only a Payment has two parties. An offer, a cancel and a trustline are
//! signed by this account and go nowhere — the ledger has no `Destination` on
//! them — so a `from` line would name the same wallet every time and say
//! nothing. No `· this wallet` tag either, on any kind: the row's sign and
//! its state column already carry the direction.
//!
//! ## Cancel is the RBF slot
//!
//! An open offer's expanded block ends in `cancel order ›`, in the exact place
//! a pending BTC row draws `modify fee (RBF) — soon`. It is the one *live*
//! tail action in the app, which is how it earns the red. It arms the existing
//! cancel card (`xrpcancel`) — nothing in that flow changed.
//!
//! ## Where the facts come from
//!
//! Rows ride `CHANNEL.transactions_rx` exactly as `ws/commands/get_transaction.rs`
//! builds them: `fee` is **drops** as a string, `receiver` is the payment's
//! `Destination` (empty on every other type), `timestamp` is `close_time_iso`
//! verbatim — RFC 3339 UTC, rendered here in the reader's own clock. Direction
//! is `sender == own`, which is exact on XRPL: every transaction has one
//! `Account`. An offer's `flags` is the raw `Flags` word; the panel reads the
//! IOC/FOK bits out of it and prints the code the trade ticket's Time-in-force
//! segments use, never the number.

use iced::Color;
use chrono::{DateTime, Datelike, Local};
use crate::channel::{CHANNEL, TransactionData, TransactionStatus};
use crate::controller::message::Message;
use crate::ui::components::tx_panel::{Line, Seg, Tail};
use crate::utils::theme::CompactPalette;

/// Rendered in place of any value that cannot currently be measured.
const NA: &str = "—";

/// Drops per XRP.
const DROPS: f64 = 1_000_000.0;

// How far back this list goes is the ledger node's business, not ours. Nodes
// prune — full XRPL history is ~39TB — so an import rebuilds only as far back
// as the node still holds, and cached history accumulates live from there,
// capped at 20 rows server-side. None of that is said on the panel any more:
// the footer (`tx_panel::FOOTER`) points at the docs, where it is explained
// once and properly.

// ── Data ─────────────────────────────────────────────────────────────────────

/// What an XRP account does. `Other` is a type this app never places and the
/// relay never files (`HISTORY_TYPES` drops them at import) — the match is
/// total so a future type renders rather than panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Payment,
    Offer,
    Cancel,
    Trustline,
    Other,
}

impl Kind {
    fn from_wire(order_type: &str) -> Self {
        match order_type.to_lowercase().as_str() {
            "payment" => Kind::Payment,
            "offercreate" | "offer_create" => Kind::Offer,
            "offercancel" | "offer_cancel" => Kind::Cancel,
            "trustset" | "trust_set" => Kind::Trustline,
            _ => Kind::Other,
        }
    }

    /// The row's second column.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Kind::Payment => "payment",
            // The user's word, not the ledger's: they pressed Trade, and
            // "offer" is XRPL mechanics they were never shown.
            Kind::Offer => "trade",
            Kind::Cancel => "cancel",
            Kind::Trustline => "trustline",
            Kind::Other => "other",
        }
    }
}

/// One transaction, resolved from the channel into exactly what the card
/// draws. Built once per render so a summary row and its expanded detail
/// cannot disagree about the same transaction.
pub(crate) struct Tx {
    pub(crate) hash: String,
    pub(crate) kind: Kind,
    pub(crate) status: TransactionStatus,
    /// The account this wallet is — `sender == own`. Exact on XRPL.
    incoming: bool,
    /// The wire's amount string, kept as a string: token values are decimal
    /// strings of the issuer's precision and must not round-trip through f64.
    amount: String,
    currency: String,
    /// Fee in drops, or `None` when the wire carried nothing usable.
    fee_drops: Option<u64>,
    /// Epoch seconds of the validating ledger's close, or `None`.
    pub(crate) closed_at: Option<u64>,
    /// The `close_time_iso` string — what the list SORTS on (fixed-width
    /// Zulu ISO-8601 is lexicographically ordered).
    stamp_raw: String,
    sender: String,
    receiver: String,
    /// A payment's destination tag, when the sender set one.
    destination_tag: Option<u32>,
    /// The rate that was asked for, already formatted and ORIENTED by the
    /// relay (`outcome::rate`): token per XRP when one leg is XRP, receive per
    /// pay otherwise. [`Self::rate_words`] says which.
    pub(crate) price: Option<String>,
    pub(crate) sequence: Option<u32>,
    flags: Option<String>,
    /// Offers only — what actually traded, what arrived, and at what rate.
    /// Absent on a record written before the relay reported outcomes.
    /// The pay side of the request. Its CURRENCY is deliberately not carried:
    /// the relay sets `pay_currency` and `filled_currency` from the same value
    /// (`TakerGets`), so `filled_currency` already names this unit and a second
    /// copy could only ever drift from it.
    pay_amount: Option<String>,
    filled: Option<String>,
    filled_currency: Option<String>,
    received: Option<String>,
    received_currency: Option<String>,
    /// Still outstanding on a resting offer, in the receive asset.
    remaining: Option<String>,
    remaining_currency: Option<String>,
    fill_price: Option<String>,
}

impl Tx {
    pub(crate) fn from(tx: &TransactionData, own: &str) -> Self {
        Self {
            hash: tx.tx_id.clone(),
            kind: Kind::from_wire(&tx.order_type),
            status: tx.status.clone(),
            incoming: !tx.sender.is_empty() && tx.sender != own,
            amount: tx.amount.clone(),
            currency: tx.currency.clone(),
            fee_drops: tx.fee.parse::<u64>().ok(),
            closed_at: DateTime::parse_from_rfc3339(&tx.timestamp)
                .ok()
                .map(|dt| dt.timestamp().max(0) as u64),
            stamp_raw: tx.timestamp.clone(),
            sender: tx.sender.clone(),
            receiver: tx.receiver.clone(),
            destination_tag: tx.destination_tag,
            price: Some(tx.execution_price.trim())
                .filter(|p| !p.is_empty() && *p != "0")
                .map(str::to_string),
            sequence: tx.sequence,
            flags: tx.flags.clone().filter(|f| !f.is_empty()),
            pay_amount: tx.pay_amount.clone(),
            filled: tx.filled.clone(),
            filled_currency: tx.filled_currency.clone(),
            received: tx.received.clone(),
            received_currency: tx.received_currency.clone(),
            remaining: tx.remaining.clone(),
            remaining_currency: tx.remaining_currency.clone(),
            fill_price: tx.fill_price.clone(),
        }
    }

    /// The words after a rate — `price` and `fill_price` are bare numbers on
    /// the wire, oriented by the relay: token per XRP whenever one leg is XRP,
    /// receive per pay otherwise. Two forms: the price row names both units
    /// ("RLUSD per XRP"); the filled row, which has just printed both amounts,
    /// names only the denominator ("per XRP").
    ///
    /// The pay unit is `filled_currency` (the relay sets it from `TakerGets`
    /// and deliberately sends no second copy); the receive unit is `currency`.
    /// A record from before the outcome fields has no pay unit — if it
    /// received XRP the words still hold, otherwise the number stands alone
    /// rather than under a guessed label.
    fn rate_words(&self) -> (String, String) {
        let recv = ticker(&self.currency);
        let pay = self
            .filled_currency
            .as_deref()
            .filter(|c| !c.is_empty())
            .map(ticker);
        match (pay, recv) {
            (Some(p), "XRP") => (format!("{p} per XRP"), "per XRP".to_string()),
            (Some("XRP"), r) => (format!("{r} per XRP"), "per XRP".to_string()),
            (Some(p), r) => (format!("{r} per {p}"), format!("per {p}")),
            (None, "XRP") => ("per XRP".to_string(), "per XRP".to_string()),
            (None, _) => (String::new(), String::new()),
        }
    }

    /// Was the order anchored on the side it PAYS?
    ///
    /// `tfSell` (0x00080000) is the ledger's own stopping condition: with it the
    /// order is done when the whole `TakerGets` is spent, without it when the
    /// whole `TakerPays` is received. It is the flag and never the side that
    /// says which amount the user typed, and therefore which one a shortfall is
    /// measured against.
    fn pay_anchored(&self) -> bool {
        self.flags
            .as_deref()
            .and_then(|f| f.parse::<u32>().ok())
            .is_some_and(|f| f & 0x0008_0000 != 0)
    }

    /// What actually traded, as one phrase, when the wire carried it and it
    /// is not zero. `None` on an order that has not moved anything and on any
    /// record predating the outcome fields.
    ///
    /// **It states what was ASKED FOR too.** "612 XRP → 839.21 RLUSD" reads as
    /// a completed trade; only "612 of 1,000" says an order fell short. The
    /// activity log says it once, at the moment of the fill, and then it is
    /// gone — this row is the only lasting place a partial fill is legible, and
    /// without the "of" it was not legible here either.
    #[cfg(test)]
    fn fill_phrase(&self) -> Option<String> {
        let (paid, got, at) = self.fill_parts()?;
        let mut phrase = format!("{paid} \u{2192} {got}");
        if let Some(at) = at {
            phrase.push_str(&format!(" at {at}"));
        }
        Some(phrase)
    }

    /// [`fill_phrase`](Self::fill_phrase) in its three runs — what was paid,
    /// what arrived, the rate — so the panel can ink the arrow and the rate
    /// tail differently from the figures.
    fn fill_parts(&self) -> Option<(String, String, Option<String>)> {
        let f: f64 = self.filled.as_deref()?.parse().ok()?;
        let r: f64 = self.received.as_deref()?.parse().ok()?;
        if f <= 0.0 || r <= 0.0 {
            return None;
        }
        let fc = self.filled_currency.as_deref().unwrap_or("");
        let rc = self.received_currency.as_deref().unwrap_or("");
        let filled = amount_text(self.filled.as_deref().unwrap_or(""), fc);
        let received = amount_text(self.received.as_deref().unwrap_or(""), rc);

        // The "of" goes on the anchored side, in the unit the user typed.
        let (asked, asked_ccy) = if self.pay_anchored() {
            (self.pay_amount.as_deref(), fc)
        } else {
            (Some(self.amount.as_str()), rc)
        };
        let short = asked
            .and_then(|a| a.parse::<f64>().ok())
            .is_some_and(|a| a > 0.0 && (if self.pay_anchored() { f } else { r }) < a - 1e-6);

        let (paid, got) = if short {
            let a = amount_text(asked.unwrap_or(""), asked_ccy);
            if self.pay_anchored() {
                (format!("{} of {} {}", filled, a, ticker(fc)), format!("{} {}", received, ticker(rc)))
            } else {
                (format!("{} {}", filled, ticker(fc)), format!("{} of {} {}", received, a, ticker(rc)))
            }
        } else {
            (format!("{} {}", filled, ticker(fc)), format!("{} {}", received, ticker(rc)))
        };
        let at = self.fill_price.as_ref().map(|r| {
            let (_, per) = self.rate_words();
            if per.is_empty() { r.clone() } else { format!("{r} {per}") }
        });
        Some((paid, got, at))
    }

    /// The code the trade ticket's Time-in-force segments use — `IOC`, `FOK`,
    /// else `GTC` — read out of the offer's flag bits. `None` when the relay
    /// carried no flags, so the line is absent rather than guessed.
    fn time_in_force(&self) -> Option<&'static str> {
        let f = self.flags.as_deref()?.parse::<u32>().ok()?;
        Some(if f & 0x0002_0000 != 0 {
            "IOC"
        } else if f & 0x0004_0000 != 0 {
            "FOK"
        } else {
            "GTC"
        })
    }

    /// The orders pane's type word (handoff, 2026-09-09): in a list that is
    /// nothing but offers, `trade` on every row says nothing, so the row
    /// carries its side against XRP instead — `sell` pays XRP, `buy`
    /// receives it. A token/token offer has no XRP side and keeps `trade`.
    /// The pay unit is `filled_currency` (set from `TakerGets`), the receive
    /// unit `currency`.
    pub(crate) fn side_word(&self) -> &'static str {
        let pays_xrp = self.filled_currency.as_deref().is_some_and(|c| c == "XRP");
        let gets_xrp = self.currency == "XRP" || self.currency.is_empty();
        if pays_xrp {
            "sell"
        } else if gets_xrp {
            "buy"
        } else {
            "trade"
        }
    }

    /// Still resting on the book. The ONLY thing the open tab holds.
    pub(crate) fn unresolved(&self) -> bool {
        matches!(self.status, TransactionStatus::Pending)
    }

    /// An open offer this wallet can cancel: pending, an OfferCreate, and the
    /// relay knew its sequence.
    pub(crate) fn cancellable(&self) -> Option<u32> {
        (self.unresolved() && self.kind == Kind::Offer).then_some(self.sequence).flatten()
    }

    /// The flexing column.
    ///
    /// A payment is signed — the sign is the direction and it is the first
    /// thing read. An offer shows what it would bring in (`TakerPays`),
    /// unsigned: nothing has moved yet, or it moved at fill time in a way one
    /// number cannot state. A cancel or trustline moves nothing and reads `—`.
    #[cfg(test)]
    fn amount_str(&self) -> String {
        match self.amount_parts() {
            Some((figure, unit)) => format!("{figure} {unit}"),
            None => NA.to_string(),
        }
    }

    /// [`amount_str`](Self::amount_str) as figure and unit, so the panel can
    /// ink the unit `muted`. `None` for the kinds that move nothing.
    pub(crate) fn amount_parts(&self) -> Option<(String, &str)> {
        match self.kind {
            Kind::Payment => Some((
                format!(
                    "{}{}",
                    if self.incoming { "+" } else { "-" },
                    amount_text(&self.amount, &self.currency),
                ),
                ticker(&self.currency),
            )),
            // A resting offer shows what is STILL OUTSTANDING on it, which is
            // what the cancel button beside it would take back. A settled one
            // shows what was asked for; what actually traded is on the detail
            // lines, because one number cannot carry both.
            Kind::Offer => {
                let (amt, ccy) = match (&self.remaining, &self.remaining_currency) {
                    (Some(a), Some(c)) if self.unresolved() => (a.as_str(), c.as_str()),
                    _ => (self.amount.as_str(), self.currency.as_str()),
                };
                Some((amount_text(amt, ccy), ticker(ccy)))
            }
            Kind::Cancel | Kind::Trustline | Kind::Other => None,
        }
    }

    /// The fourth column: the OUTCOME for a settled row, `open` for one still
    /// on the book. `failed` is honest on XRPL — a validated `tec` result is
    /// the ledger itself declining, and it charged the fee for saying so.
    pub(crate) fn state_label(&self) -> &'static str {
        match self.status {
            TransactionStatus::Success => "success",
            // Both of these used to arrive here as "success": an
            // immediate-or-cancel order returns tesSUCCESS whether it filled
            // all of the size, some of it, or none.
            TransactionStatus::Partial => "partial",
            TransactionStatus::Killed => "killed",
            TransactionStatus::Failed => "failed",
            TransactionStatus::Pending => "open",
            TransactionStatus::Cancelled => "cancelled",
        }
    }

    pub(crate) fn state_color(&self, p: &'static CompactPalette) -> Color {
        match self.status {
            TransactionStatus::Success => p.green,
            TransactionStatus::Failed => p.red,
            // Amber: a live order is the dashboard's "money is moving" colour,
            // not a success and not a fault. A partial fill earns the same
            // colour for the same reason — something happened and something
            // did not, and neither green nor red says that.
            TransactionStatus::Pending | TransactionStatus::Partial => p.amber,
            // Nothing moved but the fee. Not a fault of the ledger's, so not
            // red; the same dim reading a cancelled order gets.
            TransactionStatus::Killed | TransactionStatus::Cancelled => p.dim,
        }
    }
}

/// Both sets, each newest-first. A transaction is in exactly one of them.
pub(crate) fn split(own: &str) -> (Vec<Tx>, Vec<Tx>) {
    let txs = CHANNEL.transactions_rx.borrow();
    let mut all: Vec<Tx> = txs.transactions.values().map(|t| Tx::from(t, own)).collect();
    // `close_time_iso` is fixed-width Zulu, so the string order IS the time
    // order; an absent stamp sorts last.
    all.sort_by(|a, b| b.stamp_raw.cmp(&a.stamp_raw));
    all.into_iter().partition(|t| t.unresolved())
}

// ── Rows ─────────────────────────────────────────────────────────────────────

/// The flexing column as runs: the figure in `text`, the unit `muted`. A
/// `—` for the kinds that move nothing goes `faint`.
pub(crate) fn amount_segs(tx: &Tx, p: &'static CompactPalette) -> Vec<Seg> {
    match tx.amount_parts() {
        Some((figure, unit)) => vec![Seg::mono(figure, p.text), Seg::mono(format!(" {unit}"), p.muted)],
        None => vec![Seg::mono(NA, p.faint)],
    }
}

/// The detail lines under an expanded row. Every kind shares `hash` and `fee`
/// at the top; the middle is the kind's own facts; a payment closes with its
/// two parties.
pub(crate) fn detail_lines(tx: &Tx, p: &'static CompactPalette) -> Vec<Line> {
    let mut rows: Vec<Line> = Vec::new();

    rows.push(("hash", vec![Seg::mono(elide_addr(&tx.hash), p.text)]));

    // Drops are what the sender signed for, so they lead; the XRP figure is
    // the same number in the unit the hero speaks. No fiat: every XRP fee
    // rounds to the same sub-cent, which would say nothing.
    rows.push((
        "fee",
        if tx.kind == Kind::Payment && tx.incoming {
            vec![Seg::word("incoming — the sender paid it", p.muted)]
        } else {
            match tx.fee_drops {
                Some(d) => vec![
                    Seg::mono(format!("{d} drops"), p.amber),
                    Seg::mono(" \u{b7} ", p.faint),
                    Seg::mono(format!("{:.6} XRP", d as f64 / DROPS), p.muted),
                ],
                None => vec![Seg::mono(NA, p.muted)],
            }
        },
    ));

    match tx.kind {
        Kind::Offer => {
            rows.push((
                "price",
                match &tx.price {
                    Some(pr) => {
                        let (words, _) = tx.rate_words();
                        let mut segs = vec![Seg::mono(pr.clone(), p.text)];
                        if !words.is_empty() {
                            segs.push(Seg::mono(format!(" {words}"), p.muted));
                        }
                        segs
                    }
                    None => vec![Seg::mono(NA, p.muted)],
                },
            ));
            // What actually traded, whenever anything did. On a partial this
            // is the whole point of the row: the order asked for one amount
            // and the ledger delivered another, and until this line existed
            // the difference was nowhere on screen.
            if let Some((paid, got, at)) = tx.fill_parts() {
                let mut segs = vec![
                    Seg::mono(paid, p.text),
                    Seg::mono(" \u{2192} ", p.faint),
                    Seg::mono(got, p.text),
                ];
                if let Some(at) = at {
                    segs.push(Seg::mono(format!(" at {at}"), p.muted));
                }
                rows.push(("filled", segs));
            }
            rows.push((
                "sequence",
                match tx.sequence {
                    Some(s) => vec![Seg::mono(s.to_string(), p.text)],
                    None => vec![Seg::mono(NA, p.muted)],
                },
            ));
            // The code the trade ticket's Time-in-force segments use, read out
            // of the flag bits — never the raw `Flags` word, which is ledger
            // mechanics nobody trades in.
            if let Some(tif) = tx.time_in_force() {
                rows.push(("type", vec![Seg::mono(tif, p.text)]));
            }
        }
        Kind::Trustline => {
            rows.push((
                "limit",
                vec![
                    Seg::mono(amount_text(&tx.amount, &tx.currency), p.text),
                    Seg::mono(format!(" {}", ticker(&tx.currency)), p.muted),
                ],
            ));
        }
        Kind::Payment | Kind::Cancel | Kind::Other => {}
    }

    // Only a payment has two parties; every other kind is signed by this
    // account and goes nowhere, so a `from` on it would restate the wallet.
    // The tag rides under `to`: it is part of the destination, and it is
    // the one thing that tells a tagged deposit from the next. Untagged
    // payments — most of them — draw no row rather than a `none`.
    if tx.kind == Kind::Payment {
        for (name, addr) in [("from", &tx.sender), ("to", &tx.receiver)] {
            if addr.is_empty() {
                continue;
            }
            rows.push((name, vec![Seg::mono(elide_addr(addr), p.text)]));
        }
        if let Some(tag) = tx.destination_tag {
            rows.push(("tag", vec![Seg::mono(tag.to_string(), p.text)]));
        }
    }

    match tx.status {
        // The remainder is not resting anywhere and is not coming back. Said
        // plainly, because an IOC's short fill looks exactly like a completed
        // trade everywhere else on this row.
        TransactionStatus::Partial => rows.push((
            "status",
            vec![Seg::word("part of the order filled — the rest did not, and is gone", p.amber)],
        )),
        TransactionStatus::Killed => rows.push((
            "status",
            vec![Seg::word("nothing filled — the book moved; only the fee was charged", p.muted)],
        )),
        // A validated failure IS a ledger decision: the fee was charged and
        // nothing else moved. Stated without a remedy — the tec code that
        // says why is not on the wire.
        TransactionStatus::Failed => rows.push((
            "status",
            vec![Seg::word("the ledger rejected it — nothing moved", p.red)],
        )),
        _ => {}
    }

    rows
}

/// The line that closes an expanded transaction. Settled rows offer the hash;
/// an open offer offers its cancel — the slot BTC reserves for the fee bump,
/// and the one place on either panel a tail line is live and red.
pub(crate) fn tail(tx: &Tx, p: &'static CompactPalette) -> Tail {
    if let Some(seq) = tx.cancellable() {
        return Tail::Link { label: "cancel order", color: p.red, msg: Message::CancelOrderClicked(seq) };
    }
    if tx.unresolved() {
        // An open offer the relay never gave a sequence for — nothing to
        // cancel by. Faint and without the `›`, exactly as BTC draws a
        // control that cannot act.
        return Tail::Prose { s: "cancel unavailable — no sequence", color: p.faint };
    }
    Tail::Link { label: "copy hash", color: p.dim, msg: Message::CopyTxHash(tx.hash.clone()) }
}

// ── Formatters ───────────────────────────────────────────────────────────────

/// `rN7n7otQDd…8Wd8Kbm3` — 10 and 8, the BTC panel's cut, so `from` and `to`
/// land on the same width. A 64-hex hash takes the same cut.
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

/// `08-15 09:44` in LOCAL time, and `2025-11-02` for a different year.
pub(crate) fn stamp(secs: u64) -> String {
    let Some(dt) = DateTime::from_timestamp(secs as i64, 0) else {
        return NA.to_string();
    };
    let dt = dt.with_timezone(&Local);
    if dt.year() == Local::now().year() {
        dt.format("%m-%d %H:%M").to_string()
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}

/// The wire's amount as the row prints it. XRP arrives at six places from the
/// relay and is left alone; a token value is padded to at least two so a
/// round `25` reads as money.
fn amount_text(amount: &str, currency: &str) -> String {
    if currency == "XRP" || currency.is_empty() {
        return amount.to_string();
    }
    match amount.split_once('.') {
        None => format!("{amount}.00"),
        Some((whole, frac)) if frac.len() < 2 => format!("{whole}.{frac:0<2}"),
        _ => amount.to_string(),
    }
}

/// The ticker as the app spells it. Empty on the wire means XRP.
fn ticker(currency: &str) -> &str {
    match currency {
        "" => "XRP",
        "EUROP" => "EURØP",
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::theme;

    const OWN: &str = "rOwnAccountAAAAAAAAAAAAAAAAAAAAAAA";
    const THEM: &str = "rThemAccountBBBBBBBBBBBBBBBBBBBBBB";

    fn tx(kind: &str, status: TransactionStatus, sender: &str, receiver: &str, amount: &str, ccy: &str) -> TransactionData {
        TransactionData {
            tx_id: "A3F9000000000000000000000000000000000000000000000000000000C21D".to_string(),
            status,
            execution_price: "0".to_string(),
            order_type: kind.to_string(),
            timestamp: "2026-07-30T09:32:02Z".to_string(),
            amount: amount.to_string(),
            currency: ccy.to_string(),
            fee: "12".to_string(),
            flags: None,
            receiver: receiver.to_string(),
            sender: sender.to_string(),
            sequence: Some(41),
            destination_tag: None,
            pay_amount: None,
            pay_currency: None,
            filled: None,
            filled_currency: None,
            received: None,
            received_currency: None,
            remaining: None,
            remaining_currency: None,
            coverage: None,
            fill_price: None,
        }
    }

    /// An offer with an outcome on it: what was asked, what actually traded,
    /// and what is left. `remaining` is what a still-open offer is carrying.
    fn offer_with_fill(status: TransactionStatus) -> TransactionData {
        TransactionData {
            // tfSell — the pay side is what the user typed.
            flags: Some("524288".to_string()),
            pay_amount: Some("1000.0000".to_string()),
            pay_currency: Some("XRP".to_string()),
            filled: Some("612.0000".to_string()),
            filled_currency: Some("XRP".to_string()),
            received: Some("839.2100".to_string()),
            received_currency: Some("RLUSD".to_string()),
            remaining: Some("532.1900".to_string()),
            remaining_currency: Some("RLUSD".to_string()),
            coverage: Some(0.612),
            fill_price: Some("1.3713".to_string()),
            ..tx("offercreate", status, OWN, "", "1371.4000", "RLUSD")
        }
    }

    const P: &CompactPalette = &theme::COMPACT_OBSIDIAN;

    fn flat(lines: &[Line]) -> String {
        lines
            .iter()
            .flat_map(|(l, runs)| std::iter::once(l.to_string()).chain(runs.iter().map(|seg| seg.s.clone())))
            .collect()
    }

    /// Direction is the sender: ours is outgoing and negative, theirs is
    /// incoming and positive. Exact on XRPL — one `Account` per transaction.
    #[test]
    fn direction_comes_from_the_sender() {
        let out = Tx::from(&tx("payment", TransactionStatus::Success, OWN, THEM, "12.000000", "XRP"), OWN);
        assert!(!out.incoming);
        assert_eq!(out.amount_str(), "-12.000000 XRP");

        let inc = Tx::from(&tx("payment", TransactionStatus::Success, THEM, OWN, "25", "RLUSD"), OWN);
        assert!(inc.incoming);
        assert_eq!(inc.amount_str(), "+25.00 RLUSD");
    }

    /// Only a resting offer is open; everything else — including a cancelled
    /// offer and a failed payment — is settled.
    #[test]
    fn only_a_resting_offer_is_unresolved() {
        for (kind, status, want) in [
            ("offercreate", TransactionStatus::Pending, true),
            ("offercreate", TransactionStatus::Success, false),
            ("offercreate", TransactionStatus::Cancelled, false),
            ("payment", TransactionStatus::Failed, false),
            ("trustset", TransactionStatus::Success, false),
        ] {
            let t = Tx::from(&tx(kind, status, OWN, "", "1", "XRP"), OWN);
            assert_eq!(t.unresolved(), want, "{kind}");
        }
    }

    /// The cancel is offered exactly once: an open OfferCreate whose sequence
    /// the relay knew. A pending row without one draws the inert line.
    #[test]
    fn cancel_needs_an_open_offer_with_a_sequence() {
        let open = Tx::from(&tx("offercreate", TransactionStatus::Pending, OWN, "", "1", "XRP"), OWN);
        assert_eq!(open.cancellable(), Some(41));

        let mut no_seq = tx("offercreate", TransactionStatus::Pending, OWN, "", "1", "XRP");
        no_seq.sequence = None;
        assert_eq!(Tx::from(&no_seq, OWN).cancellable(), None);

        let done = Tx::from(&tx("offercreate", TransactionStatus::Success, OWN, "", "1", "XRP"), OWN);
        assert_eq!(done.cancellable(), None);
    }

    /// Cancels and trustlines move nothing and say so; a trustline's limit is
    /// a detail line, not an amount.
    #[test]
    fn non_moving_kinds_read_as_a_dash() {
        let cancel = Tx::from(&tx("offercancel", TransactionStatus::Success, OWN, "", "0", "XRP"), OWN);
        assert_eq!(cancel.amount_str(), NA);

        let line = Tx::from(&tx("trustset", TransactionStatus::Success, OWN, "", "1000000", "RLUSD"), OWN);
        assert_eq!(line.amount_str(), NA);
        let d = flat(&detail_lines(&line, P));
        assert!(d.contains("limit1000000.00 RLUSD"), "{d}");
        // No parties at all: a trustline is signed by this account and has
        // no Destination, so neither line exists rather than reading a dash.
        assert!(!d.contains("to"), "{d}");
        assert!(!d.contains("from"), "{d}");
    }

    /// A tagged payment names its tag under `to`; an untagged one has no
    /// `tag` row at all — and no other kind ever does.
    #[test]
    fn a_payment_shows_its_destination_tag_only_when_it_has_one() {
        let mut data = tx("payment", TransactionStatus::Success, THEM, OWN, "10.000000", "XRP");
        data.destination_tag = Some(11747);
        let tagged = Tx::from(&data, OWN);
        let lines = flat(&detail_lines(&tagged, P));
        assert!(lines.contains("to"), "{lines}");
        assert!(lines.contains("tag11747"), "{lines}");

        data.destination_tag = None;
        let plain = Tx::from(&data, OWN);
        assert!(!flat(&detail_lines(&plain, P)).contains("tag"));
    }

    /// The fee is the sender's; an incoming payment says so instead of
    /// billing the recipient for it. Both parties are named, and neither
    /// wears a `this wallet` tag — the row's sign already says which is ours.
    #[test]
    fn an_incoming_payment_does_not_bill_the_fee() {
        let inc = Tx::from(&tx("payment", TransactionStatus::Success, THEM, OWN, "1", "XRP"), OWN);
        let d = flat(&detail_lines(&inc, P));
        assert!(d.contains("incoming — the sender paid it"), "{d}");
        assert!(d.contains("from") && d.contains("to"), "{d}");
        assert!(!d.contains("wallet"), "{d}");

        let out = Tx::from(&tx("payment", TransactionStatus::Success, OWN, THEM, "1", "XRP"), OWN);
        let d = flat(&detail_lines(&out, P));
        assert!(d.contains("12 drops"), "{d}");
        assert!(d.contains("0.000012 XRP"), "{d}");
    }

    /// An offer is signed by this account and goes nowhere, so it has no
    /// parties to name — and its flags read as the ticket's time-in-force
    /// code, never as the raw `Flags` word.
    #[test]
    fn an_offer_names_no_party_and_reads_its_flags_as_a_code() {
        // tfSell alone — nothing about time in force set, so GTC.
        let gtc = Tx::from(&offer_with_fill(TransactionStatus::Success), OWN);
        let d = flat(&detail_lines(&gtc, P));
        assert!(!d.contains("from"), "{d}");
        assert!(!d.contains("flags"), "{d}");
        assert!(!d.contains("524288"), "{d}");
        assert!(d.contains("typeGTC"), "{d}");

        for (flags, want) in [("131072", "IOC"), ("262144", "FOK"), ("786432", "FOK"), ("655360", "IOC"), ("0", "GTC")] {
            let mut raw = offer_with_fill(TransactionStatus::Success);
            raw.flags = Some(flags.to_string());
            assert_eq!(Tx::from(&raw, OWN).time_in_force(), Some(want), "{flags}");
        }
        let mut raw = offer_with_fill(TransactionStatus::Success);
        raw.flags = None;
        assert_eq!(Tx::from(&raw, OWN).time_in_force(), None);
        assert!(!flat(&detail_lines(&Tx::from(&raw, OWN), P)).contains("type"));
    }

    #[test]
    fn the_close_time_parses_and_an_absent_one_reads_as_a_dash() {
        let t = Tx::from(&tx("payment", TransactionStatus::Success, OWN, THEM, "1", "XRP"), OWN);
        assert_eq!(t.closed_at, Some(1_785_403_922));

        let mut none = tx("payment", TransactionStatus::Success, OWN, THEM, "1", "XRP");
        none.timestamp.clear();
        assert_eq!(Tx::from(&none, OWN).closed_at, None);
    }

    #[test]
    fn amounts_pad_tokens_but_leave_xrp_alone() {
        assert_eq!(amount_text("25", "RLUSD"), "25.00");
        assert_eq!(amount_text("25.5", "RLUSD"), "25.50");
        assert_eq!(amount_text("25.123456", "RLUSD"), "25.123456");
        assert_eq!(amount_text("12.000000", "XRP"), "12.000000");
        assert_eq!(ticker(""), "XRP");
        assert_eq!(ticker("EUROP"), "EURØP");
    }

    /// A partial fill's detail lines have to carry BOTH numbers. Stating only
    /// what filled reads as a completed trade; stating only what was asked for
    /// hides that anything traded at all. Neither is what happened.
    #[test]
    fn a_partial_fill_states_what_traded_and_that_the_rest_is_gone() {
        let t = Tx::from(&offer_with_fill(TransactionStatus::Partial), OWN);
        let out = flat(&detail_lines(&t, P));
        assert!(out.contains("612.0000 of 1000.0000 XRP"), "the SHORTFALL, not just the fill: {out}");
        assert!(out.contains("839.21"), "what arrived: {out}");
        assert!(out.contains("at 1.3713 per XRP"), "the realized rate, with its unit: {out}");
        assert!(out.contains("the rest did not, and is gone"), "the remainder is gone: {out}");
        assert_eq!(t.state_label(), "partial");
    }

    /// The row is a "trade" to the user — "offer" is the ledger's word for it
    /// and was never on the ticket they pressed.
    #[test]
    fn an_offer_is_called_a_trade() {
        assert_eq!(Kind::Offer.word(), "trade");
        assert_eq!(Kind::Cancel.word(), "cancel");
    }

    /// A BUY reads in the ticket's unit. This is the live 2026-09-05 trade:
    /// 2 XRP bought for 2.8021 RLUSD on the AMM. The relay used to send
    /// receive-per-pay here — 0.7137 — while the ticket had just said 1.40, and
    /// a sell of the same market said 1.40 on this very row.
    #[test]
    fn a_buy_reads_in_rlusd_per_xrp_like_the_ticket() {
        let raw = TransactionData {
            execution_price: "1.4011".to_string(),
            flags: Some("131072".to_string()), // IOC, receive-anchored
            pay_amount: Some("2.8021".to_string()),
            pay_currency: Some("RLUSD".to_string()),
            filled: Some("2.8021".to_string()),
            filled_currency: Some("RLUSD".to_string()),
            received: Some("2.0000".to_string()),
            received_currency: Some("XRP".to_string()),
            coverage: Some(1.0),
            fill_price: Some("1.4011".to_string()),
            ..tx("offercreate", TransactionStatus::Success, OWN, "", "2.0000", "XRP")
        };
        let t = Tx::from(&raw, OWN);
        let out = flat(&detail_lines(&t, P));
        assert!(out.contains("price1.4011 RLUSD per XRP"), "price row with its unit: {out}");
        assert!(
            out.contains("2.8021 RLUSD \u{2192} 2.0000 XRP at 1.4011 per XRP"),
            "filled row names the denominator only: {out}"
        );
        assert!(!out.contains("0.7137"), "the engine's orientation never reaches the panel: {out}");
    }

    /// Without an XRP leg the words carry both tickers; without a pay unit at
    /// all (a record from before the outcome fields) the number stands alone.
    #[test]
    fn rate_words_cover_token_pairs_and_old_records() {
        let mut raw = offer_with_fill(TransactionStatus::Success);
        raw.currency = "USDC".to_string();
        raw.filled_currency = Some("RLUSD".to_string());
        let t = Tx::from(&raw, OWN);
        assert_eq!(t.rate_words(), ("USDC per RLUSD".to_string(), "per RLUSD".to_string()));

        let old = Tx::from(&tx("offercreate", TransactionStatus::Success, OWN, "", "1371.4", "RLUSD"), OWN);
        assert_eq!(old.rate_words(), (String::new(), String::new()));
        let old_xrp = Tx::from(&tx("offercreate", TransactionStatus::Success, OWN, "", "2", "XRP"), OWN);
        assert_eq!(old_xrp.rate_words().0, "per XRP");
    }

    /// An order that filled nothing must not read as a completed trade, and
    /// must not read as a ledger failure either — the transaction applied.
    #[test]
    fn a_killed_order_says_nothing_filled_and_is_not_a_failure() {
        let mut raw = offer_with_fill(TransactionStatus::Killed);
        raw.filled = Some("0.0000".to_string());
        raw.received = Some("0.0000".to_string());
        raw.fill_price = None;
        let t = Tx::from(&raw, OWN);
        let out = flat(&detail_lines(&t, P));
        assert_eq!(t.state_label(), "killed");
        assert!(out.contains("nothing filled"), "{out}");
        assert!(!out.contains("rejected"), "the ledger applied it: {out}");
        assert!(t.fill_phrase().is_none(), "a zero fill has no fill phrase");
    }

    /// The amount column on an OPEN offer is what is still outstanding — the
    /// figure the cancel button beside it would take back. On a settled one it
    /// is what was asked for, and what traded is on the detail lines. One
    /// field carrying both meanings is the defect this split removed.
    #[test]
    fn an_open_offer_shows_the_remainder_and_a_settled_one_shows_the_request() {
        let open = Tx::from(&offer_with_fill(TransactionStatus::Pending), OWN);
        assert!(open.amount_str().contains("532.19"), "{}", open.amount_str());

        let settled = Tx::from(&offer_with_fill(TransactionStatus::Partial), OWN);
        assert!(settled.amount_str().contains("1371.4"), "{}", settled.amount_str());
        assert!(!settled.unresolved(), "a partial fill is settled, not open");
    }

    /// Partial and killed are SETTLED. Letting either into the open tab would
    /// put a row users can press "cancel order" on in front of an order that
    /// no longer exists.
    #[test]
    fn neither_new_outcome_is_ever_unresolved() {
        for s in [TransactionStatus::Partial, TransactionStatus::Killed] {
            let t = Tx::from(&offer_with_fill(s), OWN);
            assert!(!t.unresolved());
            assert!(t.cancellable().is_none());
        }
    }

    /// A complete fill must NOT say "of" — "1,000 of 1,000" reads as a
    /// qualification on an order that had none.
    #[test]
    fn a_complete_fill_states_no_shortfall() {
        let mut raw = offer_with_fill(TransactionStatus::Success);
        raw.filled = Some("1000.0000".to_string());
        raw.received = Some("1371.4000".to_string());
        let t = Tx::from(&raw, OWN);
        let phrase = t.fill_phrase().expect("it filled");
        assert!(!phrase.contains(" of "), "{phrase}");
    }

    /// The "of" follows the ANCHOR, not the side. The same fill under a
    /// receive-anchored order states its shortfall in the receive unit,
    /// because that is the number the user typed.
    #[test]
    fn the_shortfall_is_stated_in_the_unit_the_user_typed() {
        let pay_anchored = Tx::from(&offer_with_fill(TransactionStatus::Partial), OWN);
        let phrase = pay_anchored.fill_phrase().unwrap();
        assert!(phrase.contains("612.0000 of 1000.0000 XRP"), "{phrase}");

        let mut raw = offer_with_fill(TransactionStatus::Partial);
        raw.flags = Some("0".to_string()); // no tfSell — receive anchored
        let receive_anchored = Tx::from(&raw, OWN);
        let phrase = receive_anchored.fill_phrase().unwrap();
        assert!(phrase.contains("of 1371.4000 RLUSD"), "{phrase}");
        assert!(!phrase.contains("612.0000 of"), "the pay side was not the anchor: {phrase}");
    }

    /// A record whose status was INFERRED rather than measured arrives with the
    /// numeric fields cleared (the relay's import backfill does this), and must
    /// not render a fill phrase built out of nothing.
    #[test]
    fn an_inferred_row_shows_no_fill_phrase() {
        let mut raw = offer_with_fill(TransactionStatus::Success);
        raw.filled = None;
        raw.received = None;
        raw.fill_price = None;
        raw.coverage = None;
        assert!(Tx::from(&raw, OWN).fill_phrase().is_none());
    }
}
