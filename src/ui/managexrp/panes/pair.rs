//! The top bar's pair control (design handoff `Pair selection`, 2026-09-11).
//!
//! **One pair per chain tab, chosen in one place.** The XRP grid holds a
//! single market — `trade_pay_asset` / `trade_receive_asset` in the engine's
//! orientation, [`trade_market_pair`] in the trader's — and the top bar is
//! the only control that sets it. The `ticket`, `order book` and `depth`
//! panes read it and print it in their title rows as plain type; none of
//! them holds a pair of its own, and none of them opens the search. That is
//! the whole reason it moved up here: a book or depth pane opened without
//! the ticket on screen used to say `No pair selected` with no way to fix
//! it short of adding the ticket back (user, 2026-09-11).
//!
//! Three states, in order: the **chip** — an outlined pill after the chain
//! name, no glyph, no `×` (once a pair is picked there is no way back to
//! none; you switch, you do not clear); the **field** it becomes when
//! pressed — the search glyph riding at its right, nothing under it until
//! something is typed; and the **results** — the picker's rows, measured
//! depth and all, from the first keystroke. Enter takes the first row;
//! a click anywhere under the bar, or `Esc`, closes the field. On first run
//! no pair is held and the field simply shows — the one thing on screen
//! asking to be used.
//!
//! The book gate is the wallet, not this control ([`sync_trade_books`]):
//! picking a pair subscribes nothing, so the panes redraw on the held frame
//! the moment the pick lands.
//!
//! [`sync_trade_books`]: crate::controller::xrp::sync_trade_books

use iced::widget::{button, column, container, mouse_area, opaque, row, svg, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow, Vector};

use crate::controller::app_state::AppState;
use crate::controller::message::{Message, PlainField};
use crate::controller::panes::{self, GridMsg};
use crate::controller::xrp::trade_market_pair;
use crate::ui::components::compact;
use crate::ui::components::tui::MONO_ADVANCE;
use crate::ui::managexrp::tokens as tokens_page;
use crate::ui::managexrp::xrptrade::disp;
use crate::ui::managexrp::xrptrade::picker;
use crate::utils::fonts::MONO;
use crate::utils::icons;
use crate::utils::liquidity;
use crate::utils::orderbook::fmt_price;
use crate::utils::plain_field::plain_field_grid_ink_focus;
use crate::utils::theme::CompactPalette;
use crate::utils::tokens;

use super::{token_row, NA, STRIP};
use crate::ui::components::grid::scroller_hug;

// ── Type scale ──────────────────────────────────────────────────────────────

/// The chip's label and the field's text (mono).
const LABEL: f32 = 10.0;
/// A pane's pair line, after its title (mono, not upper).
pub const PANE_LINE: f32 = 9.5;
/// The results rows' right column.
const MID: f32 = 12.5;
const SECOND: f32 = 10.5;

// ── Geometry ────────────────────────────────────────────────────────────────

/// Chip and field: 22 tall, radius 5 — `panels +`'s height.
const CONTROL_H: f32 = 22.0;
const RADIUS: f32 = 5.0;
const CHIP_PAD_H: f32 = 9.0;
const FIELD_W: f32 = 206.0;
const FIELD_PAD_H: f32 = 8.0;
const FIELD_GAP: f32 = 7.0;
/// The search glyph, 11 × 11.
const GLYPH: f32 = 11.0;
/// The results panel: `top 39` from the bar's top edge, left-aligned to the
/// field — the chain word, its padding and the gap — 324 wide, radius 10.
const PANEL_W: f32 = 324.0;
const PANEL_LEFT: f32 = 45.0;
const PANEL_TOP: f32 = 39.0;
const PANEL_RADIUS: f32 = 10.0;
const PANEL_PAD_H: f32 = 12.0;
/// The rows scroll past this rather than run off the window.
const PANEL_MAX_H: f32 = 380.0;
/// Between the pane title and its pair line.
pub const PANE_GAP: f32 = 8.0;

fn no_pair(state: &AppState) -> bool {
    state.trade_pay_asset.is_empty() || state.trade_receive_asset.is_empty()
}

/// The field is up: pressed open over a held pair, or first run with none.
fn field_shown(state: &AppState) -> bool {
    no_pair(state) || state.trade_pair_search_open
}

/// The pair as the bar and the panes print it — `XRP / RLUSD` — or `None`
/// before one is held.
pub fn label(state: &AppState) -> Option<String> {
    if no_pair(state) {
        return None;
    }
    let (base, quote) = trade_market_pair(state);
    Some(format!("{} / {}", disp(base), disp(quote)))
}

// ── The control in the bar ──────────────────────────────────────────────────

/// The slot after the chain name: the chip, or the field it becomes.
pub fn control<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if field_shown(state) {
        field(state, cp, scale)
    } else {
        chip(label(state).unwrap_or_default(), cp, scale)
    }
}

/// An outlined chip: transparent fill, 1px `border`, mono 10 `text`; `hover`
/// fill under a `focus` edge while hovered. No glyph — a caret promises a
/// dropdown, and this is a search. The outline alone says pressable.
fn chip<'a>(pair: String, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(
        container(text(pair).font(MONO).size(LABEL * scale))
            .height(Length::Fixed(CONTROL_H * scale))
            .align_y(Alignment::Center),
    )
    .on_press(Message::Grid(GridMsg::PairSearchOpened))
    .padding(Padding::new(0.0).left(CHIP_PAD_H * scale).right(CHIP_PAD_H * scale))
    .style(move |_, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if hot { cp.hover } else { Color::TRANSPARENT }.into()),
            border: Border { color: if hot { cp.focus } else { cp.border }, width: 1.0, radius: (RADIUS * scale).into() },
            text_color: cp.text,
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .into()
}

/// The field: 206 wide, `field` fill under a `focus` edge, `search pairs`
/// in `faint` until typed into, the glyph at its right in `muted`. Mounts
/// focused. Enter takes the first live row.
fn field<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let s = LABEL * scale;
    let advance = s * MONO_ADVANCE;
    let cols = ((FIELD_W - 2.0 * FIELD_PAD_H - FIELD_GAP - GLYPH) * scale / advance).floor().max(1.0) as usize;
    let (input, _) = plain_field_grid_ink_focus(
        PlainField::TradePairQuery,
        state.trade_pair_query.as_str(),
        cols,
        1,
        "search pairs",
        Some(Message::Grid(GridMsg::PairSearchSubmitted)),
        advance,
        s * 1.6,
        s,
        MONO,
        cp.text,
        cp.text,
        cp.faint,
        true,
    );
    let glyph = svg(icons::SEARCH.clone())
        .width(Length::Fixed(GLYPH * scale))
        .height(Length::Fixed(GLYPH * scale))
        .style(move |_, _| svg::Style { color: Some(cp.muted) });

    container(
        row![
            container(input).width(Length::Fixed(cols as f32 * advance)),
            Space::new().width(Length::Fill),
            glyph,
        ]
        .spacing(FIELD_GAP * scale)
        .align_y(Alignment::Center),
    )
    .width(Length::Fixed(FIELD_W * scale))
    .height(Length::Fixed(CONTROL_H * scale))
    .align_y(Alignment::Center)
    .padding(Padding::new(0.0).left(FIELD_PAD_H * scale).right(FIELD_PAD_H * scale))
    .style(move |_| container::Style {
        background: Some(cp.field.into()),
        border: Border { color: cp.focus, width: 1.0, radius: (RADIUS * scale).into() },
        ..Default::default()
    })
    .into()
}

// ── The layer under the bar ─────────────────────────────────────────────────

/// The screen under the bar while the field is up: a click anywhere in it
/// closes the field, and the results panel sits at its top left once there
/// is a query. `None` when there is nothing to dismiss — first run with
/// nothing typed is just the field, and the grid is live under it.
pub fn layer<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Option<Element<'a, Message>> {
    let searching = !state.trade_pair_query.trim().is_empty();
    if !state.trade_pair_search_open && !searching {
        return None;
    }
    let content: Element<'a, Message> = if searching {
        opaque(panel(state, cp, scale))
    } else {
        Space::new().into()
    };
    // The bar itself is not in the area: the field keeps taking clicks and
    // `panels +` keeps working (opening it folds the search).
    let bar = panes::BAR_H * scale + 1.0;
    Some(
        column![
            Space::new().height(Length::Fixed(bar)),
            mouse_area(
                container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Start)
                    .align_y(Alignment::Start)
                    .padding(Padding::new(0.0).top(PANEL_TOP * scale - bar).left(PANEL_LEFT * scale)),
            )
            // `on_release`, NOT `on_press` — this is the fix for the
            // double-click (user, 2026-09-12: "I was typing in search, then I
            // clicked remove wallet… as if the search stack was interfering").
            //
            // This area covers the whole grid below the bar. `mouse_area`
            // calls `shell.capture_event()` on a press but NOT on a release
            // (iced_widget 0.14.2, `mouse_area.rs`), so with `on_press` the
            // first click anywhere on the dashboard was swallowed by the
            // dismisser and only the second reached the button under it.
            // On release the dismiss is published without capturing, so one
            // click both folds the search and works the control beneath.
            //
            // Clicks on the results panel are unaffected: `opaque` captures
            // them, and `mouse_area::update` runs its child first and returns
            // early when the event was captured — so picking a pair never
            // reaches this handler either way.
            .on_release(Message::Grid(GridMsg::PairSearchDismissed)),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
    )
}

/// The results: a head, then the picker's rows — the same rows the ticket
/// used to draw, moved up a level. Measured depth, never a spread rule; a
/// market with no liquidity is listed, says so, and can still be picked
/// (2026-09-15) — and since 2026-09-16 depth gates nothing, not even the
/// Market contract: the ticket prints what the walk finds and the user
/// decides. A Limit order is the user's price — resting on an empty book is
/// how a book stops being empty, and a maker was locked out by the old gate.
fn panel<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let query = state.trade_pair_query.as_str();
    let (rows, _) = picker::listing(query);
    let (cur_base, cur_quote) = trade_market_pair(state);

    let eyebrow = |s: String| text(s.to_uppercase()).font(MONO).size(STRIP * scale).color(cp.muted);
    let head = container(
        row![
            eyebrow(match rows.len() {
                1 => "1 pair".to_string(),
                n => format!("{n} pairs"),
            }),
            Space::new().width(Length::Fill),
            eyebrow("mid \u{b7} market".to_string()),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(9.0 * scale).bottom(6.0 * scale).left(PANEL_PAD_H * scale).right(PANEL_PAD_H * scale));

    let mut list = column![].width(Length::Fill);
    if rows.is_empty() {
        list = list.push(
            container(text("no market found").size(super::ISSUER * scale).color(cp.muted))
                .width(Length::Fill)
                .padding(Padding::new(14.0 * scale)),
        );
    }
    for (b, q) in rows {
        list = list.push(market_row(b, q, b == cur_base && q == cur_quote, cp, scale));
    }

    container(
        column![
            head,
            compact::hairline(cp.rule),
            // `scroller_hug`, not `scroller`: the panel is as tall as the
            // rows it holds, capped at PANEL_MAX_H. `scroller` is Fill-height
            // for pane bodies, and borrowing it here stood the panel at the
            // full 380 on a single match (user, 2026-09-12). Typing `xrp/rl`
            // should drop a two-row panel, not a half-screen one.
            container(scroller_hug(list.into(), cp, scale)).max_height(PANEL_MAX_H * scale),
        ]
        .width(Length::Fill),
    )
    .width(Length::Fixed(PANEL_W * scale))
    .clip(true)
    .style(move |_| container::Style {
        background: Some(cp.panel.into()),
        border: Border { color: cp.border, width: 1.0, radius: (PANEL_RADIUS * scale).into() },
        shadow: Shadow {
            color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.45 },
            offset: Vector::new(0.0, 18.0 * scale),
            blur_radius: 44.0 * scale,
        },
        ..Default::default()
    })
    .into()
}

fn market_row<'a>(base: &'static str, quote: &'static str, selected: bool, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let market = liquidity::market(base, quote);

    let issuer = match (base, quote) {
        ("XRP", t) | (t, "XRP") => tokens::by_code(t)
            .map(|t| format!("{} \u{b7} {}", t.issuer_name, tokens_page::short_issuer(t.issuer)))
            .unwrap_or_default(),
        (a, b) => format!(
            "{} \u{b7} {}",
            tokens::by_code(a).map(|t| t.issuer_name).unwrap_or(a),
            tokens::by_code(b).map(|t| t.issuer_name).unwrap_or(b),
        ),
    };
    // The book's own mid whenever it has one — a trivial book's is still
    // its number; the spread beside it says how much to trust it.
    let (mid_str, mid_ink) = match market {
        Some(m) if m.mid > 0.0 => (fmt_price(m.mid), cp.text),
        _ => (NA.to_string(), cp.faint),
    };
    // One line, the same two WORDS on every measured row — the market's
    // liquidity and its stability over the last ~20 ledgers
    // (`liquidity::label`), the same line the ticket's `Book` row prints,
    // in the same ink (`liquidity::healthy`: green for liquid AND stable —
    // the market worth the name, not a grey one — amber for anything
    // less). No percentage, no size: those are the book and depth panes'
    // to show (user, 2026-09-15). What the Market contract makes of an
    // order is the ticket's `Market` row to say.
    let (second, second_ink, second_size) = match liquidity::healthy(base, quote) {
        Some(good) => (liquidity::label(base, quote), if good { cp.green } else { cp.amber }, SECOND),
        None => ("MEASURING\u{2026}".to_string(), cp.faint, STRIP),
    };
    let right = column![
        text(mid_str).font(MONO).size(MID * scale).color(mid_ink),
        Space::new().height(3.0 * scale),
        text(second).font(MONO).size(second_size * scale).color(second_ink),
    ]
    .align_x(Alignment::End);

    let body = token_row(&format!("{} / {}", disp(base), disp(quote)), issuer, right.into(), false, cp, scale);
    button(body)
        .width(Length::Fill)
        .padding(Padding::ZERO)
        .on_press(Message::TradePairSelected(base.to_string(), quote.to_string()))
        .style(move |_, status| {
            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let bg = if selected { cp.pill } else if hot { cp.hover } else { Color::TRANSPARENT };
            button::Style {
                background: Some(bg.into()),
                border: Border::default(),
                text_color: cp.text,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .into()
}

