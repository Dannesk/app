
use iced::Widget as _;
use iced::mouse;
use iced::widget::{button, column, container, mouse_area, row, text, Column, Row, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow, Size};

use crate::channel::CHANNEL;
use crate::controller::app_state::{AppState, EnableInputMode, TradeKind, TradeSide, TradeTif, TradeView};
use crate::controller::message::{Message, PlainField, SecureField};
use crate::controller::panes::{GridMsg, PaneKind};
use crate::controller::xrp::{
    review_ready, trade_available, trade_decimals, trade_fee_xrp_ok, trade_market_pair, trade_pay_asset, typed_price,
    xrp_reserve,
};
use crate::ui::components::compact;
use crate::ui::components::phrase_entry;
use crate::ui::components::send_screen::{self as screen, STAGE_PAD};
use crate::ui::components::signing::SignFields;
use crate::ui::components::tui::MONO_ADVANCE;
use crate::ui::components::tx_panel::Seg;
use crate::ui::managexrp::xrpsend::{NO_XRP_FOR_FEE, OVER_AVAILABLE};
use crate::ui::managexrp::xrptrade::disp;
use crate::utils::fonts::MONO;
use crate::utils::liquidity;
use crate::utils::orderbook::fmt_price;
use crate::utils::plain_field::plain_field_grid_requested;
use crate::utils::theme::CompactPalette;
use crate::utils::trade::{TradeQuote, Verdict};
use crate::utils::format_token_amount;

use super::{fee_ink, link, pressable, runs, sign_mode, swap_word_sized, NA};
use crate::ui::components::grid::{self, fill_or_scroll_fit};

const TICKET_MIN_H: f32 = 280.0;
const TOP_H: f32 = 101.0;
const ACTION_H: f32 = 8.0 + grid::BUTTON_H + 2.0;
const SYM: f32 = 12.0;
const ISSUER: f32 = 10.5;
const ADDR: f32 = 9.5;
const MID: f32 = 12.5;
const SEG: f32 = 11.0;
const SEG_H: f32 = 18.0;
const STAGE_LABEL: f32 = 9.0;
use crate::ui::managexrp::xrpdashboard::FIGURE_SIZES;
const COUNTER: f32 = 10.0;
const NOTE: f32 = 9.5;
const ROW_KEY: f32 = 9.5;
const ROW_VALUE: f32 = 10.0;
const ROW_H: f32 = 26.0;
const FACT_H: f32 = 18.0;
const CHIP_W: f32 = 40.0;
const CHIP_H: f32 = 20.0;
const CHIP: f32 = 8.5;
const PRICE_STAGE_H: f32 = 90.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Amount,
    Pair,
    Review,
    Sign,
}

fn face(state: &AppState) -> Face {
    if state.trade_pair_list == Some(PaneKind::Ticket) {
        return Face::Pair;
    }
    match &state.trade_view {
        TradeView::Sign(_) => Face::Sign,
        TradeView::Review(_) => Face::Review,
        TradeView::Ticket => Face::Amount,
    }
}

fn locked(state: &AppState) -> Option<&TradeQuote> {
    match &state.trade_view {
        TradeView::Review(q) | TradeView::Sign(q) => Some(q),
        _ => None,
    }
}

pub fn view<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    fill_or_scroll_fit(
        move |size: Size| min_h(state, (size.width / scale).max(1.0), cp, scale),
        move |size: Size| form(state, (size.width / scale).max(1.0), cp, scale),
        cp,
        scale,
    )
}

fn min_h(state: &AppState, inner: f32, cp: &'static CompactPalette, scale: f32) -> f32 {
    let price_stage = if state.trade_mode == Some(TradeKind::Limit) { PRICE_STAGE_H } else { 0.0 };
    match (face(state), locked(state)) {
        (Face::Review, Some(q)) => TOP_H + 2.0 * STAGE_PAD + review_rows(q, cp) as f32 * FACT_H + ACTION_H,
        (Face::Sign, _) => TOP_H + sign_h(inner, cp, scale) + ACTION_H,
        _ => TICKET_MIN_H + price_stage,
    }
}

fn sign_h(inner: f32, cp: &'static CompactPalette, scale: f32) -> f32 {
    let boxed = screen::box_h(compact::ROW);
    let credential = match sign_mode() {
        EnableInputMode::Passphrase => boxed,
        EnableInputMode::Seed => phrase_entry::height(&phrase_entry::pane(cp, scale), inner * scale) / scale,
    };
    2.0 * STAGE_PAD + grid::STRIP * 1.3 + 4.0 + credential + 8.0 + boxed
}

fn form<'a>(state: &'a AppState, inner: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let quote = quote_now();
    let market_ok = quote.as_ref().is_some_and(|q| q.market_ok);
    let (middle, action) = match (face(state), locked(state)) {
        (Face::Review, Some(q)) => review_face(state, q, cp, scale),
        (Face::Sign, Some(q)) => sign_face(state, q, inner, cp, scale),
        (Face::Pair, _) => pair_face(state, cp, scale),
        _ => amount_face(state, quote, inner, cp, scale),
    };
    column![
        pair_row(state, cp, scale),
        Space::new().height(10.0 * scale),
        side_seg(state, cp, scale),
        Space::new().height(5.0 * scale),
        mode_seg(state, market_ok, cp, scale),
        middle,
        Space::new().height(8.0 * scale),
        action,
        Space::new().height(2.0 * scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

fn placeholder(asset: &str) -> &'static str {
    if asset == "XRP" { "0.000000" } else { "0.00" }
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
    colour: Option<Color>,
}

fn segmented<'a>(items: Vec<Segment>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let mut r: Row<Element<'_, Message>> = row![].spacing(4.0 * scale).width(Length::Fill);
    for s in items {
        let Segment { label, active, msg, colour } = s;
        let live = msg.is_some();
        let (line, ink) = match (active, colour) {
            (true, Some(c)) => (cp.border, c),
            (true, None) => (cp.border, cp.text),
            (false, _) if !live => (cp.border_soft, cp.faint),
            (false, _) => (cp.border, cp.muted),
        };
        let face = container(text(label).size(SEG * scale).color(ink))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .boxed();
        let face = if active { compact::cornered(face, colour.unwrap_or(cp.focus), 7.0 * scale, scale) } else { face };
        r = r.push(
            button(face)
            .on_press_maybe(msg)
            .width(Length::Fill)
            .height(Length::Fixed(SEG_H * scale))
            .padding(Padding::ZERO)
            .style(move |_, status| {
                let hot = live && matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: hot.then_some(iced::Background::Color(cp.hover)),
                    border: Border { color: line, width: 1.0, radius: (7.0 * scale).into() },
                    text_color: ink,
                    shadow: Shadow::default(),
                    snap: false,
                }
            })
            .boxed(),
        );
    }
    r.boxed()
}

fn side_seg<'a>(state: &AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let buy = state.trade_side == Some(TradeSide::Buy);
    let sell = state.trade_side == Some(TradeSide::Sell);
    segmented(
        vec![
            Segment { label: "Buy", active: buy, msg: Some(Message::TradeSideSet(TradeSide::Buy)), colour: buy.then_some(cp.green) },
            Segment { label: "Sell", active: sell, msg: Some(Message::TradeSideSet(TradeSide::Sell)), colour: sell.then_some(cp.red) },
        ],
        cp,
        scale,
    )
}

fn mode_seg<'a>(state: &AppState, market_ok: bool, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    segmented(
        vec![
            Segment {
                label: "Market",
                active: state.trade_mode == Some(TradeKind::Market),
                msg: market_ok.then_some(Message::TradeMarketSelected),
                colour: None,
            },
            Segment {
                label: "Limit",
                active: state.trade_mode == Some(TradeKind::Limit),
                msg: Some(Message::TradeLimitSelected),
                colour: None,
            },
        ],
        cp,
        scale,
    )
}

#[allow(clippy::too_many_arguments)]
fn figure<'a>(
    field: PlainField,
    value: &'a str,
    placeholder: &'static str,
    room: f32,
    on_submit: Option<Message>,
    ink: Color,
    focus: Option<u32>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let shown = if value.is_empty() { placeholder } else { value };
    let cols = shown.chars().count() + 1;
    let size = FIGURE_SIZES
        .into_iter()
        .find(|s| (cols as f32 + 1.0) * s * MONO_ADVANCE <= room)
        .unwrap_or(FIGURE_SIZES[2]);
    let s = size * scale;
    let advance = s * MONO_ADVANCE;
    let input = plain_field_grid_requested(
        field,
        value,
        cols,
        placeholder,
        on_submit,
        advance,
        s * 1.2,
        s,
        MONO,
        ink,
        cp.text,
        cp.faint,
        focus,
    );
    row![
        Space::new().width(Length::Fixed(advance)),
        container(input).width(Length::Fixed(cols as f32 * advance)),
    ]
    .boxed()
}

fn stage<'a>(content: Element<'a, Message>, field: PlainField, align: Alignment, scale: f32) -> Element<'a, Message> {
    mouse_area(
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(align)
            .padding(Padding::new(8.0 * scale).left(0.0).right(0.0)),
    )
    .on_press(Message::FocusField(field))
    .interaction(mouse::Interaction::Text)
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
            None => state.trade_side == Some(TradeSide::Sell) && avail.is_some_and(|a| typed > a + 1e-9),
        };
    let resting = state.trade_mode == Some(TradeKind::Limit) && state.trade_tif == TradeTif::Gtc;
    let note = match max {
        Some(m) if resting && !over_wallet && !broke => xrp_reserve().map(|r| {
            (
                format!(
                    "Max {} {}. GTC orders require {} reserve per order",
                    format_token_amount(m, 6),
                    disp(&state.trade_base),
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
        ("Available", cp.dim)
    };
    Gate { red: broke || max.is_some() || !fee_ok, label, note }
}

fn stage_note(state: &AppState, quote: Option<&TradeQuote>, cp: &'static CompactPalette) -> Option<(String, Color)> {
    const LIMIT_ONLY: &str = "Limit only: market orders need XRP on one side";
    if state.trade_mode != Some(TradeKind::Market) {
        let offered = quote.is_some_and(|q| q.market_ok);
        if !offered && quote.is_some() && state.trade_limit_price.trim().is_empty() {
            return Some((LIMIT_ONLY.to_string(), cp.dim));
        }
        return None;
    }
    match quote.map(|q| q.verdict)? {
        Verdict::Walked { coverage_pct, bound_pct, .. } if coverage_pct < 100.0 - 1e-6 => Some((
            format!("Only {coverage_pct:.0}% fills now \u{b7} {} vs mid", signed_pct(bound_pct)),
            cp.amber,
        )),
        Verdict::Walked { bound_pct, off_market: true, .. } => {
            Some((format!("Off market \u{b7} {} vs mid", signed_pct(bound_pct)), cp.amber))
        }
        Verdict::Walked { unstable: true, .. } => Some(("Unstable book".to_string(), cp.amber)),
        Verdict::NoFrame | Verdict::Measuring => Some(("Waiting for the book\u{2026}".to_string(), cp.faint)),
        Verdict::NoDepth => Some((
            "No offers right now \u{b7} a limit order can still rest".to_string(),
            cp.amber,
        )),
        Verdict::LimitOnly => Some((LIMIT_ONLY.to_string(), cp.dim)),
        Verdict::Idle | Verdict::Walked { .. } => None,
    }
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

type FaceParts<'a> = (Element<'a, Message>, Element<'a, Message>);

fn amount_face<'a>(
    state: &'a AppState,
    quote: Option<TradeQuote>,
    inner: f32,
    cp: &'static CompactPalette,
    scale: f32,
) -> FaceParts<'a> {
    let quote = quote.as_ref();
    let (base, quote_asset) = trade_market_pair(state);
    let q_unit = disp(quote_asset);
    let fee = CHANNEL.xrp_node_rx.borrow().open_ledger_fee;
    let ready = quote.is_some_and(|q| review_ready(q, fee));
    let manual = state.trade_mode == Some(TradeKind::Limit);
    let price_lit = manual && state.trade_figure == PlainField::TradeLimit;
    let g = gate(state, quote, cp);

    let counter_value = quote.and_then(|q| q.counter).filter(|v| *v > 0.0);
    let (counter_text, counter_ink) = match counter_value {
        Some(v) if !manual => (format!("\u{2248} {} {}", format_token_amount(v, trade_decimals(quote_asset)), q_unit), cp.dim),
        Some(v) => (format!("{} {}", format_token_amount(v, trade_decimals(quote_asset)), q_unit), cp.dim),
        None => (format!("{} {}", placeholder(quote_asset), q_unit), cp.faint),
    };
    let counter = swap_word_sized(counter_text, counter_ink, Message::TradePairSwapped, COUNTER, cp, scale);

    let field = PlainField::TradeAmount;
    let amount_ink = if g.red {
        cp.red
    } else if price_lit {
        cp.dim
    } else {
        cp.text
    };
    let mut amount: Column<Element<'_, Message>> = column![
        text(disp(base).to_uppercase()).font(MONO).size(STAGE_LABEL * scale).color(cp.dim),
        Space::new().height(8.0 * scale),
        figure(
            field,
            &state.trade_amount,
            placeholder(base),
            inner,
            ready.then_some(Message::TradeReviewPressed),
            amount_ink,
            state.focus_token(field),
            cp,
            scale,
        ),
        Space::new().height(3.0 * scale),
        counter,
    ]
    .align_x(Alignment::Center);
    if let Some((note, ink)) = g.note.clone().or_else(|| stage_note(state, quote, cp)) {
        amount = amount.push(Space::new().height(4.0 * scale).boxed()).push(
            text(note)
                .size(NOTE * scale)
                .color(ink)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .boxed(),
        );
    }

    let mut figures: Column<Element<'_, Message>> = column![stage(
        amount.width(Length::Fill).boxed(),
        field,
        if manual { Alignment::End } else { Alignment::Center },
        scale,
    )]
    .width(Length::Fill)
    .height(Length::Fill);
    if manual {
        figures = figures.push(price_stage(state, base, quote_asset, inner, ready, price_lit, cp, scale));
    }

    let slot_row: Element<'a, Message> = if manual {
        let mut chips: Row<Element<'_, Message>> = row![].spacing(4.0 * scale).align_y(Alignment::Center);
        for tif in [TradeTif::Gtc, TradeTif::Ioc, TradeTif::Fok] {
            let active = state.trade_tif == tif;
            let ink = if active { cp.text } else { cp.muted };
            chips = chips.push(
                button(
                    container(text(tif_code(tif)).font(MONO).size(CHIP * scale).color(ink))
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
                        background: hot.then_some(iced::Background::Color(cp.hover)),
                        border: Border {
                            color: if active { cp.focus } else { cp.border_soft },
                            width: 1.0,
                            radius: (5.0 * scale).into(),
                        },
                        text_color: ink,
                        shadow: Shadow::default(),
                        snap: false,
                    }
                })
                .boxed(),
            );
        }
        row26("Time in force", cp.dim, chips.boxed(), None, cp, scale)
    } else {
        let value = match quote.and_then(|q| q.price_row).filter(|p| *p > 0.0) {
            Some(p) => value_runs(
                vec![
                    Seg::mono(format!("\u{2248} {}", fmt_price(p)), cp.text),
                    Seg::mono(format!(" {q_unit}"), cp.muted),
                ],
                scale,
            ),
            None => value_runs(vec![Seg::mono(NA, cp.faint)], scale),
        };
        row26("Price", cp.dim, value, None, cp, scale)
    };

    let avail_value = match (trade_pay_asset(state), trade_available(state)) {
        (Some(pay), Some(a)) => value_runs(
            vec![
                Seg::mono(format_token_amount(a, 6), cp.text),
                Seg::mono(format!(" {}", disp(pay)), cp.muted),
            ],
            scale,
        ),
        _ => value_runs(vec![Seg::mono(NA, cp.faint)], scale),
    };
    let can_max = quote.and_then(|q| q.max_base).is_some_and(|m| m > 0.0);
    let max_link = can_max.then(|| link("Max", cp.dim, Message::TradeMaxClicked, cp, scale));
    let avail_row = row26(g.label.0, g.label.1, avail_value, max_link, cp, scale);

    let mark = match state.trade_side {
        Some(TradeSide::Buy) => cp.green,
        Some(TradeSide::Sell) => cp.red,
        None => cp.focus,
    };
    (
        column![figures, slot_row, avail_row].width(Length::Fill).height(Length::Fill).boxed(),
        grid::marked_button("Review", ready, Message::TradeReviewPressed, mark, cp, scale),
    )
}

#[allow(clippy::too_many_arguments)]
fn price_stage<'a>(
    state: &'a AppState,
    base: &str,
    quote_asset: &str,
    inner: f32,
    ready: bool,
    lit: bool,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let typed = typed_price(&state.trade_limit_price);
    let mid = liquidity::market(base, quote_asset).map(|m| m.mid).filter(|m| *m > 0.0);
    let (mid_line, mid_ink) = match (typed, mid) {
        (Some(p), Some(m)) => {
            let dev = (p - m) / m * 100.0;
            let sign = if dev < 0.0 { "\u{2212}" } else { "+" };
            (format!("{sign}{:.2}% vs mid {}", dev.abs(), fmt_price(m)), cp.dim)
        }
        (None, Some(m)) => (format!("mid {}", fmt_price(m)), cp.dim),
        _ => (format!("mid {NA}"), cp.faint),
    };
    let field = PlainField::TradeLimit;
    let content = column![
        text(disp(quote_asset).to_uppercase()).font(MONO).size(STAGE_LABEL * scale).color(cp.dim),
        Space::new().height(8.0 * scale),
        figure(
            field,
            &state.trade_limit_price,
            placeholder(quote_asset),
            inner,
            ready.then_some(Message::TradeReviewPressed),
            if lit { cp.text } else { cp.dim },
            state.focus_token(field),
            cp,
            scale,
        ),
        Space::new().height(6.0 * scale),
        text(mid_line).font(MONO).size(COUNTER * scale).color(mid_ink),
    ]
    .align_x(Alignment::Center);
    stage(content.width(Length::Fill).boxed(), field, Alignment::Start, scale)
}

fn pair_face<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> FaceParts<'a> {
    let middle = column![Space::new().height(8.0 * scale), super::pair::face(state, cp, scale)]
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed();
    (middle, grid::quiet_button("Cancel", Message::Grid(GridMsg::PairDoor(PaneKind::Ticket)), cp, scale))
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
    Seg::mono(format!(" {}", disp(unit)), cp.muted)
}

fn issuer_fact<'a>(base: &str, quote: &str, cp: &'static CompactPalette, scale: f32) -> Option<Element<'a, Message>> {
    let segs = issuer_segs(base, quote, cp);
    if segs.is_empty() {
        return None;
    }
    Some(fact("Issuer", segs, cp, scale))
}

fn review_rows(q: &TradeQuote, cp: &'static CompactPalette) -> usize {
    let t = &q.ticket;
    let figures = if t.kind == Some(TradeKind::Market) {
        3 + usize::from(q.price_row.is_some_and(|p| p > 0.0)) + usize::from(q.route.is_some())
    } else {
        3 + usize::from(t.limit.is_some()) + usize::from(t.tif == TradeTif::Gtc)
    };
    figures + usize::from(!issuer_segs(&t.base, &t.quote, cp).is_empty()) + 1
}

fn review_face<'a>(state: &'a AppState, q: &'a TradeQuote, cp: &'static CompactPalette, scale: f32) -> FaceParts<'a> {
    let t = &q.ticket;
    let market = t.kind == Some(TradeKind::Market);
    let selling = t.side == Some(TradeSide::Sell);
    let (typed_key, other_key) = if selling { ("Sell", "Receive") } else { ("Buy", "Pay") };
    let (typed_asset, other_asset) = (t.base.as_str(), t.quote.as_str());
    let anchor_pay = t.anchor_is_pay();
    let (typed_signed, other_signed) = if anchor_pay { (&q.pay, &q.receive) } else { (&q.receive, &q.pay) };
    let signed = |s: &Option<String>| pad_places(s.as_deref().unwrap_or(NA));

    let mut facts: Column<Element<'_, Message>> = column![].width(Length::Fill);
    facts = facts.push(fact(typed_key, vec![Seg::mono(signed(typed_signed), cp.text), unit_seg(typed_asset, cp)], cp, scale));
    if market {
        let estimate = q
            .counter
            .map(|v| format_token_amount(v, trade_decimals(other_asset)))
            .unwrap_or_else(|| NA.to_string());
        facts = facts.push(fact(
            other_key,
            vec![Seg::mono(format!("\u{2248} {estimate}"), cp.text), unit_seg(other_asset, cp)],
            cp,
            scale,
        ));
        facts = facts.push(fact(
            if anchor_pay { "At least" } else { "At most" },
            vec![Seg::mono(signed(other_signed), cp.text), unit_seg(other_asset, cp)],
            cp,
            scale,
        ));
        if let Some(p) = q.price_row.filter(|p| *p > 0.0) {
            facts = facts.push(fact(
                "Price",
                vec![Seg::mono(format!("\u{2248} {}", fmt_price(p)), cp.text), unit_seg(&t.quote, cp)],
                cp,
                scale,
            ));
        }
        if let Some(route) = q.route.clone() {
            facts = facts.push(fact("Routed via", vec![Seg::word(route, cp.text)], cp, scale));
        }
    } else {
        if let Some(p) = t.limit {
            facts = facts.push(fact(
                "Limit price",
                vec![Seg::mono(fmt_price(p), cp.text), unit_seg(&t.quote, cp)],
                cp,
                scale,
            ));
        }
        facts = facts.push(fact(other_key, vec![Seg::mono(signed(other_signed), cp.text), unit_seg(other_asset, cp)], cp, scale));
        facts = facts.push(fact("Time in force", vec![Seg::word(tif_name(t.tif), cp.text)], cp, scale));
    }
    if let Some(row) = issuer_fact(&t.base, &t.quote, cp, scale) {
        facts = facts.push(row);
    }
    if !market && t.tif == TradeTif::Gtc && let Some(r) = xrp_reserve().map(|r| r.per_object) {
        facts = facts.push(fact("Reserve", vec![Seg::mono(format!("{r:.6}"), cp.text), unit_seg("XRP", cp)], cp, scale));
    }
    let fee = state.trade_review_fee;
    facts = facts.push(fact(
        "Network fee",
        vec![Seg::mono(fee.map_or(NA.to_string(), |d| format!("{d} drops")), fee_ink(fee, cp))],
        cp,
        scale,
    ));

    let action = grid::button_pair(
        grid::quiet_button("Cancel", Message::TradeReviewCancelled, cp, scale),
        grid::primary("Continue", review_armed(state, q), Message::TradeReviewContinued, cp, scale),
        scale,
    );
    (screen::review_column(facts.boxed(), STAGE_PAD, scale), action)
}

fn review_armed(state: &AppState, q: &TradeQuote) -> bool {
    state.trade_review_fee.is_some() && q.pay.is_some() && q.receive.is_some()
}

fn sign_face<'a>(state: &'a AppState, q: &'a TradeQuote, inner: f32, cp: &'static CompactPalette, scale: f32) -> FaceParts<'a> {
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
    let armed = review_armed(state, q);
    let action = grid::button_pair(
        grid::quiet_button("Cancel", Message::TradeReviewCancelled, cp, scale),
        grid::primary("Sign & send", armed && fields.can_submit(), sign_msg, cp, scale),
        scale,
    );
    let middle = column![
        grid::label("sign", cp, scale),
        grid::sign_fields(&fields, &state.trade_passphrase, &state.trade_seed_text, &state.trade_bip39, armed, inner, cp, scale),
    ]
    .width(Length::Fill);
    (screen::stage_box(middle.boxed(), scale), action)
}
