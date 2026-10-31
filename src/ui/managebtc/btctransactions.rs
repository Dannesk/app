

use iced::Color;
use chrono::{Datelike, Local, TimeZone};
use crate::controller::message::Message;
use crate::channel::CHANNEL;
use crate::channel::btc::{BitcoinTransactionStatus, BtcTransactionData};
use crate::ui::components::tx_panel::{IdLine, Line, Seg, Tail};
use crate::utils::add_commas;
use crate::utils::theme::CompactPalette;

const NA: &str = "—";


pub(crate) struct Tx {
    pub(crate) txid: String,
    signed_btc: f64,
    fee_sats: Option<u64>,
    pub(crate) incoming: bool,
    pub(crate) confirmed: bool,
    dropped: bool,
    replaced: bool,
    replaced_by: Option<String>,

    replacement_fee: Option<u64>,
    
    replaces: Option<String>,
    original_fee: Option<u64>,
    first_seen: Option<u64>,
    confirmed_at: Option<u64>,
    dropped_at: Option<u64>,
    block_height: Option<u32>,
    from: Option<Party>,
    to: Option<Party>,
}


struct Party {
    addr: String,

    extra: usize,
}


fn party(list: &[String], ours: &[String]) -> Option<Party> {
    if let Some(mine) = ours.iter().find(|a| list.contains(a)) {
        return Some(Party { addr: mine.clone(), extra: list.len() - 1 });
    }
    let first = list.first()?;
    Some(Party { addr: first.clone(), extra: list.len() - 1 })
}

impl Tx {
  
    fn from(tx: &BtcTransactionData, ours: &[String]) -> Self {
        let incoming = !tx.sender_addresses.iter().any(|a| ours.contains(a));
        let amount = tx.amount.parse::<f64>().unwrap_or(0.0);

     
        let (from, to) = if incoming {
            let paid = tx
                .outputs
                .iter()
                .filter_map(|o| o.address.as_ref())
                .find(|a| ours.contains(a))
                .cloned()
                .or_else(|| ours.first().cloned())
                .unwrap_or_default();
            (party(&tx.sender_addresses, ours), Some(Party { addr: paid, extra: 0 }))
        } else {
            (party(&tx.sender_addresses, ours), party(&tx.receiver_addresses, ours))
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

  
    pub(crate) fn unresolved(&self) -> bool {
        !self.confirmed && !self.dropped && !self.replaced
    }


    pub(crate) fn at(&self) -> Option<u64> {
        self.confirmed_at.or(self.dropped_at).or(self.first_seen)
    }

    pub(crate) fn direction_word(&self) -> &'static str {
        if self.incoming { "receive" } else { "send" }
    }

    pub(crate) fn amount_str(&self) -> String {
        format!("{:+.8}", self.signed_btc)
    }

    pub(crate) fn state_label(&self) -> &'static str {
        if self.confirmed {
            "success"
        } else if self.replaced {
        
            "replaced"
        } else if self.dropped {
            "rejected"
        } else {
            "pending"
        }
    }

 
    pub(crate) fn state_color(&self, p: &'static CompactPalette) -> Color {
        if self.dropped {
            p.red
        } else if self.confirmed {
            p.green
        } else {
            p.dim
        }
    }
}


pub(crate) struct Ctx {
    pub(crate) btc_rate: f64,
    pub(crate) ccy: &'static str,
    pub(crate) tip: Option<u64>,
    pub(crate) p: &'static CompactPalette,
}


pub(crate) fn split(ours: &[String]) -> (Vec<Tx>, Vec<Tx>) {
    let txs = CHANNEL.btc_transactions_rx.borrow();
    let mut all = fold_replaced(txs.transactions.values().map(|t| Tx::from(t, ours)).collect());
   
    all.sort_by_key(|t| std::cmp::Reverse(t.at().unwrap_or(0)));
 
    all.into_iter().partition(|t| t.unresolved())
}


fn fold_replaced(mut all: Vec<Tx>) -> Vec<Tx> {
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


pub(crate) fn time_label(tx: &Tx, now: u64) -> String {
    if !tx.unresolved() {
        tx.at().map_or(NA.to_string(), stamp)
    } else {
        match tx.first_seen {
          
            Some(t) => age(now.saturating_sub(t)),
            None => NA.to_string(),
        }
    }
}


pub(crate) fn id_line(tx: &Tx, copied: Option<&str>) -> IdLine {
    IdLine {
        key: "txid",
        value: elide_addr(&tx.txid),
        copy: Message::BtcCopyTxid(tx.txid.clone()),
        copied: copied == Some(tx.txid.as_str()),
    }
}

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

  
    rows.push((
        "fee",
        if tx.incoming {
            vec![Seg::word("incoming — the sender paid it", p.muted)]
        } else if tx.dropped || tx.replaced {
           
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
                  
                    if let Some(was) = tx.original_fee.filter(|_| tx.replaces.is_some()) {
                        r.push(Seg::word(" \u{b7} bumped from ", p.faint));
                        r.push(Seg::mono(format!("{was} sats"), p.muted));
                    }
                    r
                }
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
  
        rows.push(("note", vec![Seg::word("this transaction will not confirm", p.muted)]));
    }

    rows
}


pub(crate) fn tail(tx: &Tx, p: &'static CompactPalette) -> Option<Tail> {
    if tx.confirmed || tx.dropped || tx.replaced || tx.incoming {
        return None;
    }
    Some(Tail::Link { label: "modify fee", color: p.dim, msg: Message::BtcBumpClicked(tx.txid.clone()) })
}


fn confirmations(height: u32, tip: Option<u64>) -> Option<u64> {
    let tip = tip?;
    (tip >= height as u64).then(|| tip - height as u64 + 1)
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


fn age(secs: u64) -> String {
    if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{:.1}h ago", secs as f64 / 3600.0)
    } else {
        format!("{}d ago", secs / 86_400)
    }
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
    const CHANGE: &str = "bc1qchange";

    fn ours() -> Vec<String> {
        vec![OWN.to_string()]
    }

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
        let out = Tx::from(&tx("a", "0.00004120", "56", 100, &[OWN], &[THEM], None), &ours());
        assert!(!out.incoming);
        assert!(out.amount_str().starts_with('-'));

        let inc = Tx::from(&tx("b", "0.00002500", "0", 100, &[THEM], &[], None), &ours());
        assert!(inc.incoming);
        assert!(inc.amount_str().starts_with('+'));
    }


    #[test]
    fn an_incoming_transaction_is_addressed_to_this_wallet() {
        let payers_change = "bc1qtheirchangeaddressxxxxxxxxxxxxxxxxxxxxxx";
        let inc = Tx::from(
            &tx("b", "0.00002500", "0", 100, &[THEM], &[payers_change], None), &ours());
        let to = inc.to.as_ref().expect("an incoming tx always has a destination: us");
        assert_eq!(to.addr, OWN, "the destination is this wallet's own address");
        assert!(!to.addr.contains("theirchange"), "never the payer's change");

        let from = inc.from.as_ref().expect("the payer is on the frame");
        assert_eq!(from.addr, THEM);

      
        let d = flat(&detail_lines(&inc, &ctx()));
        assert!(d.contains(&elide_addr(OWN)), "{d}");
        assert!(!d.contains("wallet"), "{d}");
    }


    #[test]
    fn our_address_wins_an_unordered_sender_list() {
        for senders in [vec![OWN, THEM], vec![THEM, OWN]] {
            let t = Tx::from(&tx("a", "0.1", "56", 100, &senders, &[THEM], None), &ours());
            let from = t.from.unwrap();
            assert_eq!(from.addr, OWN, "ours must win regardless of iteration order");
            assert_eq!(from.extra, 1, "and must say how many it is standing in for");
        }
    }

  
    #[test]
    fn zero_fee_is_unknown_not_free() {
        let unknown = Tx::from(&tx("a", "0.0001", "0", 100, &[OWN], &[THEM], None), &ours());
        assert_eq!(unknown.fee_sats, None);
        assert!(flat(&detail_lines(&unknown, &ctx())).contains(NA));

        let known = Tx::from(&tx("a", "0.0001", "56", 100, &[OWN], &[THEM], None), &ours());
        assert_eq!(known.fee_sats, Some(56));
        let d = flat(&detail_lines(&known, &ctx()));
        assert!(d.contains("56 sats"));
        assert!(d.contains("0.03 USD"), "{d}");
        assert!(!d.contains('$'), "{d}");
    }


    #[test]
    fn a_confirmed_row_reads_the_block_time_not_first_seen() {
        let seen_at = 1_786_750_000;
        let mined_at = 1_786_761_453;
        let t = Tx::from(
            &tx("a", "0.021", "210", seen_at, &[THEM], &[], Some((962_488, mined_at))), &ours());
        assert!(t.confirmed);
        assert_eq!(t.first_seen, Some(seen_at), "the wait is still on the record");
        assert_eq!(t.at(), Some(mined_at), "but the row reads the block");
        assert_eq!(t.block_height, Some(962_488));
    }


    #[test]
    fn a_dropped_transaction_files_as_settled_not_pending() {
        let mut raw = tx("a", "0.001", "300", 100, &[OWN], &[THEM], None);
        raw.status = BitcoinTransactionStatus::Dropped;
        raw.dropped_at = Some("500".to_string());
        let t = Tx::from(&raw, &ours());

        assert!(t.dropped);
        assert!(!t.confirmed, "it must never be filed as a success");
        assert!(!t.unresolved(), "but it IS resolved — that is the whole point");
        assert_eq!(t.state_label(), "rejected");
        assert_eq!(t.state_color(P), P.red);
        assert_eq!(t.at(), Some(500));

        let (pending, settled): (Vec<Tx>, Vec<Tx>) =
            vec![t].into_iter().partition(|t| t.unresolved());
        assert!(pending.is_empty(), "pending holds only what is waiting");
        assert_eq!(settled.len(), 1);
    }

    #[test]
    fn a_dropped_fee_is_shown_as_never_paid() {
        let mut raw = tx("a", "0.001", "300", 100, &[OWN], &[THEM], None);
        raw.status = BitcoinTransactionStatus::Dropped;
        let t = Tx::from(&raw, &ours());

        let s = flat(&detail_lines(&t, &ctx()));
        assert!(s.contains("never paid"), "a dropped fee must say it was not paid");
        assert!(!s.contains("USD"), "and must not be priced in fiat: {s}");
        assert!(s.contains("will not confirm"));
      
        assert!(!s.contains("avg block"));
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
            let parsed = parse_btc_tx(&v)
                .unwrap_or_else(|| panic!("status {wire:?} was discarded by the parser"));
            assert_eq!(parsed.status, want);
        }
    }

  
    #[test]
    fn a_send_from_a_change_coin_is_outgoing() {
        let wallet = vec![OWN.to_string(), CHANGE.to_string()];
        let t = Tx::from(&tx("e", "0.00002343", "300", 100, &[CHANGE], &[THEM], None), &wallet);
        assert_eq!(t.direction_word(), "send");
        assert!(t.signed_btc < 0.0);
        assert_eq!(t.from.as_ref().unwrap().addr, CHANGE);
        assert_eq!(t.to.as_ref().unwrap().addr, THEM);
    }


    #[test]
    fn the_lead_word_is_direction_and_the_state_column_is_outcome() {
        let won = Tx::from(
            &tx("a", "0.021", "210", 100, &[THEM], &[], Some((962_488, 1_786_761_453))), &ours());
        assert_eq!(won.state_label(), "success");
        assert_eq!(won.state_color(P), P.green);

        let mut raw = tx("b", "0.001", "300", 100, &[OWN], &[THEM], None);
        raw.status = BitcoinTransactionStatus::Dropped;
        let lost = Tx::from(&raw, &ours());
        assert_eq!(lost.state_label(), "rejected");
        assert_eq!(lost.state_color(P), P.red);

       
        let out = Tx::from(&tx("c", "0.001", "300", 100, &[OWN], &[THEM], None), &ours());
        let inc = Tx::from(&tx("d", "0.001", "0", 100, &[THEM], &[], None), &ours());
        assert_eq!(out.state_label(), "pending");
        assert_eq!(inc.state_label(), "pending");
        assert_eq!(out.direction_word(), "send");
        assert_eq!(inc.direction_word(), "receive");
        assert_eq!(out.state_color(P), P.dim);
        assert_eq!(inc.state_color(P), P.dim);
    }


    #[test]
    fn a_replaced_row_is_settled_but_not_a_failure() {
        let mut raw = tx("b", "0.001", "300", 100, &[OWN], &[THEM], None);
        raw.status = BitcoinTransactionStatus::Replaced;
        raw.replaced_by = Some("c0ffee0000000000000000000000000000000000000000000000000000001234".into());
        raw.dropped_at = Some("150".into());
        let t = Tx::from(&raw, &ours());
        assert!(!t.unresolved());
        assert_eq!(t.state_label(), "replaced");
        assert_eq!(t.state_color(P), P.dim);
        assert_eq!(t.at(), Some(150));
        let lines = detail_lines(&t, &ctx());
        assert!(flat(&lines).contains("never paid"));
        assert!(flat(&lines).contains("c0ff\u{2026}1234"));
        assert!(flat(&lines).contains("replaced at a higher fee"));
        let mut t = t;
        t.replacement_fee = Some(300);
        assert!(flat(&detail_lines(&t, &ctx())).contains("replaced at 300 sats"));
        assert!(tail(&t, P).is_none());
    }

    #[test]
    fn a_replaced_original_folds_into_its_live_replacement() {
        let mut orig = tx("729a", "0.00012505", "28", 100, &[OWN], &[THEM], None);
        orig.status = BitcoinTransactionStatus::Replaced;
        orig.replaced_by = Some("4c25".into());
        orig.dropped_at = Some("150".into());
        let bump = tx("4c25", "0.00012505", "300", 160, &[OWN], &[THEM], None);

        let rows = fold_replaced(vec![Tx::from(&orig, &ours()), Tx::from(&bump, &ours())]);
        assert_eq!(rows.len(), 1, "two txids, one payment, one row");
        let live = &rows[0];
        assert_eq!(live.txid, "4c25");
        assert_eq!(live.replaces.as_deref(), Some("729a"));
        assert_eq!(live.original_fee, Some(28));
        let text = flat(&detail_lines(live, &ctx()));
        assert!(text.contains("300 sats"), "{text}");
        assert!(text.contains("bumped from 28 sats"), "{text}");
        assert!(text.contains("729a"), "{text}");

        let mined = tx("4c25", "0.00012505", "300", 160, &[OWN], &[THEM], Some((965_706, 200)));
        let rows = fold_replaced(vec![Tx::from(&orig, &ours()), Tx::from(&mined, &ours())]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state_label(), "success");
        assert!(flat(&detail_lines(&rows[0], &ctx())).contains("bumped from 28 sats"));


        let mut dead = tx("4c25", "0.00012505", "300", 160, &[OWN], &[THEM], None);
        dead.status = BitcoinTransactionStatus::Dropped;
        let rows = fold_replaced(vec![Tx::from(&orig, &ours()), Tx::from(&dead, &ours())]);
        assert_eq!(rows.len(), 2);
        let o = rows.iter().find(|t| t.txid == "729a").unwrap();
        assert_eq!(o.replacement_fee, Some(300));

        let rows = fold_replaced(vec![Tx::from(&orig, &ours())]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state_label(), "replaced");
    }

    #[test]
    fn the_bump_link_is_offered_exactly_to_pending_outgoing_rows() {
        let out = Tx::from(&tx("c", "0.001", "300", 100, &[OWN], &[THEM], None), &ours());
        assert!(matches!(tail(&out, P), Some(Tail::Link { label: "modify fee", msg: Message::BtcBumpClicked(_), .. })));
        let inc = Tx::from(&tx("d", "0.001", "0", 100, &[THEM], &[], None), &ours());
        assert!(tail(&inc, P).is_none());
        let won = Tx::from(&tx("a", "0.021", "210", 100, &[OWN], &[THEM], Some((1, 2))), &ours());
        assert!(tail(&won, P).is_none());
    }

    #[test]
    fn the_txid_line_carries_the_copy() {
        let t = Tx::from(&tx("4c25", "0.001", "300", 100, &[OWN], &[THEM], Some((1, 2))), &ours());
        let id = id_line(&t, None);
        assert_eq!(id.key, "txid");
        assert!(!id.copied);
        assert!(matches!(id.copy, Message::BtcCopyTxid(ref s) if s == "4c25"));
        assert!(id_line(&t, Some("4c25")).copied);
        assert!(!id_line(&t, Some("729a")).copied, "the tick belongs to the row that was copied");
    }

    #[test]
    fn confirmations_count_from_the_live_tip() {
        assert_eq!(confirmations(962_488, Some(962_493)), Some(6));
        assert_eq!(confirmations(962_488, Some(962_488)), Some(1), "in the tip block");
        assert_eq!(confirmations(962_488, None), None);
        assert_eq!(confirmations(962_488, Some(962_487)), None);
    }

    #[test]
    fn the_two_sets_are_disjoint_and_newest_first() {
        let mut all = vec![
            Tx::from(&tx("old", "0.1", "10", 100, &[OWN], &[THEM], Some((1, 150))), &ours()),
            Tx::from(&tx("new", "0.1", "10", 200, &[OWN], &[THEM], Some((2, 250))), &ours()),
            Tx::from(&tx("pend", "0.1", "10", 300, &[OWN], &[THEM], None), &ours()),
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

    
    #[test]
    fn age_and_stamp_fit_the_time_column() {
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
