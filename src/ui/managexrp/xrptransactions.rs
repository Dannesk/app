
use iced::Color;
use chrono::{DateTime, Datelike, Local};
use crate::channel::{CHANNEL, TransactionData, TransactionStatus};
use crate::controller::app_state::TradeTif;
use crate::ui::components::grid::elide_addr;
use crate::ui::components::tx_panel::{Fact, Seg};
use crate::ui::managexrp::panes::ticket::tif_name;
use crate::ui::managexrp::panes::tokens::limit_text;
use crate::ui::managexrp::tokens::short_issuer;
use crate::utils::orderbook::fmt_price;
use crate::utils::theme::CompactPalette;
use crate::utils::{add_commas, format_token_amount, tokens};

const NA: &str = "\u{2014}";

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
            Kind::Payment => "Payment",
            Kind::Offer => "Trade",
            Kind::Cancel => "Cancel",
            Kind::Trustline => "Trustline",
            Kind::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Sell,
    Buy,
    Trade,
}

impl Side {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Side::Sell => "Sell",
            Side::Buy => "Buy",
            Side::Trade => "Trade",
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            Side::Sell => "Sell",
            Side::Buy => "Buy",
            Side::Trade => "Trade",
        }
    }
}

pub(crate) struct Tx {
    pub(crate) hash: String,
    pub(crate) kind: Kind,
    pub(crate) status: TransactionStatus,
    result: Option<String>,
    incoming: bool,
    amount: String,
    currency: String,
    fee_drops: Option<u64>,
    pub(crate) closed_at: Option<u64>,
    stamp_raw: String,
    sender: String,
    receiver: String,
    destination_tag: Option<u32>,
    price: Option<f64>,
    pub(crate) sequence: Option<u32>,
    flags: Option<u32>,
    pay_amount: Option<String>,
    pay_currency: Option<String>,
    filled: Option<String>,
    filled_currency: Option<String>,
    received: Option<String>,
    received_currency: Option<String>,
    remaining: Option<String>,
    remaining_currency: Option<String>,
    remaining_pay: Option<String>,
    remaining_pay_currency: Option<String>,
    fill_price: Option<f64>,
}

impl Tx {
    pub(crate) fn from(tx: &TransactionData, own: &str) -> Self {
        let positive = |s: &str| s.trim().parse::<f64>().ok().filter(|p| *p > 0.0);
        Self {
            hash: tx.tx_id.clone(),
            kind: Kind::from_wire(&tx.order_type),
            status: tx.status.clone(),
            result: tx.result.clone(),
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
            price: positive(&tx.execution_price),
            sequence: tx.sequence,
            flags: tx.flags.as_deref().and_then(|f| f.parse::<u32>().ok()),
            pay_amount: tx.pay_amount.clone(),
            pay_currency: tx.pay_currency.clone(),
            filled: tx.filled.clone(),
            filled_currency: tx.filled_currency.clone(),
            received: tx.received.clone(),
            received_currency: tx.received_currency.clone(),
            remaining: tx.remaining.clone(),
            remaining_currency: tx.remaining_currency.clone(),
            remaining_pay: tx.remaining_pay.clone(),
            remaining_pay_currency: tx.remaining_pay_currency.clone(),
            fill_price: tx.fill_price.as_deref().and_then(positive),
        }
    }

    pub(crate) fn open(&self) -> bool {
        self.kind == Kind::Offer && self.status == TransactionStatus::Pending
    }

    pub(crate) fn cancellable(&self) -> Option<u32> {
        self.open().then_some(self.sequence).flatten()
    }

    pub(crate) fn price(&self) -> Option<f64> {
        self.price
    }

    pub(crate) fn side(&self) -> Side {
        let pay = self.filled_currency.as_deref().filter(|c| !c.is_empty()).or(self.pay_currency.as_deref()).unwrap_or("");
        if pay == "XRP" {
            Side::Sell
        } else if self.currency == "XRP" || self.currency.is_empty() {
            Side::Buy
        } else {
            Side::Trade
        }
    }

    pub(crate) fn word(&self) -> &'static str {
        if self.kind == Kind::Offer { self.side().word() } else { self.kind.word() }
    }

    fn sell(&self) -> bool {
        self.side() == Side::Sell
    }

    fn base_unit(&self) -> String {
        if self.sell() {
            unit_of(self.remaining_pay_currency.as_deref().or(self.pay_currency.as_deref()).unwrap_or("XRP"))
        } else {
            unit_of(&self.currency)
        }
    }

    pub(crate) fn base(&self) -> (Option<f64>, String) {
        let value = match (self.open(), self.sell()) {
            (true, true) => num(&self.remaining_pay).or_else(|| less(&self.pay_amount, &self.filled)),
            (true, false) => num(&self.remaining).or_else(|| less(&Some(self.amount.clone()), &self.received)),
            (false, true) => num(&self.pay_amount),
            (false, false) => self.amount.parse().ok(),
        };
        (value, self.base_unit())
    }

    pub(crate) fn filled_base(&self) -> Option<f64> {
        if self.sell() { num(&self.filled) } else { num(&self.received) }
    }

    fn rest(&self) -> (Option<f64>, String) {
        if self.sell() {
            let v = num(&self.remaining).or_else(|| less(&Some(self.amount.clone()), &self.received));
            (v, unit_of(self.remaining_currency.as_deref().unwrap_or(&self.currency)))
        } else {
            let v = num(&self.remaining_pay).or_else(|| less(&self.pay_amount, &self.filled));
            (v, unit_of(self.remaining_pay_currency.as_deref().or(self.pay_currency.as_deref()).unwrap_or("XRP")))
        }
    }

    fn moved(&self) -> (Option<f64>, String) {
        if self.sell() {
            (num(&self.received), unit_of(self.received_currency.as_deref().unwrap_or(&self.currency)))
        } else {
            (num(&self.filled), unit_of(self.filled_currency.as_deref().or(self.pay_currency.as_deref()).unwrap_or("XRP")))
        }
    }

    pub(crate) fn rate_unit(&self) -> String {
        let recv = unit_of(&self.currency);
        let Some(pay) = self.filled_currency.as_deref().filter(|c| !c.is_empty()).or(self.pay_currency.as_deref()) else {
            return String::new();
        };
        let pay = unit_of(pay);
        if recv == "XRP" {
            pay
        } else if pay == "XRP" {
            recv
        } else {
            format!("{recv} per {pay}")
        }
    }

    fn tif(&self) -> TradeTif {
        let f = self.flags.unwrap_or(0);
        if f & 0x0002_0000 != 0 {
            TradeTif::Ioc
        } else if f & 0x0004_0000 != 0 {
            TradeTif::Fok
        } else {
            TradeTif::Gtc
        }
    }

    fn issuers(&self) -> Vec<&'static tokens::TokenDef> {
        let mut out: Vec<&'static tokens::TokenDef> = Vec::new();
        for code in [self.pay_currency.as_deref(), Some(self.currency.as_str())].into_iter().flatten() {
            if let Some(t) = tokens::by_code(code).filter(|t| !out.iter().any(|o| o.code == t.code)) {
                out.push(t);
            }
        }
        out
    }

    pub(crate) fn amount_parts(&self, reserve: Option<f64>) -> Option<(String, String)> {
        match self.kind {
            Kind::Payment => Some((
                format!("{}{}", if self.incoming { "+" } else { "\u{2212}" }, figure(&self.amount, &self.currency)),
                unit_of(&self.currency),
            )),
            Kind::Trustline => reserve.map(|r| (six(r), "XRP".to_string())),
            Kind::Offer => {
                let (v, unit) = self.base();
                v.map(|v| (six(v), unit))
            }
            Kind::Cancel | Kind::Other => None,
        }
    }

    pub(crate) fn state_label(&self) -> &'static str {
        match self.status {
            TransactionStatus::Success if self.kind == Kind::Offer => "filled",
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
            TransactionStatus::Pending => p.amber,
            TransactionStatus::Partial if self.kind != Kind::Offer => p.amber,
            TransactionStatus::Partial | TransactionStatus::Killed | TransactionStatus::Cancelled => p.dim,
        }
    }
}

pub(crate) fn split(own: &str) -> (Vec<Tx>, Vec<Tx>) {
    let txs = CHANNEL.transactions_rx.borrow();
    let mut all: Vec<Tx> = txs
        .transactions
        .values()
        .map(|t| Tx::from(t, own))
        .filter(|t| t.kind != Kind::Other)
        .collect();
    all.sort_by(|a, b| b.stamp_raw.cmp(&a.stamp_raw).then_with(|| a.hash.cmp(&b.hash)));
    all.into_iter().partition(Tx::open)
}

pub(crate) fn facts(tx: &Tx, reserve: Option<f64>, p: &'static CompactPalette) -> Vec<Fact> {
    let plain = |key: &'static str, segs: Vec<Seg>| Fact { key, dot: None, segs, copy: None };
    let address = |key: &'static str, a: &str| Fact {
        key,
        dot: None,
        segs: vec![Seg::mono(elide_addr(a), p.text)],
        copy: Some(a.to_string()),
    };
    let value = |v: Option<f64>, unit: &str| match v {
        Some(v) => vec![Seg::mono(six(v), p.text), Seg::mono(format!(" {unit}"), p.muted)],
        None => vec![Seg::mono(NA, p.muted)],
    };
    let rate = |r: Option<f64>| match r {
        Some(r) => {
            let unit = tx.rate_unit();
            let mut segs = vec![Seg::mono(fmt_price(r), p.text)];
            if !unit.is_empty() {
                segs.push(Seg::mono(format!(" {unit}"), p.muted));
            }
            segs
        }
        None => vec![Seg::mono(NA, p.muted)],
    };
    let issuer = |t: &tokens::TokenDef| {
        plain("Issuer", vec![Seg::word(t.issuer_name, p.text), Seg::mono(format!("  {}", short_issuer(t.issuer)), p.dim)])
    };
    let fee = || {
        plain(
            "Network fee",
            match tx.fee_drops {
                Some(d) => vec![Seg::mono(d.to_string(), p.dim), Seg::mono(" drops", p.muted)],
                None => vec![Seg::mono(NA, p.muted)],
            },
        )
    };

    let mut rows = Vec::new();
    match tx.kind {
        Kind::Payment => {
            if !tx.sender.is_empty() {
                rows.push(address("From", &tx.sender));
            }
            if !tx.receiver.is_empty() {
                rows.push(address("To", &tx.receiver));
            }
            if let Some(tag) = tx.destination_tag {
                rows.push(plain("Destination tag", vec![Seg::mono(tag.to_string(), p.text)]));
            }
            if let Some(t) = tokens::by_code(&tx.currency) {
                rows.push(issuer(t));
            }
            if tx.status == TransactionStatus::Failed
                && let Some(code) = &tx.result
            {
                rows.push(plain("Result", vec![Seg::mono(code.clone(), p.text)]));
            }
            if !tx.incoming {
                rows.push(fee());
            }
        }
        Kind::Trustline => {
            let unit = unit_of(&tx.currency);
            rows.push(plain("Token", vec![Seg::mono(unit.clone(), p.text)]));
            if let Some(t) = tokens::by_code(&tx.currency) {
                rows.push(issuer(t));
            }
            let limit = tx.amount.parse::<f64>().ok().map(limit_text);
            rows.push(plain(
                "Limit",
                match limit {
                    Some(l) => vec![Seg::mono(l, p.text), Seg::mono(format!(" {unit}"), p.muted)],
                    None => vec![Seg::mono(NA, p.muted)],
                },
            ));
            rows.push(fee());
        }
        Kind::Cancel | Kind::Other => rows.push(fee()),
        Kind::Offer => {
            let base_unit = tx.base_unit();
            rows.push(plain("Limit price", rate(tx.price)));
            rows.push(plain("Filled", value(tx.filled_base(), &base_unit)));
            if tx.open() {
                let (rest, unit) = tx.rest();
                rows.push(plain(if tx.sell() { "Receive" } else { "Pay" }, value(rest, &unit)));
            } else {
                let (moved, unit) = tx.moved();
                rows.push(plain(if tx.sell() { "Received" } else { "Paid" }, value(moved, &unit)));
                if tx.fill_price.is_some() {
                    rows.push(plain("Filled price", rate(tx.fill_price)));
                }
            }
            rows.push(plain("Time in force", vec![Seg::word(tif_name(tx.tif()), p.text)]));
            for t in tx.issuers() {
                rows.push(issuer(t));
            }
            if tx.open() {
                rows.push(plain("Reserve", value(reserve, "XRP")));
            }
            rows.push(fee());
        }
    }
    rows.push(address("Hash", &tx.hash));
    rows
}

pub(crate) fn note(tx: &Tx, reserve: Option<f64>) -> Option<String> {
    match tx.kind {
        Kind::Offer if tx.open() => {
            let r = match reserve {
                Some(r) => format!("{} XRP reserve", format_token_amount(r, 2)),
                None => "reserve".to_string(),
            };
            Some(match tx.filled_base().filter(|f| *f > 0.0) {
                Some(f) => format!(
                    "{} {} has filled and stays yours. Cancelling takes the rest off the book and returns the {r}.",
                    six(f),
                    tx.base_unit()
                ),
                None => format!("Nothing has filled yet. Cancelling takes it off the book and returns the {r}."),
            })
        }
        Kind::Offer => None,
        _ if tx.status == TransactionStatus::Failed => Some(failed_note(tx)),
        _ => None,
    }
}

fn failed_note(tx: &Tx) -> String {
    let unit = unit_of(&tx.currency);
    let why = if tx.kind != Kind::Payment {
        "The ledger rejected it.".to_string()
    } else {
        match tx.result.as_deref() {
            Some("tecPATH_DRY") => format!("Nothing was sent: no {unit} path to the recipient, usually a missing trustline."),
            Some("tecPATH_PARTIAL") => "Nothing was sent: not enough liquidity to deliver the full amount.".to_string(),
            Some("tecNO_DST") => "Nothing was sent: the recipient account doesn\u{2019}t exist yet.".to_string(),
            Some("tecNO_DST_INSUF_XRP") => {
                "Nothing was sent: the recipient account doesn\u{2019}t exist, and the amount was too small to activate it.".to_string()
            }
            Some("tecDST_TAG_NEEDED") => "Nothing was sent: the recipient requires a destination tag.".to_string(),
            Some("tecUNFUNDED_PAYMENT") => "Nothing was sent: the balance wasn\u{2019}t enough when it reached the ledger.".to_string(),
            Some("tecNO_PERMISSION") => {
                "Nothing was sent: the recipient only accepts payments from accounts it has approved.".to_string()
            }
            _ => "The ledger rejected it.".to_string(),
        }
    };
    why
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

pub(crate) fn six(v: f64) -> String {
    let s = format!("{v:.6}");
    match s.split_once('.') {
        Some((whole, frac)) => match whole.parse::<i64>() {
            Ok(w) if !whole.starts_with('-') => format!("{}.{frac}", add_commas(w)),
            Ok(w) => format!("-{}.{frac}", add_commas(w.abs())),
            Err(_) => s,
        },
        None => s,
    }
}

fn figure(amount: &str, currency: &str) -> String {
    let Ok(v) = amount.parse::<f64>() else {
        return amount.to_string();
    };
    if currency.is_empty() || currency == "XRP" { six(v) } else { format_token_amount(v, 6) }
}

fn unit_of(currency: &str) -> String {
    match currency {
        "" | "XRP" => "XRP".to_string(),
        c => tokens::by_code(c).map_or(c, |t| t.display).to_string(),
    }
}

fn num(v: &Option<String>) -> Option<f64> {
    v.as_deref().and_then(|s| s.parse().ok())
}

fn less(asked: &Option<String>, done: &Option<String>) -> Option<f64> {
    Some((num(asked)? - num(done).unwrap_or(0.0)).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::theme;

    const OWN: &str = "rOwnAccountAAAAAAAAAAAAAAAAAAAAAAA";
    const THEM: &str = "rThemAccountBBBBBBBBBBBBBBBBBBBBBB";
    const P: &CompactPalette = &theme::COMPACT_OBSIDIAN;

    fn tx(kind: &str, status: TransactionStatus, sender: &str, receiver: &str, amount: &str, ccy: &str) -> TransactionData {
        TransactionData {
            tx_id: "A3F9000000000000000000000000000000000000000000000000000000C21D".to_string(),
            status,
            result: None,
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
            remaining_pay: None,
            remaining_pay_currency: None,
            coverage: None,
            fill_price: None,
        }
    }

    fn sell(status: TransactionStatus) -> TransactionData {
        TransactionData {
            execution_price: "1.3714".to_string(),
            flags: Some("524288".to_string()),
            pay_amount: Some("1000.0000".to_string()),
            pay_currency: Some("XRP".to_string()),
            filled: Some("612.0000".to_string()),
            filled_currency: Some("XRP".to_string()),
            received: Some("839.2100".to_string()),
            received_currency: Some("RLUSD".to_string()),
            remaining: Some("532.1900".to_string()),
            remaining_currency: Some("RLUSD".to_string()),
            remaining_pay: Some("388.0000".to_string()),
            remaining_pay_currency: Some("XRP".to_string()),
            fill_price: Some("1.3713".to_string()),
            ..tx("offercreate", status, OWN, "", "1371.4000", "RLUSD")
        }
    }

    fn keys(facts: &[Fact]) -> Vec<&'static str> {
        facts.iter().map(|f| f.key).collect()
    }

    fn flat(facts: &[Fact]) -> String {
        facts.iter().map(|f| format!("{}={}", f.key, f.segs.iter().map(|s| s.s.as_str()).collect::<String>())).collect::<Vec<_>>().join("|")
    }

    #[test]
    fn a_payment_is_signed_by_its_direction_and_formatted_as_mobile_formats_it() {
        let out = Tx::from(&tx("payment", TransactionStatus::Success, OWN, THEM, "1234.5", "XRP"), OWN);
        assert_eq!(out.amount_parts(None), Some(("\u{2212}1,234.500000".to_string(), "XRP".to_string())));
        let inc = Tx::from(&tx("payment", TransactionStatus::Success, THEM, OWN, "25", "RLUSD"), OWN);
        assert_eq!(inc.amount_parts(None), Some(("+25.00".to_string(), "RLUSD".to_string())));
        assert_eq!(inc.word(), "Payment");
    }

    #[test]
    fn a_payment_names_both_parties_with_copies_and_no_fee_when_it_came_in() {
        let mut raw = tx("payment", TransactionStatus::Success, THEM, OWN, "10", "XRP");
        raw.destination_tag = Some(11747);
        let f = facts(&Tx::from(&raw, OWN), None, P);
        assert_eq!(keys(&f), ["From", "To", "Destination tag", "Hash"]);
        assert_eq!(f[0].copy.as_deref(), Some(THEM));
        assert_eq!(f[3].copy.as_deref(), Some(raw.tx_id.as_str()));
        let out = facts(&Tx::from(&tx("payment", TransactionStatus::Success, OWN, THEM, "1", "XRP"), OWN), None, P);
        assert!(flat(&out).contains("Network fee=12 drops"), "{}", flat(&out));
    }

    #[test]
    fn a_failed_payment_shows_its_code_and_what_it_means() {
        let mut raw = tx("payment", TransactionStatus::Failed, OWN, THEM, "2.5", "RLUSD");
        raw.result = Some("tecPATH_DRY".to_string());
        let t = Tx::from(&raw, OWN);
        let f = facts(&t, None, P);
        assert!(keys(&f).contains(&"Result") && keys(&f).contains(&"Issuer"), "{:?}", keys(&f));
        let n = note(&t, None).unwrap();
        assert!(n.starts_with("Nothing was sent: no RLUSD path"), "{n}");
        assert!(n.ends_with("usually a missing trustline."), "{n}");
    }

    #[test]
    fn a_trustline_row_shows_the_reserve_and_its_card_the_limit() {
        let t = Tx::from(&tx("trustset", TransactionStatus::Success, OWN, "", "1000000", "RLUSD"), OWN);
        assert_eq!(t.amount_parts(Some(0.2)), Some(("0.200000".to_string(), "XRP".to_string())));
        assert_eq!(keys(&facts(&t, Some(0.2), P)), ["Token", "Issuer", "Limit", "Network fee", "Hash"]);
        assert!(flat(&facts(&t, Some(0.2), P)).contains("Limit=1,000,000 RLUSD"));
    }

    #[test]
    fn a_cancel_has_no_amount_and_shows_its_fee() {
        let t = Tx::from(&tx("offercancel", TransactionStatus::Success, OWN, "", "0", "XRP"), OWN);
        assert_eq!(t.amount_parts(None), None);
        assert_eq!(keys(&facts(&t, None, P)), ["Network fee", "Hash"]);
        assert_eq!(t.word(), "Cancel");
    }

    #[test]
    fn an_open_sell_shows_the_xrp_left_and_mobiles_facts() {
        let t = Tx::from(&sell(TransactionStatus::Pending), OWN);
        assert!(t.open());
        assert_eq!(t.word(), "Sell");
        assert_eq!(t.amount_parts(None), Some(("388.000000".to_string(), "XRP".to_string())));
        let f = facts(&t, Some(0.2), P);
        assert_eq!(keys(&f), ["Limit price", "Filled", "Receive", "Time in force", "Issuer", "Reserve", "Network fee", "Hash"]);
        assert!(flat(&f).contains("Limit price=1.3714 RLUSD"), "{}", flat(&f));
        assert!(flat(&f).contains("Filled=612.000000 XRP"));
        assert!(flat(&f).contains("Receive=532.190000 RLUSD"));
        assert!(flat(&f).contains("Time in force=Good til Cancelled"));
        assert!(!keys(&f).contains(&"Sequence"));
        let n = note(&t, Some(0.2)).unwrap();
        assert_eq!(n, "612.000000 XRP has filled and stays yours. Cancelling takes the rest off the book and returns the 0.20 XRP reserve.");
        assert_eq!(t.cancellable(), Some(41));
    }

    #[test]
    fn an_open_order_without_a_remainder_reads_the_ask_less_what_crossed() {
        let mut raw = sell(TransactionStatus::Pending);
        raw.remaining = None;
        raw.remaining_pay = None;
        let t = Tx::from(&raw, OWN);
        assert_eq!(t.base().0, Some(388.0));
        assert_eq!(t.rest().0, Some(1371.4 - 839.21));
    }

    #[test]
    fn a_finished_order_shows_what_moved_and_has_no_cancel() {
        let t = Tx::from(&sell(TransactionStatus::Partial), OWN);
        assert!(!t.open());
        assert_eq!(t.cancellable(), None);
        assert_eq!(t.amount_parts(None), Some(("1,000.000000".to_string(), "XRP".to_string())));
        let f = facts(&t, Some(0.2), P);
        assert_eq!(keys(&f), ["Limit price", "Filled", "Received", "Filled price", "Time in force", "Issuer", "Network fee", "Hash"]);
        assert!(flat(&f).contains("Received=839.210000 RLUSD"));
        assert_eq!(note(&t, None), None);
        assert_eq!((t.state_label(), t.state_color(P)), ("partial", P.dim));
    }

    #[test]
    fn a_cancelled_order_reads_cancelled_with_its_fill_and_no_line() {
        let t = Tx::from(&sell(TransactionStatus::Cancelled), OWN);
        assert_eq!((t.state_label(), t.state_color(P)), ("cancelled", P.dim));
        assert!(flat(&facts(&t, None, P)).contains("Filled=612.000000 XRP"));
        assert_eq!(note(&t, None), None);
    }

    #[test]
    fn a_buy_reads_the_xrp_it_receives_and_the_token_it_pays() {
        let raw = TransactionData {
            execution_price: "1.4011".to_string(),
            flags: Some("0".to_string()),
            pay_amount: Some("2.8021".to_string()),
            pay_currency: Some("RLUSD".to_string()),
            filled: Some("2.8021".to_string()),
            filled_currency: Some("RLUSD".to_string()),
            received: Some("2.0000".to_string()),
            received_currency: Some("XRP".to_string()),
            fill_price: Some("1.4011".to_string()),
            ..tx("offercreate", TransactionStatus::Success, OWN, "", "2.0000", "XRP")
        };
        let t = Tx::from(&raw, OWN);
        assert_eq!(t.word(), "Buy");
        assert_eq!(t.state_label(), "filled");
        let f = flat(&facts(&t, None, P));
        assert!(f.contains("Filled=2.000000 XRP"), "{f}");
        assert!(f.contains("Paid=2.802100 RLUSD"), "{f}");
        assert!(f.contains("Limit price=1.4011 RLUSD"), "{f}");
    }

    #[test]
    fn open_orders_lead_and_unknown_kinds_are_left_out() {
        assert!(Tx::from(&sell(TransactionStatus::Pending), OWN).open());
        assert!(!Tx::from(&sell(TransactionStatus::Cancelled), OWN).open());
        assert!(!Tx::from(&tx("payment", TransactionStatus::Pending, OWN, THEM, "1", "XRP"), OWN).open());
        assert_eq!(Kind::from_wire("escrowcreate"), Kind::Other);
    }

    #[test]
    fn six_places_carry_separators_and_the_sign() {
        assert_eq!(six(1234567.5), "1,234,567.500000");
        assert_eq!(six(0.2), "0.200000");
        assert_eq!(six(-12.0), "-12.000000");
        assert_eq!(figure("25.123456789", "RLUSD"), "25.123457");
        assert_eq!(unit_of(""), "XRP");
        assert_eq!(unit_of("EUROP"), "EUR\u{d8}P");
    }
}
