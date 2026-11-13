use std::collections::HashSet;

use chrono::{Datelike, Local, TimeZone};
use iced::Color;

use crate::channel::btc::{BitcoinTransactionStatus, BtcTransactionData};
use crate::channel::CHANNEL;
use crate::ui::components::tx_panel::{Fact, Seg};
use crate::ui::managebtc::node::{rate_text, Verdict};
use crate::utils::add_commas;
use crate::utils::theme::CompactPalette;

const NA: &str = "\u{2014}";

pub(crate) struct Tx {
    pub(crate) txid: String,
    sats: u64,
    fee_sats: Option<u64>,
    vsize: Option<u64>,
    pub(crate) incoming: bool,
    pub(crate) confirmed: bool,
    evicted: bool,
    replaced: bool,
    replaced_by: Option<String>,
    first_seen: Option<u64>,
    confirmed_at: Option<u64>,
    dropped_at: Option<u64>,
    block_height: Option<u32>,
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
}

fn ours_first(list: &[String], ours: &[String]) -> Option<String> {
    ours.iter().find(|a| list.contains(a)).or_else(|| list.first()).cloned()
}

impl Tx {
    pub(crate) fn from(tx: &BtcTransactionData, ours: &[String]) -> Self {
        let incoming = !tx.sender_addresses.iter().any(|a| ours.contains(a));
        let (from, to) = if incoming {
            let paid = tx
                .outputs
                .iter()
                .filter_map(|o| o.address.as_ref())
                .find(|a| ours.contains(a))
                .cloned()
                .or_else(|| ours.first().cloned());
            (tx.sender_addresses.first().cloned(), paid)
        } else {
            (ours_first(&tx.sender_addresses, ours), tx.receiver_addresses.first().cloned())
        };
        Self {
            txid: tx.txid.clone(),
            sats: tx.amount.trim().parse::<f64>().map_or(0, |v| (v * 1e8).round() as u64),
            fee_sats: tx.fees.parse::<u64>().ok().filter(|f| *f > 0),
            vsize: tx.vsize.filter(|v| *v > 0),
            incoming,
            confirmed: matches!(tx.status, BitcoinTransactionStatus::Success),
            evicted: matches!(
                tx.status,
                BitcoinTransactionStatus::Dropped | BitcoinTransactionStatus::Failed | BitcoinTransactionStatus::Cancelled
            ),
            replaced: matches!(tx.status, BitcoinTransactionStatus::Replaced),
            replaced_by: tx.replaced_by.clone(),
            first_seen: tx.timestamp.parse::<u64>().ok().filter(|t| *t > 0),
            confirmed_at: tx.confirmed_at.as_deref().and_then(|s| s.parse().ok()).filter(|t| *t > 0),
            dropped_at: tx.dropped_at.as_deref().and_then(|s| s.parse().ok()).filter(|t| *t > 0),
            block_height: tx.block_height.as_deref().and_then(|s| s.parse().ok()).filter(|h| *h > 0),
            from,
            to,
        }
    }

    pub(crate) fn unresolved(&self) -> bool {
        !self.confirmed && !self.evicted && !self.replaced
    }

    pub(crate) fn can_bump(&self) -> bool {
        self.unresolved() && !self.incoming
    }

    pub(crate) fn at(&self) -> Option<u64> {
        self.confirmed_at.or(self.dropped_at).or(self.first_seen)
    }

    pub(crate) fn sats(&self) -> u64 {
        self.sats
    }

    pub(crate) fn direction_word(&self) -> &'static str {
        if self.incoming { "Receive" } else { "Send" }
    }

    pub(crate) fn amount_str(&self) -> String {
        let sign = if self.incoming { '+' } else { '\u{2212}' };
        format!("{sign}{}", btc(self.sats))
    }

    pub(crate) fn state_label(&self) -> &'static str {
        if self.confirmed {
            "success"
        } else if self.replaced {
            "replaced"
        } else if self.evicted {
            "evicted"
        } else {
            "pending"
        }
    }

    pub(crate) fn state_color(&self, p: &'static CompactPalette) -> Color {
        if self.confirmed {
            p.green
        } else if self.replaced {
            p.dim
        } else if self.evicted {
            p.red
        } else {
            p.amber
        }
    }

    pub(crate) fn rate(&self) -> Option<f32> {
        match (self.fee_sats, self.vsize) {
            (Some(f), Some(v)) => Some((f as f64 / v as f64) as f32),
            _ => None,
        }
    }
}

pub(crate) fn btc(sats: u64) -> String {
    format!("{}.{:08}", sats / 100_000_000, sats % 100_000_000)
}

pub(crate) fn split(ours: &[String]) -> (Vec<Tx>, Vec<Tx>) {
    let txs = CHANNEL.btc_transactions_rx.borrow();
    let mut all = fold_replaced(txs.transactions.values().map(|t| Tx::from(t, ours)).collect());
    all.sort_by_key(|t| std::cmp::Reverse(t.at().unwrap_or(0)));
    all.into_iter().partition(|t| t.unresolved())
}

fn fold_replaced(all: Vec<Tx>) -> Vec<Tx> {
    let live: HashSet<String> = all.iter().filter(|t| !t.evicted).map(|t| t.txid.clone()).collect();
    all.into_iter()
        .filter(|t| !(t.replaced && t.replaced_by.as_ref().is_some_and(|by| live.contains(by))))
        .collect()
}

pub(crate) fn wait(rate: f32, tiers: &[f32; 4]) -> Verdict {
    (0..4).rev().find(|&i| rate >= tiers[i]).map_or(Verdict::BackOfQueue, |i| Verdict::of(tiers, i))
}

pub(crate) fn facts(tx: &Tx, tiers: Option<[f32; 4]>, tip: Option<u64>, p: &'static CompactPalette) -> Vec<Fact> {
    let copyable = |key: &'static str, value: Option<&str>| Fact {
        key,
        dot: None,
        segs: vec![value.map_or(Seg::mono(NA, p.muted), |v| Seg::mono(elide_addr(v), p.text))],
        copy: value.map(str::to_string),
    };
    let mut rows = vec![copyable("From", tx.from.as_deref()), copyable("To", tx.to.as_deref()), block(tx, tiers, tip, p)];
    if !tx.incoming && !tx.evicted && !tx.replaced {
        rows.push(Fact { key: "Network fee", dot: None, segs: fee_segs(tx, p), copy: None });
    }
    rows.push(copyable("Txid", Some(&tx.txid)));
    rows
}

fn block(tx: &Tx, tiers: Option<[f32; 4]>, tip: Option<u64>, p: &'static CompactPalette) -> Fact {
    let (dot, segs) = if tx.confirmed {
        let segs = match (tx.block_height, tx.block_height.and_then(|h| confirmations(h, tip))) {
            (Some(h), Some(n)) => vec![
                Seg::mono(add_commas(h as i64), p.text),
                Seg::mono(format!(" \u{b7} {n} confirmation{}", if n == 1 { "" } else { "s" }), p.muted),
            ],
            (Some(h), None) => vec![Seg::mono(add_commas(h as i64), p.text)],
            _ => vec![Seg::mono(NA, p.muted)],
        };
        (None, segs)
    } else if tx.evicted {
        (None, vec![Seg::word("never mined", p.red)])
    } else if tx.replaced {
        (None, vec![Seg::word("replaced", p.dim)])
    } else {
        match (tx.rate(), tiers) {
            (Some(r), Some(t)) => {
                let v = wait(r, &t);
                (Some(v.colour(p)), vec![Seg::word(v.word(), p.dim)])
            }
            _ => (None, vec![Seg::mono(NA, p.muted)]),
        }
    };
    Fact { key: "Block", dot, segs, copy: None }
}

fn fee_segs(tx: &Tx, p: &'static CompactPalette) -> Vec<Seg> {
    let Some(f) = tx.fee_sats else {
        return vec![Seg::mono(NA, p.muted)];
    };
    let mut segs = vec![Seg::mono(add_commas(f as i64), p.dim), Seg::mono(" sat", p.muted)];
    if let Some(r) = tx.rate() {
        segs.push(Seg::mono(" \u{b7} ", p.faint));
        segs.push(Seg::mono(rate_text(r), p.dim));
        segs.push(Seg::mono(" sat/vB", p.muted));
    }
    segs
}

fn confirmations(height: u32, tip: Option<u64>) -> Option<u64> {
    let tip = tip?;
    (tip >= height as u64).then(|| tip - height as u64 + 1)
}

pub(crate) fn elide_addr(a: &str) -> String {
    elide(a, 10, 8)
}

fn elide(s: &str, head: usize, tail: usize) -> String {
    let n = s.chars().count();
    if n <= head + tail + 1 {
        return s.to_string();
    }
    let h: String = s.chars().take(head).collect();
    let t: String = s.chars().skip(n - tail).collect();
    format!("{h}\u{2026}{t}")
}

pub(crate) fn stamp_of(tx: &Tx) -> String {
    tx.at().map_or(NA.to_string(), stamp)
}

pub(crate) fn full_stamp(secs: u64) -> String {
    Local
        .timestamp_opt(secs as i64, 0)
        .single()
        .map_or(NA.to_string(), |dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::btc::BtcRbfOutput;
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
            status: if confirmed.is_some() { BitcoinTransactionStatus::Success } else { BitcoinTransactionStatus::Pending },
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
    const TODAY: [f32; 4] = [0.10, 0.52, 1.20, 3.10];
    const P: &CompactPalette = &theme::COMPACT_OBSIDIAN;

    fn ours() -> Vec<String> {
        vec![OWN.to_string()]
    }

    fn flat(facts: &[Fact]) -> String {
        facts
            .iter()
            .flat_map(|f| std::iter::once(f.key.to_string()).chain(f.segs.iter().map(|s| s.s.clone())))
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn direction_comes_from_the_sender_side() {
        let out = Tx::from(&tx("a", "0.00250000", "48", 100, &[OWN], &[THEM], None), &ours());
        assert!(!out.incoming);
        assert_eq!(out.amount_str(), "\u{2212}0.00250000");
        let inc = Tx::from(&tx("b", "0.00120000", "0", 100, &[THEM], &[], None), &ours());
        assert!(inc.incoming);
        assert_eq!(inc.amount_str(), "+0.00120000");
    }

    #[test]
    fn a_receive_names_both_parties_and_has_no_fee_row() {
        let mut raw = tx("b", "0.00120000", "300", 100, &[THEM], &["bc1qtheirchange"], Some((973_947, 200)));
        raw.outputs = vec![BtcRbfOutput { vout: 0, sats: 120_000, address: Some(OWN.into()), spk: None }];
        let inc = Tx::from(&raw, &ours());
        assert_eq!(inc.from.as_deref(), Some(THEM));
        assert_eq!(inc.to.as_deref(), Some(OWN), "the destination is this wallet, never the payer's leftover");
        let f = facts(&inc, Some(TODAY), Some(974_753), P);
        assert_eq!(f.iter().map(|f| f.key).collect::<Vec<_>>(), ["From", "To", "Block", "Txid"]);
        assert!(flat(&f).contains("973,947| \u{b7} 807 confirmations"), "{}", flat(&f));
        assert_eq!(f[0].copy.as_deref(), Some(THEM));
        assert_eq!(f[3].copy.as_deref(), Some("b"));
    }

    #[test]
    fn a_send_shows_its_fee_and_rate_when_the_size_is_known() {
        let mut raw = tx("a", "0.00250000", "48", 100, &[OWN], &[THEM], None);
        let without = facts(&Tx::from(&raw, &ours()), Some(TODAY), None, P);
        assert!(flat(&without).contains("fee|48| sat"), "{}", flat(&without));
        assert!(!flat(&without).contains("sat/vB"));
        raw.vsize = Some(141);
        let with = facts(&Tx::from(&raw, &ours()), Some(TODAY), None, P);
        assert!(flat(&with).contains("fee|48| sat| \u{b7} |0.34| sat/vB"), "{}", flat(&with));
    }

    #[test]
    fn a_pending_row_reads_its_time_at_todays_rates() {
        let mut raw = tx("a", "0.00250000", "48", 100, &[OWN], &[THEM], None);
        raw.vsize = Some(141);
        let t = Tx::from(&raw, &ours());
        let b = &facts(&t, Some(TODAY), None, P)[2];
        assert_eq!((b.key, b.dot), ("Block", Some(P.red)));
        assert_eq!(b.segs[0].s, "back of the queue");
        raw.fees = "74".into();
        let b = &facts(&Tx::from(&raw, &ours()), Some(TODAY), None, P)[2];
        assert_eq!((b.segs[0].s.as_str(), b.dot), ("~6 blocks", Some(P.amber)));
    }

    #[test]
    fn the_round_down_rule_names_the_highest_tier_reached() {
        assert!(matches!(wait(0.34, &TODAY), Verdict::BackOfQueue));
        assert!(matches!(wait(0.52, &TODAY), Verdict::SixBlocks));
        assert!(matches!(wait(1.25, &TODAY), Verdict::ThreeBlocks));
        assert!(matches!(wait(3.10, &TODAY), Verdict::NextBlock));
        assert!(matches!(wait(0.05, &TODAY), Verdict::BackOfQueue));
    }

    #[test]
    fn an_evicted_send_files_as_settled_in_red_and_paid_nothing() {
        let mut raw = tx("a", "0.001", "300", 100, &[OWN], &[THEM], None);
        raw.status = BitcoinTransactionStatus::Dropped;
        raw.dropped_at = Some("500".to_string());
        let t = Tx::from(&raw, &ours());
        assert!(!t.unresolved());
        assert!(!t.can_bump());
        assert_eq!((t.state_label(), t.state_color(P)), ("evicted", P.red));
        assert_eq!(t.at(), Some(500));
        let f = flat(&facts(&t, Some(TODAY), None, P));
        assert!(f.contains("never mined"), "{f}");
        assert!(!f.contains("fee|"), "{f}");
    }

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
            let parsed = parse_btc_tx(&v).unwrap_or_else(|| panic!("status {wire:?} was discarded by the parser"));
            assert_eq!(parsed.status, want);
        }
    }

    #[test]
    fn a_replaced_send_leaves_once_its_replacement_is_in_view() {
        let mut orig = tx("729a", "0.00250000", "48", 100, &[OWN], &[THEM], None);
        orig.status = BitcoinTransactionStatus::Replaced;
        orig.replaced_by = Some("4c25".into());
        orig.dropped_at = Some("150".into());
        let bump = tx("4c25", "0.00250000", "74", 160, &[OWN], &[THEM], None);
        let rows = fold_replaced(vec![Tx::from(&orig, &ours()), Tx::from(&bump, &ours())]);
        assert_eq!(rows.iter().map(|t| t.txid.as_str()).collect::<Vec<_>>(), ["4c25"]);
        assert!(rows[0].can_bump(), "a replacement can be bumped again");

        let mut dead = bump.clone();
        dead.status = BitcoinTransactionStatus::Dropped;
        let rows = fold_replaced(vec![Tx::from(&orig, &ours()), Tx::from(&dead, &ours())]);
        assert_eq!(rows.len(), 2, "with its replacement gone, the original stays");
        let rows = fold_replaced(vec![Tx::from(&orig, &ours())]);
        assert_eq!(rows[0].state_label(), "replaced");
    }

    #[test]
    fn only_a_pending_send_can_be_bumped() {
        assert!(Tx::from(&tx("c", "0.001", "300", 100, &[OWN], &[THEM], None), &ours()).can_bump());
        assert!(!Tx::from(&tx("d", "0.001", "0", 100, &[THEM], &[], None), &ours()).can_bump());
        assert!(!Tx::from(&tx("a", "0.021", "210", 100, &[OWN], &[THEM], Some((1, 2))), &ours()).can_bump());
    }

    #[test]
    fn pending_rows_are_amber_and_settled_ones_green() {
        let out = Tx::from(&tx("c", "0.001", "300", 100, &[OWN], &[THEM], None), &ours());
        assert_eq!((out.state_label(), out.state_color(P)), ("pending", P.amber));
        let won = Tx::from(&tx("a", "0.021", "210", 100, &[THEM], &[], Some((962_488, 1_786_761_453))), &ours());
        assert_eq!((won.state_label(), won.state_color(P)), ("success", P.green));
        assert_eq!(won.at(), Some(1_786_761_453), "a confirmed row reads the block's time");
    }

    #[test]
    fn confirmations_count_from_the_live_tip() {
        assert_eq!(confirmations(962_488, Some(962_493)), Some(6));
        assert_eq!(confirmations(962_488, Some(962_488)), Some(1));
        assert_eq!(confirmations(962_488, None), None);
        assert_eq!(confirmations(962_488, Some(962_487)), None);
    }

    #[test]
    fn addresses_elide_to_one_fixed_width() {
        for a in [
            "bc1qyfxmsjaaaaaaaaaaaaaaaaaaaaaaaaaaamh06f67",
            "bc1pmzfrwwndsqmk5yh69yjr5lfgfg4ev8c0tsc06e2ahcm7cq0v6nrsgvdvgd",
            "1NbFKXag72gY1EuNEaQxBu2rstfo9JEB5n",
            "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy",
        ] {
            assert_eq!(elide_addr(a).chars().count(), 19, "{a}");
        }
        assert_eq!(elide_addr("bc1qshort"), "bc1qshort");
    }
}
