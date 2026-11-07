
use iced::Widget as _;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame};
use iced::widget::pane_grid::Pane;
use iced::widget::{column, container, responsive, row, text, Column, Space};
use iced::{Alignment, Color, Element, Length, Padding, Point, Rectangle, Size};

use crate::channel::{Socket, XrpNodeStats as NodeFrame, CHANNEL};
use crate::ui::components::orbit::{self, Middle, Node, Status};
use crate::controller::app_state::{AppState, ChartPeriod, EnableInputMode, SendAnchor, XrpTokenTab};
use crate::controller::message::{Message, PlainField, SecureField, TagPane};
use crate::controller::panes::{Chain, GridMsg, LedgerBar, PaneKind};
use crate::controller::xrp::{self, send_fits, xrp_fee, xrp_reserve};
use crate::ui::components::grid::{self, big_line, HERO_UNIT, ROW_KEY, ROW_TOTAL, ADDR, HINT};
use crate::ui::components::send_screen::{self as screen};
use crate::ui::components::signing::SignFields;
use crate::ui::components::tui::MONO_ADVANCE;
use crate::utils::plain_field::plain_field_grid_ink;
use crate::utils::{fiat_amount, money};
use crate::ui::components::compact;
use crate::ui::managexrp::panes as panes_ui;
use crate::ui::managexrp::xrpsend;
use crate::utils::fonts::{LIGHT, MONO};
use crate::utils::sparkline::{change_pct, chart_data};
use crate::utils::theme::{self, CompactPalette};
use crate::utils::{add_commas, format_token_amount, price, tokens};

pub(crate) use crate::ui::components::grid::{
    drow, group_rule, label, pane_button, BIP39_PLACEHOLDER, NA, ROW_VALUE, STRIP,
};

const TAPE_PITCH: f32 = 11.5;
const TAPE_GAP: f32 = 2.0;
const TAIL: usize = 3;

pub fn view(state: &AppState) -> Element<'_, Message> {
    let cp = theme::compact(&state.theme);
    let scale = state.scale();
    let g = &state.xrp_grid;
    let host = grid::Host { chain: Chain::Xrp, title: "XRP", grid: g, wrap: Message::Grid };
    grid::screen(host, state, pane_content, cp, scale)
}

fn four_dp(v: f32) -> String {
    format!("{v:.4}")
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
    let door_strip = |word: &str, door_kind: PaneKind| -> Element<'a, Message> {
        row![
            strip(word),
            Space::new().width(8.0 * scale),
            panes_ui::pair::door(state, door_kind, cp, scale),
        ]
        .align_y(Alignment::Center)
        .boxed()
    };
    let list_open = |door_kind: PaneKind| state.trade_pair_list == Some(door_kind);
    let list_meta = |door_kind: PaneKind| -> Option<Element<'a, Message>> {
        list_open(door_kind).then(|| {
            text(panes_ui::pair::count_meta(state))
                .font(MONO)
                .size(STRIP * scale)
                .color(cp.faint)
                .boxed()
        })
    };

    let (title, meta, body): (Element<'a, Message>, Option<Element<'a, Message>>, Element<'a, Message>) =
        match kind {
            PaneKind::Balance => (strip("balance"), None, balance_pane(state, cp, scale)),
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
                        grid::period_pill(state.xrp_grid.chart_period, Message::Grid, cp, scale),
                    ]
                    .align_y(Alignment::Center)
                    .boxed(),
                    Some(text(word).font(MONO).size(STRIP * scale).color(colour).boxed()),
                    grid::chart_pane(
                        series,
                        price::cross("XRP", ccy) as f32,
                        state.xrp_grid.chart_period,
                        &format!("xrp / {}", ccy.to_lowercase()),
                        four_dp,
                        theme::is_dark(&state.theme),
                        cp,
                        scale,
                    ),
                )
            }
            PaneKind::Send => (strip(send_title(state)), None, send_pane(state, cp, scale)),
            PaneKind::Receive => {
                let address = CHANNEL
                    .wallet_balance_rx
                    .borrow()
                    .1
                    .as_deref()
                    .map(|a| xrp::receive_address(state, a))
                    .unwrap_or_else(|| "No Address".to_string());
                let tagged = xrp::tag_of(&state.receive_tag).is_some();
                (
                    strip(if tagged { "receive \u{b7} tagged" } else { "receive" }),
                    None,
                    grid::receive_pane(
                        address.clone(),
                        address,
                        0,
                        state.xrp_copy_feedback,
                        Message::CopyAddress,
                        None,
                        move || receive_line(state, cp, scale),
                        cp,
                        scale,
                    ),
                )
            }
            PaneKind::Network => (strip("network"), None, network_pane(state, cp, scale)),
            PaneKind::Wallet => (strip(panes_ui::wallet::title(state)), None, panes_ui::wallet::view(state, cp, scale)),
            PaneKind::Ticket => (
                strip("ticket"),
                list_meta(PaneKind::Ticket),
                panes_ui::ticket::view(state, cp, scale),
            ),
            PaneKind::Book => (
                door_strip("order book", PaneKind::Book),
                list_meta(PaneKind::Book),
                if list_open(PaneKind::Book) {
                    panes_ui::pair::face(state, cp, scale)
                } else {
                    panes_ui::market::book(state, cp, scale)
                },
            ),
            PaneKind::Depth => (
                door_strip("depth", PaneKind::Depth),
                list_meta(PaneKind::Depth).or_else(|| {
                    Some(
                        text(panes_ui::market::depth_meta(state))
                            .font(MONO)
                            .size(STRIP * scale)
                            .color(cp.faint)
                            .boxed(),
                    )
                }),
                if list_open(PaneKind::Depth) {
                    panes_ui::pair::face(state, cp, scale)
                } else {
                    panes_ui::market::depth(state, cp, scale)
                },
            ),
            PaneKind::Transactions => (strip("transactions"), None, panes_ui::lists::transactions(state, cp, scale)),
            PaneKind::Orders => (strip(panes_ui::lists::orders_title(state)), None, panes_ui::lists::orders(state, cp, scale)),
            PaneKind::Tokens => (strip(&panes_ui::tokens::held_title(state)), None, panes_ui::tokens::held(state, cp, scale)),
            PaneKind::AvailableTokens => (
                strip(&panes_ui::tokens::available_title(state)),
                None,
                panes_ui::tokens::available(state, cp, scale),
            ),
            PaneKind::Fees | PaneKind::Blocks | PaneKind::Mempool | PaneKind::Intervals | PaneKind::Empty => {
                (Space::new().boxed(), None, grid::well_pane(Message::Grid, cp, scale))
            }
        };

    grid::pane_frame(pane, kind, maximized, title, meta, body, Message::Grid, cp, scale)
}

fn balance_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    grid::fill_or_scroll_by(orbit::PANE_MIN_H, move |size: Size| balance_body(state, size, cp, scale), cp, scale)
}

fn balance_body<'a>(state: &'a AppState, size: Size, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let (xrp_amount, _, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
    let hide = state.hide_balance;
    let ccy = state.base_currency.code();
    let mask = |s: String| if hide { "\u{2022}".repeat(4) } else { s };

    let loaded = *CHANNEL.loaded_rx.borrow();
    let ready = loaded.wallets && loaded.xrp;
    let xrp_rate = price::cross("XRP", ccy);
    let mut total = (ready && xrp_rate > 0.0).then(|| xrp_amount * xrp_rate);
    for t in tokens::TOKENS {
        let (bal, has, _, _) = CHANNEL.token(t.code);
        if has && bal > 0.0 {
            let rate = price::cross(t.rate_key, ccy);
            total = total.filter(|_| rate > 0.0).map(|sum| sum + bal * rate);
        }
    }

    let node = CHANNEL.xrp_node_rx.borrow().clone();
    let socket = orbit::socket();
    let xrp = orbit::xrp_node(socket == Socket::Live);
    let middle = orbit::middle(socket, [Some(xrp), None], total.is_some(), [ready, true], orbit::rates_pending());
    let (word, word_ink) = match xrp {
        Node::CatchingUp(_) => ("Syncing", cp.amber),
        _ => ("Ledger", cp.muted),
    };
    let fresh = xrp == Node::Live && state.ledger_at.is_some_and(|t| t.elapsed() < orbit::PULSE);
    let mut ledger = row![text(word).size(ROW_KEY * scale).color(word_ink)]
        .spacing(6.0 * scale)
        .align_y(Alignment::End);
    if let Some(i) = node.ledger_index {
        ledger = ledger.push(
            text(add_commas(i as i64))
                .font(MONO)
                .size(ROW_VALUE * scale)
                .color(if fresh { cp.green } else { cp.dim })
                .boxed(),
        );
    }

    let ring_d = orbit::ring_d(size.width, scale);
    let fit = |chars: usize, extra: f32| orbit::fit(chars, extra, ring_d);
    let native = row![
        text(mask(format!("{xrp_amount:.6}"))).font(MONO).size(ROW_VALUE * scale).color(cp.dim),
        Space::new().width(5.0 * scale),
        text("XRP").font(MONO).size(ROW_VALUE * scale).color(cp.muted),
    ]
    .align_y(Alignment::End);
    let figures: Element<'a, Message> = match middle {
        Middle::Figure { dim, .. } => {
            let shown = if hide { "\u{2022}".repeat(6) } else { money(total.unwrap_or(0.0)) };
            let unit_w = ccy.chars().count() as f32 * HERO_UNIT * MONO_ADVANCE;
            let fsize = fit(shown.chars().count(), 7.0 + unit_w);
            let hero = row![
                text(shown).font(LIGHT).size(fsize * scale).color(if dim { cp.dim } else { cp.text }),
                Space::new().width(7.0 * scale),
                container(text(ccy.to_uppercase()).font(MONO).size(HERO_UNIT * scale).color(cp.muted))
                    .padding(Padding::new(0.0).bottom((fsize * 0.17).round() * scale)),
            ]
            .align_y(Alignment::End);
            column![hero, Space::new().height(8.0 * scale), native].align_x(Alignment::Center).boxed()
        }
        Middle::Status(status) => {
            let words = text(status.words())
                .font(LIGHT)
                .size(fit(status.words().chars().count(), 0.0) * scale)
                .color(if status.strong() { cp.text } else { cp.dim });
            if ready && status != Status::Connecting {
                column![words, Space::new().height(8.0 * scale), native].align_x(Alignment::Center).boxed()
            } else {
                words.boxed()
            }
        }
    };
    orbit::pane(ring_d, figures, ledger.boxed(), xrp, state.socket_laps, &state.theme, cp, scale)
}

fn chart_series(state: &AppState) -> Vec<(u64, f32)> {
    let history = match state.xrp_grid.chart_period {
        ChartPeriod::OneHour => &state.rate_history_short,
        ChartPeriod::OneDay => &state.rate_history_long,
    };
    chart_data("XRP", state.base_currency.code(), history)
}

fn send_title(state: &AppState) -> &'static str {
    if state.send_step == 2 {
        "review"
    } else if state.send_asset_picker_open {
        "asset"
    } else {
        "send"
    }
}

fn send_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.send_step == 2 {
        return review_face(state, cp, scale);
    }
    if state.send_asset_picker_open {
        return xrpsend::asset_face(&state.xrp_token_tab, state.base_currency.code(), cp, scale);
    }
    grid::fill_or_scroll_by(send_form_min_h(state), move |size: Size| send_form(state, size, cp, scale), cp, scale)
}

const FIGURE_SIZES: [f32; 3] = [24.0, 20.0, 16.0];
const FIGURE_GAP: f32 = 6.0;
const STAGE_MIN: f32 = 104.0;
const STAGE_PAD: f32 = 12.0;

fn recipient_hint(state: &AppState) -> Option<&'static str> {
    let addr = state.send_recipient.trim();
    let (_, own_xrp, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
    let is_self = dannesk_xrpl_codec::xaddress::resolve(addr)
        .is_some_and(|r| own_xrp.as_deref() == Some(r.classic.as_str()));
    if is_self { Some(xrpsend::SELF_ERROR) } else { xrpsend::recipient_fault(addr) }
}

fn send_form_min_h(state: &AppState) -> f32 {
    let hint = if recipient_hint(state).is_some() { 5.0 + HINT * 1.3 } else { 0.0 };
    let to_block = STRIP * 1.3 + 4.0 + screen::box_h(ADDR) + hint + 6.0 + grid::RECEIVE_LINE_H;
    to_block + 9.0 + STAGE_MIN + ROW_VALUE * 1.3 + 10.0 + grid::BUTTON_H + 2.0
}

fn send_form<'a>(state: &'a AppState, size: Size, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let inner = (size.width / scale).max(1.0);
    let is_xrp = matches!(state.xrp_token_tab, XrpTokenTab::Xrp);
    let display = xrpsend::asset_display(&state.xrp_token_tab);
    let unit = display.to_lowercase();
    let ccy = state.base_currency.code();
    let (_, own_xrp, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();

    let addr = state.send_recipient.trim();
    let resolved = dannesk_xrpl_codec::xaddress::resolve(addr);
    let from_x = resolved.as_ref().is_some_and(|r| r.from_xaddress);
    let baked_tag = resolved.as_ref().and_then(|r| if r.from_xaddress { r.tag } else { None });
    let is_self = resolved
        .as_ref()
        .is_some_and(|r| own_xrp.as_deref() == Some(r.classic.as_str()));
    let addr_ok = !is_self && resolved.is_some();
    let amount = state.send_amount.trim().parse::<f64>().ok().filter(|v| *v > 0.0);
    let insufficient = amount.is_some_and(|a| !send_fits(state, a));
    let valid = addr_ok && amount.is_some() && !insufficient && !(state.send_tag_editing && !from_x);
    let has_choice = xrpsend::holdings().len() > 1;
    let rate = xrp::send_rate(state);

    let mut head = row![label("to", cp, scale)].align_y(Alignment::Center).width(Length::Fill);
    if !addr.is_empty() {
        head = head
            .push(Space::new().width(Length::Fill).boxed())
            .push(panes_ui::link("clear", cp.dim, Message::SendRecipientCleared, cp, scale));
    }
    let mut to_block = column![
        head,
        screen::boxed_plain(
            PlainField::XrpSendRecipient,
            &state.send_recipient,
            "r\u{2026}  \u{b7}  X\u{2026}",
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
    to_block = to_block.push(tag_row(state, from_x, baked_tag, cp, scale));

    let anchor_fiat = matches!(state.send_anchor, SendAnchor::Fiat);
    let asset_placeholder = if is_xrp { "0.000000" } else { "0.00" };
    let (field, typed, placeholder, figure_unit) = if anchor_fiat {
        (PlainField::XrpSendFiat, state.send_fiat_amount.as_str(), "0.00", ccy.to_lowercase())
    } else {
        (PlainField::XrpSendAmount, state.send_amount.as_str(), asset_placeholder, unit.clone())
    };
    let chip: Element<'a, Message> = if has_choice {
        screen::asset_chip(display, false, Message::SendAssetPickerToggled, cp, scale)
    } else {
        screen::unit(display, cp, scale)
    };
    let figure = figure_input(
        field,
        typed,
        placeholder,
        &figure_unit,
        inner - 2.0 * STAGE_PAD,
        insufficient,
        valid.then_some(Message::SendContinueClicked),
        cp,
        scale,
    );
    let other = if anchor_fiat { state.send_amount.trim() } else { state.send_fiat_amount.trim() };
    let twin_unit = if anchor_fiat { display.to_uppercase() } else { ccy.to_uppercase() };
    let twin_placeholder = if anchor_fiat { asset_placeholder } else { "0.00" };
    let (twin, twin_ink) = if typed.trim().is_empty() {
        (format!("\u{2248} {twin_placeholder} {twin_unit}"), cp.faint)
    } else if other.is_empty() {
        (NA.to_string(), cp.faint)
    } else {
        (format!("\u{2248} {other} {twin_unit}"), cp.dim)
    };
    let twin = if rate > 0.0 {
        panes_ui::swap_word(twin, twin_ink, Message::SendAnchorToggled, cp, scale)
    } else {
        panes_ui::link_word(twin, twin_ink, None, cp, scale)
    };
    let mut stage = column![
        chip,
        Space::new().height(8.0 * scale),
        figure,
        Space::new().height(4.0 * scale),
        twin,
    ]
    .align_x(Alignment::Center)
    .width(Length::Fill);
    if let XrpTokenTab::Token(code) = state.xrp_token_tab
        && let Some(t) = tokens::by_code(code)
    {
        stage = stage.push(Space::new().height(4.0 * scale).boxed()).push(compact::mono_runs(
            vec![
                (t.issuer_name.to_string(), cp.dim),
                (format!("  \u{b7}  {}", crate::ui::managexrp::tokens::short_issuer(t.issuer)), cp.muted),
            ],
            HINT * scale,
        ));
    }
    let stage = container(stage)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding::new(STAGE_PAD * scale).left(0.0).right(0.0));

    let available = crate::controller::xrp::xrp_available();
    let (avail_label, avail_ink) = match state.xrp_token_tab {
        _ if !insufficient => ("available", cp.muted),
        XrpTokenTab::Xrp => (xrpsend::OVER_AVAILABLE, cp.red),
        XrpTokenTab::Token(code) if amount.is_some_and(|a| a <= CHANNEL.token(code).0) => (xrpsend::NO_XRP_FOR_FEE, cp.red),
        XrpTokenTab::Token(_) => (xrpsend::OVER_AVAILABLE, cp.red),
    };
    let avail_value: Vec<(String, Color)> = match state.xrp_token_tab {
        XrpTokenTab::Xrp => match available {
            Some(a) => vec![(format_token_amount(a, 6), cp.text), (format!(" {unit}"), cp.faint)],
            None => vec![(NA.to_string(), cp.faint)],
        },
        XrpTokenTab::Token(code) => vec![(format_token_amount(CHANNEL.token(code).0, 6), cp.text), (format!(" {unit}"), cp.faint)],
    };
    let can_max = match state.xrp_token_tab {
        XrpTokenTab::Xrp => available.is_some_and(|a| a > xrp_fee()),
        XrpTokenTab::Token(code) => CHANNEL.token(code).0 > 0.0,
    };
    let mut avail_row = row![
        text(avail_label.to_uppercase()).font(MONO).size(STRIP * scale).color(avail_ink),
        Space::new().width(Length::Fill),
        compact::mono_runs(avail_value, ROW_VALUE * scale),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill);
    if can_max {
        avail_row = avail_row
            .push(Space::new().width(6.0 * scale).boxed())
            .push(panes_ui::link("max", cp.dim, Message::SendMaxClicked, cp, scale));
    }

    column![
        to_block,
        Space::new().height(8.0 * scale),
        compact::hairline(cp.rule),
        stage,
        avail_row,
        Space::new().height(10.0 * scale),
        grid::primary("Review", valid, Message::SendContinueClicked, cp, scale),
        Space::new().height(2.0 * scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn figure_input<'a>(
    field: PlainField,
    value: &'a str,
    placeholder: &str,
    unit: &str,
    room: f32,
    red: bool,
    on_submit: Option<Message>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let shown = if value.is_empty() { placeholder } else { value };
    let cols = shown.chars().count() + 1;
    let unit_w = unit.chars().count() as f32 * screen::UNIT * MONO_ADVANCE;
    let size = FIGURE_SIZES
        .into_iter()
        .find(|s| cols as f32 * s * MONO_ADVANCE + FIGURE_GAP + unit_w <= room)
        .unwrap_or(FIGURE_SIZES[2]);
    let s = size * scale;
    let advance = s * MONO_ADVANCE;
    let (input, _) = plain_field_grid_ink(
        field,
        value,
        cols,
        1,
        placeholder,
        on_submit,
        advance,
        s * 1.4,
        s,
        MONO,
        if red { cp.red } else { cp.text },
        cp.focus,
        cp.faint,
    );
    row![
        container(input).width(Length::Fixed(cols as f32 * advance)),
        Space::new().width(FIGURE_GAP * scale),
        container(text(unit.to_uppercase()).font(MONO).size(screen::UNIT * scale).color(cp.muted))
            .padding(Padding::new(0.0).bottom((size * 0.17).round() * scale)),
    ]
    .align_y(Alignment::End)
    .boxed()
}

fn review_face<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let is_xrp = matches!(state.send_tab, XrpTokenTab::Xrp);
        let unit = xrpsend::asset_display(&state.send_tab).to_lowercase();
        let ccy = state.base_currency.code();
        let (_, _, key_deleted, key_mode) = CHANNEL.wallet_balance_rx.borrow().clone();
        let addr = state.send_recipient.trim();
        let resolved = dannesk_xrpl_codec::xaddress::resolve(addr);
        let tag = match &resolved {
            Some(r) if r.from_xaddress => r.tag,
            _ => xrp::tag_of(&state.send_destination_tag),
        };
        let amount = state.send_amount.trim().parse::<f64>().ok().filter(|v| *v > 0.0);
        let fee_drops = state.send_fee;
        let fee = fee_drops.map_or(0.0, |d| d as f64 / 1e6);
        let rate = price::cross("XRP", ccy);

        let mut facts = column![
            screen::address_block((!addr.is_empty()).then_some(addr), TAIL, cp, scale),
            Space::new().height(7.0 * scale),
        ]
        .width(Length::Fill);
        if let Some(t) = tag {
            facts = facts.push(drow("destination tag", vec![(t.to_string(), cp.text)], 2.0, cp, scale));
        }
        facts = facts.push(drow(
            "amount",
            match amount {
                Some(_) => vec![(state.send_amount.trim().to_string(), cp.text), (format!(" {unit}"), cp.faint)],
                None => vec![(NA.to_string(), cp.faint)],
            },
            2.0,
            cp,
            scale,
        ));
        if let XrpTokenTab::Token(code) = state.send_tab
            && let Some(t) = tokens::by_code(code)
        {
            facts = facts.push(drow(
                "issuer",
                vec![
                    (t.issuer_name.to_string(), cp.text),
                    (format!("  \u{b7}  {}", crate::ui::managexrp::tokens::short_issuer(t.issuer)), cp.muted),
                ],
                2.0,
                cp,
                scale,
            ));
        }
        facts = facts.push(drow(
            "network fee",
            vec![(fee_drops.map_or(NA.to_string(), |d| format!("{d} drops")), panes_ui::fee_ink(fee_drops, cp))],
            2.0,
            cp,
            scale,
        ));
        if is_xrp {
            let total = match amount {
                Some(a) => {
                    let mut runs = vec![(format_token_amount(a + fee, 6), cp.text), (" xrp".to_string(), cp.faint)];
                    if rate > 0.0 {
                        runs.push((format!("  \u{b7}  {} {}", fiat_amount((a + fee) * rate), ccy.to_lowercase()), cp.faint));
                    }
                    runs
                }
                None => vec![(NA.to_string(), cp.faint)],
            };
            facts = facts.push(group_rule(cp, scale)).push(
                row![
                    text("total").size(ROW_KEY * scale).color(cp.dim),
                    Space::new().width(Length::Fill),
                    compact::mono_runs(total, ROW_TOTAL * scale),
                ]
                .align_y(Alignment::Center)
                .boxed(),
            );
        }

        let sign_msg = Message::Grid(GridMsg::SendSign);
        let fields = SignFields {
            mode: EnableInputMode::detect(key_deleted, key_mode),
            secret: SecureField::SendPassphrase,
            secret_buf: &state.send_passphrase,
            secret_reveal: state.send_passphrase_reveal,
            phrase: SecureField::SendSeed,
            phrase_buf: &state.send_seed_text,
            phrase_reveal: state.send_seed_reveal,
            bip39: SecureField::SendBip39,
            bip39_buf: &state.send_bip39,
            bip39_reveal: state.send_bip39_reveal,
            on_submit: sign_msg.clone(),
        };
        let armed = resolved.is_some() && fee_drops.is_some() && amount.is_some_and(|a| xrp::send_fits_for(&state.send_tab, a, fee));
        let buttons = grid::button_pair(
            grid::quiet_button("Cancel", Message::SendReviewCancelled, cp, scale),
            grid::primary("Sign & send", armed && fields.can_submit(), sign_msg, cp, scale),
            scale,
        );
        let sign = grid::sign_block(
            &fields,
            &state.send_passphrase,
            &state.send_seed_text,
            &state.send_bip39,
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


fn receive_line<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let gap = 7.5 * scale; 

    if state.receive_tag_editing {
        let draft = state.receive_tag_draft.trim();
      
        let ok = !draft.is_empty() && draft.parse::<u32>().is_ok();
        return row![
            grid::line_label("tag", cp, scale),
            grid::tag_field(TagPane::Receive, &state.receive_tag_draft, cp, scale),
            grid::line_ok(ok.then_some(Message::TagSet(TagPane::Receive)), cp, scale),
            grid::line_no(Message::TagCancelled(TagPane::Receive), cp, scale),
        ]
        .spacing(gap)
        .align_y(Alignment::Center)
        .boxed();
    }

    match xrp::tag_of(&state.receive_tag) {
        None => grid::pane_link("set destination tag", Message::TagEdit(TagPane::Receive), cp, scale),
        Some(tag) => row![
            grid::line_label("tag", cp, scale),
            grid::line_value(tag.to_string(), Message::TagEdit(TagPane::Receive), cp, scale),
            grid::copy_glyph(state.xrp_tag_copied, Message::CopyReceiveTag, cp, scale),
            grid::line_no(Message::TagCleared(TagPane::Receive), cp, scale),
        ]
        .spacing(gap)
        .align_y(Alignment::Center)
        .boxed(),
    }
}

fn tag_row<'a>(
    state: &'a AppState,
    from_x: bool,
    baked_tag: Option<u32>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let gap = 7.5 * scale;
    let line: Element<'a, Message> = if from_x {
        let val = baked_tag.map_or_else(|| "none".to_string(), |t| t.to_string());
        compact::mono_runs(
            vec![(format!("tag {val}"), cp.dim), ("  \u{b7}  from the x-address".to_string(), cp.muted)],
            HINT * scale,
        )
    } else if state.send_tag_editing {
        let draft = state.send_tag_draft.trim();
        let ok = !draft.is_empty() && draft.parse::<u32>().is_ok();
        let mut col = column![
            row![
                grid::line_label("tag", cp, scale),
                grid::tag_field(TagPane::Send, &state.send_tag_draft, cp, scale),
                grid::line_ok(ok.then_some(Message::TagSet(TagPane::Send)), cp, scale),
                grid::line_no(Message::TagCancelled(TagPane::Send), cp, scale),
            ]
            .spacing(gap)
            .align_y(Alignment::Center),
        ]
        .width(Length::Fill);
        if xrp::tag_wrong(&state.send_tag_draft) {
            col = col
                .push(Space::new().height(5.0 * scale).boxed())
                .push(compact::mono_runs(vec![(xrpsend::TAG_ERROR.to_string(), cp.red)], HINT * scale));
        }
        col.boxed()
    } else {
        match xrp::tag_of(&state.send_destination_tag) {
            None => grid::pane_link("set destination tag", Message::TagEdit(TagPane::Send), cp, scale),
            Some(tag) => row![
                grid::line_label("tag", cp, scale),
                grid::line_value(tag.to_string(), Message::TagEdit(TagPane::Send), cp, scale),
                grid::line_no(Message::TagCleared(TagPane::Send), cp, scale),
            ]
            .spacing(gap)
            .align_y(Alignment::Center)
            .boxed(),
        }
    };
    column![Space::new().height(6.0 * scale), line].width(Length::Fill).boxed()
}

const FEE_WORDS: [&str; 3] = ["quiet", "busy", "congested"];
const QUIET_TENTHS: u64 = 12; // quiet up to 1.2× the base fee
const CONGESTED_MULTIPLE: u64 = 5;

fn band_color(band: usize, p: &'static CompactPalette) -> Color {
    match band {
        0 => p.green,
        1 => p.amber,
        _ => p.red,
    }
}

pub(crate) trait NodeFrameView {
    fn verdict(&self, p: &'static CompactPalette) -> Option<(&'static str, Color)>;
}

impl NodeFrameView for NodeFrame {
    fn verdict(&self, p: &'static CompactPalette) -> Option<(&'static str, Color)> {
        let (open, base) = (self.open_ledger_fee?, self.fee_base?);
        if base == 0 {
            return None;
        }
        let band = if open * 10 <= base * QUIET_TENTHS {
            0
        } else if open <= base * CONGESTED_MULTIPLE {
            1
        } else {
            2
        };
        Some((FEE_WORDS[band], band_color(band, p)))
    }
}

const NETWORK_MIN_H: f32 = 165.0;

fn network_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    grid::fill_or_scroll(NETWORK_MIN_H, move || network_body(state, cp, scale), cp, scale)
}

fn network_body<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let node = CHANNEL.xrp_node_rx.borrow().clone();
    let fee = node.open_ledger_fee.map_or(NA.to_string(), |d| d.to_string());
    let verdict = node.verdict(cp);
    let (word, dot) = match orbit::xrp_node(orbit::socket() == Socket::Live) {
        Node::Live => ("synced".to_string(), cp.green),
        Node::CatchingUp(_) => (node.state.clone().unwrap_or_else(|| "syncing".to_string()), cp.amber),
        Node::Waiting | Node::NoPeers | Node::Offline => ("offline".to_string(), cp.red),
    };

    let tape = Canvas::new(Tape {
        bars: state.ledger_tape.iter().copied().collect(),
        faint: cp.faint,
        amber: cp.amber,
        dim: cp.dim,
        scale,
    })
    .width(Length::Fill)
    .height(Length::Fill);

    let mask = |s: String| if state.hide_balance { "\u{2022}".repeat(4) } else { s };
    let mut col: Column<Element<'a, Message>> = column![
        big_line(fee, "drops", verdict, cp, scale),
        container(tape)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::new(0.0).top(6.0 * scale).bottom(9.0 * scale)),
        drow("status", vec![(format!("\u{25cf} {word}"), dot)], 3.0, cp, scale),
        drow(
            "ledger",
            vec![(node.ledger_index.map_or(NA.to_string(), |h| add_commas(h as i64)), cp.dim)],
            3.0,
            cp,
            scale,
        ),
        drow("peers", vec![(node.peers.map_or(NA.to_string(), |n| n.to_string()), cp.dim)], 3.0, cp, scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill);
    if CHANNEL.xrp_exists() {
        match xrp_reserve() {
            Some(r) => {
                col = col
                    .push(drow(
                        "reserved",
                        vec![(format!("{:.6}", r.total), cp.dim), (" xrp".to_string(), cp.faint)],
                        3.0,
                        cp,
                        scale,
                    ))
                    .push(drow(
                        "available",
                        vec![(mask(format!("{:.6}", r.available)), cp.text), (" xrp".to_string(), cp.faint)],
                        3.0,
                        cp,
                        scale,
                    ));
            }
            None => {
                col = col
                    .push(drow("reserved", vec![(NA.to_string(), cp.muted)], 3.0, cp, scale))
                    .push(drow("available", vec![(NA.to_string(), cp.muted)], 3.0, cp, scale));
            }
        }
    } else {
        col = col.push(drow("account", vec![("inactive".to_string(), cp.dim)], 3.0, cp, scale));
    }
    col.boxed()
}

struct Tape {
    bars: Vec<LedgerBar>,
    faint: Color,
    amber: Color,
    dim: Color,
    scale: f32,
}

impl<Message> canvas::Program<Message> for Tape {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let w = bounds.width;
        let h = bounds.height;
        let s = self.scale;
        let mut frame = Frame::new(renderer, bounds.size());
        if self.bars.is_empty() || w <= 0.0 || h <= 0.0 {
            return vec![frame.into_geometry()];
        }
        let gap = TAPE_GAP * s;
        let slots = ((w + gap) / (TAPE_PITCH * s)).floor().max(1.0) as usize;
        let bar_w = ((w - gap * (slots as f32 - 1.0)) / slots as f32).max(1.0);
        let start = self.bars.len().saturating_sub(slots);
        let shown = &self.bars[start..];
        let n = shown.len();
        let busiest = shown.iter().map(|b| b.txns).max().unwrap_or(1).max(1) as f32;
        let x0 = w - (n as f32 * bar_w + (n as f32 - 1.0).max(0.0) * gap);
        for (i, bar) in shown.iter().enumerate() {
            let frac = 0.2 + 0.8 * (bar.txns as f32 / busiest);
            let bh = (h * frac).max(1.0);
            let x = x0 + i as f32 * (bar_w + gap);
            let colour = if i + 1 == n {
                self.dim
            } else if bar.escalated {
                self.amber
            } else {
                self.faint
            };
            frame.fill_rectangle(Point::new(x, h - bh), Size::new(bar_w, bh), colour);
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(open: u64, base: u64) -> NodeFrame {
        NodeFrame { open_ledger_fee: Some(open), fee_base: Some(base), ..Default::default() }
    }

    #[test]
    fn the_verdict_word_matches_its_band() {
        let p = &theme::COMPACT_OBSIDIAN;
        for (open, want) in [(10, "quiet"), (12, "quiet"), (13, "busy"), (50, "busy"), (51, "congested"), (5000, "congested")] {
            assert_eq!(at(open, 10).verdict(p).unwrap().0, want, "wrong word at {open}");
        }
    }

    #[test]
    fn no_fee_means_no_verdict() {
        assert!(NodeFrame::default().verdict(&theme::COMPACT_OBSIDIAN).is_none());
        assert!(at(10, 0).verdict(&theme::COMPACT_OBSIDIAN).is_none());
    }


    #[test]
    fn the_fee_ramp_only_climbs() {
        let p = &theme::COMPACT_OBSIDIAN;
        assert_eq!(at(10, 10).verdict(p).unwrap().1, p.green);
        assert_eq!(at(50, 10).verdict(p).unwrap().1, p.amber);
        assert_eq!(at(500, 10).verdict(p).unwrap().1, p.red);
    }
}
