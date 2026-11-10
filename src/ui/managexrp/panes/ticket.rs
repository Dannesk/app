
use iced::Widget as _;
use iced::widget::{button, column, container, row, text, Column, Row, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow, Size};

use crate::channel::CHANNEL;
use crate::controller::app_state::{AppState, TradeKind, TradeSide, TradeTif, TradeView};
use crate::controller::message::{Message, PlainField, SecureField};
use crate::controller::panes::{GridMsg, PaneKind};
use crate::controller::xrp::{review_ready, trade_available, trade_decimals, trade_fee_xrp_ok, trade_market_pair, xrp_reserve};
use crate::ui::components::compact;
use crate::ui::components::signing::SignFields;
use crate::ui::components::tui::MONO_ADVANCE;
use crate::ui::components::tx_panel::Seg;
use crate::ui::managexrp::xrpsend::{NO_XRP_FOR_FEE, OVER_AVAILABLE};
use crate::ui::managexrp::xrptrade::disp;
use crate::utils::fonts::MONO;
use crate::utils::liquidity;
use crate::utils::orderbook::fmt_price;
use crate::utils::plain_field::plain_field_grid_ink_focus;
use crate::utils::theme::CompactPalette;
use crate::utils::trade::{TradeQuote, Verdict};
use crate::utils::format_token_amount;

use super::{fee_ink, link, pressable, quiet_button, runs, scroller, sign_block, sign_mode, NA};
use crate::ui::components::grid::{self, fill_or_scroll_by};

const TICKET_MIN_H: f32 = 280.0;
const LIMIT_MIN_H: f32 = 200.0;
const SYM: f32 = 12.0;
const ISSUER: f32 = 10.5;
const ADDR: f32 = 9.5;
const MID: f32 = 12.5;
const SEG: f32 = 11.0;
const STAGE_LABEL: f32 = 9.0;
const FIGURE_SIZES: [f32; 3] = [28.0, 24.0, 20.0];
const COUNTER: f32 = 10.0;
const NOTE: f32 = 9.5;
const ROW_KEY: f32 = 9.5;
const ROW_VALUE: f32 = 10.0;
const ROW_H: f32 = 26.0;
const LINK: f32 = 8.5;
const FLAG: f32 = 8.0;
const TIF_ROW_H: f32 = 30.0;
const CHIP_W: f32 = 40.0;
const CHIP_H: f32 = 20.0;
const CHIP: f32 = 8.5;
const SCROLL_CLEAR: f32 = 7.0;

pub fn view<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.trade_pair_list == Some(PaneKind::Ticket) {
        return super::pair::face(state, cp, scale);
    }
    match &state.trade_view {
        TradeView::Ticket => fill_or_scroll_by(
            TICKET_MIN_H,
            move |size: Size| ticket_face(state, (size.width / scale).max(1.0), cp, scale),
            cp,
            scale,
        ),
        TradeView::Limit => fill_or_scroll_by(
            LIMIT_MIN_H,
            move |size: Size| limit_face(state, (size.width / scale).max(1.0), cp, scale),
            cp,
            scale,
        ),
        TradeView::Review(q) => review_face(state, q, cp, scale),
    }
}

fn tif_code(tif: TradeTif) -> &'static str {
    match tif {
        TradeTif::Gtc => "GTC",
        TradeTif::Ioc => "IOC",
        TradeTif::Fok => "FOK",
    }
}

pub(crate) fn tif_name(tif: TradeTif) -> &'static str {
    match tif {
        TradeTif::Gtc => "Good til Cancelled",
        TradeTif::Ioc => "Immediate or Cancel",
        TradeTif::Fok => "Fill or Kill",
    }
}

fn pad_places(s: &str) -> String {
    match s.split_once('.') {
        Some((whole, frac)) if frac.len() >= 6 => format!("{whole}.{frac}"),
        Some((whole, frac)) => format!("{whole}.{frac:0<6}"),
        None => format!("{s}.000000"),
    }
}

fn signed_pct(v: f64) -> String {
    if v < 0.0 {
        format!("\u{2212}{:.1}%", v.abs())
    } else {
        format!("+{:.1}%", v)
    }
}

fn quote_now() -> Option<TradeQuote> {
    CHANNEL.trade_quote_rx.borrow().clone()
}

fn issuer_segs(base: &str, quote: &str, cp: &'static CompactPalette) -> Vec<Seg> {
    super::pair::issuer_segs(base, quote, cp)
}

fn pair_row<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let (base, quote) = trade_market_pair(state);
    let mid = liquidity::market(base, quote).map(|m| m.mid).filter(|m| *m > 0.0);
    let (mid_str, mid_ink) = match mid {
        Some(m) => (fmt_price(m), cp.text),
        None => (NA.to_string(), cp.faint),
    };
    let left = column![
        row![
            text(format!("{} / {}", disp(base), disp(quote))).size(SYM * scale).color(cp.text),
            text("\u{25be}").font(MONO).size(8.0 * scale).color(cp.faint),
        ]
        .spacing(6.0 * scale)
        .align_y(Alignment::Center),
        Space::new().height(3.0 * scale),
        runs(issuer_segs(base, quote, cp), ADDR * scale, ISSUER * scale),
    ];
    let right = column![
        text(mid_str).font(MONO).size(MID * scale).color(mid_ink),
        Space::new().height(3.0 * scale),
        super::pair::class_words(base, quote, cp, scale),
    ]
    .align_x(Alignment::End);
    let inner = container(
        row![left, Space::new().width(Length::Fill), right]
            .spacing(10.0 * scale)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(7.0 * scale).bottom(8.0 * scale));

    column![
        compact::hairline(cp.rule),
        pressable(inner.boxed(), Some(Message::Grid(GridMsg::PairDoor(PaneKind::Ticket))), cp),
        compact::hairline(cp.rule),
    ]
    .width(Length::Fill)
    .boxed()
}

struct Segment {
    label: &'static str,
    active: bool,
    msg: Option<Message>,
    colour: Option<(Color, Color, Color)>,
}

fn segmented<'a>(items: Vec<Segment>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let mut r: Row<Element<'_, Message>> = row![].spacing(4.0 * scale).width(Length::Fill);
    for s in items {
        let Segment { label, active, msg, colour } = s;
        let live = msg.is_some();
        let (fill, line, ink) = match (active, colour) {
            (true, Some(c)) => c,
            (true, None) => (cp.neutral, cp.border, cp.text),
            (false, _) if !live => (Color::TRANSPARENT, cp.border_soft, cp.faint),
            (false, _) => (Color::TRANSPARENT, cp.border, cp.muted),
        };
        r = r.push(
            button(
                text(label)
                    .size(SEG * scale)
                    .width(Length::Fill)
                    .align_x(Alignment::Center)
                    .color(ink),
            )
            .on_press_maybe(msg)
            .width(Length::Fill)
            .padding(Padding::new(0.0).top(4.0 * scale).bottom(4.0 * scale))
            .style(move |_, status| {
                let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: Some(fill.into()),
                    border: Border { color: line, width: 1.0, radius: (7.0 * scale).into() },
                    text_color: if live && !active && hot { cp.dim } else { ink },
                    shadow: Shadow::default(),
                    snap: false,
                }
            })
            .boxed(),
        );
    }
    r.boxed()
}

#[allow(clippy::too_many_arguments)]
fn figure<'a>(
    field: PlainField,
    value: &'a str,
    room: f32,
    on_submit: Option<Message>,
    red: bool,
    autofocus: bool,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let shown = if value.is_empty() { "0" } else { value };
    let cols = shown.chars().count() + 1;
    let size = FIGURE_SIZES
        .into_iter()
        .find(|s| (cols as f32 + 1.0) * s * MONO_ADVANCE <= room)
        .unwrap_or(FIGURE_SIZES[2]);
    let s = size * scale;
    let advance = s * MONO_ADVANCE;
    let (input, _) = plain_field_grid_ink_focus(
        field,
        value,
        cols,
        1,
        "0",
        on_submit,
        advance,
        s * 1.2,
        s,
        MONO,
        if red { cp.red } else { cp.text },
        cp.text,
        cp.faint,
        None,
        autofocus,
    );
    row![
        Space::new().width(Length::Fixed(advance)),
        container(input).width(Length::Fixed(cols as f32 * advance)),
    ]
    .boxed()
}

struct Gate {
    red: bool,
    label: (&'static str, Color),
    note: Option<(String, Color)>,
}

fn gate(state: &AppState, quote: Option<&TradeQuote>, cp: &'static CompactPalette) -> Gate {
    let typed: f64 = state.trade_amount.trim().parse().unwrap_or(0.0);
    let avail = trade_available(state);
    let fee_ok = quote.map(trade_fee_xrp_ok).unwrap_or(true);
    let broke = typed > 0.0 && quote.is_some() && avail.is_some_and(|a| a <= 0.0);
    let max = quote.and_then(|q| q.max_base).filter(|m| typed > m + 1e-9);
    let over_wallet = max.is_some()
        && match quote.and_then(|q| q.pay.as_deref()).and_then(|p| p.parse::<f64>().ok()) {
            Some(pay) => avail.is_some_and(|a| pay > a + 1e-9),
            None => state.trade_side == TradeSide::Sell && avail.is_some_and(|a| typed > a + 1e-9),
        };
    let resting = state.trade_mode == TradeKind::Limit && state.trade_tif == TradeTif::Gtc;
    let note = match max {
        Some(m) if resting && !over_wallet && !broke => xrp_reserve().map(|r| {
            let (base, _) = trade_market_pair(state);
            (
                format!(
                    "Max {} {}. GTC orders require {} reserve per order",
                    format_token_amount(m, 6),
                    disp(base).to_lowercase(),
                    format_token_amount(r.per_object, 2),
                ),
                cp.amber,
            )
        }),
        _ => None,
    };
    let over_plain = broke || (max.is_some() && note.is_none());
    let label = if over_plain {
        (OVER_AVAILABLE, cp.red)
    } else if !fee_ok && max.is_none() {
        (NO_XRP_FOR_FEE, cp.red)
    } else {
        ("available", cp.dim)
    };
    Gate { red: broke || max.is_some() || !fee_ok, label, note }
}

fn stage_note(state: &AppState, quote: Option<&TradeQuote>, cp: &'static CompactPalette) -> Option<(String, Color)> {
    const LIMIT_ONLY: &str = "Limit only: market orders need XRP on one side";
    if state.trade_mode == TradeKind::Limit {
        let offered = quote.is_some_and(|q| q.market_ok);
        if !offered && quote.is_some() && state.trade_limit_price.trim().is_empty() {
            return Some((LIMIT_ONLY.to_string(), cp.dim));
        }
        return None;
    }
    match quote.map(|q| q.verdict)? {
        Verdict::Walked { coverage_pct, bound_pct, held: false } if coverage_pct < 100.0 - 1e-6 => Some((
            format!("Only {coverage_pct:.0}% fills now \u{b7} {} vs mid", signed_pct(bound_pct)),
            cp.amber,
        )),
        Verdict::Walked { bound_pct, held: true, .. } => Some((
            format!("Off market: {} vs mid \u{b7} wait a ledger", signed_pct(bound_pct)),
            cp.amber,
        )),
        Verdict::NoFrame | Verdict::Measuring => Some(("Waiting for the book\u{2026}".to_string(), cp.faint)),
        Verdict::NoDepth => Some((
            "No offers right now \u{b7} a limit order can still rest".to_string(),
            cp.amber,
        )),
        Verdict::LimitOnly => Some((LIMIT_ONLY.to_string(), cp.dim)),
        Verdict::Idle | Verdict::Walked { .. } => None,
    }
}

fn link_word<'a>(word: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(text(format!("{} \u{203a}", word.to_uppercase())).font(MONO).size(LINK * scale).color(cp.dim))
        .padding(Padding::new(3.0 * scale).left(7.0 * scale).right(7.0 * scale))
        .boxed()
}

fn flag_tag<'a>(tif: TradeTif, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(text(tif_code(tif)).font(MONO).size(FLAG * scale).color(cp.dim))
        .padding(Padding::new(1.0 * scale).left(4.0 * scale).right(4.0 * scale))
        .style(move |_| container::Style {
            border: Border { color: cp.border_soft, width: 1.0, radius: (4.0 * scale).into() },
            ..Default::default()
        })
        .boxed()
}

fn row26<'a>(key: &'static str, key_ink: Color, value: Element<'a, Message>, tail: Option<Element<'a, Message>>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let mut line = row![
        text(key).size(ROW_KEY * scale).color(key_ink),
        Space::new().width(Length::Fill),
        value,
    ]
    .spacing(8.0 * scale)
    .align_y(Alignment::Center);
    if let Some(t) = tail {
        line = line.push(t);
    }
    column![
        compact::hairline(cp.rule),
        container(line)
            .width(Length::Fill)
            .height(Length::Fixed(ROW_H * scale))
            .align_y(Alignment::Center),
    ]
    .width(Length::Fill)
    .boxed()
}

fn value_runs<'a>(segs: Vec<Seg>, scale: f32) -> Element<'a, Message> {
    runs(segs, ROW_VALUE * scale, ROW_VALUE * scale)
}

fn ticket_face<'a>(state: &'a AppState, inner: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let quote = quote_now();
    let (base, quote_asset) = trade_market_pair(state);
    let q_unit = disp(quote_asset).to_lowercase();
    let q_dec = trade_decimals(quote_asset);
    let fee = CHANNEL.xrp_node_rx.borrow().open_ledger_fee;
    let ready = quote.as_ref().is_some_and(|q| review_ready(q, fee));
    let market_ok = quote.as_ref().is_some_and(|q| q.market_ok);
    let manual = state.trade_mode == TradeKind::Limit;
    let g = gate(state, quote.as_ref(), cp);

    let side_seg = segmented(
        vec![
            Segment {
                label: "Buy",
                active: state.trade_side == TradeSide::Buy,
                msg: Some(Message::TradeSideSet(TradeSide::Buy)),
                colour: (state.trade_side == TradeSide::Buy)
                    .then_some((super::super::xrptrade::BUY_FILL, super::super::xrptrade::BUY_BORDER, cp.green)),
            },
            Segment {
                label: "Sell",
                active: state.trade_side == TradeSide::Sell,
                msg: Some(Message::TradeSideSet(TradeSide::Sell)),
                colour: (state.trade_side == TradeSide::Sell)
                    .then_some((super::super::xrptrade::SELL_FILL, super::super::xrptrade::SELL_BORDER, cp.red)),
            },
        ],
        cp,
        scale,
    );
    let mode_seg = segmented(
        vec![
            Segment {
                label: "Market",
                active: !manual,
                msg: market_ok.then_some(Message::TradeMarketSelected),
                colour: None,
            },
            Segment { label: "Limit", active: manual, msg: Some(Message::TradeLimitOpened), colour: None },
        ],
        cp,
        scale,
    );

    let counter_value = quote.as_ref().and_then(|q| q.counter).filter(|v| *v > 0.0);
    let (counter_text, counter_ph) = match (manual, counter_value) {
        (false, Some(v)) => (format!("\u{2248} {} {}", format_token_amount(v, q_dec), q_unit), false),
        (true, Some(v)) => (format!("{} {}", format_token_amount(v, q_dec), q_unit), false),
        _ => (format!("0.00 {q_unit}"), true),
    };
    let mut stage: Column<Element<'_, Message>> = column![
        text(disp(base).to_uppercase()).font(MONO).size(STAGE_LABEL * scale).color(cp.dim),
        Space::new().height(8.0 * scale),
        figure(
            PlainField::TradeAmount,
            &state.trade_amount,
            inner,
            ready.then_some(Message::TradeReviewPressed),
            g.red,
            false,
            cp,
            scale,
        ),
        Space::new().height(6.0 * scale),
        text(counter_text)
            .font(MONO)
            .size(COUNTER * scale)
            .color(if counter_ph { cp.faint } else { cp.dim }),
    ]
    .align_x(Alignment::Center);
    if let Some((note, ink)) = g.note.clone().or_else(|| stage_note(state, quote.as_ref(), cp)) {
        stage = stage.push(Space::new().height(7.0 * scale).boxed()).push(
            text(note)
                .size(NOTE * scale)
                .color(ink)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .boxed(),
        );
    }
    let stage = container(stage.width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding::new(8.0 * scale).left(0.0).right(0.0));

    let price_row: Element<'a, Message> = if manual {
        let typed = state.trade_limit_price.trim().parse::<f64>().ok().filter(|p| *p > 0.0);
        let value: Element<'a, Message> = match typed {
            Some(p) => row![
                value_runs(
                    vec![Seg::mono(fmt_price(p), cp.text), Seg::mono(format!(" {q_unit}"), cp.muted)],
                    scale,
                ),
                flag_tag(state.trade_tif, cp, scale),
            ]
            .spacing(5.0 * scale)
            .align_y(Alignment::Center)
            .boxed(),
            None => value_runs(vec![Seg::mono(NA, cp.faint)], scale),
        };
        let word = if typed.is_some() { "change" } else { "set" };
        let inner_row = row![
            text("limit price").size(ROW_KEY * scale).color(cp.dim),
            Space::new().width(Length::Fill),
            value,
            link_word(word, cp, scale),
        ]
        .spacing(8.0 * scale)
        .align_y(Alignment::Center);
        column![
            compact::hairline(cp.rule),
            pressable(
                container(inner_row)
                    .width(Length::Fill)
                    .height(Length::Fixed(ROW_H * scale))
                    .align_y(Alignment::Center)
                    .boxed(),
                Some(Message::TradeLimitOpened),
                cp,
            ),
        ]
        .width(Length::Fill)
        .boxed()
    } else {
        let value = match quote.as_ref().and_then(|q| q.price_row).filter(|p| *p > 0.0) {
            Some(p) => value_runs(
                vec![
                    Seg::mono(format!("\u{2248} {}", fmt_price(p)), cp.text),
                    Seg::mono(format!(" {q_unit}"), cp.muted),
                ],
                scale,
            ),
            None => value_runs(vec![Seg::mono(NA, cp.faint)], scale),
        };
        row26("price", cp.dim, value, None, cp, scale)
    };

    let pay_unit = disp(&state.trade_pay_asset).to_lowercase();
    let avail = trade_available(state);
    let avail_value = match avail {
        Some(a) => value_runs(
            vec![
                Seg::mono(format_token_amount(a, 6), cp.text),
                Seg::mono(format!(" {pay_unit}"), cp.muted),
            ],
            scale,
        ),
        None => value_runs(vec![Seg::mono(NA, cp.faint)], scale),
    };
    let can_max = quote.as_ref().and_then(|q| q.max_base).is_some_and(|m| m > 0.0);
    let max_link = can_max.then(|| link("max", cp.dim, Message::TradeMaxClicked, cp, scale));
    let avail_row = row26(g.label.0, g.label.1, avail_value, max_link, cp, scale);

    column![
        pair_row(state, cp, scale),
        Space::new().height(10.0 * scale),
        side_seg,
        Space::new().height(5.0 * scale),
        mode_seg,
        stage,
        price_row,
        avail_row,
        Space::new().height(8.0 * scale),
        grid::primary("Review", ready, Message::TradeReviewPressed, cp, scale),
        Space::new().height(2.0 * scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

fn limit_face<'a>(state: &'a AppState, inner: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let (base, quote_asset) = trade_market_pair(state);
    let typed = state.trade_limit_price.trim().parse::<f64>().ok().filter(|p| *p > 0.0);
    let mid = liquidity::market(base, quote_asset).map(|m| m.mid).filter(|m| *m > 0.0);

    let mut chips: Row<Element<'_, Message>> = row![].spacing(4.0 * scale).align_y(Alignment::Center);
    for tif in [TradeTif::Gtc, TradeTif::Ioc, TradeTif::Fok] {
        let active = state.trade_tif == tif;
        chips = chips.push(
            button(
                container(
                    text(tif_code(tif))
                        .font(MONO)
                        .size(CHIP * scale)
                        .color(if active { cp.text } else { cp.muted }),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
            )
            .on_press(Message::TradeTifSet(tif))
            .width(Length::Fixed(CHIP_W * scale))
            .height(Length::Fixed(CHIP_H * scale))
            .padding(Padding::ZERO)
            .style(move |_, status| {
                let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: Some(if active { cp.neutral.into() } else { Color::TRANSPARENT.into() }),
                    border: Border {
                        color: if active { cp.border } else { cp.border_soft },
                        width: 1.0,
                        radius: (5.0 * scale).into(),
                    },
                    text_color: if active {
                        cp.text
                    } else if hot {
                        cp.dim
                    } else {
                        cp.muted
                    },
                    shadow: Shadow::default(),
                    snap: false,
                }
            })
            .boxed(),
        );
    }
    let tif_row = column![
        compact::hairline(cp.rule),
        container(
            row![
                text("time in force").size(ROW_KEY * scale).color(cp.dim),
                Space::new().width(Length::Fill),
                chips,
            ]
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fixed(TIF_ROW_H * scale))
        .align_y(Alignment::Center),
        compact::hairline(cp.rule),
    ]
    .width(Length::Fill);

    let (mid_line, mid_ink) = match (typed, mid) {
        (Some(p), Some(m)) => {
            let dev = (p - m) / m * 100.0;
            let sign = if dev < 0.0 { "\u{2212}" } else { "+" };
            (format!("{sign}{:.2}% vs mid {}", dev.abs(), fmt_price(m)), cp.dim)
        }
        (None, Some(m)) => (format!("mid {}", fmt_price(m)), cp.dim),
        _ => (format!("mid {NA}"), cp.faint),
    };
    let stage: Column<Element<'_, Message>> = column![
        figure(
            PlainField::TradeLimit,
            &state.trade_limit_price,
            inner,
            typed.is_some().then_some(Message::TradeLimitSet),
            false,
            true,
            cp,
            scale,
        ),
        Space::new().height(6.0 * scale),
        text(mid_line).font(MONO).size(COUNTER * scale).color(mid_ink),
    ]
    .align_x(Alignment::Center);
    let stage = container(stage.width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding::new(8.0 * scale).left(0.0).right(0.0));

    column![
        tif_row,
        stage,
        grid::primary("Set price", typed.is_some(), Message::TradeLimitSet, cp, scale),
        Space::new().height(5.0 * scale),
        quiet_button("Cancel", Message::TradeLimitCancelled, cp, scale),
        Space::new().height(2.0 * scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

fn fact<'a>(key: &'static str, segs: Vec<Seg>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(
        row![
            text(key).size(ROW_KEY * scale).color(cp.dim),
            Space::new().width(Length::Fill),
            runs(segs, ROW_VALUE * scale, ROW_VALUE * scale),
        ]
        .spacing(10.0 * scale)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(2.5 * scale).bottom(2.5 * scale))
    .boxed()
}

fn unit_seg(unit: &str, cp: &'static CompactPalette) -> Seg {
    Seg::mono(format!(" {}", unit.to_lowercase()), cp.muted)
}

fn issuer_fact<'a>(base: &str, quote: &str, cp: &'static CompactPalette, scale: f32) -> Option<Element<'a, Message>> {
    let segs = issuer_segs(base, quote, cp);
    if segs.is_empty() {
        return None;
    }
    Some(fact("issuer", segs, cp, scale))
}

fn review_face<'a>(
    state: &'a AppState,
    q: &'a TradeQuote,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    iced::widget::responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let t = &q.ticket;
        let market = t.kind == TradeKind::Market;
        let q_dec = trade_decimals(&t.quote);
        let side_key = match t.side {
            TradeSide::Sell => "sell",
            TradeSide::Buy => "buy",
        };

        let mut facts: Column<Element<'_, Message>> = column![grid::rule_label("review", cp, scale)].width(Length::Fill);
        facts = facts.push(fact(
            side_key,
            vec![Seg::mono(pad_places(t.amount.trim()), cp.text), unit_seg(&t.base, cp)],
            cp,
            scale,
        ));
        if market {
            let estimate = q.counter.map(|v| format_token_amount(v, q_dec)).unwrap_or_else(|| NA.to_string());
            let (est_key, bound_key, bound_value) = match t.side {
                TradeSide::Sell => ("receive", "at least", q.receive.clone()),
                TradeSide::Buy => ("pay", "at most", q.pay.clone()),
            };
            facts = facts.push(fact(
                est_key,
                vec![Seg::mono(format!("\u{2248} {estimate}"), cp.text), unit_seg(&t.quote, cp)],
                cp,
                scale,
            ));
            facts = facts.push(fact(
                bound_key,
                vec![
                    Seg::mono(pad_places(bound_value.as_deref().unwrap_or(NA)), cp.text),
                    unit_seg(&t.quote, cp),
                ],
                cp,
                scale,
            ));
            if let Some(p) = q.price_row.filter(|p| *p > 0.0) {
                facts = facts.push(fact(
                    "price",
                    vec![Seg::mono(format!("\u{2248} {}", fmt_price(p)), cp.text), unit_seg(&t.quote, cp)],
                    cp,
                    scale,
                ));
            }
            if let Some(route) = q.route.clone() {
                facts = facts.push(fact("routed via", vec![Seg::word(route, cp.text)], cp, scale));
            }
        } else {
            if let Some(p) = t.limit {
                facts = facts.push(fact(
                    "limit price",
                    vec![Seg::mono(fmt_price(p), cp.text), unit_seg(&t.quote, cp)],
                    cp,
                    scale,
                ));
            }
            let (exact_key, exact_value) = match t.side {
                TradeSide::Sell => ("receive", q.receive.clone()),
                TradeSide::Buy => ("pay", q.pay.clone()),
            };
            facts = facts.push(fact(
                exact_key,
                vec![
                    Seg::mono(pad_places(exact_value.as_deref().unwrap_or(NA)), cp.text),
                    unit_seg(&t.quote, cp),
                ],
                cp,
                scale,
            ));
            facts = facts.push(fact(
                "time in force",
                vec![Seg::word(tif_name(t.tif), cp.text)],
                cp,
                scale,
            ));
        }
        if let Some(row) = issuer_fact(&t.base, &t.quote, cp, scale) {
            facts = facts.push(row);
        }
        let resting = !market && t.tif == TradeTif::Gtc;
        let reserve = xrp_reserve().map(|r| r.per_object);
        if resting && let Some(r) = reserve {
            facts = facts.push(fact(
                "reserve",
                vec![Seg::mono(format!("{r:.6}"), cp.text), unit_seg("XRP", cp)],
                cp,
                scale,
            ));
        }
        let fee = state.trade_review_fee;
        facts = facts.push(fact(
            "network fee",
            vec![Seg::mono(fee.map_or(NA.to_string(), |d| format!("{d} drops")), fee_ink(fee, cp))],
            cp,
            scale,
        ));

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
            line: &state.phrase_line,
            on_submit: sign_msg.clone(),
        };
        let live = fields.can_submit();
        let verb = if market {
            match t.side {
                TradeSide::Sell => "Sign & sell",
                TradeSide::Buy => "Sign & buy",
            }
        } else {
            "Sign & place"
        };
        let buttons = column![
            grid::primary(verb, live, sign_msg, cp, scale),
            Space::new().height(5.0 * scale),
            quiet_button("Cancel", Message::TradeReviewCancelled, cp, scale),
        ]
        .width(Length::Fill)
        .boxed();
        let sign = sign_block(
            &fields,
            &state.trade_passphrase,
            &state.trade_seed_text,
            &state.trade_bip39,
            true,
            buttons,
            inner,
            cp,
            scale,
        );

        scroller(
            container(column![facts, sign, Space::new().height(2.0 * scale)].width(Length::Fill))
                .width(Length::Fill)
                .padding(Padding::new(0.0).right(SCROLL_CLEAR * scale))
                .boxed(),
            cp,
            scale,
        )
    })
    .boxed()
}
