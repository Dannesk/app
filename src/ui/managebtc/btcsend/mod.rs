//! The BTC send's **logic** — what is Bitcoin about a send: the address
//! forms, the fee arithmetic against the coins this wallet would actually
//! spend, the tier resolution, the outlook sentences — plus the tier cell
//! the bump face draws. The screen itself is the `send` pane
//! ([`crate::ui::managebtc::btcdashboard`]); the routed send screen this
//! file used to draw (send v3) went with the old BTC chain on 2026-09-10.
//!
//! There is still **no `max ›`** on this chain: max would be available − fee,
//! and the fee moves with the tier, so a correct max needs recomputing on
//! every tier click. A stale one is worse than none.
//!
//! ## Where the fee comes from
//!
//! One place, [`resolved_fee`], read by the tier row, the review pane, the
//! gate on `Sign transaction` and the controller's own check. The four named
//! tiers are `btc_node_rx.tiers` — the SAME field the dashboard's priority
//! table draws, never a second estimate. When the node hasn't reported them
//! the named tiers go inert rather than inventing a number: a fee is money
//! spent, and a plausible-but-unmeasured one is worse than none (the rule the
//! whole NODE box is built on).
//!
//! The review recomputes on every keystroke — until `Send payment` is
//! pressed, when the controller writes the resolved number into
//! `btc_send_fee` and the review reads **that** ([`committed_fee`]): a named
//! tier is a live quote off a node frame that moves, and the number under
//! the button must be the number that gets signed.

use iced::Widget as _;
use iced::widget::{button, column, container, text, Space};
use iced::{Border, Color, Element, Length, Padding, Shadow};

use crate::btc_script_type::BtcScriptType;
use crate::channel::CHANNEL;
use crate::controller::app_state::{AppState, BtcFeeTier};
use crate::controller::message::Message;
use crate::utils::theme::CompactPalette;
use crate::ws::commands::bitcoin_payment::{self, Utxo};
// The size and fee arithmetic is the core's, beside the signer it must agree with.
pub use crate::ws::commands::bitcoin_payment::{kinds_of, tier_sats_at, vsize_for};

/// This wallet's own scriptPubKey length — what the change output costs.
/// Read from btc.json's `script_type`; a wallet without the field is bc1q.
fn change_spk_len() -> usize {
    crate::btc_script_type::stored().spk_len()
}

/// The eligible coins of the current wallet, in signing's own selection order.
/// Empty when the pushed set hasn't arrived — every consumer here treats
/// "no coins" as "nothing to price against" and falls back to a 1-input shape.
fn eligible() -> Vec<Utxo> {
    let (_, wallet, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
    wallet
        .and_then(|w| bitcoin_payment::eligible_utxos(&w).ok())
        .unwrap_or_default()
}

/// What this wallet can actually sign against right now, in BTC: the sum of
/// the eligible set (confirmed coins + own pending change). This is the
/// compose pane's `available` and the affordability base — NOT the hero
/// balance, which is confirmed-only and can disagree in both directions while
/// coins move.
pub fn available_btc() -> f64 {
    available_sats() as f64 / 1e8
}

/// The same sum in satoshis — what every affordability check compares in.
pub fn available_sats() -> u64 {
    eligible().iter().map(|u| u.amount).sum::<u64>()
}

/// An amount in BTC as whole satoshis. Every "amount plus fee fits" test goes
/// through this rather than adding BTC as `f64`: `0.00006189 + 0.00000041`
/// lands a hair above `0.0000623` in floating point, which is exactly enough
/// to fail an amount typed as exactly `available` minus the fee.
pub fn to_sats(btc: f64) -> u64 {
    (btc * 1e8).round() as u64
}

/// Whether `amount` BTC plus `fee` sats fits the eligible set, in integers.
pub fn fits(amount: f64, fee: u64) -> bool {
    to_sats(amount).saturating_add(fee) <= available_sats()
}

/// The recipient's scriptPubKey length, for output sizing. Falls back to the
/// P2WPKH length when the field doesn't parse — the gate holds the send until
/// the address is valid, so the fallback only ever prices the empty-form
/// state.
fn recipient_spk_len(state: &AppState) -> usize {
    dannesk_btc_codec::address::Address::parse(state.btc_send_recipient.trim())
        .map(|a| a.script_pubkey().len())
        .unwrap_or(22)
}

/// First-fit input count to reach `target` — the same walk `select_utxos`
/// does at signing time, so the quote and the signed transaction agree on the
/// coin count by construction. `None` = the set cannot cover it.
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

/// One rate priced against the transaction that will ACTUALLY be built.
///
/// Iterates because the fee is part of the selection target: assume one input,
/// price it, re-select at amount+fee, re-price at the real input count —
/// converging in a step or two because the fee is monotone in the count. With
/// no amount typed yet this prices the 1-in/2-out shape, which is what the
/// old constant approximated; the win is every send that needs more inputs.
///
/// A send that leaves no room for a change output falls through to the
/// change-less shape (every coin in, one output), which is smaller and may fit
/// where the two-output one doesn't — so an amount that empties the wallet
/// is priced for the shape that can actually carry it.
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
    // The first `n` coins in selection order, in their families; with no
    // coins on hand, one input of the wallet's own type.
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

/// The lowest fee that is worth broadcasting at all, in satoshis.
///
/// This is `tiers[0]`, which indexd reads from `getmempoolinfo.mempoolminfee` —
/// our node's own live accept threshold, which rises on its own as the mempool
/// fills. Below it our node rejects the transaction outright, so the broadcast
/// is a round trip that cannot succeed. That is the only honest hard gate, and
/// unlike a constant it moves when the network moves.
///
/// With no tiers reported at all, the blind fallback is 1 sat/vB at the real
/// planned size — the long-standing Core default, priced against the same
/// shape everything else here prices.
pub fn floor_sats(state: &AppState) -> u64 {
    tier_quotes(state).map_or_else(|| quote_fee(1.0, state).max(1), |q| q[0].max(1))
}

/// The four named tiers in satoshis, or `None` when the node hasn't reported
/// them.
pub fn tier_quotes(state: &AppState) -> Option<[u64; 4]> {
    let tiers = CHANNEL.btc_node_rx.borrow().tiers?;
    Some([
        quote_fee(tiers[0], state),
        quote_fee(tiers[1], state),
        quote_fee(tiers[2], state),
        quote_fee(tiers[3], state),
    ])
}

/// Whether the whole table has collapsed onto the relay floor.
///
/// This is a real state and not an artefact: indexd reads the tiers at 0.5, 2.5
/// and 5.5 MB of mempool depth (mid-block), so a mempool holding less than half
/// a block's worth has no depth to read them off and every tier is the floor. Four equal numbers
/// mean the chain is quiet — there is genuinely nothing to trade off, and the
/// row says so instead of pretending otherwise.
pub fn tiers_all_at_floor(state: &AppState) -> bool {
    tier_quotes(state).is_some_and(|q| q.iter().all(|s| *s == q[0]))
}

/// The tier actually in force.
///
/// Falls back to `Custom` when the node has no tiers to price the named ones
/// with: the selection in state stays whatever it was, but nothing on screen or
/// in the totals may act on a tier that has no number behind it.
pub fn effective_tier(state: &AppState) -> BtcFeeTier {
    let named = match state.btc_send_fee_tier {
        BtcFeeTier::Custom => return BtcFeeTier::Custom,
        named => named,
    };
    // No tiers to price it with: nothing but a hand-typed fee is available.
    let Some(q) = tier_quotes(state) else { return BtcFeeTier::Custom };
    let Some(i) = named.index() else { return BtcFeeTier::Custom };

    // The selection lands on the CHEAPEST tier that costs what it costs. On a
    // quiet chain the default `med` prices identically to `minimum`, and
    // showing `med` as chosen while `minimum` sits right beside it at the same
    // price claims a distinction that isn't there.
    let price = q[i];
    BtcFeeTier::ALL
        .iter()
        .find(|t| t.index().is_some_and(|j| q[j] == price))
        .copied()
        .unwrap_or(named)
}

/// The fee that will be signed, in satoshis — `None` when there isn't one yet.
///
/// `None` covers both an empty custom field and a named tier the node cannot
/// price. Either way there is no number, and every consumer treats "no number"
/// the same way: nothing to total, nothing to sign.
pub fn resolved_fee(state: &AppState) -> Option<u64> {
    match effective_tier(state) {
        BtcFeeTier::Custom => state.btc_send_fee.trim().parse::<u64>().ok(),
        named => {
            let i = named.index()?;
            tier_quotes(state).map(|q| q[i])
        }
    }
}

/// The fee that was **committed** when `Sign transaction` was pressed, in
/// satoshis.
///
/// The controller writes the resolved number into `btc_send_fee` at that
/// moment and the review pane behind the sign stack reads it from there. A
/// named tier is a live quote off the node frame, and that frame moves —
/// re-resolving it under the scrim would let the number change between the
/// review someone read and the transaction they signed.
pub fn committed_fee(state: &AppState) -> u64 {
    state.btc_send_fee.trim().parse().unwrap_or(0)
}

// ── The recipient's forms ───────────────────────────────────────────────────

/// Whether `addr` is a Bitcoin address this wallet can pay — **the real
/// answer**, from the same parser that builds the output at signing time.
///
/// The SAME test the controller gates on (`controller::btc`), deliberately: a
/// screen that lit the CTA on a rule the controller then rejected would
/// produce an error message for something the user was just told was fine.
/// Legacy `1…` and P2SH `3…` are accepted on purpose: this wallet cannot be
/// *imported* from one, but it has always been able to *pay* one.
///
/// ## Why the crate and not a rule of our own (2026-09-12)
///
/// This used to be a prefix and a length range, which let a mistyped address
/// straight through the gate — the module's own test fixture was a `bc1…`
/// string that fails its checksum, and it passed. Checking properly is not
/// optional arithmetic: base58check carries a four-byte double-SHA256
/// checksum, so a `1…`/`3…` address that fails it was never an address at
/// all, and **there is no risk of refusing a real one**.
///
/// Bech32 is where hand-rolling goes wrong. Witness v0 (`bc1q…`) is checksummed
/// under BIP-173 bech32, and **taproot (`bc1p…`) under BIP-350 bech32m, a
/// different constant** — a validator that knows only bech32 silently rejects
/// every taproot address in existence. The codec's `Address` knows both, along
/// with the witness-program length rules and the network prefix, and it is
/// already what `recipient_spk_len` and `bitcoin_payment` parse with. The gate
/// was the last place still guessing.
///
/// Witness versions 2–16 parse and are accepted. None are in use yet; refusing
/// them helps nobody today and would become a false rejection later.
///
/// The codec is mainnet-only: a `tb1…` (testnet, signet, regtest) is refused
/// rather than parsed, and paying one from mainnet coins is a loss.
pub fn is_valid(addr: &str) -> bool {
    dannesk_btc_codec::address::Address::parse(addr).is_ok()
}

/// The shortest each form can be. Only [`recipient_fault`] uses these, and only
/// to tell "unfinished" from "wrong" — the ceiling is [`is_valid`]'s business
/// now, not a number of ours.
const LEGACY_MIN: usize = 25;
/// A v0 program is 20 or 32 bytes; nothing shorter than this is a bech32
/// address of any version.
const BECH32_MIN: usize = 14;

/// Whether `addr` can still *become* an address — i.e. it is a prefix of one.
///
/// This is what separates "not finished" from "wrong", and it is the only
/// reason the hint line can stay quiet while someone types.
///
/// **Case-insensitive on the bech32 arm**, and that is not a nicety: bech32 is
/// defined case-insensitively precisely so a QR code can carry an address in
/// the compact uppercase alphanumeric mode, so `BC1Q…` off a scanner is a
/// perfectly good address. Matching `"bc1"` alone rejected it — a real address,
/// refused, and the reason this function stopped using `starts_with` directly.
pub(crate) fn is_plausible(addr: &str) -> bool {
    addr.starts_with('1') || addr.starts_with('3') || is_bech32ish(addr) || "bc1".starts_with(addr)
}

/// `bc1…` in either case — what a scanner or a wallet may hand over.
///
/// `get` rather than `addr[..3]`: the field filter admits only ASCII, but this
/// is also called straight off a controller message, and slicing a multi-byte
/// character down the middle panics.
fn is_bech32ish(addr: &str) -> bool {
    addr.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("bc1"))
}

/// Whether two address strings name the same output.
///
/// Not `==`: bech32 is case-insensitive, so `BC1Q…` off a QR code and the
/// `bc1q…` we display are one address written two ways, and the self-send
/// guard has to see through that — it exists to stop someone burning a fee to
/// move money nowhere, and a case difference would walk straight past it.
///
/// Base58 is deliberately left exact. `1Bv…` and `1BV…` are *different*
/// addresses, and folding their case would wrongly accuse someone of paying
/// themselves — or, worse, be trusted somewhere it decides an output.
pub(crate) fn same_address(a: &str, b: &str) -> bool {
    a == b || (is_bech32ish(a) && is_bech32ish(b) && a.eq_ignore_ascii_case(b))
}

/// What is wrong with `addr`, or `None` while it is good — **or still being
/// typed**. The hint line's whole verdict, in one place.
///
/// Three states, and the middle one is the point. **Wrong** from the first
/// character (a prefix no address starts with) → say so at once. **Unfinished**
/// (below the form's floor) → say nothing, because every prefix of a perfectly
/// good address fails its checksum and a red line mid-typing is an error in
/// the middle of an answer. **Full length and still refused** → say so.
///
/// [`is_valid`] alone decides the CTA; this only picks the line under the
/// field, and the two cannot disagree because this asks that one.
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
        // A strict prefix of `bc1` — a character or two in, nothing to judge.
        return None;
    };
    (addr.len() >= floor && !is_valid(addr)).then_some(CHECKSUM_ERROR)
}

// ── Copy ────────────────────────────────────────────────────────────────────

/// The four forms a recipient takes, as the field's placeholder — the whole
/// legend, since nothing sits under the field any more.
pub(crate) const PLACEHOLDER: &str = "bc1q\u{2026}  \u{b7}  bc1p\u{2026}  \u{b7}  1\u{2026}  \u{b7}  3\u{2026}";

/// The amount line's verdict when amount + fee exceeds the eligible set.
pub const OVER_AVAILABLE: &str = "more than available";
pub(crate) const SELF_ERROR: &str = "that is this wallet's own address";
pub(crate) const PREFIX_ERROR: &str = "a bitcoin address starts with 1, 3 or bc1";
/// Said of an address that is long enough to judge and still refused — a
/// mistyped character, a wrong length, a testnet prefix. It names the checksum
/// because that is what catches the overwhelmingly common case, and because
/// "invalid" tells someone nothing about which assumption of theirs is wrong.
/// Below the form's floor nothing is said at all: that address is unfinished,
/// not wrong.
pub(crate) const CHECKSUM_ERROR: &str = "that address fails its checksum";
pub(crate) const NO_TIERS: &str = "no fee rates from the node yet \u{2014} set one yourself";

/// Said when the node has no tiers to place the fee against. It is a statement
/// about our own ignorance, not about the fee.
const UNPRICED: &str = "no estimate yet";

/// The rungs, cheapest first. Each one is a comparison against a rate the node
/// actually reported — never a confirmation time, which is not ours to claim.
const BACK_OF_QUEUE: &str = "back of the queue";
const WELL_UNDER: &str = "well under the going rate";
const JUST_UNDER: &str = "just under the going rate";
const GOING_RATE: &str = "around the going rate";
const ABOVE_GOING: &str = "above the going rate";
const NEXT_BLOCK: &str = "at the next-block rate";
const OVER_THE_TOP: &str = "more than the next block needs";

/// Every tier priced the same, because the mempool is shallower than the block
/// depths indexd reads the tiers off. Not a contradiction and not a fault —
/// there is genuinely one price on offer, and paying more buys nothing.
pub(crate) const QUIET: &str = "every tier costs the same";

/// Where a fee sits, as a sentence beside its price.
///
/// **Derived from the number, never from the tier that was clicked.** A
/// sentence keyed to the tier *name* would tell you `high` is "priced for the
/// next block" and `min` "can sit for days" even on a day when the two cost
/// the same. Every rung is a comparison against a rate that was actually
/// measured; the top rung is the one that earns its keep — past twice the
/// next-block rate there is nothing left to buy, and a fat-fingered extra zero
/// on `custom` otherwise reads exactly like a correct next-block fee.
pub(crate) fn fee_outlook(state: &AppState, sats: u64) -> &'static str {
    let Some(q) = tier_quotes(state) else {
        return UNPRICED;
    };
    // Halfway between two quotes. Saturating because nothing guarantees the
    // array arrives ascending, and a wrapped subtraction here would put the
    // boundary above both ends. Descending, so a collapsed pair resolves to
    // its upper rung rather than to a midpoint that equals both ends.
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

/// The colour of the **selected** tier: green while the fee is at or above the
/// going rate, amber below it — the mock's "the color marks confirmation risk
/// of the selected tier", derived from the number rather than the word.
/// Applied to the tier's word, the rule under it and the sats value at once,
/// because they are one statement. Unselected tiers are never coloured.
pub(crate) fn selected_colour(state: &AppState, sats: Option<u64>, cp: &'static CompactPalette) -> Color {
    match (sats, tier_quotes(state)) {
        (Some(f), Some(q)) if f >= q[2] => cp.green,
        (Some(_), Some(_)) => cp.amber,
        // Nothing to compare against yet — the fee is not a risk we can rate.
        _ => cp.dim,
    }
}

// ── The tier row (the send pane's chips and the bump face's cells) ─────────

/// The custom-fee box: a fee in satoshis is at most a few hundred thousand.
pub const FEE_BOX_W: f32 = 96.0;

/// The gap between tier words, and their type size (Inter).
pub const TIER_GAP: f32 = 15.0;
pub const TIER_SIZE: f32 = 10.5;

/// Roughly what one lowercase Inter glyph advances, as a fraction of the font
/// size — the rule under a selected tier has to be the width of that tier's
/// word, and iced cannot measure text while the view is being built.
pub const INTER_ADVANCE: f32 = 0.52;

/// One tier: the word, and — when it is the chosen one — a 2px rule the width
/// of the word beneath it. The rule reserves its height either way, so picking
/// a tier cannot shift the output line by two pixels.
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

    /// Real mainnet addresses, one of each form the field accepts. Every one
    /// is checksum-verified; a made-up string would fail the parser now, which
    /// is the whole point of the change.
    const REAL: [&str; 5] = [
        "1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2",                                 // P2PKH
        "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy",                                 // P2SH
        "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4",                         // P2WPKH, bech32
        "bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3",     // P2WSH,  bech32
        "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0",     // Taproot, BECH32M
    ];

    /// The gap this closed: the gate counted characters, so a `bc1…` string of
    /// the right length sailed through however mistyped it was. This module's
    /// OWN fixture used to be such a string.
    ///
    /// The taproot row is the one that matters most. It is checksummed under
    /// BIP-350 bech32m, a different constant from BIP-173 bech32 — a validator
    /// that knew only bech32 would refuse every taproot address ever issued,
    /// which is precisely the false rejection hand-rolling invites.
    #[test]
    fn only_real_addresses_pass() {
        for addr in REAL {
            assert!(is_valid(addr), "{addr} is a real address and was refused");
            assert_eq!(recipient_fault(addr), None, "{addr} was painted red");
        }
        // The old fixture: right prefix, plausible length, broken checksum.
        // It passed the length gate and fails now.
        let fake = "bc1qu5g2twq0udg5g09h2u03u54x2ve8zkcum7szm";
        assert!(!is_valid(fake), "a bad checksum still passes the gate");
        assert_eq!(recipient_fault(fake), Some(CHECKSUM_ERROR));
        // Every single-character edit of a real address is caught.
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

    /// Bech32 is case-insensitive by design, so a QR scanner may hand over an
    /// address in the compact uppercase alphanumeric mode. `BC1Q…` is a real
    /// address and must be payable — the old `starts_with("bc1")` refused it.
    /// Mixed case is illegal per BIP-173 and must stay refused.
    #[test]
    fn an_uppercase_bech32_address_is_payable() {
        let lower = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
        let upper = lower.to_uppercase();
        assert!(is_valid(&upper), "an uppercase address off a QR code was refused");
        assert_eq!(recipient_fault(&upper), None);
        assert!(is_plausible("BC1Q"), "an uppercase address reads as wrong while typed");
        // Mixed case is not an address in either direction.
        let mixed = format!("BC1Q{}", &lower[4..]);
        assert!(!is_valid(&mixed));
    }

    /// The self-send guard has to see through bech32's case-insensitivity —
    /// and must NOT see through base58's case-sensitivity, where two spellings
    /// really are two different addresses.
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

    /// A dark CTA is never silent, and a half-typed address is never red.
    /// Each case names its verdict outright, so a change to the rule has to be
    /// written down here too.
    #[test]
    fn a_refused_address_is_never_silent() {
        let cases: Vec<(String, Option<&str>)> = vec![
            // Unfinished — below the floor, so nothing to say yet.
            ("bc1".to_string(), None),
            ("1".repeat(24), None),
            // Long enough to judge, and refused.
            (format!("bc1{}", "q".repeat(90)), Some(CHECKSUM_ERROR)),
            ("1".repeat(40), Some(CHECKSUM_ERROR)),
            ("1".repeat(25), Some(CHECKSUM_ERROR)),
            // Never an address at all — wrong from the first character, and
            // this is also what rejects testnet.
            ("0xdeadbeef".to_string(), Some(PREFIX_ERROR)),
            ("tb1qw508d6qejxtdg4y5r3zarvaryvaxxpcs".to_string(), Some(PREFIX_ERROR)),
        ];
        for (addr, want) in cases {
            assert_eq!(recipient_fault(&addr), want, "{addr:?}");
        }
        // The invariant behind the table, swept: nothing the gate refuses is
        // silent unless it is genuinely unfinished.
        for addr in REAL {
            // Each form has its own floor, and below it silence is correct.
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

    /// Quiet while someone types — the other half of the rule.
    #[test]
    fn the_hint_stays_quiet_until_there_is_something_to_judge() {
        for part in ["", "b", "bc", "bc1", "bc1q", "BC1Q", "1", "3A"] {
            assert_eq!(recipient_fault(part), None, "{part:?} spoke too early");
        }
    }

    /// The amount crosses to the signer as a decimal string and is read back
    /// as `round(f64 × 1e8)` — here in the gate, and identically in
    /// `bitcoin_payment`. That recovers the intended satoshi count exactly
    /// for every amount the chain can hold: the worst relative error of an
    /// `f64` at 2.1×10¹⁵ is under a quarter of a satoshi, and `round` absorbs
    /// it. Sweeps the awkward cases — every-digit fractions, the supply cap,
    /// one sat, and the textbook binary-unfriendly decimals.
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
            // The signer's own expression, verbatim.
            assert_eq!((parsed * 100_000_000.0).round() as u64, sats, "{text} in the blob");
            // And the string the controller writes back is the same number.
            assert_eq!(to_sats(format!("{parsed:.8}").parse().unwrap()), sats, "{text} re-formatted");
        }
        // Exhaustive over the first 10 million satoshi values: every one
        // survives the decimal string and back.
        for sats in (0..10_000_000u64).step_by(7) {
            let text = format!("{:.8}", sats as f64 / 1e8);
            assert_eq!(to_sats(text.parse().unwrap()), sats, "{text}");
        }
    }

    /// The selection walk the quotes run is the same first-fit signing runs,
    /// so the input count a fee was priced at is the count that gets signed.
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

    /// The ladder has to actually move across the `med` → `high` gap, which is
    /// the bug it was built for: with those live at 71 and 444 sats, every fee
    /// in between used to read "around the going rate".
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

    /// Collapsed tiers must resolve upward. When the mempool is shallow enough
    /// that `med == high`, a fee at that price IS the next-block rate — it must
    /// not read "above the going rate" off a midpoint equal to both ends.
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

    /// Sending TO legacy and P2SH works and always has; only *importing* one
    /// doesn't. The formats hint advertises all three, so the validator has to
    /// actually accept all three.
    #[test]
    fn every_advertised_format_validates() {
        // Was a hand-made `bc1…` string here, waved through by a length
        // check. Every row is now a checksum-verified address.
        for addr in REAL {
            assert!(is_valid(addr), "{addr} is not accepted");
        }
        assert!(!is_valid(""));
        assert!(!is_valid("xrp1qqqqqqqqqqqqqqqqqqqqqqqqq"));
    }

    /// Typing an address must not be an error the whole way in — that is the
    /// difference between "not finished" and "wrong".
    #[test]
    fn a_partial_address_is_not_wrong() {
        for prefix in ["b", "bc", "bc1", "bc1q", "bc1qu5g2twq", "1", "3A"] {
            assert!(is_plausible(prefix), "{prefix:?} reads as wrong while still being typed");
        }
        for junk in ["r", "0x1234", "2N1"] {
            assert!(!is_plausible(junk), "{junk:?} can still become an address");
        }
    }

    /// The five tier words plus their gaps fit a default-size pane — the
    /// bump face draws them at 299 − 2 × 12 of padding. Priced with the mono
    /// advance, which over-estimates Inter — if it fits at that width it fits
    /// at the real one.
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

    /// A total is only claimed once both halves exist — a fee with no amount
    /// must not render as if the fee were the total.
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
