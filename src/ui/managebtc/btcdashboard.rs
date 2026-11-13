

use iced::widget::{Column, Row};
use iced::Widget as _;
use iced::widget::canvas::Canvas;
use iced::widget::pane_grid::Pane;
use iced::widget::text::{Ellipsis, Span, Wrapping};
use iced::widget::{button, column, container, responsive, rich_text, row, span, stack, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow, Size};

use crate::channel::{BtcNodeStats as NodeFrame, Socket, CHANNEL};
use crate::controller::app_state::{AppState, BtcFeeTier, ChartPeriod, EnableInputMode, SendAnchor};
use crate::controller::message::{Message, PlainField, SecureField};
use crate::controller::panes::{Chain, GridMsg, PaneKind};
use crate::ui::components::grid::{
    self, drow, group_rule, label, DashedWell, ADDR, FIELD_VALUE, HERO_UNIT, HINT, NA,
    ROW_KEY, ROW_TOTAL, STRIP,
};
use crate::ui::components::send_screen::{self as screen};
use crate::ui::components::signing::SignFields;
use crate::utils::{fiat_amount, money};
use crate::ui::components::compact;
use crate::ui::components::orbit::{self, Middle, Status};
use crate::ui::components::tui::MONO_ADVANCE;
use crate::ui::managebtc::node::{band_color, fee_band, is_quiet, rate_text, tip_age, NodeFrameView, Verdict};
use crate::ui::managebtc::btcsend::{self, FeeQuote};
use crate::ui::managexrp::panes as xrp_panes;
use crate::ui::managexrp::xrpdashboard;
use crate::ui::managebtc::panes as panes_ui;
use crate::utils::fonts::{LIGHT, MONO};
use crate::utils::sparkline::{change_pct, chart_data};
use crate::utils::theme::{self, CompactPalette};
use crate::utils::{add_commas, fiat_cost, format_token_amount, price};


const FEE_NAME: f32 = 11.0;
const FEE_DOT: f32 = 5.0;

const CARD_GAP: f32 = 4.0;
const CARD_PAD_V: f32 = 5.0;
const CARD_PAD_H: f32 = 4.0;
const CARD_RADIUS: f32 = 2.0;
const CARD_HEIGHT: f32 = 9.0;
const CARD_AGE: f32 = 7.5;
const TRAIN_CARDS: usize = 5;

const SWAP_PAD_V: f32 = 3.0;
const SWAP_GAP: f32 = 8.0 - SWAP_PAD_V;

pub(crate) const TAIL: usize = 5;


pub fn view(state: &AppState) -> Element<'_, Message> {
    let cp = theme::compact(&state.theme);
    let scale = state.scale();
    let host = grid::Host { chain: Chain::Btc, title: "BTC", grid: &state.btc_grid, wrap: Message::BtcGrid };
    grid::screen(host, state, pane_content, cp, scale)
}

fn as_money(v: f32) -> String {
    money(v as f64)
}

fn pane_content<'a>(
    state: &'a AppState,
    pane: Pane,
    kind: PaneKind,
    maximized: bool,
    cp: &'static CompactPalette,
    scale: f32,
) -> grid::PaneContent<'a> {
    let strip = |word: &str| grid::strip(word, cp, scale);

    let (title, meta, body): (Element<'a, Message>, Option<Element<'a, Message>>, Element<'a, Message>) =
        match kind {
            PaneKind::Balance => (strip("balance"), None, balance_pane(state, cp, scale)),
            PaneKind::Fees => (strip("fees"), None, fees_pane(state, cp, scale)),
            PaneKind::Chart => {
                let series = chart_series(state);
                let prices: Vec<f32> = series.iter().map(|p| p.1).collect();
                let ccy = state.base_currency.code();
                let (word, colour) = match change_pct(&prices) {
                    Some(p) => (format!("{p:+.2}%"), if p >= 0.0 { cp.green } else { cp.red }),
                    None => (NA.to_string(), cp.faint),
                };
                (
                    row![
                        strip("chart"),
                        Space::new().width(9.0 * scale),
                        grid::period_pill(state.btc_grid.chart_period, Message::BtcGrid, cp, scale),
                    ]
                    .align_y(Alignment::Center)
                    .boxed(),
                    Some(text(word).font(MONO).size(STRIP * scale).color(colour).boxed()),
                    grid::chart_pane(
                        series,
                        price::cross("BTC", ccy) as f32,
                        state.btc_grid.chart_period,
                        &format!("BTC / {ccy}"),
                        as_money,
                        theme::is_dark(&state.theme),
                        cp,
                        scale,
                    ),
                )
            }
            PaneKind::Send => (strip(send_title(state)), None, send_pane(state, cp, scale)),
            PaneKind::Wallet => (strip(panes_ui::wallet::title(state)), None, panes_ui::wallet::view(state, cp, scale)),
            PaneKind::Receive => {
                let address = CHANNEL.bitcoin_wallet_rx.borrow().1.clone().unwrap_or_else(|| "No Address".to_string());
                let line_address = address.clone();
                (
                    strip("receive"),
                    None,
                    grid::receive_pane(
                        address.clone(),
                        address,
                        ADDR_TAIL,
                        state.btc_copy_feedback,
                        Message::BtcCopyAddress,
                        None,
                        move || receive_line(&line_address, cp, scale),
                        cp,
                        scale,
                    ),
                )
            }
            PaneKind::Blocks => (strip("network"), None, blocks_pane(state, cp, scale)),
            PaneKind::Transactions => (
                strip(panes_ui::transactions::title(state)),
                panes_ui::transactions::meta(state, cp, scale),
                panes_ui::transactions::view(state, cp, scale),
            ),
            PaneKind::Mempool => (strip("mempool"), None, panes_ui::mempool::view(state, cp, scale)),
            PaneKind::Intervals => (strip("block intervals"), None, panes_ui::intervals::view(state, cp, scale)),
           
            PaneKind::Network
            | PaneKind::Ticket
            | PaneKind::Book
            | PaneKind::Depth
            | PaneKind::Tokens
            | PaneKind::AvailableTokens
            | PaneKind::Empty => (Space::new().boxed(), None, grid::well_pane(Message::BtcGrid, cp, scale)),
        };

    grid::pane_frame(pane, kind, maximized, title, meta, body, Message::BtcGrid, cp, scale)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BalanceFigures {
    confirmed: u64,
    unconfirmed: i64,
}

fn balance_figures(union: &[crate::channel::BtcUtxo]) -> BalanceFigures {
    let confirmed: u64 = union.iter().filter(|u| u.height > 0).map(|u| u.sats).sum();
    let created: u64 = union.iter().filter(|u| u.height == 0 && u.spent_by.is_none()).map(|u| u.sats).sum();
    let consumed: u64 = union.iter().filter(|u| u.height > 0 && u.spent_by.is_some()).map(|u| u.sats).sum();
    BalanceFigures { confirmed, unconfirmed: created as i64 - consumed as i64 }
}


fn balance_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    grid::fill_or_scroll_by(orbit::PANE_MIN_H, move |size: Size| balance_body(state, size, cp, scale), cp, scale)
}

fn balance_body<'a>(state: &'a AppState, size: Size, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let hide = state.hide_balance;
    let ccy = state.base_currency.code();
    let mask = |s: String| if hide { "\u{2022}".repeat(4) } else { s };
    let figures = balance_figures(&CHANNEL.btc_utxos_rx.borrow().1);
    let sats_mode = state.btc_balance_sats;
    let unit = if sats_mode { "sats" } else { "BTC" };
    let amount = |sats: u64| if sats_mode { add_commas(sats as i64) } else { format!("{:.8}", sats as f64 / 1e8) };

    let loaded = *CHANNEL.loaded_rx.borrow();
    let ready = loaded.wallets && loaded.btc;
    let rate = price::cross("BTC", ccy);
    let total = (ready && rate > 0.0).then(|| figures.confirmed as f64 / 1e8 * rate);

    let socket = orbit::socket();
    let btc = orbit::btc_node(socket == Socket::Live);
    let middle = orbit::middle(socket, [None, Some(btc)], total.is_some(), [true, ready], orbit::rates_pending());

    let ring_d = orbit::ring_d(size, scale);
    let grow = orbit::grow(ring_d);
    let s = scale * grow;
    let native = native_swap(mask(amount(figures.confirmed)), unit, cp, s);
    let centre: Element<'a, Message> = match middle {
        Middle::Figure { dim, .. } => {
            let shown = if hide { "\u{2022}".repeat(6) } else { money(total.unwrap_or(0.0)) };
            let unit_w = ccy.chars().count() as f32 * HERO_UNIT * MONO_ADVANCE;
            let fsize = orbit::fit(shown.chars().count(), (7.0 + unit_w) * grow, ring_d);
            let hero = row![
                text(shown).font(LIGHT).size(fsize * scale).color(if dim { cp.dim } else { cp.text }),
                Space::new().width(7.0 * s),
                container(text(ccy.to_uppercase()).font(MONO).size(HERO_UNIT * s).color(cp.muted))
                    .padding(Padding::new(0.0).bottom((fsize * 0.17).round() * scale)),
            ]
            .align_y(Alignment::End);
            column![hero, Space::new().height(SWAP_GAP * s), native].align_x(Alignment::Center).boxed()
        }
        Middle::Status(status) => {
            let words = text(status.words())
                .font(LIGHT)
                .size(orbit::fit(status.words().chars().count(), 0.0, ring_d) * scale)
                .color(if status.strong() { cp.text } else { cp.dim });
            if ready && status != Status::Connecting {
                column![words, Space::new().height(SWAP_GAP * s), native].align_x(Alignment::Center).boxed()
            } else {
                words.boxed()
            }
        }
    };

    let line = if figures.unconfirmed != 0 {
        let sign = if figures.unconfirmed < 0 { "\u{2212}" } else { "+" };
        unconfirmed_line(mask(format!("{sign}{}", amount(figures.unconfirmed.unsigned_abs()))), unit, cp, s)
    } else {
        block_line(state, btc, cp, s)
    };
    orbit::pane(ring_d, centre, line, btc, state.socket_laps, &state.theme, cp, scale)
}

fn native_swap<'a>(amount: String, unit: &'static str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let line = row![
        text(amount).font(MONO).size(grid::ROW_VALUE * scale).color(cp.dim),
        Space::new().width(5.0 * scale),
        text(unit).font(MONO).size(grid::ROW_VALUE * scale).color(cp.muted),
        Space::new().width(6.0 * scale),
        grid::swap_glyph(grid::ROW_VALUE * scale, cp.faint),
    ]
    .align_y(Alignment::Center);
    button(line)
        .on_press(Message::BtcBalanceUnitToggled)
        .padding(Padding::new(SWAP_PAD_V * scale).left(7.0 * scale).right(7.0 * scale))
        .style(move |_, status| button::Style {
            background: Some(
                match status {
                    button::Status::Hovered | button::Status::Pressed => cp.neutral,
                    _ => Color::TRANSPARENT,
                }
                .into(),
            ),
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (5.0 * scale).into() },
            text_color: cp.text,
            shadow: Shadow::default(),
            snap: false,
        })
        .boxed()
}

fn unconfirmed_line<'a>(figure: String, unit: &'static str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    row![
        text("Unconfirmed").size(ROW_KEY * scale).color(cp.muted),
        text(figure).font(MONO).size(grid::ROW_VALUE * scale).color(cp.text),
        text(unit).font(MONO).size(grid::ROW_VALUE * scale).color(cp.muted),
    ]
    .spacing(6.0 * scale)
    .align_y(Alignment::End)
    .boxed()
}

fn block_line<'a>(state: &'a AppState, btc: orbit::Node, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let node = NodeFrame::current();
    let (word, ink) = match btc {
        orbit::Node::NoPeers => ("No peers", cp.amber),
        orbit::Node::CatchingUp(_) => ("Syncing", cp.amber),
        _ => ("Block", cp.muted),
    };
    let live = btc == orbit::Node::Live;
    let fresh = live
        && state
            .btc_tip_seen
            .is_some_and(|(h, at)| node.tip_height == Some(h) && at.is_some_and(|t| t.elapsed() < orbit::PULSE));
    let mut line = row![text(word).size(ROW_KEY * scale).color(ink)]
        .spacing(6.0 * scale)
        .align_y(Alignment::End);
    if let Some(h) = node.tip_height {
        line = line.push(
            text(add_commas(h as i64))
                .font(MONO)
                .size(grid::ROW_VALUE * scale)
                .color(if fresh { cp.green } else { cp.dim })
                .boxed(),
        );
    }
    if let Some(age) = tip_age(node.tip_at).filter(|_| live) {
        line = line.push(text("\u{b7}").size(ROW_KEY * scale).color(cp.faint).boxed()).push(
            row![
                text(age_word(age)).font(MONO).size(grid::ROW_VALUE * scale).color(cp.muted),
                text(" ago").size(ROW_KEY * scale).color(cp.muted),
            ]
            .align_y(Alignment::End)
            .boxed(),
        );
    }
    line.boxed()
}


fn fees_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let node = NodeFrame::current();
    let ccy = state.base_currency.code();
    let price = price::cross("BTC", ccy);
    let vsize = btcsend::typical_vsize();
    let quiet = is_quiet(&node);

    let mut col: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for (i, name) in ["min", "low", "med", "high"].into_iter().enumerate() {
        if i > 0 {
            col = col.push(compact::hairline(cp.rule));
        }
        let row = match node.tiers {
            Some(t) => {
                let sats = btcsend::tier_sats_at(t[i], vsize);
                let mut cost = vec![
                    (add_commas(sats as i64), cp.dim),
                    (" sat".to_string(), cp.muted),
                    (" \u{b7} ".to_string(), cp.faint),
                ];
                if price > 0.0 {
                    cost.push((fiat_cost(sats as f64 / 1e8 * price), cp.dim));
                    cost.push((format!(" {}", ccy), cp.muted));
                } else {
                    cost.push((NA.to_string(), cp.faint));
                }
                fee_row(name, Some((rate_text(t[i]), vsize)), cost, (!quiet).then(|| Verdict::of(&t, i)), cp, scale)
            }
            None => fee_row(name, None, vec![(" ".to_string(), cp.dim)], None, cp, scale),
        };
        col = col.push(row);
    }
    if quiet {
        col = col.push(compact::hairline(cp.rule)).push(quiet_line(cp, scale));
    }
    grid::scroller(col.boxed(), cp, scale)
}

fn fee_row<'a>(
    name: &str,
    rate: Option<(String, u64)>,
    cost: Vec<(String, Color)>,
    verdict: Option<Verdict>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let mut left = column![text(name.to_string()).size(FEE_NAME * scale).color(cp.text)].spacing(2.0 * scale);
    if let Some(v) = verdict {
        left = left.push(
            row![
                fee_dot(v.colour(cp), scale),
                text(v.word())
                    .size(ROW_KEY * scale)
                    .color(cp.dim)
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::End),
            ]
            .spacing(5.0 * scale)
            .align_y(Alignment::Center)
            .boxed(),
        );
    }
    let rate_line: Element<'a, Message> = match rate {
        Some((r, vsize)) => rate_and_size(r, vsize, cp, scale),
        None => text(NA).font(MONO).size(ROW_TOTAL * scale).color(cp.faint).boxed(),
    };
    container(
        row![
            container(left).width(Length::Fill).clip(true),
            column![rate_line, compact::mono_runs(cost, ROW_KEY * scale)]
                .spacing(2.0 * scale)
                .align_x(Alignment::End),
        ]
        .spacing(10.0 * scale)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(4.0 * scale).bottom(4.0 * scale))
    .boxed()
}

pub(crate) fn rate_and_size<'a>(rate: String, vsize: u64, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let spans: Vec<Span<'static, ()>> = vec![
        span(rate).font(MONO).size(ROW_TOTAL * scale).color(cp.text),
        span(" sat/vB").font(MONO).size(STRIP * scale).color(cp.muted),
        span(" \u{b7} ").font(MONO).size(STRIP * scale).color(cp.faint),
        span(add_commas(vsize as i64)).font(MONO).size(STRIP * scale).color(cp.dim),
        span(" vB").font(MONO).size(STRIP * scale).color(cp.muted),
    ];
    rich_text(spans).wrapping(Wrapping::None).boxed()
}

fn quiet_line<'a>(cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let words: Vec<Span<'static, ()>> = vec![
        span("Quiet:").size(ROW_KEY * scale).color(cp.text),
        span(" every tier gets into the next block").size(ROW_KEY * scale).color(cp.dim),
    ];
    container(
        row![fee_dot(cp.green, scale), rich_text(words).wrapping(Wrapping::None)]
            .spacing(6.0 * scale)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(6.0 * scale))
    .clip(true)
    .boxed()
}

pub(crate) fn fee_dot<'a>(colour: Color, scale: f32) -> Element<'a, Message> {
    let size = FEE_DOT * scale;
    container(Space::new().width(size).height(size))
        .style(move |_| container::Style {
            background: Some(colour.into()),
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (size / 2.0).into() },
            ..Default::default()
        })
        .boxed()
}


fn chart_series(state: &AppState) -> Vec<(u64, f32)> {
    let history = match state.btc_grid.chart_period {
        ChartPeriod::OneHour => &state.rate_history_short,
        ChartPeriod::OneDay => &state.rate_history_long,
    };
    chart_data("BTC", state.base_currency.code(), history)
}


fn send_title(state: &AppState) -> &'static str {
    if state.btc_send_step == 2 {
        "review"
    } else if state.btc_send_fee_face {
        "select fee"
    } else {
        "send"
    }
}

fn send_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.btc_send_step == 2 {
        return send_review(state, cp, scale);
    }
    if state.btc_send_fee_face {
        return fee_face(state, cp, scale);
    }
    grid::fill_or_scroll_by(send_form_min_h(state), move |size: Size| send_form(state, size, cp, scale), cp, scale)
}

const STAGE_MIN: f32 = 104.0;
const STAGE_PAD: f32 = 12.0;
const FEE_ROW_H: f32 = 40.0;
pub(crate) const GUTTER_W: f32 = 9.0;

fn recipient_hint(state: &AppState) -> Option<&'static str> {
    let addr = state.btc_send_recipient.trim();
    let own = CHANNEL.bitcoin_wallet_rx.borrow().1.clone();
    if !addr.is_empty() && own.as_deref().is_some_and(|o| btcsend::same_address(o, addr)) {
        Some(btcsend::SELF_ERROR)
    } else {
        btcsend::recipient_fault(addr)
    }
}

fn send_form_min_h(state: &AppState) -> f32 {
    let hint = if recipient_hint(state).is_some() { 5.0 + HINT * 1.3 } else { 0.0 };
    let wrap = if state.btc_send_recipient.trim().chars().count() > 34 { ADDR * 1.6 } else { 0.0 };
    let to_block = STRIP * 1.3 + 4.0 + screen::box_h(ADDR) + wrap + hint;
    to_block + 8.0 + FEE_ROW_H + STAGE_MIN + grid::ROW_VALUE * 1.3 + 10.0 + grid::BUTTON_H + 2.0
}

fn send_form<'a>(state: &'a AppState, size: Size, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let inner = (size.width / scale).max(1.0);
    let ccy = state.base_currency.code();
    let rate = price::cross("BTC", ccy);
    let quote = btcsend::quote();
    let gate = btcsend::gate(state, quote.as_ref());
    let fee = quote.as_ref().and_then(|q| btcsend::in_use(state, q));
    let addr = state.btc_send_recipient.trim();

    let mut head = row![label("to", cp, scale)].align_y(Alignment::Center).width(Length::Fill);
    if !addr.is_empty() {
        head = head
            .push(Space::new().width(Length::Fill).boxed())
            .push(xrp_panes::link("Clear", cp.dim, Message::BtcSendRecipientCleared, cp, scale));
    }
    let mut to_block = column![
        head,
        screen::boxed_plain(
            PlainField::BtcSendRecipient,
            &state.btc_send_recipient,
            btcsend::PLACEHOLDER,
            inner,
            ADDR,
            true,
            None,
            0.0,
            None,
            0.0,
            None,
            cp,
            scale,
        ),
    ]
    .width(Length::Fill);
    if let Some(note) = recipient_hint(state) {
        to_block = to_block
            .push(Space::new().height(5.0 * scale).boxed())
            .push(compact::mono_runs(vec![(note.to_string(), cp.red)], HINT * scale));
    }

    let tiers = quote.as_ref().and_then(|q| q.tiers);
    let caption = match state.btc_send_fee_tier {
        BtcFeeTier::Custom => fee.zip(tiers).map(|(f, t)| Caption::from(btcsend::custom_read(f.fee, &t))),
        named => tiers.and_then(|t| named.index().map(|i| Caption::Verdict(Verdict::of(&t.map(|q| q.rate), i)))),
    };
    let fee_line = choice_button(
        fee_choice(
            "Network fee",
            Some(state.btc_send_fee_tier.label()),
            caption,
            fee.map_or(RateCell::Dash, |f| RateCell::Text(rate_text(f.rate), f.vsize)),
            fee_cost(fee, rate, ccy, cp),
            Gutter::Chevron,
            cp,
            scale,
        ),
        false,
        Some(Message::BtcSendFeeFaceToggled),
        cp,
        scale,
    );

    let anchor_fiat = matches!(state.btc_send_anchor, SendAnchor::Fiat);
    let (field, typed, placeholder, figure_unit) = if anchor_fiat {
        (PlainField::BtcSendFiat, state.btc_send_fiat_amount.as_str(), "0.00", ccy.to_string())
    } else {
        (PlainField::BtcSendAmount, state.btc_send_amount.as_str(), "0.00000000", "btc".to_string())
    };
    let figure = xrpdashboard::figure_input(
        field,
        typed,
        placeholder,
        &figure_unit,
        inner - 2.0 * STAGE_PAD,
        gate.red(),
        gate.ready.then_some(Message::BtcSendContinueClicked),
        cp,
        scale,
    );
    let other = if anchor_fiat { state.btc_send_amount.trim() } else { state.btc_send_fiat_amount.trim() };
    let twin_unit = if anchor_fiat { "BTC".to_string() } else { ccy.to_uppercase() };
    let twin_placeholder = if anchor_fiat { "0.00000000" } else { "0.00" };
    let (twin, twin_ink) = if typed.trim().is_empty() {
        (format!("\u{2248} {twin_placeholder} {twin_unit}"), cp.faint)
    } else if other.is_empty() {
        (NA.to_string(), cp.faint)
    } else {
        (format!("\u{2248} {other} {twin_unit}"), cp.dim)
    };
    let twin = if rate > 0.0 {
        xrp_panes::swap_word(twin, twin_ink, Message::BtcSendAnchorToggled, cp, scale)
    } else {
        xrp_panes::link_word(twin, twin_ink, None, cp, scale)
    };
    let stage = column![
        screen::unit("btc", cp, scale),
        Space::new().height(8.0 * scale),
        figure,
        Space::new().height(4.0 * scale),
        twin,
    ]
    .align_x(Alignment::Center)
    .width(Length::Fill);
    let stage = container(stage)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding::new(STAGE_PAD * scale).left(0.0).right(0.0));

    let spendable = quote.as_ref().and_then(|q| q.spendable);
    let dust = quote.as_ref().and_then(|q| q.dust);
    let (avail_label, avail_ink) = if gate.insufficient {
        (btcsend::OVER_AVAILABLE.to_string(), cp.red)
    } else if gate.under_dust {
        (btcsend::dust_words(dust.unwrap_or(0)), cp.red)
    } else {
        ("available".to_string(), cp.muted)
    };
    let avail_value: Vec<(String, Color)> = match spendable {
        Some(s) => vec![(format_token_amount(s as f64 / 1e8, 8), cp.text), (" BTC".to_string(), cp.faint)],
        None => vec![(NA.to_string(), cp.faint)],
    };
    let mut avail_row = row![
        text(avail_label.to_uppercase()).font(MONO).size(STRIP * scale).color(avail_ink),
        Space::new().width(Length::Fill),
        compact::mono_runs(avail_value, grid::ROW_VALUE * scale),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill);
    if spendable.is_some() && fee.is_some_and(|f| f.max > 0) {
        avail_row = avail_row
            .push(Space::new().width(6.0 * scale).boxed())
            .push(xrp_panes::link("Max", cp.dim, Message::BtcSendMaxClicked, cp, scale));
    }

    column![
        to_block,
        Space::new().height(8.0 * scale),
        compact::hairline(cp.rule),
        fee_line,
        compact::hairline(cp.rule),
        stage,
        avail_row,
        Space::new().height(10.0 * scale),
        grid::primary("Review", gate.ready, Message::BtcSendContinueClicked, cp, scale),
        Space::new().height(2.0 * scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

fn fee_face<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let ccy = state.base_currency.code();
    let price = price::cross("BTC", ccy);
    let quote = btcsend::quote();
    let tiers = quote.as_ref().and_then(|q| q.tiers);
    let quiet = is_quiet(&NodeFrame::current());
    let open = state.btc_send_custom_open;
    let live = quote.as_ref().is_some_and(|q| q.draft == btcsend::draft(state));

    let mut col: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for (i, t) in [BtcFeeTier::Minimum, BtcFeeTier::Low, BtcFeeTier::Med, BtcFeeTier::High].into_iter().enumerate() {
        if i > 0 {
            col = col.push(compact::hairline(cp.rule));
        }
        let q = tiers.map(|t| t[i]);
        let caption = tiers.filter(|_| !quiet).map(|t| Caption::Verdict(Verdict::of(&t.map(|f| f.rate), i)));
        let picked = !open && state.btc_send_fee_tier == t;
        let body = fee_choice(
            t.label(),
            None,
            caption,
            q.map_or(RateCell::Dash, |f| RateCell::Text(rate_text(f.rate), f.vsize)),
            fee_cost(q, price, ccy, cp),
            Gutter::Check(picked),
            cp,
            scale,
        );
        col = col.push(choice_button(body, picked, tiers.is_some().then_some(Message::BtcSendFeeTierSelected(t)), cp, scale));
    }
    if quiet {
        col = col.push(compact::hairline(cp.rule)).push(quiet_line(cp, scale));
    }
    col = col.push(compact::hairline(cp.rule));

    let typed_rate = state.btc_send_fee.trim().parse::<f32>().ok().filter(|r| r.is_finite() && *r > 0.0);
    let typed = if open && typed_rate.is_some() { quote.as_ref().and_then(|q| q.custom) } else { None };
    let read = typed.zip(tiers).map(|(f, t)| btcsend::custom_read(f.fee, &t));
    let can_set = open && live && typed.is_some() && !matches!(read, Some(btcsend::CustomRead::Warning(_)));
    let custom_row = if open {
        let caption = read.map(Caption::from).unwrap_or(Caption::Plain(btcsend::OWN_RATE));
        let field = screen::boxed_plain(
            PlainField::BtcSendFee,
            &state.btc_send_fee,
            "0.00",
            btcsend::FEE_BOX_W,
            FIELD_VALUE,
            false,
            None,
            0.0,
            None,
            0.0,
            can_set.then_some(Message::BtcSendSetFee),
            cp,
            scale,
        );
        let body = fee_choice("Custom", None, Some(caption), RateCell::Field(field), fee_cost(typed, price, ccy, cp), Gutter::Check(false), cp, scale);
        choice_button(body, true, None, cp, scale)
    } else if let (BtcFeeTier::Custom, Some(f)) = (state.btc_send_fee_tier, quote.as_ref().and_then(|q| q.custom)) {
        let caption = tiers.map(|t| Caption::from(btcsend::custom_read(f.fee, &t)));
        let body = fee_choice("Custom", None, caption, RateCell::Text(rate_text(f.rate), f.vsize), fee_cost(Some(f), price, ccy, cp), Gutter::Check(true), cp, scale);
        choice_button(body, true, Some(Message::BtcSendFeeTierSelected(BtcFeeTier::Custom)), cp, scale)
    } else {
        let body = fee_choice("Custom", None, Some(Caption::Plain(btcsend::OWN_RATE)), RateCell::Empty, Vec::new(), Gutter::Chevron, cp, scale);
        choice_button(body, false, tiers.is_some().then_some(Message::BtcSendFeeTierSelected(BtcFeeTier::Custom)), cp, scale)
    };
    col = col.push(custom_row);

    let buttons = if open {
        grid::button_pair(
            grid::quiet_button("Cancel", Message::BtcSendFeeFaceToggled, cp, scale),
            grid::primary("Set fee", can_set, Message::BtcSendSetFee, cp, scale),
            scale,
        )
    } else {
        grid::quiet_button("Cancel", Message::BtcSendFeeFaceToggled, cp, scale)
    };
    column![grid::scroller(col.boxed(), cp, scale), Space::new().height(12.0 * scale), buttons]
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed()
}

fn send_review<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let ccy = state.base_currency.code();
        let rate = price::cross("BTC", ccy);
        let (_, _, key_deleted, key_mode) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
        let addr = state.btc_send_recipient.trim();
        let amount = btcsend::amount_sats(state);
        let fee = state.btc_send_locked;
        let quote = btcsend::quote();
        let spendable = quote.as_ref().and_then(|q| q.spendable);
        let dust = quote.as_ref().and_then(|q| q.dust);
        let btc = |s: u64| format!("{}.{:08}", s / 100_000_000, s % 100_000_000);

        let mut facts = column![
            screen::address_block((!addr.is_empty()).then_some(addr), TAIL, cp, scale),
            Space::new().height(7.0 * scale),
        ]
        .width(Length::Fill);
        facts = facts.push(drow(
            "Amount",
            match amount {
                Some(a) => vec![(btc(a), cp.text), (" BTC".to_string(), cp.faint)],
                None => vec![(NA.to_string(), cp.faint)],
            },
            2.0,
            cp,
            scale,
        ));
        let fee_runs = match fee {
            Some(f) => {
                let mut runs = vec![(add_commas(f as i64), cp.dim), (" sat".to_string(), cp.faint)];
                if rate > 0.0 {
                    runs.push((format!("  \u{b7}  {} {}", fiat_cost(f as f64 / 1e8 * rate), ccy), cp.faint));
                }
                runs
            }
            None => vec![(NA.to_string(), cp.faint)],
        };
        facts = facts.push(
            container(
                row![
                    text("Network fee").size(ROW_KEY * scale).color(cp.dim),
                    Space::new().width(5.0 * scale),
                    text(state.btc_send_fee_tier.label()).font(MONO).size(HINT * scale).color(cp.faint),
                    Space::new().width(Length::Fill),
                    compact::mono_runs(fee_runs, grid::ROW_VALUE * scale),
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(Padding::new(0.0).top(2.0 * scale).bottom(3.0 * scale))
            .boxed(),
        );
        let total = match (amount, fee) {
            (Some(a), Some(f)) => {
                let sum = a + f;
                let mut runs = vec![(btc(sum), cp.text), (" BTC".to_string(), cp.faint)];
                if rate > 0.0 {
                    runs.push((format!("  \u{b7}  {} {}", fiat_amount(sum as f64 / 1e8 * rate), ccy), cp.faint));
                }
                runs
            }
            _ => vec![(NA.to_string(), cp.faint)],
        };
        facts = facts.push(group_rule(cp, scale)).push(
            row![
                text("Total").size(ROW_KEY * scale).color(cp.dim),
                Space::new().width(Length::Fill),
                compact::mono_runs(total, ROW_TOTAL * scale),
            ]
            .align_y(Alignment::Center)
            .boxed(),
        );

        let sign_msg = Message::BtcGrid(GridMsg::SendSign);
        let fields = SignFields {
            mode: EnableInputMode::detect(key_deleted, key_mode),
            secret: SecureField::BtcSendPassphrase,
            secret_buf: &state.btc_send_passphrase,
            secret_reveal: state.btc_send_passphrase_reveal,
            phrase: SecureField::BtcSendSeed,
            phrase_buf: &state.btc_send_seed_text,
            phrase_reveal: state.btc_send_seed_reveal,
            bip39: SecureField::BtcSendBip39,
            bip39_buf: &state.btc_send_bip39,
            bip39_reveal: state.btc_send_bip39_reveal,
            line: &state.phrase_line,
            on_submit: sign_msg.clone(),
        };
        let armed = match (amount, fee, spendable) {
            (Some(a), Some(f), Some(s)) => a + f <= s && dust.is_none_or(|d| a >= d),
            _ => false,
        };
        let buttons = grid::button_pair(
            grid::quiet_button("Cancel", Message::BtcSendReviewCancelled, cp, scale),
            grid::primary("Sign & send", armed && fields.can_submit(), sign_msg, cp, scale),
            scale,
        );
        let sign = grid::sign_block(
            &fields,
            &state.btc_send_passphrase,
            &state.btc_send_seed_text,
            &state.btc_send_bip39,
            armed,
            buttons,
            inner,
            cp,
            scale,
        );

        grid::scroller(column![facts, sign].width(Length::Fill).boxed(), cp, scale)
    })
    .boxed()
}

pub(crate) enum Caption {
    Verdict(Verdict),
    Warning(String),
    Plain(&'static str),
    Note(String),
}

impl From<btcsend::CustomRead> for Caption {
    fn from(read: btcsend::CustomRead) -> Self {
        match read {
            btcsend::CustomRead::Verdict(v) => Caption::Verdict(v),
            btcsend::CustomRead::Warning(w) => Caption::Warning(w),
        }
    }
}

pub(crate) enum RateCell<'a> {
    Text(String, u64),
    Dash,
    Empty,
    Field(Element<'a, Message>),
}

pub(crate) enum Gutter {
    Check(bool),
    Chevron,
}

fn fee_cost(q: Option<FeeQuote>, price: f64, ccy: &str, cp: &'static CompactPalette) -> Vec<(String, Color)> {
    sats_cost(q.map(|f| f.fee), price, ccy, cp)
}

pub(crate) fn sats_cost(sats: Option<u64>, price: f64, ccy: &str, cp: &'static CompactPalette) -> Vec<(String, Color)> {
    let Some(fee) = sats else {
        return vec![(" ".to_string(), cp.dim)];
    };
    let mut cost = vec![
        (add_commas(fee as i64), cp.dim),
        (" sat".to_string(), cp.muted),
        (" \u{b7} ".to_string(), cp.faint),
    ];
    if price > 0.0 {
        cost.push((fiat_cost(fee as f64 / 1e8 * price), cp.dim));
        cost.push((format!(" {}", ccy), cp.muted));
    } else {
        cost.push((NA.to_string(), cp.faint));
    }
    cost
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn fee_choice<'a>(
    top: &str,
    prefix: Option<&str>,
    caption: Option<Caption>,
    rate: RateCell<'a>,
    cost: Vec<(String, Color)>,
    gutter: Gutter,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let mut left = column![text(top.to_string()).size(FEE_NAME * scale).color(cp.text)].spacing(2.0 * scale);
    let (dot, word, ink) = match &caption {
        Some(Caption::Verdict(v)) => (Some(v.colour(cp)), Some(v.word().to_string()), cp.dim),
        Some(Caption::Warning(w)) => (Some(cp.red), Some(w.clone()), cp.red),
        Some(Caption::Plain(p)) => (None, Some(p.to_string()), cp.dim),
        Some(Caption::Note(n)) => (None, Some(n.clone()), cp.dim),
        None => (None, None, cp.dim),
    };
    let line: Vec<String> = [prefix.map(str::to_string), word].into_iter().flatten().collect();
    if !line.is_empty() {
        let mut caption_row: Row<Element<'_, Message>> = row![].spacing(5.0 * scale).align_y(Alignment::Center);
        if let Some(d) = dot {
            caption_row = caption_row.push(fee_dot(d, scale));
        }
        caption_row = caption_row.push(
            text(line.join(" \u{b7} "))
                .size(ROW_KEY * scale)
                .color(ink)
                .wrapping(Wrapping::None)
                .ellipsis(Ellipsis::End)
                .boxed(),
        );
        left = left.push(caption_row.boxed());
    }
    let rate_line: Element<'a, Message> = match rate {
        RateCell::Text(r, vsize) => rate_and_size(r, vsize, cp, scale),
        RateCell::Field(f) => row![
            f,
            Space::new().width(4.0 * scale),
            text("sat/vB").font(MONO).size(STRIP * scale).color(cp.muted),
        ]
        .align_y(Alignment::Center)
        .boxed(),
        RateCell::Dash => text(NA).font(MONO).size(ROW_TOTAL * scale).color(cp.faint).boxed(),
        RateCell::Empty => Space::new().boxed(),
    };
    let mark: Element<'a, Message> = match gutter {
        Gutter::Check(true) => text("\u{2713}").size(ROW_KEY * scale).color(cp.green).boxed(),
        Gutter::Check(false) => Space::new().boxed(),
        Gutter::Chevron => text("\u{203a}").font(MONO).size(ROW_KEY * scale).color(cp.faint).boxed(),
    };
    container(
        row![
            container(left).width(Length::Fill).clip(true),
            column![rate_line, compact::mono_runs(cost, ROW_KEY * scale)]
                .spacing(2.0 * scale)
                .align_x(Alignment::End),
            container(mark).width(Length::Fixed(GUTTER_W * scale)).align_x(Alignment::End),
        ]
        .spacing(10.0 * scale)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(4.0 * scale).bottom(4.0 * scale))
    .boxed()
}

pub(crate) fn choice_button<'a>(
    body: Element<'a, Message>,
    picked: bool,
    msg: Option<Message>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let live = msg.is_some();
    button(body)
        .on_press_maybe(msg)
        .width(Length::Fill)
        .padding(Padding::new(0.0).left(4.0 * scale).right(4.0 * scale))
        .style(move |_, status| {
            let hot = live && matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if picked { cp.pill } else if hot { cp.hover } else { Color::TRANSPARENT }.into()),
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (5.0 * scale).into() },
                text_color: cp.text,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}



const ADDR_TAIL: usize = 5;

fn receive_line<'a>(address: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    match crate::btc_script_type::BtcScriptType::of_address(address) {
        Some(kind) => text(format!("{} \u{b7} {}", kind.name(), kind.bip()))
            .font(MONO)
            .size(HINT * scale)
            .color(cp.muted)
            .boxed(),
        None => Space::new().boxed(),
    }
}


const BLOCKS_MIN_H: f32 = 170.0;

fn blocks_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    grid::fill_or_scroll(BLOCKS_MIN_H, move || blocks_body(state, cp, scale), cp, scale)
}

fn blocks_body<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let node = NodeFrame::current();
    let now = now_secs();
    let dark = theme::is_dark(&state.theme);
    let (word, dot) = if orbit::btc_node(orbit::socket() == Socket::Live).up() { node.status(cp) } else { ("offline", cp.red) };
    let newest = node.blocks[0];

    let since = match tip_age(node.tip_at) {
        Some(age) => vec![(age_word(age), cp.text)],
        None => vec![(NA.to_string(), cp.muted)],
    };
    let weight = match newest {
        Some(b) => vec![
            (format!("{:.2} MWu", b.weight as f64 / 1e6), cp.dim),
            (format!("  {}%", (b.weight as f64 / 40_000.0).floor() as u64), cp.faint),
        ],
        None => vec![(NA.to_string(), cp.muted)],
    };

    column![
        container(block_train(&node, now, dark, cp, scale))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::new(0.0).top(2.0 * scale).bottom(10.0 * scale)),
        drow("Status", vec![(format!("\u{25cf} {word}"), dot)], 3.0, cp, scale),
        drow(
            "Block",
            vec![(node.tip_height.map_or(NA.to_string(), |h| add_commas(h as i64)), cp.text)],
            3.0,
            cp,
            scale,
        ),
        drow("Since last block", since, 3.0, cp, scale),
        drow("Weight", weight, 3.0, cp, scale),
        drow("Peers", vec![(node.peers.map_or(NA.to_string(), |n| n.to_string()), cp.dim)], 3.0, cp, scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}


fn block_train<'a>(node: &NodeFrame, now: u64, dark: bool, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let count = node.next_block_txs.map_or(NA.to_string(), |n| format!("{} tx", add_commas(n as i64)));
    let mut cards = row![candidate_card("next", count, cp, scale)]
        .spacing(CARD_GAP * scale)
        .width(Length::Fill)
        .height(Length::Fill);
    let mut floor_age = 0u64;
    for b in node.blocks.iter().flatten().take(TRAIN_CARDS) {
        let age = now.saturating_sub(b.at).max(floor_age);
        floor_age = age;
        cards = cards.push(block_card(b.height.to_string(), age_word(age), block_tint(b.feerate, dark, cp), cp, scale));
    }
    cards.boxed()
}


fn block_card<'a>(height: String, age: String, tint: Color, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(card_lines(height, age, cp, scale))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::new(0.0).top(CARD_PAD_V * scale).bottom(CARD_PAD_V * scale).left(CARD_PAD_H * scale).right(CARD_PAD_H * scale))
        .style(move |_| container::Style {
            background: Some(tint.into()),
            border: Border { color: cp.border_soft, width: 1.0, radius: (CARD_RADIUS * scale).into() },
            ..Default::default()
        })
        .boxed()
}

fn candidate_card<'a>(word: &str, count: String, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let edge = Canvas::new(DashedWell { color: cp.border_soft, radius: CARD_RADIUS * scale, dash: 3.0 * scale })
        .width(Length::Fill)
        .height(Length::Fill);
    stack![
        edge,
        container(card_lines(word.to_string(), count, cp, scale))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::new(0.0).top(CARD_PAD_V * scale).bottom(CARD_PAD_V * scale).left(CARD_PAD_H * scale).right(CARD_PAD_H * scale)),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}


fn card_lines<'a>(top: String, bottom: String, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    column![
        text(top).font(MONO).size(CARD_HEIGHT * scale).color(cp.text),
        Space::new().height(Length::Fill),
        text(bottom).font(MONO).size(CARD_AGE * scale).color(cp.muted),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

fn block_tint(feerate: f32, dark: bool, cp: &'static CompactPalette) -> Color {
    let band = fee_band(feerate);
    let a = match (band, dark) {
        (0 | 1, false) => 0.12,
        (0 | 1, true) => 0.13,
        (_, false) => 0.11,
        (_, true) => 0.12,
    };
    Color { a, ..band_color(band, cp) }
}


fn age_word(secs: u64) -> String {
    match secs {
        s if s < 60 => "<1m".to_string(),
        s if s < 3600 => format!("{}m", s / 60),
        s => format!("{}h {}m", s / 3600, (s % 3600) / 60),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

   
    #[test]
    fn the_ladder_colours_come_from_the_ladder() {
        let p = &theme::COMPACT_OBSIDIAN;
        let live = [0.1f32, 0.2, 0.3, 0.338];
        assert_eq!(Verdict::of(&live, 0).colour(p), p.red);
        assert_eq!(Verdict::of(&live, 1).colour(p), p.amber);
        assert_eq!(Verdict::of(&live, 2).colour(p), p.amber);
        assert_eq!(Verdict::of(&live, 3).colour(p), p.green);
        let quiet = [0.1f32; 4];
        for i in 0..4 {
            assert_eq!(Verdict::of(&quiet, i).colour(p), p.green, "rung {i}");
        }
     
        let partial = [0.1f32, 0.1, 0.3, 0.3];
        assert_eq!(Verdict::of(&partial, 1).colour(p), p.red);
        assert_eq!(Verdict::of(&partial, 2).colour(p), p.green);
    }

    #[test]
    fn a_block_is_tinted_by_what_getting_in_cost() {
        let p = &theme::COMPACT_OBSIDIAN;
        let cheap = block_tint(0.95, true, p);
        assert_eq!(Color { a: 1.0, ..cheap }, p.green);
        assert!((cheap.a - 0.13).abs() < 1e-6);
        let busy = block_tint(30.0, false, p);
        assert_eq!(Color { a: 1.0, ..busy }, p.amber);
        assert!((busy.a - 0.11).abs() < 1e-6);
        let dear = block_tint(200.0, true, p);
        assert_eq!(Color { a: 1.0, ..dear }, p.red);
        assert!((dear.a - 0.12).abs() < 1e-6);
    }

    #[test]
    fn ages_read_as_the_cards_show_them() {
        assert_eq!(age_word(12), "<1m");
        assert_eq!(age_word(6 * 60 + 12), "6m");
        assert_eq!(age_word(59 * 60 + 59), "59m");
        assert_eq!(age_word(3 * 3600 + 5 * 60), "3h 5m");
    }
}

#[cfg(test)]
mod balance_figure_tests {
    use super::{balance_figures, BalanceFigures};
    use crate::channel::BtcUtxo;

    const OWN: &str = "bc1qown";
    const CHANGE: &str = "bc1qchange";

    fn coin(txid: &str, sats: u64, height: u64, address: &str, spent_by: Option<&str>) -> BtcUtxo {
        BtcUtxo {
            txid: txid.to_string(),
            vout: 0,
            sats,
            address: address.to_string(),
            height,
            spent_by: spent_by.map(String::from),
        }
    }

    #[test]
    fn a_quiet_wallet_reads_zero_unconfirmed() {
        let union = vec![coin("a", 106_145, 955_129, OWN, None), coin("b", 93_764_195, 965_706, CHANGE, None)];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 93_870_340, unconfirmed: 0 });
    }

 
    #[test]
    fn a_pending_send_keeps_the_chain_figure_and_states_the_delta() {
        let union = vec![
            coin("5154", 5_802, 969_974, OWN, Some("33cf")),
            coin("b4d4", 5_923, 965_711, CHANGE, Some("33cf")),
            coin("33cf", 5_568, 0, CHANGE, None),
        ];
        let f = balance_figures(&union);
        assert_eq!(f, BalanceFigures { confirmed: 11_725, unconfirmed: -(5_857 + 300) });
        assert_eq!(f.confirmed as i64 + f.unconfirmed, 5_568);
    }

   
    #[test]
    fn the_figures_are_a_function_of_the_set_alone() {
        let a = vec![coin("x", 10_000, 10, OWN, Some("p")), coin("p", 6_700, 0, CHANGE, None)];
        let b: Vec<BtcUtxo> = a.iter().rev().cloned().collect();
        assert_eq!(balance_figures(&a), balance_figures(&b));
        assert_eq!(balance_figures(&a), BalanceFigures { confirmed: 10_000, unconfirmed: 6_700 - 10_000 });
    }

    #[test]
    fn a_foreign_mempool_coin_is_unconfirmed_and_positive() {
        let union = vec![coin("a", 1_000, 10, OWN, None), coin("in", 5_000, 0, OWN, None)];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 1_000, unconfirmed: 5_000 });
    }

    #[test]
    fn a_send_on_pending_change_counts_the_confirmed_coin_once() {
      
        let union = vec![
            coin("c", 10_000, 10, OWN, Some("p")),
            coin("p", 3_000, 0, CHANGE, Some("q")),
            coin("q", 1_500, 0, CHANGE, None),
        ];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 10_000, unconfirmed: 1_500 - 10_000 });
    }

    #[test]
    fn confirmation_moves_the_figure_once() {
        let union = vec![coin("33cf", 5_568, 970_141, CHANGE, None)];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 5_568, unconfirmed: 0 });
    }
}
