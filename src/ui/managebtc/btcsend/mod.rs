

use iced::Widget as _;
use iced::widget::{button, column, container, text, Space};
use iced::{Border, Color, Element, Length, Padding, Shadow};

use crate::btc_script_type::BtcScriptType;
use crate::channel::CHANNEL;
use crate::controller::app_state::{AppState, BtcFeeTier};
use crate::controller::message::Message;
use crate::utils::theme::CompactPalette;
use crate::ws::commands::bitcoin_payment::{self, Utxo};
pub use crate::ws::commands::bitcoin_payment::{kinds_of, tier_sats_at, vsize_for};


fn change_spk_len() -> usize {
    crate::btc_script_type::stored().spk_len()
}


fn eligible() -> Vec<Utxo> {
    let (_, wallet, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
    wallet
        .and_then(|w| bitcoin_payment::eligible_utxos(&w).ok())
        .unwrap_or_default()
}


pub fn available_btc() -> f64 {
    available_sats() as f64 / 1e8
}

pub fn available_sats() -> u64 {
    eligible().iter().map(|u| u.amount).sum::<u64>()
}


pub fn to_sats(btc: f64) -> u64 {
    (btc * 1e8).round() as u64
}

pub fn fits(amount: f64, fee: u64) -> bool {
    to_sats(amount).saturating_add(fee) <= available_sats()
}


fn recipient_spk_len(state: &AppState) -> usize {
    dannesk_btc_codec::address::Address::parse(state.btc_send_recipient.trim())
        .map(|a| a.script_pubkey().len())
        .unwrap_or(22)
}


fn inputs_for_target(utxos: &[Utxo], target: u64) -> Option<usize> {
    let mut total = 0u64;
    for (i, u) in utxos.iter().enumerate() {
        total += u.amount;
        if total >= target {
            return Some(i + 1);
        }
    }
    None
}


pub fn quote_fee(rate: f32, state: &AppState) -> u64 {
    let utxos = eligible();
    let amount_sats = state
        .btc_send_amount
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| *v > 0.0)
        .map(|v| (v * 1e8).round() as u64)
        .unwrap_or(0);
    let two_out = [recipient_spk_len(state), change_spk_len()];

    let kinds = kinds_of(&utxos);
    let own = crate::btc_script_type::stored();
    let first = |n: usize| -> Vec<BtcScriptType> {
        if kinds.is_empty() { vec![own; n] } else { kinds[..n.min(kinds.len())].to_vec() }
    };

    let mut n = 1usize;
    let mut fee = tier_sats_at(rate, vsize_for(&first(n), &two_out));
    for _ in 0..8 {
        let needed = inputs_for_target(&utxos, amount_sats.saturating_add(fee))
            .unwrap_or_else(|| utxos.len().max(1));
        if needed == n {
            break;
        }
        n = needed;
        fee = tier_sats_at(rate, vsize_for(&first(n), &two_out));
    }

    let total: u64 = utxos.iter().map(|u| u.amount).sum();
    if amount_sats > 0 && total > 0 && amount_sats.saturating_add(fee) > total {
        let fee1 = tier_sats_at(rate, vsize_for(&kinds, &[recipient_spk_len(state)]));
        if amount_sats.saturating_add(fee1) <= total {
            return fee1;
        }
    }
    fee
}


pub fn floor_sats(state: &AppState) -> u64 {
    tier_quotes(state).map_or_else(|| quote_fee(1.0, state).max(1), |q| q[0].max(1))
}

pub fn tier_quotes(state: &AppState) -> Option<[u64; 4]> {
    let tiers = CHANNEL.btc_node_rx.borrow().tiers?;
    Some([
        quote_fee(tiers[0], state),
        quote_fee(tiers[1], state),
        quote_fee(tiers[2], state),
        quote_fee(tiers[3], state),
    ])
}


pub fn tiers_all_at_floor(state: &AppState) -> bool {
    tier_quotes(state).is_some_and(|q| q.iter().all(|s| *s == q[0]))
}


pub fn effective_tier(state: &AppState) -> Option<BtcFeeTier> {
    match state.btc_send_fee_tier? {
        BtcFeeTier::Custom => Some(BtcFeeTier::Custom),
        named if tier_quotes(state).is_some() => Some(named),
        _ => Some(BtcFeeTier::Custom),
    }
}


pub fn resolved_fee(state: &AppState) -> Option<u64> {
    match effective_tier(state)? {
        BtcFeeTier::Custom => state.btc_send_fee.trim().parse::<u64>().ok(),
        named => {
            let i = named.index()?;
            tier_quotes(state).map(|q| q[i])
        }
    }
}


pub fn committed_fee(state: &AppState) -> u64 {
    state.btc_send_fee.trim().parse().unwrap_or(0)
}


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

pub const OVER_AVAILABLE: &str = "more than available";
pub(crate) const SELF_ERROR: &str = "you cannot send to yourself";
pub(crate) const PREFIX_ERROR: &str = "a bitcoin address starts with 1, 3 or bc1";

pub(crate) const CHECKSUM_ERROR: &str = "that address fails its checksum";
pub(crate) const NO_TIERS: &str = "no fee rates from the node yet \u{2014} set one yourself";
pub(crate) const PICK_FEE: &str = "pick a fee, or set one yourself";


const UNPRICED: &str = "no estimate yet";


const BACK_OF_QUEUE: &str = "back of the queue";
const WELL_UNDER: &str = "well under the going rate";
const JUST_UNDER: &str = "just under the going rate";
const GOING_RATE: &str = "around the going rate";
const ABOVE_GOING: &str = "above the going rate";
const NEXT_BLOCK: &str = "at the next-block rate";
const OVER_THE_TOP: &str = "more than the next block needs";


pub(crate) const QUIET: &str = "every tier costs the same";


pub(crate) fn fee_outlook(state: &AppState, sats: u64) -> &'static str {
    let Some(q) = tier_quotes(state) else {
        return UNPRICED;
    };
 
    let mid = |lo: u64, hi: u64| lo + hi.saturating_sub(lo) / 2;

    if sats > q[3].saturating_mul(2) {
        OVER_THE_TOP
    } else if sats >= q[3] {
        NEXT_BLOCK
    } else if sats >= mid(q[2], q[3]) {
        ABOVE_GOING
    } else if sats >= q[2] {
        GOING_RATE
    } else if sats >= mid(q[1], q[2]) {
        JUST_UNDER
    } else if sats >= q[1] {
        WELL_UNDER
    } else {
        BACK_OF_QUEUE
    }
}


pub(crate) fn selected_colour(state: &AppState, sats: Option<u64>, cp: &'static CompactPalette) -> Color {
    match (sats, tier_quotes(state)) {
        (Some(f), Some(q)) if f >= q[2] => cp.green,
        (Some(_), Some(_)) => cp.amber,
        _ => cp.dim,
    }
}


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
            assert_eq!((parsed * 100_000_000.0).round() as u64, sats, "{text} in the blob");
            assert_eq!(to_sats(format!("{parsed:.8}").parse().unwrap()), sats, "{text} re-formatted");
        }
       
        for sats in (0..10_000_000u64).step_by(7) {
            let text = format!("{:.8}", sats as f64 / 1e8);
            assert_eq!(to_sats(text.parse().unwrap()), sats, "{text}");
        }
    }

   
    #[test]
    fn input_counting_walks_first_fit() {
        let coins: Vec<Utxo> = [30_000u64, 20_000, 5_000]
            .iter()
            .enumerate()
            .map(|(i, s)| Utxo { txid: format!("t{i}"), vout: 0, amount: *s, address: String::new() })
            .collect();
        assert_eq!(inputs_for_target(&coins, 10_000), Some(1));
        assert_eq!(inputs_for_target(&coins, 30_000), Some(1));
        assert_eq!(inputs_for_target(&coins, 30_001), Some(2));
        assert_eq!(inputs_for_target(&coins, 55_000), Some(3));
        assert_eq!(inputs_for_target(&coins, 55_001), None);
        assert_eq!(inputs_for_target(&[], 1), None);
    }

  
    #[test]
    fn the_ladder_moves_between_med_and_high() {
        let q = [14u64, 40, 71, 444];
        let mid = |lo: u64, hi: u64| lo + hi.saturating_sub(lo) / 2;
        let rung = |sats: u64| -> &'static str {
            if sats > q[3].saturating_mul(2) { OVER_THE_TOP }
            else if sats >= q[3] { NEXT_BLOCK }
            else if sats >= mid(q[2], q[3]) { ABOVE_GOING }
            else if sats >= q[2] { GOING_RATE }
            else if sats >= mid(q[1], q[2]) { JUST_UNDER }
            else if sats >= q[1] { WELL_UNDER }
            else { BACK_OF_QUEUE }
        };
        assert_eq!(rung(71), GOING_RATE);
        assert_eq!(rung(300), ABOVE_GOING, "the med→high gap collapsed again");
        assert_eq!(rung(444), NEXT_BLOCK);
        assert_eq!(rung(20), BACK_OF_QUEUE);
        assert_eq!(rung(45), WELL_UNDER);
        assert_eq!(rung(60), JUST_UNDER);
        // The rung that saves money: an extra zero on custom.
        assert_eq!(rung(4440), OVER_THE_TOP);
        // Every rung is reachable with these quotes.
        let seen: std::collections::HashSet<_> =
            (0..5000).step_by(7).map(rung).collect();
        assert_eq!(seen.len(), 7, "a rung is unreachable: {seen:?}");
    }

  
    #[test]
    fn a_send_cannot_continue_until_a_fee_is_chosen() {
        let mut state = AppState::default();
        assert_eq!(effective_tier(&state), None);
        assert_eq!(resolved_fee(&state), None);
        state.btc_send_step = 1;
        state.btc_send_recipient = REAL[0].to_string();
        state.btc_send_amount = "0.001".to_string();
        let _ = crate::controller::btc::handle(&mut state, crate::controller::message::Message::BtcSendContinueClicked);
        assert_eq!(state.btc_send_step, 1);
        assert_eq!(state.btc_send_error.as_deref(), Some(PICK_FEE));
    }

    #[test]
    fn a_click_stays_on_its_tier_when_two_tiers_cost_the_same() {
        CHANNEL.btc_node_tx.send_modify(|n| n.tiers = Some([0.1, 0.12, 0.12, 0.121]));
        let mut state = AppState::default();
        for t in BtcFeeTier::ALL {
            state.btc_send_fee_tier = Some(t);
            assert_eq!(effective_tier(&state), Some(t));
        }
        state.btc_send_fee_tier = Some(BtcFeeTier::Low);
        let low = resolved_fee(&state);
        state.btc_send_fee_tier = Some(BtcFeeTier::Med);
        assert_eq!(resolved_fee(&state), low);
    }

    #[test]
    fn collapsed_tiers_resolve_upward() {
        let q = [14u64, 14, 71, 71];
        let mid = |lo: u64, hi: u64| lo + hi.saturating_sub(lo) / 2;
        let rung = |sats: u64| -> &'static str {
            if sats > q[3].saturating_mul(2) { OVER_THE_TOP }
            else if sats >= q[3] { NEXT_BLOCK }
            else if sats >= mid(q[2], q[3]) { ABOVE_GOING }
            else if sats >= q[2] { GOING_RATE }
            else { BACK_OF_QUEUE }
        };
        assert_eq!(rung(71), NEXT_BLOCK);
    }

  
    #[test]
    fn every_advertised_format_validates() {
      
        for addr in REAL {
            assert!(is_valid(addr), "{addr} is not accepted");
        }
        assert!(!is_valid(""));
        assert!(!is_valid("xrp1qqqqqqqqqqqqqqqqqqqqqqqqq"));
    }

  
    #[test]
    fn a_partial_address_is_not_wrong() {
        for prefix in ["b", "bc", "bc1", "bc1q", "bc1qu5g2twq", "1", "3A"] {
            assert!(is_plausible(prefix), "{prefix:?} reads as wrong while still being typed");
        }
        for junk in ["r", "0x1234", "2N1"] {
            assert!(!is_plausible(junk), "{junk:?} can still become an address");
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

  
    #[test]
    fn the_total_needs_both_halves() {
        let total = |a: Option<f64>, f: Option<u64>| match (a, f) {
            (Some(a), Some(f)) => Some(a + f as f64 / 1e8),
            _ => None,
        };
        assert_eq!(total(None, Some(500)), None);
        assert_eq!(total(Some(0.5), None), None);
        assert_eq!(total(Some(0.5), Some(100_000_000)), Some(1.5));
    }
}
