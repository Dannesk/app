
use iced::widget::Column;
use iced::Widget as _;
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

const SHOW: usize = 10;
const BROW: f32 = 10.5;
const BROW_H: f32 = 17.5;
const BROW_PAD_H: f32 = 6.0;
const EYEBROW: f32 = 8.0;
const MID: f32 = 11.0;
const SPREAD: f32 = 10.0;
const FOOT_VALUE: f32 = 10.0;
const AXIS: f32 = 8.0;
const GUTTER_W: f32 = 9.0;
const GUTTER_GAP: f32 = 5.0;

fn fmt_amount(a: f64) -> String {
    if a >= 1000.0 { add_commas(a.round() as i64) } else if a >= 100.0 { format!("{a:.0}") } else { format!("{a:.2}") }
}

fn fmt_xrp(a: f64) -> String {
    if a >= 1000.0 { add_commas(a.round() as i64) } else { format!("{a:.0}") }
}

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
        let picking = trade::manual(state);
        let reviewing = matches!(state.trade_view, crate::controller::app_state::TradeView::Review(_));
        let clickable = picking && !reviewing;
        let selected = match &state.trade_view {
            crate::controller::app_state::TradeView::Review(q) => q.ticket.limit,
            _ => state.trade_limit_price.trim().parse::<f64>().ok(),
        }
        .filter(|p| *p > 0.0 && picking)
        .map(fmt_price);
        let max_amount = asks.iter().chain(bids.iter()).map(|&(_, a, _)| a).fold(0.0_f64, f64::max).max(f64::MIN_POSITIVE);

        let eyebrow = |s: String, ink: Color| text(s).font(MONO).size(EYEBROW * scale).color(ink);

        let gutter = if picking { GUTTER_W + GUTTER_GAP } else { 0.0 };
        let header = container(
            row![
                eyebrow(format!("price \u{b7} {}", disp(quote)).to_uppercase(), cp.muted),
                Space::new().width(Length::Fill),
                eyebrow(format!("amount \u{b7} {}", disp(base)).to_uppercase(), cp.muted),
            ]
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(
            Padding::new(0.0)
                .left(BROW_PAD_H * scale)
                .right((BROW_PAD_H + gutter) * scale)
                .bottom(5.0 * scale),
        );

        let level_row = |price: f64, amount: f64, ask: bool| -> Element<'a, Message> {
            let ink = if ask { cp.red } else { cp.green };
            let wash = if ask { ASK_WASH } else { BID_WASH };
            let picked = selected.as_deref() == Some(fmt_price(price).as_str());
            let bar_w = ((amount / max_amount) as f32).clamp(0.0, 1.0) * (inner - gutter * scale).max(1.0);
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
            .align_y(Alignment::Center)
            .padding(Padding::new(0.0).right(gutter * scale));
            let mut words_line = row![
                text(fmt_price(price)).font(MONO).size(BROW * scale).color(ink),
                Space::new().width(Length::Fill),
                text(fmt_amount(amount))
                    .font(MONO)
                    .size(BROW * scale)
                    .color(if picked { cp.text } else { cp.dim }),
            ]
            .align_y(Alignment::Center);
            if picking {
                let mark: Element<'a, Message> = if picked {
                    text("\u{2713}").font(MONO).size(9.5 * scale).color(cp.green).boxed()
                } else {
                    Space::new().boxed()
                };
                words_line = words_line.push(
                    container(mark)
                        .width(Length::Fixed((GUTTER_W + GUTTER_GAP) * scale))
                        .align_x(Alignment::End)
                        .boxed(),
                );
            }
            let words = container(words_line)
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
                    let bg = if picked { cp.pill } else if hot { cp.hover } else { Color::TRANSPARENT };
                    button::Style {
                        background: Some(bg.into()),
                        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (3.0 * scale).into() },
                        text_color: cp.text,
                        shadow: Shadow::default(),
                        snap: false,
                    }
                })
                .boxed()
        };

        let mut ladder: Column<Element<'_, Message>> = column![].width(Length::Fill);
        for &(p, a, _) in asks.iter().rev() {
            ladder = ladder.push(level_row(p, a, true));
        }
        let mid = mid_and_spread(&book);
        let (mid_str, spread_str, spread_ink) = match mid {
            Some((m, s)) if s >= 0.0 && s <= DEGENERATE_SPREAD_PCT => (fmt_price(m), format!("{s:.2}%"), cp.dim),
            Some((_, s)) if s < 0.0 => (NA.to_string(), "crossed".to_string(), cp.amber),
            Some(_) => (NA.to_string(), "wide".to_string(), cp.amber),
            None => (NA.to_string(), "one-sided".to_string(), cp.amber),
        };
        let mid_ink = if mid_str == NA { cp.faint } else { cp.text };
        ladder = ladder.push(
            column![
                Space::new().height(3.0 * scale),
                compact::hairline(cp.rule),
                container(
                    row![
                        eyebrow("MID".to_string(), cp.muted),
                        Space::new().width(6.0 * scale),
                        text(mid_str).font(MONO).size(MID * scale).color(mid_ink),
                        Space::new().width(14.0 * scale),
                        eyebrow("SPREAD".to_string(), cp.muted),
                        Space::new().width(6.0 * scale),
                        text(spread_str).font(MONO).size(SPREAD * scale).color(spread_ink),
                    ]
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .padding(Padding::new(0.0).top(6.0 * scale).bottom(6.0 * scale)),
                compact::hairline(cp.rule),
                Space::new().height(3.0 * scale),
            ]
            .width(Length::Fill).boxed(),
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
                .boxed()
        } else {
            super::scroller(ladder.boxed(), cp, scale)
        };

        let index = trade_index_rate(state);
        let value = |s: String| text(s).font(MONO).size(FOOT_VALUE * scale).color(cp.dim);
        let mut foot = row![eyebrow("INDEX".to_string(), cp.muted), Space::new().width(6.0 * scale)].align_y(Alignment::Center);
        if index > 0.0 && base != quote {
            foot = foot.push(value(fmt_price(index)).boxed());
            if let Some((m, s)) = mid {
                if s >= 0.0 && s <= DEGENERATE_SPREAD_PCT {
                    let dev = (m - index) / index * 100.0;
                    foot = foot
                        .push(Space::new().width(14.0 * scale).boxed())
                        .push(eyebrow("BOOK".to_string(), cp.muted).boxed())
                        .push(Space::new().width(6.0 * scale).boxed())
                        .push(value(format!("{dev:+.2}%")).boxed());
                }
            }
        } else {
            foot = foot.push(value(NA.to_string()).boxed());
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
            .boxed()
    })
    .boxed()
}

fn typed_limit(state: &AppState) -> Option<f64> {
    trade::manual(state)
        .then(|| state.trade_limit_price.trim().parse::<f64>().ok())
        .flatten()
        .filter(|p| *p > 0.0)
}

fn depth_window(state: &AppState) -> (Option<f64>, f64) {
    let (base, quote) = trade_market_pair(state);
    let (book, _) = oriented(base, quote).unwrap_or_default();
    let mid = mid_and_spread(&book).map(|(m, _)| m).filter(|m| *m > 0.0);
    let pct = depth::window_pct(&book.bids, &book.asks, mid.unwrap_or(0.0), typed_limit(state));
    (mid, pct)
}

pub fn depth_meta(state: &AppState) -> String {
    if no_pair(state) {
        return String::new();
    }
    match typed_limit(state) {
        Some(p) => format!("at {}", fmt_price(p)),
        None => format!("\u{b1} {:.2}%", depth_window(state).1),
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
    let limit_mark = typed_limit(state).filter(|p| *p >= lo && *p <= hi);
    let base_unit = disp(base).to_uppercase();

    grid::fill_or_scroll(
        DEPTH_MIN_H,
        move || {
            depth_body(bids.clone(), asks.clone(), lo, hi, mid, limit_mark, cum_bids, cum_asks, base_unit.clone(), cp, scale)
        },
        cp,
        scale,
    )
}

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
    base_unit: String,
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
    .boxed();

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
        c.boxed()
    };
    let footer = column![
        Space::new().height(8.0 * scale),
        compact::hairline(cp.rule),
        container(
            row![
                foot("CUM. BIDS", format!("{} {}", fmt_xrp(cum_bids), base_unit), cp.green, false),
                Space::new().width(Length::Fill),
                foot("CUM. ASKS", format!("{} {}", fmt_xrp(cum_asks), base_unit), cp.red, true),
            ]
            .align_y(Alignment::End),
        )
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(8.0 * scale).left(2.0 * scale).right(2.0 * scale)),
    ]
    .width(Length::Fill);

    column![col, footer].width(Length::Fill).height(Length::Fill).boxed()
}
