

use iced::Widget as _;
use iced::widget::{button, column, container, text, Space};
use iced::{Border, Color, Element, Length, Padding, Shadow};

use crate::btc_script_type::BtcScriptType;
use crate::channel::CHANNEL;
use crate::controller::app_state::{AppState, BtcFeeTier};
use crate::controller::message::Message;
use crate::ui::managebtc::node::{rate_text, Verdict};
use crate::utils::theme::CompactPalette;
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
    let custom = if state.btc_send_custom_open {
        Some(state.btc_send_fee.trim().to_string())
    } else {
        (!state.btc_send_custom.is_empty()).then(|| state.btc_send_custom.clone())
    };
    BtcSendDraft {
        recipient: state.btc_send_recipient.trim().to_string(),
        amount: state.btc_send_amount.trim().to_string(),
        custom,
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
        BtcFeeTier::Custom if state.btc_send_custom_open => None,
        BtcFeeTier::Custom => q.custom,
        named => named.index().and_then(|i| q.tiers.map(|t| t[i])),
    }
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
        None => CustomRead::Warning(format!("min {}", rate_text(tiers[0].rate))),
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
        && !state.btc_send_custom_open;
    Gate { insufficient, under_dust, blocked, ready }
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


pub const FEE_BOX_W: f32 = 96.0;

pub const TIER_GAP: f32 = 15.0;
pub const TIER_SIZE: f32 = 10.5;


pub const INTER_ADVANCE: f32 = 0.52;


pub fn tier_cell<'a>(
    t:        BtcFeeTier,
    selected: bool,
    offered:  bool,
    colour:   Color,
    on_press: fn(BtcFeeTier) -> Message,
    cp:       &'static CompactPalette,
    scale:    f32,
) -> Element<'a, Message> {
    let word = t.label();
    let (tone, live) = match (selected, offered) {
        (true, _) => (colour, false),
        (false, true) => (cp.muted, true),
        (false, false) => (cp.faint, false),
    };

    let label: Element<'a, Message> = if live {
        button(text(word.to_string()).size(TIER_SIZE * scale).color(tone))
            .on_press(on_press(t))
            .padding(Padding::ZERO)
            .style(|_, _| button::Style {
                background: None,
                border: Border::default(),
                text_color: Color::TRANSPARENT,
                shadow: Shadow::default(),
                snap: false,
            })
            .boxed()
    } else {
        text(word.to_string()).size(TIER_SIZE * scale).color(tone).boxed()
    };

    let mark: Element<'a, Message> = if selected {
        container(Space::new())
            .width(Length::Fixed(
                word.chars().count() as f32 * TIER_SIZE * INTER_ADVANCE * scale,
            ))
            .height(Length::Fixed(2.0 * scale))
            .style(move |_| container::Style {
                background: Some(colour.into()),
                ..Default::default()
            })
            .boxed()
    } else {
        Space::new().height(2.0 * scale).boxed()
    };

    column![label, Space::new().height(6.0 * scale), mark].boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::components::tui::MONO_ADVANCE;


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
        assert_eq!(custom_read(14, &t), CustomRead::Warning("min 0.10".to_string()));
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
    fn the_tier_row_fits_the_pane() {
        const PANE_CONTENT_W: f32 = 299.0 - 2.0 * crate::ui::components::grid::PANE_PAD_H;
        let words: f32 = BtcFeeTier::ALL
            .iter()
            .map(|t| t.label().chars().count() as f32 * TIER_SIZE * MONO_ADVANCE)
            .sum();
        let widest = words + 4.0 * TIER_GAP;
        assert!(widest <= PANE_CONTENT_W, "the tier row is {widest}px");
    }
}
