//! The `ticket` pane — the trade screen's ticket at 299px.
//!
//! **The pair is not chosen here.** Since 2026-09-11 the top bar holds the
//! one pair control for the whole XRP tab ([`super::pair`]); this pane reads
//! the market it sets, prints it in its title row and in the `market` row
//! of the review, and before one is held draws the same empty state the
//! book and depth panes do. (The search used to sit in an `asset pair`
//! field at the top of this pane, which left a book or depth pane opened
//! without the ticket saying `No pair selected` with no way to fix it.)
//!
//! **The pane is the trade screen's own ticket** — [`ticket_screen::form`],
//! the one implementation both draw: Market / Limit (Market only where a
//! book can be walked), Buy / Sell, the linked fields, time in force under
//! Limit, and the stats as the live review. Then the sign block — key plus
//! 25th word — and `Buy XRP`.

use iced::widget::{column, responsive};
use iced::{Element, Length, Size};

use crate::controller::app_state::{AppState, TradeSide};
use crate::controller::message::{Message, SecureField};
use crate::controller::panes::GridMsg;
use crate::controller::xrp::trade_market_pair;
use crate::ui::components::signing::SignFields;
use crate::ui::managexrp::xrptrade::ticket as ticket_screen;
use crate::ui::managexrp::xrptrade::{BUY_BORDER, BUY_FILL, SELL_BORDER, SELL_FILL};
use crate::utils::theme::CompactPalette;

use super::{primary_tinted, scroller, sign_block, sign_mode};


pub fn view<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.trade_pay_asset.is_empty() || state.trade_receive_asset.is_empty() {
        return super::empty("No pair selected", cp, scale);
    }
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        ticket(state, inner, cp, scale)
    })
    .into()
}

// ── The ticket ──────────────────────────────────────────────────────────────

fn ticket<'a>(state: &'a AppState, inner: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let side = state.trade_side;
    let (base, _) = trade_market_pair(state);
    let sign_msg = Message::Grid(GridMsg::TicketSign);

    let fields = SignFields {
        mode: sign_mode(),
        secret: SecureField::TradePassphrase,
        secret_buf: &state.trade_passphrase,
        secret_reveal: state.trade_passphrase_reveal,
        phrase: SecureField::TradeSeed,
        phrase_buf: &state.trade_seed_text,
        phrase_reveal: state.trade_seed_reveal,
        bip39: SecureField::TradeBip39,
        bip39_buf: &state.trade_bip39,
        bip39_reveal: state.trade_bip39_reveal,
        on_submit: sign_msg.clone(),
    };
    let valid = ticket_screen::valid(state);
    let live = valid && fields.can_submit();
    // Enter in a field signs only once the button would.
    let enter = live.then(|| sign_msg.clone());

    // ── the ticket, verbatim ──────────────────────────────────────────────
    let form = ticket_screen::form(state, inner, enter, cp, scale);

    // ── sign ──────────────────────────────────────────────────────────────
    // The verb is the order: `Buy XRP`, `Sell RLUSD`. It lights only once the
    // credential is in, so "sign" is already said by the button being live.
    //
    // And it wears the side's colour (2026-09-16) — the SAME triple the
    // Buy/Sell segment above it takes, so the two agree by construction.
    // Green commits a buy, red commits a sell; the direction of an
    // irreversible press should be readable without reading the word.
    let verb = ticket_screen::side_label(side, base);
    let (fill, line, ink) = match side {
        TradeSide::Buy => (BUY_FILL, BUY_BORDER, cp.green),
        TradeSide::Sell => (SELL_FILL, SELL_BORDER, cp.red),
    };
    let sign = sign_block(
        &fields,
        &state.trade_passphrase,
        &state.trade_seed_text,
        &state.trade_bip39,
        valid,
        primary_tinted(&verb, live, fill, line, ink, sign_msg, cp, scale),
        inner,
        cp,
        scale,
    );

    scroller(column![form, sign].width(Length::Fill).into(), cp, scale)
}
