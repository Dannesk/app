//! The XRP send — **one screen plus the sign stack** (send v3).
//!
//! Replaces the four-step flow (`step1`–`step4`, deleted): recipient, amount
//! and fee are the compose pane's three sections, the old review step is the
//! live pane beside them, and signing floats as a stack over the whole thing.
//! The frame, the review pane and the stack are
//! [`crate::ui::components::send_screen`]'s; this file is only what is XRP
//! about a send — address resolution, the destination tag, the asset picker,
//! the ledger-fixed fee.
//!
//! ## What the gate asks
//!
//! The CTA lights on exactly what the controller re-checks on
//! `SendContinueClicked`: a resolvable address that isn't this wallet's own, a
//! u32 tag (or none, or one baked into the X-address), an amount above zero,
//! and amount + fee inside what the reserve leaves. Nothing else — every
//! failure past `Broadcast to network` is the activity log's.
//!
//! ## The asset picker
//!
//! A stack over this screen, opened from the chip in the crypto box: a choice
//! *about* a field, made and dismissed without leaving it. It wears the sign
//! stack's panel and **Tokens' rows** — symbol over issuer, balance over its
//! fiat approximation — so the list a person browses on the Tokens page is the
//! list they pick from here. No logos, no close glyph: the scrim and the back
//! chevron dismiss it, and the chip that opened it stays lit behind the scrim.
//!
//! **It chooses an asset. That is all it does.** The balances on it are total
//! balances, the same figures the Tokens page shows — no reserve arithmetic,
//! no fee, no gate. Gating is the compose pane's, against available balance,
//! and the fee is stated once in the fee section. A trustline at zero is not
//! listed: there is nothing to send, and a row that has to be disabled is
//! worse than a row that isn't there.
//!
//! Picking a token resolves the whole screen: the title takes its name, the
//! fiat box drops out (the amount already is that), the crypto box takes the
//! full width, and review swaps the fiat row for the issuer.

use iced::widget::Column;
use iced::Widget as _;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::channel::CHANNEL;
use crate::controller::app_state::XrpTokenTab;
use crate::controller::message::Message;
use crate::utils::money;
use crate::ui::components::compact;
// The Tokens page owns the holding row — its columns, its issuer shortening,
// its scroller. The picker borrows all three rather than growing a second copy
// that drifts away from it.
use crate::ui::managexrp::panes;
use crate::ui::managexrp::tokens as tokens_page;
use crate::ui::components::grid;
use crate::utils::{format_token_amount, theme, tokens};

/// What the fee is paid in, whatever is being sent — the drops come out of the
/// XRP balance even when the payment is a token.

// ── What is wrong with a recipient ─────────────────────────────────────

pub(crate) const SELF_ERROR: &str = "that is this wallet's own address";

/// The amount line's verdict when the amount (plus the fee, for XRP) exceeds
/// what the account can send. Kept in step with the BTC twin's wording.
pub(crate) const OVER_AVAILABLE: &str = "more than available";
/// A token amount that fits, with no XRP left to pay the network fee.
pub(crate) const NO_XRP_FOR_FEE: &str = "no xrp available for the network fee";
pub(crate) const PREFIX_ERROR: &str = "an xrp address starts with r, or X for an x-address";
/// Said when the address decodes but its check digits do not match — which is
/// what a mistyped or edited character looks like, and the only thing a
/// checksum exists to catch. It names the checksum rather than saying the
/// address is "invalid": the checksum is the *reason*, and someone who has
/// edited a character needs to be told which of their assumptions is wrong,
/// not merely that something is.
pub(crate) const CHECKSUM_ERROR: &str = "that address fails its checksum";

/// The shortest a classic address can come out of base58check, and the
/// longest. Below the floor an address is *unfinished*, not wrong.
const CLASSIC_MIN: usize = 25;
/// An X-address is a fixed 31-byte payload — 46 or 47 characters.
const XADDRESS_MIN: usize = 46;

/// What is wrong with `addr`, or `None` while it is good — **or still being
/// typed**.
///
/// The two verdicts are separate, the way BTC's `is_plausible` separates them.
/// A string that can never become an address is wrong from the first
/// character. One that is merely short is unfinished and says nothing: every
/// prefix of a perfectly good address fails the checksum, so a red line during
/// typing would be an error in the middle of an answer.
///
/// Nothing gates on this — [`xrpl_codec::xaddress::resolve`] is still the
/// only authority on whether a payment can go — it only decides which line
/// sits under the field. A dark button with no reason under it is the fault
/// this exists to fix: an edited address fails its checksum, which is exactly
/// what a checksum is for, and the screen used to keep that to itself.
pub(crate) fn recipient_fault(addr: &str) -> Option<&'static str> {
    let addr = addr.trim();
    if addr.is_empty() {
        return None;
    }
    let n = addr.chars().count();
    let floor = if addr.starts_with('X') {
        XADDRESS_MIN
    } else if addr.starts_with('r') {
        CLASSIC_MIN
    } else {
        return Some(PREFIX_ERROR);
    };
    if n < floor {
        return None;
    }
    xrpl_codec::xaddress::resolve(addr)
        .is_none()
        .then_some(CHECKSUM_ERROR)
}

/// The asset the unit shows: the registry code is `EUROP`, the display form
/// `EURØP`.
pub(crate) fn asset_display(tab: &XrpTokenTab) -> &'static str {
    match *tab {
        XrpTokenTab::Xrp => "XRP",
        XrpTokenTab::Token(c) => tokens::by_code(c).map(|t| t.display).unwrap_or(c),
    }
}

// ── The asset picker ────────────────────────────────────────────────────────
// A FACE of the send pane (2026-09-15) — the Tokens page's rows, one under
// the other, swapped in for the form when the chip is clicked and out again
// on a pick or `Cancel`. It was a 384-wide panel floated over the grid on
// the scrim; the user: every other pane swaps its own face, and this was
// the one modal opened from inside a pane.

/// The check gutter, reserved by every row whether it is picked or not, so the
/// numeric column never shifts between rows.
const CHECK_W: f32 = 9.0;
const CHECK: f32 = 10.0;
/// Between a row's text columns and its check.
const ROW_GAP: f32 = 14.0;

/// One row of the picker: an asset there is something to send of.
pub(crate) struct Holding {
    tab:      XrpTokenTab,
    symbol:   &'static str,
    /// The line under the symbol: `Ripple USD · rMxCK…v9Ub`, or what XRP is.
    issuer:   String,
    /// TOTAL balance — never available: no reserve arithmetic, no fee, no
    /// gate. The picker chooses an asset; the compose pane does the gating.
    balance:  f64,
    rate_key: &'static str,
}

/// XRP first, then the registry in its own order — never sorted by value,
/// which would move rows out from under the cursor as prices tick. A
/// trustline at zero is not listed at all: there is nothing to send of it,
/// and a row that must be disabled is worse than a row that isn't drawn.
/// XRP is always listed; it is the pane's default and what the fee is in.
pub(crate) fn holdings() -> Vec<Holding> {
    let (xrp_balance, _, _, _) = *CHANNEL.wallet_balance_rx.borrow();
    let mut out = vec![Holding {
        tab: XrpTokenTab::Xrp,
        symbol: "XRP",
        issuer: "native asset".to_string(),
        balance: xrp_balance,
        rate_key: "XRP",
    }];
    for t in tokens::TOKENS.iter() {
        let (balance, held, _) = CHANNEL.token(t.code);
        if held && balance > 0.0 {
            out.push(Holding {
                tab: XrpTokenTab::Token(t.code),
                symbol: t.display,
                issuer: format!("{} \u{b7} {}", t.issuer_name, tokens_page::short_issuer(t.issuer)),
                balance,
                rate_key: t.rate_key,
            });
        }
    }
    out
}

/// The face: the held assets as pickable rows, a hairline between them, and
/// `Cancel` pinned under the list. The click is the decision — the
/// controller swaps the form back on `XrpTokenTabChanged`.
pub fn asset_face<'a>(
    selected: &XrpTokenTab,
    ccy: &'static str,
    cp: &'static theme::CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let held = holdings();
    let last = held.len().saturating_sub(1);
    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for (i, h) in held.iter().enumerate() {
        list = list.push(picker_row(h, &h.tab == selected, ccy, cp, scale));
        if i != last {
            list = list.push(compact::hairline(cp.rule));
        }
    }
    column![
        grid::scroller(list.boxed(), cp, scale),
        Space::new().height(12.0 * scale),
        grid::quiet_button("Cancel", Message::SendAssetPickerToggled, cp, scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

/// One pickable row: the Tokens page's two columns, a reserved check gutter,
/// and the whole 384-wide band as the button. Selection is `pill` + the check
/// — the wash alone is too close to `hover` in the dark themes, and the check
/// alone is too quiet at row scale.
///
/// There is no confirm: the click is the decision, and the controller closes
/// the stack on `XrpTokenTabChanged`.
fn picker_row<'a>(
    h:        &Holding,
    selected: bool,
    ccy:      &'static str,
    cp:       &'static theme::CompactPalette,
    scale:    f32,
) -> Element<'a, Message> {
    // Six places at most, the same readout as the balance and tokens panes;
    // the tail past six is `max ›`'s to spend, not the row's to print.
    let bal_str = format_token_amount(h.balance, 6);
    let fiat_str = format!(
        "\u{2248} {} {ccy}",
        money(h.balance * crate::utils::price::cross(h.rate_key, ccy)),
    );
    let (left, right) =
        tokens_page::holding_columns(h.symbol, h.issuer.clone(), bal_str, fiat_str, cp, scale);

    let check: Element<'a, Message> = if selected {
        // Inter's check — JetBrains Mono has none.
        text("\u{2713}").size(CHECK * scale).color(cp.green).boxed()
    } else {
        Space::new().boxed()
    };

    let body = row![
        left,
        Space::new().width(Length::Fill),
        right,
        Space::new().width(ROW_GAP * scale),
        container(check)
            .width(Length::Fixed(CHECK_W * scale))
            .align_x(Alignment::End),
    ]
    .align_y(Alignment::Center);

    button(body)
        .width(Length::Fill)
        .padding(
            Padding::new(0.0)
                .top(tokens_page::ROW_PAD_V * scale)
                .bottom(tokens_page::ROW_PAD_V * scale)
                .left(panes::TOKEN_ROW_PAD_H * scale)
                .right(panes::TOKEN_ROW_PAD_H * scale),
        )
        .on_press(Message::XrpTokenTabChanged(h.tab.clone()))
        .style(move |_, status| {
            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let bg = if selected {
                cp.pill
            } else if hot {
                cp.hover
            } else {
                Color::TRANSPARENT
            };
            button::Style {
                background: Some(bg.into()),
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 0.0.into() },
                text_color: cp.text,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real mainnet address, checksum and all.
    const GOOD: &str = "rPEPPER7kfTD9w2To4CQk6UCfuHM9c6GDY";

    /// The bug this hint exists for: paste a good address, change a character,
    /// and the CTA goes dark — correctly, because the checksum is what catches
    /// exactly that. The screen has to SAY so. Every single-character edit of a
    /// good address must be called out; one in four billion would checksum by
    /// luck, and none of these do.
    #[test]
    fn an_edited_character_is_named_as_a_checksum_failure() {
        assert_eq!(recipient_fault(GOOD), None, "a good address must stay quiet");
        let chars: Vec<char> = GOOD.chars().collect();
        for i in 1..chars.len() {
            let mut edited = chars.clone();
            // Swap to another letter in the ripple alphabet, never to itself.
            edited[i] = if edited[i] == 'p' { 'q' } else { 'p' };
            let edited: String = edited.into_iter().collect();
            assert_eq!(
                recipient_fault(&edited),
                Some(CHECKSUM_ERROR),
                "editing slot {i} of a good address said nothing",
            );
        }
    }

    /// The other half of the rule, and the reason the verdict is not simply
    /// `resolve().is_none()`: every prefix of a good address fails its
    /// checksum, so a red line while someone types would be an error in the
    /// middle of an answer.
    #[test]
    fn a_half_typed_address_says_nothing() {
        assert_eq!(recipient_fault(""), None);
        for n in 1..CLASSIC_MIN {
            let part: String = GOOD.chars().take(n).collect();
            assert_eq!(recipient_fault(&part), None, "spoke at {n} characters");
        }
        // And an X-address is quiet right up to its own floor.
        assert_eq!(recipient_fault(&"X".repeat(XADDRESS_MIN - 1)), None);
        assert_eq!(recipient_fault(&"X".repeat(XADDRESS_MIN)), Some(CHECKSUM_ERROR));
    }

    /// A string that can never become an address is wrong from the first
    /// character — no length floor, nothing to wait for.
    #[test]
    fn a_wrong_first_character_is_wrong_at_once() {
        assert_eq!(recipient_fault("b"), Some(PREFIX_ERROR));
        assert_eq!(recipient_fault("0x1234"), Some(PREFIX_ERROR));
        assert_eq!(recipient_fault("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"), Some(PREFIX_ERROR));
    }

    /// The hint may never contradict the gate: nothing `resolve` accepts is
    /// ever painted red, and the CTA is `resolve`'s alone.
    #[test]
    fn the_hint_never_argues_with_the_gate() {
        for addr in [GOOD, "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De"] {
            assert!(xrpl_codec::xaddress::resolve(addr).is_some(), "{addr} is not a good address");
            assert_eq!(recipient_fault(addr), None);
        }
    }
    use crate::ui::components::send_screen::COMPOSE_W;
    use crate::ui::components::tui::MONO_ADVANCE;

    /// A full u32 destination tag fits the tag box.
    #[test]
    fn the_tag_fits_its_box() {
        let advance = compact::FIELD * MONO_ADVANCE;
        let cols = ((COMPOSE_W - 16.0) / advance).floor();
        assert!(u32::MAX.to_string().chars().count() as f32 <= cols);
    }
}
