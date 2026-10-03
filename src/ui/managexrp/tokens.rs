//! **Tokens** — a page in the Send/Receive shell, with exactly one scroll
//! region: the list.
//!
//! Replaces the floating Tokens modal and its second modal layer (the enable
//! card). The list is the only thing that can grow, so it is the only thing
//! that scrolls; the header strip, the reserve footer and the dock sit outside
//! the `scrollable` and never move. Entered from the XRP dashboard's
//! `tokens · N held ›` row, left by the back chevron — the same door Send,
//! Receive and Chart use.
//!
//! **XRP only.** Bitcoin has no trustlines and no Tokens entry.
//!
//! ## What this page does NOT draw
//!
//! *Logos.* The symbol in Inter over the issuer beneath it is the
//! identification, exactly as Rates does it. (`TokenDef::logo` stays — the
//! send and trade asset pickers still render it; a picker is a target, a list
//! is a reading.)
//!
//! *The trust-limit meter.* It encoded a balance against a 1,000,000 limit —
//! a bar that is always empty. Gone with the `limit 1M` text beside it: every
//! trustline this app opens carries the same limit, so the figure identified
//! nothing. It belongs in the row's own detail, when that exists.
//!
//! *An empty-state paragraph.* XRP is always the first row, so the page is
//! never blank, and `available` directly beneath it is the explanation. The
//! landing-page docs are where heavy text lives.
//!
//! ## The reserve is the ledger's number, never ours
//!
//! Every figure on this page — what a trustline costs, what is already locked,
//! whether `enable` is live — comes from [`crate::utils::reserves::Reserve`]:
//! `reserve_inc` as the validators voted it, crossed with the account's own
//! `owner_count`. Until that frame lands the numbers draw `—` and the links
//! stay dead. **Nothing here is written down**, because the reserve is
//! validator-voted and has already moved once (10 XRP → 1 XRP, Dec 2024).
//!
//! The gate is `available ≥ reserve_inc + open-ledger fee`. The fee half
//! matters: a wallet with exactly one owner reserve free passes a reserve-only
//! check and then cannot pay for the `TrustSet` that spends it.
//!
//! ## Enable signs on the send v3 stack, unchanged
//!
//! A `TrustSet` is a transaction, so it is signed like one:
//! [`send_screen::sign_stack`] verbatim — nothing added, nothing removed. No
//! summary rows: the send screen's own contract is that the stack carries no
//! transaction details, and the row being enabled stays washed and in place
//! behind the scrim. **No error ever renders on the signing surface**; the
//! reserve gate lives on the `enable` link, where it can be seen before the
//! stack opens.

use iced::Widget as _;
use iced::widget::scrollable::{Rail, Scroller};
use iced::widget::{column, text, Space};
use iced::{Alignment, Border, Color, Element};

use crate::controller::message::Message;
use crate::utils::fonts::MONO;
use crate::utils::theme::CompactPalette;

// ── Geometry ────────────────────────────────────────────────────────────────



pub const ROW_PAD_V: f32 = 9.0;

/// The scroller: 3px, thumb on `border`, no rail and no arrows, sitting inside
/// the right padding rather than taking a gutter of its own.
pub const SCROLLER_W: f32 = 3.0;

// ── Type ────────────────────────────────────────────────────────────────────

// The four the holding row is built from — public because the row itself is
// shared with the send screen's asset picker, and a type scale that lives in
// two places is a type scale that drifts.
pub const SYM: f32 = 12.0;
pub const ISSUER: f32 = 10.5;
pub const BALANCE: f32 = 12.5;
pub const FIAT: f32 = 10.5;


// ── The page ────────────────────────────────────────────────────────────────

/// The two columns a holding carries: symbol over issuer left, balance over
/// its fiat approximation right, no unit suffix on either.
///
/// Shared with the send screen's asset picker, which wraps the same pair in a
/// button and a check gutter — the list a person browses here is the list they
/// pick from there, so it is one row design and one implementation. Only the
/// wrapper differs: a reading here, a target there.
pub fn holding_columns<'a>(
    symbol:   &str,
    issuer:   String,
    bal_str:  String,
    fiat_str: String,
    cp:       &'static CompactPalette,
    scale:    f32,
) -> (Element<'a, Message>, Element<'a, Message>) {
    (
        column![
            text(symbol.to_string()).size(SYM * scale).color(cp.text),
            Space::new().height(3.0 * scale),
            text(issuer).size(ISSUER * scale).color(cp.dim),
        ].boxed(),
        column![
            text(bal_str).font(MONO).size(BALANCE * scale).color(cp.text),
            Space::new().height(3.0 * scale),
            text(fiat_str).font(MONO).size(FIAT * scale).color(cp.muted),
        ]
        .align_x(Alignment::End)
        .boxed(),
    )
}

/// A rail that is nothing but its thumb: no track, no border, no arrows.
pub fn thumb_only(thumb: Color) -> Rail {
    Rail {
        background: None,
        border: Border::default(),
        scroller: Scroller {
            background: thumb.into(),
            border: Border { radius: 2.0.into(), ..Border::default() },
        },
    }
}

/// `rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De` → `rMxCK…m5De`. Ends-emphasis, the
/// check people actually perform — the same first/last treatment receive gives
/// the address it displays.
pub fn short_issuer(addr: &str) -> String {
    let n = addr.chars().count();
    if n <= 12 {
        return addr.to_string();
    }
    let head: String = addr.chars().take(5).collect();
    let tail: String = addr.chars().skip(n - 4).collect();
    format!("{head}\u{2026}{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::reserves::Reserve;

    #[test]
    fn the_issuer_is_truncated_at_both_ends() {
        assert_eq!(short_issuer("rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De"), "rMxCK\u{2026}m5De");
        // Short enough to show whole — never truncated INTO something longer.
        assert_eq!(short_issuer("rShort"), "rShort");
    }

    /// The gate is the ledger's price plus the fee, and both halves matter: a
    /// wallet holding exactly one owner reserve can open no line, because the
    /// TrustSet that opens it still has to be paid for.
    #[test]
    fn the_enable_gate_charges_the_reserve_and_the_fee() {
        let r = |available: f64| Reserve {
            base: 1.0,
            per_object: 0.2,
            owner_count: 0,
            total: 1.0,
            available,
        };
        assert!(r(0.5).affords_new_object(0.000012));
        assert!(!r(0.2).affords_new_object(0.000012), "the fee was not charged");
        assert!(r(0.2).affords_new_object(0.0));
    }
}
