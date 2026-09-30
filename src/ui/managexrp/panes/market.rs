//! The `order book` and `depth` panes — the trade screen's two read-only
//! views at pane width, on the same held frame and the same functions.
//!
//! **Book.** Column heads `price (rlusd)` / `amount (xrp)` name the pair,
//! and since 2026-09-11 so does the title row, in plain type after the
//! word — the top bar is where it changes ([`super::pair`]). Ten asks over ten bids around the
//! mid strip, in a scroller; depth bar right-anchored behind the numbers as
//! the row's share of the largest row shown; `index / book` pinned to the
//! foot — the Kraken reference and the book's deviation from it, display
//! only. Under Limit a row is a button that publishes its price; under
//! Market the rows are inert (spec §14.6).
//!
//! **Depth.** The trade spec's chart filling the pane: cumulative bid/ask
//! areas inside the ±1% window the market is classified by, the dashed mid,
//! a dashed `muted` line for the user's limit when there is one, `cum. bids`
//! / `cum. asks` in the foot. No sentence about the order: the chart shows. The
//! title's right slot carries the state (`cumulative · ± 1.00%` or `limit at
//! 1.4260`).

use iced::widget::canvas::Canvas;
use iced::widget::{button, column, container, responsive, row, stack, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow, Size};

use crate::controller::app_state::AppState;
use crate::controller::message::Message;
use crate::controller::xrp::{trade_index_rate, trade_market_pair};
use crate::ui::components::compact;
use crate::ui::components::grid;
use crate::ui::managexrp::xrptrade::{self as trade, depth, disp, ASK_WASH, BID_WASH};
use crate::utils::add_commas;
use crate::utils::fonts::MONO;
use crate::utils::orderbook::{display_levels, fmt_price, mid_and_spread, oriented, DEGENERATE_SPREAD_PCT};
use crate::utils::theme::CompactPalette;

use super::NA;

/// Rows per side in the book.
const SHOW: usize = 10;
/// A book row: mono 10.5, `2.5px 6px`.
const BROW: f32 = 10.5;
const BROW_H: f32 = 17.5;
const BROW_PAD_H: f32 = 6.0;
/// Column heads, the mid strip's and the footer's eyebrows (mono upper).
const EYEBROW: f32 = 8.0;
/// The mid value; the footer's values.
const MID: f32 = 11.0;
const FOOT_VALUE: f32 = 10.0;
/// The depth chart's axis and footer.
const AXIS: f32 = 8.0;

fn fmt_amount(a: f64) -> String {
    if a >= 1000.0 { add_commas(a.round() as i64) } else if a >= 100.0 { format!("{a:.0}") } else { format!("{a:.2}") }
}

fn fmt_xrp(a: f64) -> String {
    if a >= 1000.0 { add_commas(a.round() as i64) } else { format!("{a:.0}") }
}

// ── Book ────────────────────────────────────────────────────────────────────

/// No market is held yet (the top bar's pair, [`super::pair`]): both views
/// say so the way an empty list does.
fn no_pair(state: &AppState) -> bool {
    state.trade_pay_asset.is_empty() || state.trade_receive_asset.is_empty()
}

pub fn book<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if no_pair(state) {
        return super::empty("No pair selected", cp, scale);
    }
    responsive(move |size: Size| {
        let inner = size.width.max(1.0);
        let (base, quote) = trade_market_pair(state);
        let (book, _) = oriented(base, quote).unwrap_or_default();
        let asks = display_levels(&book.asks, true, SHOW);
        let bids = display_levels(&book.bids, false, SHOW);
        let clickable = trade::manual(state);
        let max_amount = asks.iter().chain(bids.iter()).map(|&(_, a, _)| a).fold(0.0_f64, f64::max).max(f64::MIN_POSITIVE);

        let eyebrow = |s: String, ink: Color| text(s).font(MONO).size(EYEBROW * scale).color(ink);

        let header = container(
            row![
                eyebrow(format!("price ({})", disp(quote)).to_uppercase(), cp.muted),
                Space::new().width(Length::Fill),
                eyebrow(format!("amount ({})", disp(base)).to_uppercase(), cp.muted),
            ]
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding::new(0.0).left(BROW_PAD_H * scale).right(BROW_PAD_H * scale).bottom(5.0 * scale));

        let level_row = |price: f64, amount: f64, ask: bool| -> Element<'a, Message> {
            let ink = if ask { cp.red } else { cp.green };
            let wash = if ask { ASK_WASH } else { BID_WASH };
            let bar_w = ((amount / max_amount) as f32).clamp(0.0, 1.0) * inner;
            let bar = container(
                row![
                    Space::new().width(Length::Fill),
                    container(Space::new())
                        .width(Length::Fixed(bar_w))
                        .height(Length::Fixed((BROW_H - 2.0) * scale))
                        .style(move |_| container::Style {
                            background: Some(wash.into()),
                            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (2.0 * scale).into() },
                            ..Default::default()
                        }),
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fixed(BROW_H * scale))
            .align_y(Alignment::Center);
            let words = container(
                row![
                    text(fmt_price(price)).font(MONO).size(BROW * scale).color(ink),
                    Space::new().width(Length::Fill),
                    text(fmt_amount(amount)).font(MONO).size(BROW * scale).color(cp.dim),
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fixed(BROW_H * scale))
            .align_y(Alignment::Center)
            .padding(Padding::new(0.0).left(BROW_PAD_H * scale).right(BROW_PAD_H * scale));
            button(stack![bar, words])
                .width(Length::Fill)
                .padding(Padding::ZERO)
                .on_press_maybe(clickable.then(|| Message::TradeBookPriceClicked(format!("{price:.6}"))))
                .style(move |_, status| {
                    let hot = clickable && matches!(status, button::Status::Hovered | button::Status::Pressed);
                    button::Style {
                        background: Some(if hot { cp.hover } else { Color::TRANSPARENT }.into()),
                        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (3.0 * scale).into() },
                        text_color: cp.text,
                        shadow: Shadow::default(),
                        snap: false,
                    }
                })
                .into()
        };

        let mut ladder = column![].width(Length::Fill);
        for &(p, a, _) in asks.iter().rev() {
            ladder = ladder.push(level_row(p, a, true));
        }
        let mid = mid_and_spread(&book);
        let (mid_str, spread_str) = match mid {
            Some((m, s)) if s >= 0.0 && s <= DEGENERATE_SPREAD_PCT => (fmt_price(m), format!("{s:.2}%")),
            Some((_, s)) if s < 0.0 => (NA.to_string(), "crossed".to_string()),
            Some(_) => (NA.to_string(), "wide".to_string()),
            None => (NA.to_string(), "one-sided".to_string()),
        };
        ladder = ladder.push(
            column![
                Space::new().height(3.0 * scale),
                compact::hairline(cp.rule),
                container(
                    row![
                        eyebrow("MID".to_string(), cp.muted),
                        Space::new().width(6.0 * scale),
                        text(mid_str).font(MONO).size(MID * scale).color(cp.text),
                        Space::new().width(14.0 * scale),
                        eyebrow("SPREAD".to_string(), cp.muted),
                        Space::new().width(6.0 * scale),
                        eyebrow(spread_str, cp.muted),
                    ]
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .padding(Padding::new(0.0).top(6.0 * scale).bottom(6.0 * scale)),
                compact::hairline(cp.rule),
                Space::new().height(3.0 * scale),
            ]
            .width(Length::Fill),
        );
        for &(p, a, _) in bids.iter() {
            ladder = ladder.push(level_row(p, a, false));
        }

        let list: Element<'a, Message> = if asks.is_empty() && bids.is_empty() {
            container(text("waiting for the book\u{2026}").size(super::ISSUER * scale).color(cp.muted))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into()
        } else {
            super::scroller(ladder.into(), cp, scale)
        };

        // ── index / book — pinned ─────────────────────────────────────────
        let index = trade_index_rate(state);
        let value = |s: String| text(s).font(MONO).size(FOOT_VALUE * scale).color(cp.dim);
        let mut foot = row![eyebrow("INDEX".to_string(), cp.muted), Space::new().width(6.0 * scale)].align_y(Alignment::Center);
        if index > 0.0 && base != quote {
            foot = foot.push(value(fmt_price(index)));
            if let Some((m, s)) = mid {
                if s >= 0.0 && s <= DEGENERATE_SPREAD_PCT {
                    let dev = (m - index) / index * 100.0;
                    foot = foot
                        .push(Space::new().width(14.0 * scale))
                        .push(eyebrow("BOOK".to_string(), cp.muted))
                        .push(Space::new().width(6.0 * scale))
                        .push(value(format!("{dev:+.2}%")));
                }
            }
        } else {
            foot = foot.push(value(NA.to_string()));
        }
        let footer = column![
            compact::hairline(cp.rule),
            container(foot)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .padding(Padding::new(0.0).top(8.0 * scale)),
        ]
        .width(Length::Fill);

        column![header, list, footer]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    })
    .into()
}

// ── Depth ───────────────────────────────────────────────────────────────────

/// The Limit ticket's typed price, if it is one.
fn typed_limit(state: &AppState) -> Option<f64> {
    trade::manual(state)
        .then(|| state.trade_limit_price.trim().parse::<f64>().ok())
        .flatten()
        .filter(|p| *p > 0.0)
}

/// The window the chart draws for this pair, percent of mid either side —
/// the book's own width ([`depth::window_pct`]), which the title and the
/// chart must agree on.
fn depth_window(state: &AppState) -> (Option<f64>, f64) {
    let (base, quote) = trade_market_pair(state);
    let (book, _) = oriented(base, quote).unwrap_or_default();
    let mid = mid_and_spread(&book).map(|(m, _)| m).filter(|m| *m > 0.0);
    let pct = depth::window_pct(&book.bids, &book.asks, mid.unwrap_or(0.0), typed_limit(state));
    (mid, pct)
}

/// The title's right slot: the limit when a Limit ticket has one, else the
/// window the chart draws — the width the book needed, not a constant.
pub fn depth_meta(state: &AppState) -> String {
    if no_pair(state) {
        return String::new();
    }
    match typed_limit(state) {
        Some(p) => format!("limit at {}", fmt_price(p)),
        None => format!("cumulative \u{b7} \u{b1} {:.2}%", depth_window(state).1),
    }
}

pub fn depth<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if no_pair(state) {
        return super::empty("No pair selected", cp, scale);
    }
    let (base, quote) = trade_market_pair(state);
    let (book, _) = oriented(base, quote).unwrap_or_default();
    let (mid, pct) = depth_window(state);
    let (lo, hi) = mid
        .map(|m| (m * (1.0 - pct / 100.0), m * (1.0 + pct / 100.0)))
        .unwrap_or((0.0, 0.0));
    let cumulative = |levels: &[(f64, f64)], asks: bool| -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        let mut c = 0.0;
        for &(p, a) in levels {
            if p <= 0.0 || a <= 0.0 { continue; }
            if (asks && p > hi) || (!asks && p < lo) { break; }
            c += a;
            out.push((p, c));
        }
        out
    };
    let bids = if mid.is_some() { cumulative(&book.bids, false) } else { Vec::new() };
    let asks = if mid.is_some() { cumulative(&book.asks, true) } else { Vec::new() };
    let cum_bids = bids.last().map(|l| l.1).unwrap_or(0.0);
    let cum_asks = asks.last().map(|l| l.1).unwrap_or(0.0);
    // Inside the window by construction unless the cap cut it off.
    let limit_mark = typed_limit(state).filter(|p| *p >= lo && *p <= hi);

    grid::fill_or_scroll(
        DEPTH_MIN_H,
        move || depth_body(bids.clone(), asks.clone(), lo, hi, mid, limit_mark, cum_bids, cum_asks, cp, scale),
        cp,
        scale,
    )
}

/// The least the depth pane fills before it scrolls: a legible chart, the
/// axis, the footer.
const DEPTH_MIN_H: f32 = 150.0;

#[allow(clippy::too_many_arguments)]
fn depth_body<'a>(
    bids: Vec<(f64, f64)>,
    asks: Vec<(f64, f64)>,
    lo: f64,
    hi: f64,
    mid: Option<f64>,
    limit_mark: Option<f64>,
    cum_bids: f64,
    cum_asks: f64,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let chart: Element<'a, Message> = Canvas::new(depth::DepthChart {
        bids,
        asks,
        lo,
        hi,
        limit: limit_mark,
        bid: cp.green,
        ask: cp.red,
        base: cp.rule,
        mid_line: cp.border_soft,
        marker: cp.muted,
        tag: cp.dim,
        scale,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into();

    let eyebrow = |s: String, ink: Color| text(s).font(MONO).size(AXIS * scale).color(ink);
    let axis = container(
        row![
            eyebrow(if lo > 0.0 { fmt_price(lo) } else { NA.to_string() }, cp.muted),
            Space::new().width(Length::Fill),
            eyebrow(mid.map(|m| format!("mid {}", fmt_price(m))).unwrap_or_else(|| format!("mid {NA}")), cp.dim),
            Space::new().width(Length::Fill),
            eyebrow(if hi > 0.0 { fmt_price(hi) } else { NA.to_string() }, cp.muted),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(7.0 * scale).left(2.0 * scale).right(2.0 * scale));

    // No sentence about the order (user, 2026-09-09): the chart is a pane
    // that may not even be open beside the ticket, and reading it for the
    // user is the landing-page docs' job — the chart shows, it does not tell.
    let col = column![
        container(chart).width(Length::Fill).height(Length::Fill).padding(Padding::new(0.0).top(2.0 * scale)),
        axis,
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    let foot = |key: &'static str, value: String, ink: Color, right: bool| -> Element<'a, Message> {
        let mut c = column![
            eyebrow(key.to_string(), cp.muted),
            Space::new().height(3.0 * scale),
            text(value).font(MONO).size(FOOT_VALUE * scale).color(ink),
        ];
        if right {
            c = c.align_x(Alignment::End);
        }
        c.into()
    };
    let footer = column![
        Space::new().height(8.0 * scale),
        compact::hairline(cp.rule),
        container(
            row![
                foot("CUM. BIDS", format!("{} XRP", fmt_xrp(cum_bids)), cp.green, false),
                Space::new().width(Length::Fill),
                foot("CUM. ASKS", format!("{} XRP", fmt_xrp(cum_asks)), cp.red, true),
            ]
            .align_y(Alignment::End),
        )
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(8.0 * scale).left(2.0 * scale).right(2.0 * scale)),
    ]
    .width(Length::Fill);

    column![col, footer].width(Length::Fill).height(Length::Fill).into()
}
