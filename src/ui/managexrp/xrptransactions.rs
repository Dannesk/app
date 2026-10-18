
use iced::Color;
use chrono::{DateTime, Datelike, Local};
use crate::channel::{CHANNEL, TransactionData, TransactionStatus};
use crate::controller::message::Message;
use crate::ui::components::tx_panel::{IdLine, Line, Seg, Tail};
use crate::utils::theme::CompactPalette;

const NA: &str = "—";

const DROPS: f64 = 1_000_000.0;

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

    pub(crate) fn word(self) -> &'static str {
        match self {
            Kind::Payment => "payment",
            Kind::Offer => "trade",
            Kind::Cancel => "cancel",
            Kind::Trustline => "trustline",
            Kind::Other => "other",
        }
    }
}

pub(crate) struct Tx {
    pub(crate) hash: String,
    pub(crate) kind: Kind,
    pub(crate) status: TransactionStatus,
    incoming: bool,
    amount: String,
    currency: String,
    fee_drops: Option<u64>,
    pub(crate) closed_at: Option<u64>,
    stamp_raw: String,
    sender: String,
    receiver: String,
    destination_tag: Option<u32>,
    pub(crate) price: Option<String>,
    pub(crate) sequence: Option<u32>,
    flags: Option<String>,
    pay_amount: Option<String>,
    filled: Option<String>,
    filled_currency: Option<String>,
    received: Option<String>,
    received_currency: Option<String>,
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

    fn rate_unit(&self) -> String {
        let recv = ticker(&self.currency);
        let pay = self
            .filled_currency
            .as_deref()
            .filter(|c| !c.is_empty())
            .map(ticker);
        match (pay, recv) {
            (Some(p), "XRP") => p.to_string(),
            (Some("XRP"), r) => r.to_string(),
            (Some(p), r) => format!("{r} per {p}"),
            (None, _) => String::new(),
        }
    }

    fn pay_anchored(&self) -> bool {
        self.flags
            .as_deref()
            .and_then(|f| f.parse::<u32>().ok())
            .is_some_and(|f| f & 0x0008_0000 != 0)
    }

    #[cfg(test)]
    fn fill_phrase(&self) -> Option<String> {
        let (paid, got) = self.fill_parts()?;
        Some(format!("{paid} \u{2192} {got}"))
    }

    fn fill_parts(&self) -> Option<(String, String)> {
        let f: f64 = self.filled.as_deref()?.parse().ok()?;
        let r: f64 = self.received.as_deref()?.parse().ok()?;
        if f <= 0.0 || r <= 0.0 {
            return None;
        }
        let fc = self.filled_currency.as_deref().unwrap_or("");
        let rc = self.received_currency.as_deref().unwrap_or("");
        let filled = amount_text(self.filled.as_deref().unwrap_or(""), fc);
        let received = amount_text(self.received.as_deref().unwrap_or(""), rc);

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
        Some((paid, got))
    }

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

    pub(crate) fn unresolved(&self) -> bool {
        matches!(self.status, TransactionStatus::Pending)
    }

    pub(crate) fn cancellable(&self) -> Option<u32> {
        (self.unresolved() && self.kind == Kind::Offer).then_some(self.sequence).flatten()
    }

    #[cfg(test)]
    fn amount_str(&self) -> String {
        match self.amount_parts() {
            Some((figure, unit)) => format!("{figure} {unit}"),
            None => NA.to_string(),
        }
    }

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

    pub(crate) fn state_label(&self) -> &'static str {
        match self.status {
            TransactionStatus::Success => "success",
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
            TransactionStatus::Pending | TransactionStatus::Partial => p.amber,
            TransactionStatus::Killed | TransactionStatus::Cancelled => p.dim,
        }
    }
}

pub(crate) fn split(own: &str) -> (Vec<Tx>, Vec<Tx>) {
    let txs = CHANNEL.transactions_rx.borrow();
    let mut all: Vec<Tx> = txs.transactions.values().map(|t| Tx::from(t, own)).collect();
    all.sort_by(|a, b| b.stamp_raw.cmp(&a.stamp_raw));
    all.into_iter().partition(|t| t.unresolved())
}

pub(crate) fn amount_segs(tx: &Tx, p: &'static CompactPalette) -> Vec<Seg> {
    match tx.amount_parts() {
        Some((figure, unit)) => vec![Seg::mono(figure, p.text), Seg::mono(format!(" {unit}"), p.muted)],
        None => vec![Seg::mono(NA, p.faint)],
    }
}

pub(crate) fn id_line(tx: &Tx, copied: Option<&str>) -> IdLine {
    IdLine {
        key: "hash",
        value: elide_addr(&tx.hash),
        copy: Message::CopyTxHash(tx.hash.clone()),
        copied: copied == Some(tx.hash.as_str()),
    }
}

pub(crate) fn detail_lines(tx: &Tx, p: &'static CompactPalette) -> Vec<Line> {
    let mut rows: Vec<Line> = Vec::new();

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
            let unit = tx.rate_unit();
            let rate = |r: &str| {
                let mut segs = vec![Seg::mono(r.to_string(), p.text)];
                if !unit.is_empty() {
                    segs.push(Seg::mono(format!(" {unit}"), p.muted));
                }
                segs
            };
            rows.push((
                "limit price",
                match &tx.price {
                    Some(pr) => rate(pr),
                    None => vec![Seg::mono(NA, p.muted)],
                },
            ));
            if let Some((paid, got)) = tx.fill_parts() {
                rows.push((
                    "amount",
                    vec![
                        Seg::mono(paid, p.text),
                        Seg::mono(" \u{2192} ", p.faint),
                        Seg::mono(got, p.text),
                    ],
                ));
                if let Some(fp) = &tx.fill_price {
                    rows.push(("filled price", rate(fp)));
                }
            }
            rows.push((
                "sequence",
                match tx.sequence {
                    Some(s) => vec![Seg::mono(s.to_string(), p.text)],
                    None => vec![Seg::mono(NA, p.muted)],
                },
            ));
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
        TransactionStatus::Partial => rows.push((
            "status",
            vec![Seg::word("part of the order filled — the rest did not, and is gone", p.amber)],
        )),
        TransactionStatus::Killed => rows.push((
            "status",
            vec![Seg::word("nothing filled — the book moved; only the fee was charged", p.muted)],
        )),
        TransactionStatus::Failed => rows.push((
            "status",
            vec![Seg::word("the ledger rejected it — nothing moved", p.red)],
        )),
        _ => {}
    }

    rows
}

pub(crate) fn tail(tx: &Tx, p: &'static CompactPalette) -> Option<Tail> {
    if let Some(seq) = tx.cancellable() {
        return Some(Tail::Link { label: "cancel order", color: p.red, msg: Message::CancelOrderClicked(seq) });
    }
    if tx.unresolved() {
        return Some(Tail::Prose { s: "cancel unavailable — no sequence", color: p.faint });
    }
    None
}

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

    fn offer_with_fill(status: TransactionStatus) -> TransactionData {
        TransactionData {
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

    #[test]
    fn direction_comes_from_the_sender() {
        let out = Tx::from(&tx("payment", TransactionStatus::Success, OWN, THEM, "12.000000", "XRP"), OWN);
        assert!(!out.incoming);
        assert_eq!(out.amount_str(), "-12.000000 XRP");

        let inc = Tx::from(&tx("payment", TransactionStatus::Success, THEM, OWN, "25", "RLUSD"), OWN);
        assert!(inc.incoming);
        assert_eq!(inc.amount_str(), "+25.00 RLUSD");
    }

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

    #[test]
    fn non_moving_kinds_read_as_a_dash() {
        let cancel = Tx::from(&tx("offercancel", TransactionStatus::Success, OWN, "", "0", "XRP"), OWN);
        assert_eq!(cancel.amount_str(), NA);

        let line = Tx::from(&tx("trustset", TransactionStatus::Success, OWN, "", "1000000", "RLUSD"), OWN);
        assert_eq!(line.amount_str(), NA);
        let d = flat(&detail_lines(&line, P));
        assert!(d.contains("limit1000000.00 RLUSD"), "{d}");
        assert!(!d.contains("to"), "{d}");
        assert!(!d.contains("from"), "{d}");
    }

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

    #[test]
    fn an_offer_names_no_party_and_reads_its_flags_as_a_code() {
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

    #[test]
    fn a_partial_fill_states_what_traded_and_that_the_rest_is_gone() {
        let t = Tx::from(&offer_with_fill(TransactionStatus::Partial), OWN);
        let out = flat(&detail_lines(&t, P));
        assert!(out.contains("612.0000 of 1000.0000 XRP"), "the SHORTFALL, not just the fill: {out}");
        assert!(out.contains("839.21"), "what arrived: {out}");
        assert!(out.contains("filled price1.3713 RLUSD"), "the realized rate, with its unit: {out}");
        assert!(out.contains("the rest did not, and is gone"), "the remainder is gone: {out}");
        assert_eq!(t.state_label(), "partial");
    }

    #[test]
    fn an_offer_is_called_a_trade() {
        assert_eq!(Kind::Offer.word(), "trade");
        assert_eq!(Kind::Cancel.word(), "cancel");
    }

    #[test]
    fn a_buy_reads_in_rlusd_per_xrp_like_the_ticket() {
        let raw = TransactionData {
            execution_price: "1.4011".to_string(),
            flags: Some("131072".to_string()),
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
        assert!(out.contains("limit price1.4011 RLUSD"), "limit price in the ticket's unit: {out}");
        assert!(out.contains("amount2.8021 RLUSD \u{2192} 2.0000 XRP"), "what moved: {out}");
        assert!(out.contains("filled price1.4011 RLUSD"), "the realized rate: {out}");
        assert!(!out.contains("per XRP"), "an XRP pair is always quoted token per XRP: {out}");
        assert!(!out.contains("0.7137"), "the engine's orientation never reaches the panel: {out}");
    }

    #[test]
    fn rate_unit_covers_token_pairs_and_old_records() {
        let sell = Tx::from(&offer_with_fill(TransactionStatus::Success), OWN);
        assert_eq!(sell.rate_unit(), "RLUSD");

        let mut raw = offer_with_fill(TransactionStatus::Success);
        raw.currency = "USDC".to_string();
        raw.filled_currency = Some("RLUSD".to_string());
        let t = Tx::from(&raw, OWN);
        assert_eq!(t.rate_unit(), "USDC per RLUSD");

        let old = Tx::from(&tx("offercreate", TransactionStatus::Success, OWN, "", "1371.4", "RLUSD"), OWN);
        assert_eq!(old.rate_unit(), "");
        let old_xrp = Tx::from(&tx("offercreate", TransactionStatus::Success, OWN, "", "2", "XRP"), OWN);
        assert_eq!(old_xrp.rate_unit(), "");
    }

    #[test]
    fn the_hash_line_carries_the_copy_and_a_settled_row_has_no_tail() {
        let t = Tx::from(&offer_with_fill(TransactionStatus::Success), OWN);
        assert!(matches!(id_line(&t, None).copy, Message::CopyTxHash(ref h) if h == &t.hash));
        assert!(!id_line(&t, None).copied);
        assert!(id_line(&t, Some(t.hash.as_str())).copied);
        assert!(!id_line(&t, Some("another row")).copied, "the tick belongs to the row that was copied");
        assert!(tail(&t, P).is_none());
    }

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

    #[test]
    fn an_open_offer_shows_the_remainder_and_a_settled_one_shows_the_request() {
        let open = Tx::from(&offer_with_fill(TransactionStatus::Pending), OWN);
        assert!(open.amount_str().contains("532.19"), "{}", open.amount_str());

        let settled = Tx::from(&offer_with_fill(TransactionStatus::Partial), OWN);
        assert!(settled.amount_str().contains("1371.4"), "{}", settled.amount_str());
        assert!(!settled.unresolved(), "a partial fill is settled, not open");
    }

    #[test]
    fn neither_new_outcome_is_ever_unresolved() {
        for s in [TransactionStatus::Partial, TransactionStatus::Killed] {
            let t = Tx::from(&offer_with_fill(s), OWN);
            assert!(!t.unresolved());
            assert!(t.cancellable().is_none());
        }
    }

    #[test]
    fn a_complete_fill_states_no_shortfall() {
        let mut raw = offer_with_fill(TransactionStatus::Success);
        raw.filled = Some("1000.0000".to_string());
        raw.received = Some("1371.4000".to_string());
        let t = Tx::from(&raw, OWN);
        let phrase = t.fill_phrase().expect("it filled");
        assert!(!phrase.contains(" of "), "{phrase}");
    }

    #[test]
    fn the_shortfall_is_stated_in_the_unit_the_user_typed() {
        let pay_anchored = Tx::from(&offer_with_fill(TransactionStatus::Partial), OWN);
        let phrase = pay_anchored.fill_phrase().unwrap();
        assert!(phrase.contains("612.0000 of 1000.0000 XRP"), "{phrase}");

        let mut raw = offer_with_fill(TransactionStatus::Partial);
        raw.flags = Some("0".to_string());
        let receive_anchored = Tx::from(&raw, OWN);
        let phrase = receive_anchored.fill_phrase().unwrap();
        assert!(phrase.contains("of 1371.4000 RLUSD"), "{phrase}");
        assert!(!phrase.contains("612.0000 of"), "the pay side was not the anchor: {phrase}");
    }

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
