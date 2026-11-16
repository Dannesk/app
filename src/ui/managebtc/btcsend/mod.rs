

use crate::btc_script_type::BtcScriptType;
use crate::channel::CHANNEL;
use crate::controller::app_state::{AppState, BtcFeeTier};
use crate::ui::managebtc::node::{rate_text, Verdict};
pub use crate::utils::btc_send::{BtcSendDraft, BtcSendQuote, FeeQuote};
pub use crate::ws::commands::bitcoin_payment::{tier_sats_at, vsize_for};


pub fn typical_vsize() -> u64 {
    let address = CHANNEL.bitcoin_wallet_rx.borrow().1.clone();
    let kind = address
        .as_deref()
        .and_then(BtcScriptType::of_address)
        .unwrap_or_else(crate::btc_script_type::stored);
    vsize_for(&[kind], &[kind.spk_len(), kind.spk_len()])
}


pub fn draft(state: &AppState) -> BtcSendDraft {
    let custom = (state.btc_send_fee_tier == BtcFeeTier::Custom && !state.btc_send_custom.is_empty())
        .then(|| state.btc_send_custom.clone());
    let typed = (state.btc_send_fee_face && state.btc_send_fee_lit.is_none())
        .then(|| state.btc_send_fee.trim().to_string())
        .filter(|t| !t.is_empty());
    BtcSendDraft {
        recipient: state.btc_send_recipient.trim().to_string(),
        amount: state.btc_send_amount.trim().to_string(),
        custom,
        typed,
        max: state.btc_send_max_on,
    }
}

pub fn quote() -> Option<BtcSendQuote> {
    CHANNEL.btc_send_quote_rx.borrow().clone()
}

pub fn live_quote(state: &AppState) -> Option<BtcSendQuote> {
    quote().filter(|q| q.draft == draft(state))
}

pub fn in_use(state: &AppState, q: &BtcSendQuote) -> Option<FeeQuote> {
    match state.btc_send_fee_tier {
        BtcFeeTier::Custom => q.custom,
        named => named.index().and_then(|i| q.tiers.map(|t| t[i])),
    }
}

pub(crate) enum Line {
    Prompt(String),
    Tier(Verdict),
    Custom(Option<Verdict>),
    Under(String),
}

pub(crate) struct Editor {
    pub cost: Option<FeeQuote>,
    pub line: Line,
    pub ready: bool,
}

pub(crate) fn editor(state: &AppState, q: Option<&BtcSendQuote>) -> Editor {
    let tiers = q.and_then(|q| q.tiers);
    if let Some(i) = state.btc_send_fee_lit.and_then(BtcFeeTier::index) {
        let cost = tiers.map(|t| t[i]);
        let line = tiers.map_or(Line::Prompt(OWN_RATE.to_string()), |t| Line::Tier(Verdict::of(&t.map(|f| f.rate), i)));
        return Editor { cost, line, ready: cost.is_some() };
    }
    let positive = state.btc_send_fee.trim().parse::<f32>().ok().filter(|r| r.is_finite() && *r > 0.0);
    let cost = positive.and(q.filter(|q| q.draft == draft(state))).and_then(|q| q.typed);
    let line = match (positive, cost.zip(tiers).map(|(c, t)| custom_read(c.fee, &t))) {
        (None, _) => Line::Prompt(OWN_RATE.to_string()),
        (Some(_), Some(CustomRead::Warning(w))) => Line::Under(w),
        (Some(_), Some(CustomRead::Verdict(v))) => Line::Custom(Some(v)),
        (Some(_), None) => Line::Custom(None),
    };
    let ready = cost.is_some() && !matches!(line, Line::Under(_));
    Editor { cost, line, ready }
}

pub fn amount_sats(state: &AppState) -> Option<u64> {
    state
        .btc_send_amount
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| *v > 0.0)
        .map(|v| (v * 100_000_000.0).round() as u64)
        .filter(|s| *s > 0)
}


#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CustomRead {
    Verdict(Verdict),
    Warning(String),
}

pub(crate) fn custom_read(fee: u64, tiers: &[FeeQuote; 4]) -> CustomRead {
    match (0..4).rev().find(|&i| fee >= tiers[i].fee) {
        Some(reach) => CustomRead::Verdict(Verdict::of(&tiers.map(|t| t.rate), reach)),
        None => CustomRead::Warning(below_minimum(tiers[0].rate)),
    }
}


#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Gate {
    pub insufficient: bool,
    pub under_dust: bool,
    pub blocked: bool,
    pub ready: bool,
}

impl Gate {
    pub fn red(self) -> bool {
        self.insufficient || self.under_dust
    }
}

pub fn gate(state: &AppState, q: Option<&BtcSendQuote>) -> Gate {
    let amount = amount_sats(state);
    let fee = q.and_then(|q| in_use(state, q));
    let spendable = q.and_then(|q| q.spendable);
    let insufficient = matches!((amount, fee, spendable), (Some(a), Some(f), Some(s)) if a.saturating_add(f.fee) > s);
    let under_dust = matches!((amount, q.and_then(|q| q.dust)), (Some(a), Some(d)) if a < d);
    let blocked = state.btc_send_fee_tier == BtcFeeTier::Custom
        && matches!((fee, q.and_then(|q| q.tiers)), (Some(f), Some(t)) if f.fee < t[0].fee);
    let addr = state.btc_send_recipient.trim();
    let own = CHANNEL.bitcoin_wallet_rx.borrow().1.clone();
    let addr_ok = is_valid(addr) && !own.as_deref().is_some_and(|o| same_address(o, addr));
    let live = q.is_some_and(|q| q.draft == draft(state));
    let ready = addr_ok
        && amount.is_some()
        && live
        && fee.is_some()
        && spendable.is_some()
        && !insufficient
        && !under_dust
        && !blocked
        && !state.btc_send_fee_face;
    Gate { insufficient, under_dust, blocked, ready }
}

pub(crate) fn below_minimum(floor: f32) -> String {
    format!("below the {} minimum", rate_text(floor))
}

pub fn rate_typed(candidate: &str) -> Option<String> {
    if candidate.is_empty() {
        return Some(String::new());
    }
    let raw = if candidate.starts_with('.') { format!("0{candidate}") } else { candidate.to_string() };
    let digits = raw.trim_start_matches('0');
    let rate = if digits.is_empty() || digits.starts_with('.') { format!("0{digits}") } else { digits.to_string() };
    rate_accepts(&rate).then_some(rate)
}


pub fn rate_accepts(candidate: &str) -> bool {
    if !candidate.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return false;
    }
    let mut parts = candidate.splitn(2, '.');
    let whole = parts.next().unwrap_or("");
    let decimals = parts.next();
    if decimals.is_some_and(|d| d.contains('.')) || whole.len() > RATE_DIGITS {
        return false;
    }
    let n: u32 = whole.parse().unwrap_or(0);
    let places = if n < 10 { 2 } else if n < 100 { 1 } else { 0 };
    match decimals {
        None => true,
        Some(_) if places == 0 => false,
        Some(d) => d.len() <= places,
    }
}

const RATE_DIGITS: usize = 5;


pub fn is_valid(addr: &str) -> bool {
    dannesk_btc_codec::address::Address::parse(addr).is_ok()
}


const LEGACY_MIN: usize = 25;

const BECH32_MIN: usize = 14;


pub(crate) fn is_plausible(addr: &str) -> bool {
    addr.starts_with('1') || addr.starts_with('3') || is_bech32ish(addr) || "bc1".starts_with(addr)
}


fn is_bech32ish(addr: &str) -> bool {
    addr.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("bc1"))
}


pub(crate) fn same_address(a: &str, b: &str) -> bool {
    a == b || (is_bech32ish(a) && is_bech32ish(b) && a.eq_ignore_ascii_case(b))
}


pub(crate) fn recipient_fault(addr: &str) -> Option<&'static str> {
    if addr.is_empty() {
        return None;
    }
    if !is_plausible(addr) {
        return Some(PREFIX_ERROR);
    }
    let floor = if is_bech32ish(addr) {
        BECH32_MIN
    } else if addr.starts_with('1') || addr.starts_with('3') {
        LEGACY_MIN
    } else {
        return None;
    };
    (addr.len() >= floor && !is_valid(addr)).then_some(CHECKSUM_ERROR)
}


pub(crate) const PLACEHOLDER: &str = "bc1q\u{2026}  \u{b7}  bc1p\u{2026}  \u{b7}  1\u{2026}  \u{b7}  3\u{2026}";

pub const OVER_AVAILABLE: &str = "More than available";
pub(crate) const SELF_ERROR: &str = "You cannot send to yourself";
pub(crate) const PREFIX_ERROR: &str = "A bitcoin address starts with 1, 3 or bc1";

pub(crate) const CHECKSUM_ERROR: &str = "That address fails its checksum";
pub(crate) const OWN_RATE: &str = "your own rate, in sat/vB";

pub(crate) fn dust_words(dust: u64) -> String {
    format!("Minimum {} sat", crate::utils::add_commas(dust as i64))
}


pub(crate) const BACK_OF_QUEUE: &str = "back of the queue";

#[cfg(test)]
mod tests {
    use super::*;


    const REAL: [&str; 5] = [
        "1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2",                                 // P2PKH
        "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy",                                 // P2SH
        "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4",                         // P2WPKH, bech32
        "bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3",     // P2WSH,  bech32
        "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0",     // Taproot, BECH32M
    ];

    fn tiers(fees: [u64; 4]) -> [FeeQuote; 4] {
        let rates = [0.10, 0.13, 0.34, 1.09];
        [0, 1, 2, 3].map(|i| FeeQuote { rate: rates[i], fee: fees[i], vsize: 141, max: 0 })
    }


    #[test]
    fn only_real_addresses_pass() {
        for addr in REAL {
            assert!(is_valid(addr), "{addr} is a real address and was refused");
            assert_eq!(recipient_fault(addr), None, "{addr} was painted red");
        }

        let fake = "bc1qu5g2twq0udg5g09h2u03u54x2ve8zkcum7szm";
        assert!(!is_valid(fake), "a bad checksum still passes the gate");
        assert_eq!(recipient_fault(fake), Some(CHECKSUM_ERROR));
        for addr in REAL {
            let chars: Vec<char> = addr.chars().collect();
            for i in 1..chars.len() {
                let mut e = chars.clone();
                e[i] = if e[i] == 'q' { 'p' } else { 'q' };
                let e: String = e.into_iter().collect();
                assert!(!is_valid(&e), "editing slot {i} of {addr} still passed");
            }
        }
    }


    #[test]
    fn an_uppercase_bech32_address_is_payable() {
        let lower = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
        let upper = lower.to_uppercase();
        assert!(is_valid(&upper), "an uppercase address off a QR code was refused");
        assert_eq!(recipient_fault(&upper), None);
        assert!(is_plausible("BC1Q"), "an uppercase address reads as wrong while typed");
        let mixed = format!("BC1Q{}", &lower[4..]);
        assert!(!is_valid(&mixed));
    }


    #[test]
    fn the_self_check_folds_case_only_where_the_encoding_does() {
        let bech = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
        assert!(same_address(bech, &bech.to_uppercase()), "one address read as two");
        assert!(same_address(bech, bech));
        let legacy = "1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2";
        assert!(same_address(legacy, legacy));
        assert!(!same_address(legacy, &legacy.to_uppercase()), "base58 case was folded");
        assert!(!same_address(bech, "bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3"));
    }


    #[test]
    fn a_refused_address_is_never_silent() {
        let cases: Vec<(String, Option<&str>)> = vec![
            ("bc1".to_string(), None),
            ("1".repeat(24), None),
            (format!("bc1{}", "q".repeat(90)), Some(CHECKSUM_ERROR)),
            ("1".repeat(40), Some(CHECKSUM_ERROR)),
            ("1".repeat(25), Some(CHECKSUM_ERROR)),

            ("0xdeadbeef".to_string(), Some(PREFIX_ERROR)),
            ("tb1qw508d6qejxtdg4y5r3zarvaryvaxxpcs".to_string(), Some(PREFIX_ERROR)),
        ];
        for (addr, want) in cases {
            assert_eq!(recipient_fault(&addr), want, "{addr:?}");
        }

        for addr in REAL {
            let floor = if is_bech32ish(addr) { BECH32_MIN } else { LEGACY_MIN };
            for n in 1..=addr.len() {
                let part = &addr[..n];
                assert!(
                    is_valid(part) || recipient_fault(part).is_some() || n < floor,
                    "{part:?} was refused with nothing said",
                );
            }
        }
    }

    #[test]
    fn the_hint_stays_quiet_until_there_is_something_to_judge() {
        for part in ["", "b", "bc", "bc1", "bc1q", "BC1Q", "1", "3A"] {
            assert_eq!(recipient_fault(part), None, "{part:?} spoke too early");
        }
    }


    #[test]
    fn every_amount_round_trips_to_exact_sats() {
        let to_sats = |btc: f64| (btc * 1e8).round() as u64;
        let cases: [(&str, u64); 9] = [
            ("1.00000000", 100_000_000),
            ("0.00000001", 1),
            ("0.12345678", 12_345_678),
            ("0.1", 10_000_000),
            ("0.29", 29_000_000),
            ("1.1", 110_000_000),
            ("123.45678901", 12_345_678_901),
            ("20999999.99999999", 2_099_999_999_999_999),
            ("21000000.00000000", 2_100_000_000_000_000),
        ];
        for (text, sats) in cases {
            let parsed: f64 = text.parse().unwrap();
            assert_eq!(to_sats(parsed), sats, "{text}");
            assert_eq!(to_sats(format!("{parsed:.8}").parse().unwrap()), sats, "{text} re-formatted");
        }

        for sats in (0..10_000_000u64).step_by(7) {
            let text = format!("{:.8}", sats as f64 / 1e8);
            assert_eq!(to_sats(text.parse().unwrap()), sats, "{text}");
        }
    }

    #[test]
    fn a_custom_fee_rounds_down_to_the_tier_it_pays() {
        let t = tiers([15, 19, 48, 154]);
        assert_eq!(custom_read(71, &t), CustomRead::Verdict(Verdict::ThreeBlocks));
        assert_eq!(custom_read(154, &t), CustomRead::Verdict(Verdict::NextBlock));
        assert_eq!(custom_read(19, &t), CustomRead::Verdict(Verdict::SixBlocks));
        assert_eq!(custom_read(15, &t), CustomRead::Verdict(Verdict::BackOfQueue));
        assert_eq!(custom_read(14, &t), CustomRead::Warning("below the 0.10 minimum".to_string()));
        assert_eq!(custom_read(1_540, &t), CustomRead::Verdict(Verdict::NextBlock));
    }

    #[test]
    fn the_rate_field_stops_where_the_rate_format_does() {
        for ok in ["", "0", "0.", "0.5", "0.50", "9.99", "10.5", "99.9", "150", "12345"] {
            assert!(rate_accepts(ok), "{ok:?} was refused");
        }
        for bad in ["0.505", "10.55", "150.", "123456", "1.2.3", "1,5", "-1"] {
            assert!(!rate_accepts(bad), "{bad:?} was taken");
        }
    }


    #[test]
    fn a_typed_rate_reads_as_the_rate_format_prints_it() {
        for (typed, want) in [("", ""), (".", "0."), (".5", "0.5"), ("0", "0"), ("00", "0"), ("05", "5"), ("00.5", "0.5"), ("10.5", "10.5"), ("150", "150")] {
            assert_eq!(rate_typed(typed).as_deref(), Some(want), "{typed:?}");
        }
        for bad in ["0.505", "150.5", "123456", "1.2.3", "a"] {
            assert_eq!(rate_typed(bad), None, "{bad:?} was taken");
        }
    }
}
