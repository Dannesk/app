
use iced::Widget as _;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame};
use iced::widget::pane_grid::Pane;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, keyed_column, row, text, Column, Space};
use iced::{Alignment, Color, Element, Length, Padding, Point, Rectangle, Size};

use crate::channel::{Socket, XrpNodeStats as NodeFrame, CHANNEL};
use crate::ui::components::orbit::{self, Middle, Node, Status};
use crate::controller::app_state::{AppState, ChartPeriod, EnableInputMode, SendAnchor, XrpTokenTab};
use crate::controller::message::{Message, PlainField, SecureField, TagPane};
use crate::controller::panes::{Chain, GridMsg, LedgerBar, PaneKind};
use crate::controller::xrp::{self, send_fits, xrp_fee, xrp_reserve};
use crate::ui::components::grid::{self, big_line, HERO_UNIT, ROW_KEY, ROW_TOTAL, ADDR, HINT};
use crate::ui::components::send_screen::{self as screen, stage_box, status_row, STAGE_MIN, STAGE_PAD, STATUS_H};
use crate::ui::components::phrase_entry;
use crate::ui::components::signing::SignFields;
use crate::ui::components::tui::MONO_ADVANCE;
use crate::utils::plain_field::plain_field_grid_requested;
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
                        grid::period_links(state.xrp_grid.chart_period, Message::Grid, cp, scale),
                    ]
                    .align_y(Alignment::Center)
                    .boxed(),
                    Some(text(word).font(MONO).size(STRIP * scale).color(colour).boxed()),
                    grid::chart_pane(
                        series,
                        price::cross("XRP", ccy) as f32,
                        state.xrp_grid.chart_period,
                        &format!("XRP / {ccy}"),
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
                strip(if xrp::trade_locked(state) { "review" } else { "ticket" }),
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
            PaneKind::Transactions => (
                strip(panes_ui::lists::title(state)),
                panes_ui::lists::meta(state, cp, scale),
                panes_ui::lists::view(state, cp, scale),
            ),
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
    let ring_d = orbit::ring_d(size, scale);
    let grow = orbit::grow(ring_d);
    let s = scale * grow;
    let fit = |chars: usize, extra: f32| orbit::fit(chars, extra, ring_d);

    let fresh = xrp == Node::Live && state.ledger_at.is_some_and(|t| t.elapsed() < orbit::PULSE);
    let mut ledger = row![text(word).size(ROW_KEY * s).color(word_ink)]
        .spacing(6.0 * s)
        .align_y(Alignment::End);
    if let Some(i) = node.ledger_index {
        ledger = ledger.push(
            text(add_commas(i as i64))
                .font(MONO)
                .size(ROW_VALUE * s)
                .color(if fresh { cp.accent } else { cp.dim })
                .boxed(),
        );
    }

    let native = row![
        text(mask(format!("{xrp_amount:.6}"))).font(MONO).size(ROW_VALUE * s).color(cp.dim),
        Space::new().width(5.0 * s),
        text("XRP").font(MONO).size(ROW_VALUE * s).color(cp.muted),
    ]
    .align_y(Alignment::End);
    let figures: Element<'a, Message> = match middle {
        Middle::Figure { dim, .. } => {
            let shown = if hide { "\u{2022}".repeat(6) } else { money(total.unwrap_or(0.0)) };
            let unit_w = ccy.chars().count() as f32 * HERO_UNIT * MONO_ADVANCE;
            let fsize = fit(shown.chars().count(), (7.0 + unit_w) * grow);
            let hero = row![
                text(shown).font(LIGHT).size(fsize * scale).color(if dim { cp.dim } else { cp.text }),
                Space::new().width(7.0 * s),
                container(text(ccy.to_uppercase()).font(MONO).size(HERO_UNIT * s).color(cp.muted))
                    .padding(Padding::new(0.0).bottom((fsize * 0.17).round() * scale)),
            ]
            .align_y(Alignment::End);
            column![hero, Space::new().height(8.0 * s), native].align_x(Alignment::Center).boxed()
        }
        Middle::Status(status) => {
            let words = text(status.words())
                .font(LIGHT)
                .size(fit(status.words().chars().count(), 0.0) * scale)
                .color(if status.strong() { cp.text } else { cp.dim });
            if ready && status != Status::Connecting {
                column![words, Space::new().height(8.0 * s), native].align_x(Alignment::Center).boxed()
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
    if state.send_step >= 2 { "review" } else { "send" }
}

#[derive(Clone, Copy, PartialEq)]
enum Face {
    Amount,
    Tag,
    Asset,
    Review,
    Sign,
}

fn send_face(state: &AppState) -> Face {
    match state.send_step {
        3 => Face::Sign,
        2 => Face::Review,
        _ if state.send_tag_editing && !xrp::send_to_xaddress(state) => Face::Tag,
        _ if state.send_asset_picker_open => Face::Asset,
        _ => Face::Amount,
    }
}

fn send_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    grid::fill_or_scroll_fit(
        move |size: Size| send_min_h(state, (size.width / scale).max(1.0), cp, scale),
        move |size: Size| send_form(state, size, cp, scale),
        cp,
        scale,
    )
}

const FIGURE_SIZES: [f32; 3] = [24.0, 20.0, 16.0];
const FIGURE_GAP: f32 = 6.0;
pub(crate) const DROW_H: f32 = 18.0;
pub(crate) const TOTAL_H: f32 = 31.0;

fn recipient_hint(state: &AppState) -> Option<&'static str> {
    let addr = state.send_recipient.trim();
    let (_, own_xrp, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
    let is_self = dannesk_xrpl_codec::xaddress::resolve(addr)
        .is_some_and(|r| own_xrp.as_deref() == Some(r.classic.as_str()));
    if is_self { Some(xrpsend::SELF_ERROR) } else { xrpsend::recipient_fault(addr) }
}

fn send_min_h(state: &AppState, inner: f32, cp: &'static CompactPalette, scale: f32) -> f32 {
    let note = if recipient_hint(state).is_some() {
        5.0 + HINT * 1.3
    } else if xrp::send_to_xaddress(state) {
        5.0 + ROW_KEY * 1.3
    } else {
        0.0
    };
    let rows = if xrpsend::holdings().len() > 1 { 9.0 + screen::ROW_SLOT + 1.0 } else { 0.0 };
    let top = STRIP * 1.3 + 4.0 + screen::boxed_h_seg(&state.send_recipient, inner, ADDR, segment_w(state)) + note + rows;
    let middle = match send_face(state) {
        Face::Amount | Face::Tag | Face::Asset => STAGE_MIN + STATUS_H,
        Face::Review => review_h(state),
        Face::Sign => sign_h(inner, cp, scale),
    };
    top + middle + 10.0 + grid::BUTTON_H + 2.0
}

fn review_h(state: &AppState) -> f32 {
    let is_xrp = matches!(state.send_tab, XrpTokenTab::Xrp);
    let rows = 2 + usize::from(!is_xrp);
    let total = if is_xrp { TOTAL_H } else { 0.0 };
    2.0 * STAGE_PAD + rows as f32 * DROW_H + total
}

fn sign_h(inner: f32, cp: &'static CompactPalette, scale: f32) -> f32 {
    let (_, _, key_deleted, key_mode) = CHANNEL.wallet_balance_rx.borrow().clone();
    let boxed = screen::box_h(compact::ROW);
    let credential = match EnableInputMode::detect(key_deleted, key_mode) {
        EnableInputMode::Passphrase => boxed,
        EnableInputMode::Seed => phrase_entry::height(&phrase_entry::pane(cp, scale), inner * scale) / scale,
    };
    2.0 * STAGE_PAD + STRIP * 1.3 + 4.0 + credential + 8.0 + boxed
}

fn send_form<'a>(state: &'a AppState, size: Size, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let inner = (size.width / scale).max(1.0);
    let addr = state.send_recipient.trim();
    let hint = recipient_hint(state);
    let valid = hint.is_none() && dannesk_xrpl_codec::xaddress::resolve(addr).is_some();

    let mut head = row![label("to", cp, scale)].align_y(Alignment::Center).width(Length::Fill);
    if !addr.is_empty() {
        head = head
            .push(Space::new().width(Length::Fill).boxed())
            .push(panes_ui::link("Clear", cp.dim, Message::SendRecipientCleared, cp, scale));
    }
    let mut to_block = column![
        head,
        screen::boxed_address(
            PlainField::XrpSendRecipient,
            &state.send_recipient,
            "r\u{2026}  \u{b7}  X\u{2026}",
            inner,
            ADDR,
            if valid { TAIL } else { 0 },
            Some(tag_segment(state, cp, scale)),
            cp,
            scale,
        ),
    ]
    .width(Length::Fill);
    if let Some(note) = hint {
        to_block = to_block
            .push(Space::new().height(5.0 * scale).boxed())
            .push(compact::mono_runs(vec![(note.to_string(), cp.red)], HINT * scale));
    } else if xrp::send_to_xaddress(state) {
        to_block = to_block
            .push(Space::new().height(5.0 * scale).boxed())
            .push(text(xrpsend::X_TAG_NOTE).size(ROW_KEY * scale).color(cp.muted).boxed());
    }

    let (middle, action) = match send_face(state) {
        Face::Amount => amount_face(state, inner, cp, scale),
        Face::Tag => tag_face(state, inner, cp, scale),
        Face::Asset => asset_face(state, cp, scale),
        Face::Review => review_face(state, cp, scale),
        Face::Sign => sign_face(state, inner, cp, scale),
    };

    let mut form = column![to_block].width(Length::Fill).height(Length::Fill);
    if xrpsend::holdings().len() > 1 {
        form = form
            .push(Space::new().height(8.0 * scale).boxed())
            .push(compact::hairline(cp.rule))
            .push(asset_row(state, cp, scale))
            .push(compact::hairline(cp.rule));
    }
    form.push(middle)
        .push(Space::new().height(10.0 * scale).boxed())
        .push(action)
        .push(Space::new().height(2.0 * scale).boxed())
        .boxed()
}

fn asset_row<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let tab = if state.send_step >= 2 { state.send_tab.clone() } else { xrp::send_asset(state) };
    let value = xrpsend::asset_display(&tab).to_string();
    let msg = Some(Message::SendAssetPickerToggled);
    screen::setting_row("Asset", None, value, state.send_asset_picker_open, msg, cp, scale)
}

const SEG_TAG: f32 = 10.0;
const SEG_VALUE: f32 = 10.5;
const SEG_GAP: f32 = 6.0;
const SEG_PAD_L: f32 = 8.0;
const SEG_PAD_R: f32 = 6.0;
const SEG_RULE_H: f32 = 15.0;
const SEG_WORD_W: f32 = 19.0;

fn send_tag(state: &AppState) -> (Option<u32>, bool) {
    match dannesk_xrpl_codec::xaddress::resolve(state.send_recipient.trim()) {
        Some(r) if r.from_xaddress => (r.tag, true),
        _ => (xrp::tag_of(&state.send_destination_tag), false),
    }
}

fn segment_w(state: &AppState) -> f32 {
    let chars = send_tag(state).0.map_or(xrpsend::TAG_NONE.len(), |t| t.to_string().len());
    1.0 + screen::SEG_INSET
        + SEG_PAD_L
        + SEG_WORD_W
        + SEG_GAP
        + chars as f32 * SEG_VALUE * MONO_ADVANCE
        + SEG_GAP
        + screen::GUTTER_W
        + SEG_PAD_R
}

fn tag_segment<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> (Element<'a, Message>, f32) {
    let (tag, locked) = send_tag(state);
    let (value, ink) = match tag {
        Some(t) => (t.to_string(), cp.text),
        None => (xrpsend::TAG_NONE.to_string(), cp.faint),
    };
    let mark = if locked { screen::lock(cp, scale) } else { screen::chevron(state.send_tag_editing, cp, scale) };
    let content = row![
        text("Tag").size(SEG_TAG * scale).color(cp.dim).wrapping(Wrapping::None),
        text(value).font(MONO).size(SEG_VALUE * scale).color(ink).wrapping(Wrapping::None),
        container(mark).width(Length::Fixed(screen::GUTTER_W * scale)).align_x(Alignment::Center),
    ]
    .spacing(SEG_GAP * scale)
    .align_y(Alignment::Center);
    let live = !locked;
    let press = button(container(content).height(Length::Fill).align_y(Alignment::Center))
        .on_press_maybe(live.then_some(Message::TagEdit(TagPane::Send)))
        .height(Length::Fill)
        .padding(Padding::new(0.0).left(SEG_PAD_L * scale).right(SEG_PAD_R * scale))
        .style(move |_, status| screen::row_style(live, status, screen::SMALL_RADIUS * scale, cp));
    let rule = container(Space::new().width(Length::Fixed(1.0)).height(Length::Fixed(SEG_RULE_H * scale)))
        .style(move |_| container::Style { background: Some(cp.rule.into()), ..Default::default() });
    let segment = row![rule, Space::new().width(screen::SEG_INSET * scale), press]
        .height(Length::Fill)
        .align_y(Alignment::Center);
    (segment.boxed(), segment_w(state))
}

type FaceParts<'a> = (Element<'a, Message>, Element<'a, Message>);

fn amount_face<'a>(state: &'a AppState, inner: f32, cp: &'static CompactPalette, scale: f32) -> FaceParts<'a> {
    let tab = xrp::send_asset(state);
    let is_xrp = matches!(tab, XrpTokenTab::Xrp);
    let display = xrpsend::asset_display(&tab);
    let unit = display.to_string();
    let ccy = state.base_currency.code();
    let (_, own_xrp, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
    let resolved = dannesk_xrpl_codec::xaddress::resolve(state.send_recipient.trim());
    let is_self = resolved
        .as_ref()
        .is_some_and(|r| own_xrp.as_deref() == Some(r.classic.as_str()));
    let addr_ok = !is_self && resolved.is_some();
    let amount = state.send_amount.trim().parse::<f64>().ok().filter(|v| *v > 0.0);
    let insufficient = amount.is_some_and(|a| !send_fits(state, a));
    let valid = addr_ok && amount.is_some() && !insufficient;
    let rate = xrp::send_rate(state);

    let anchor_fiat = matches!(state.send_anchor, SendAnchor::Fiat);
    let asset_placeholder = if is_xrp { "0.000000" } else { "0.00" };
    let (field, typed, placeholder, figure_unit) = if anchor_fiat {
        (PlainField::XrpSendFiat, state.send_fiat_amount.as_str(), "0.00", ccy.to_string())
    } else {
        (PlainField::XrpSendAmount, state.send_amount.as_str(), asset_placeholder, String::new())
    };
    let figure = figure_field(
        field,
        typed,
        placeholder,
        &figure_unit,
        inner - 2.0 * STAGE_PAD,
        insufficient,
        valid.then_some(Message::SendContinueClicked),
        state.focus_token(field),
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
        screen::unit(display, cp, scale),
        Space::new().height(8.0 * scale),
        keyed_column([(Face::Amount, figure)]).boxed(),
        Space::new().height(4.0 * scale),
        twin,
    ]
    .align_x(Alignment::Center)
    .width(Length::Fill);
    if let XrpTokenTab::Token(code) = tab
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

    let available = crate::controller::xrp::xrp_available();
    let (avail_label, avail_ink) = match tab {
        _ if !insufficient => ("available", cp.muted),
        XrpTokenTab::Xrp => (xrpsend::OVER_AVAILABLE, cp.red),
        XrpTokenTab::Token(code) if amount.is_some_and(|a| a <= CHANNEL.token(code).0) => (xrpsend::NO_XRP_FOR_FEE, cp.red),
        XrpTokenTab::Token(_) => (xrpsend::OVER_AVAILABLE, cp.red),
    };
    let avail_value: Vec<(String, Color)> = match tab {
        XrpTokenTab::Xrp => match available {
            Some(a) => vec![(format_token_amount(a, 6), cp.text), (format!(" {unit}"), cp.faint)],
            None => vec![(NA.to_string(), cp.faint)],
        },
        XrpTokenTab::Token(code) => vec![(format_token_amount(CHANNEL.token(code).0, 6), cp.text), (format!(" {unit}"), cp.faint)],
    };
    let can_max = match tab {
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
            .push(panes_ui::link("Max", cp.dim, Message::SendMaxClicked, cp, scale));
    }

    let middle = column![screen::focus_stage(stage.boxed(), field, scale), status_row(avail_row.boxed(), scale)]
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed();
    (middle, grid::primary("Review", valid, Message::SendContinueClicked, cp, scale))
}

fn tag_face<'a>(state: &'a AppState, inner: f32, cp: &'static CompactPalette, scale: f32) -> FaceParts<'a> {
    let figure = figure_field(
        PlainField::XrpSendTag,
        &state.send_tag_draft,
        "",
        "",
        inner - 2.0 * STAGE_PAD,
        false,
        Some(Message::TagSet(TagPane::Send)),
        state.focus_token(PlainField::XrpSendTag),
        cp,
        scale,
    );
    let stage = column![
        screen::unit("destination tag", cp, scale),
        Space::new().height(8.0 * scale),
        keyed_column([(Face::Tag, figure)]).boxed(),
    ]
    .align_x(Alignment::Center)
    .width(Length::Fill);
    let mut status = row![Space::new().width(Length::Fill)].align_y(Alignment::Center).width(Length::Fill);
    if !state.send_tag_draft.is_empty() {
        status = status.push(panes_ui::link("Clear", cp.dim, Message::TagDraftCleared(TagPane::Send), cp, scale));
    }
    let middle = column![screen::focus_stage(stage.boxed(), PlainField::XrpSendTag, scale), status_row(status.boxed(), scale)]
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed();
    let action = grid::button_pair(
        grid::quiet_button("Cancel", Message::TagCancelled(TagPane::Send), cp, scale),
        grid::primary("Set tag", true, Message::TagSet(TagPane::Send), cp, scale),
        scale,
    );
    (middle, action)
}

fn asset_face<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> FaceParts<'a> {
    let middle = column![
        Space::new().height(8.0 * scale),
        xrpsend::asset_list(&xrp::send_asset(state), state.base_currency.code(), cp, scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed();
    (middle, grid::quiet_button("Cancel", Message::SendAssetPickerToggled, cp, scale))
}

fn send_armed(state: &AppState) -> bool {
    let fee = state.send_fee.map_or(0.0, |d| d as f64 / 1e6);
    let amount = state.send_amount.trim().parse::<f64>().ok().filter(|v| *v > 0.0);
    dannesk_xrpl_codec::xaddress::resolve(state.send_recipient.trim()).is_some()
        && state.send_fee.is_some()
        && amount.is_some_and(|a| xrp::send_fits_for(&state.send_tab, a, fee))
}

fn review_face<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> FaceParts<'a> {
    let is_xrp = matches!(state.send_tab, XrpTokenTab::Xrp);
    let unit = xrpsend::asset_display(&state.send_tab);
    let ccy = state.base_currency.code();
    let amount = state.send_amount.trim().parse::<f64>().ok().filter(|v| *v > 0.0);
    let fee_drops = state.send_fee;
    let fee = fee_drops.map_or(0.0, |d| d as f64 / 1e6);
    let rate = price::cross("XRP", ccy);

    let mut facts: Column<Element<'_, Message>> = column![].width(Length::Fill);
    facts = facts.push(drow(
        "Amount",
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
            "Issuer",
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
        "Network fee",
        vec![(fee_drops.map_or(NA.to_string(), |d| format!("{d} drops")), panes_ui::fee_ink(fee_drops, cp))],
        2.0,
        cp,
        scale,
    ));
    if is_xrp {
        let total = match amount {
            Some(a) => {
                let mut runs = vec![(format_token_amount(a + fee, 6), cp.text), (" XRP".to_string(), cp.faint)];
                if rate > 0.0 {
                    runs.push((format!("  \u{b7}  {} {}", fiat_amount((a + fee) * rate), ccy), cp.faint));
                }
                runs
            }
            None => vec![(NA.to_string(), cp.faint)],
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
    }

    let action = grid::button_pair(
        grid::quiet_button("Cancel", Message::SendReviewCancelled, cp, scale),
        grid::primary("Continue", send_armed(state), Message::SendReviewContinued, cp, scale),
        scale,
    );
    (screen::review_column(facts.boxed(), STAGE_PAD, scale), action)
}

fn sign_face<'a>(state: &'a AppState, inner: f32, cp: &'static CompactPalette, scale: f32) -> FaceParts<'a> {
    let (_, _, key_deleted, key_mode) = CHANNEL.wallet_balance_rx.borrow().clone();
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
        line: &state.phrase_line,
        on_submit: sign_msg.clone(),
    };
    let armed = send_armed(state);
    let action = grid::button_pair(
        grid::quiet_button("Cancel", Message::SendReviewCancelled, cp, scale),
        grid::primary("Sign & send", armed && fields.can_submit(), sign_msg, cp, scale),
        scale,
    );
    let middle = column![
        label("sign", cp, scale),
        grid::sign_fields(&fields, &state.send_passphrase, &state.send_seed_text, &state.send_bip39, armed, inner, cp, scale),
    ]
    .width(Length::Fill);
    (stage_box(middle.boxed(), scale), action)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn figure_field<'a>(
    field: PlainField,
    value: &'a str,
    placeholder: &str,
    unit: &str,
    room: f32,
    red: bool,
    on_submit: Option<Message>,
    focus: Option<u32>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let shown = if value.is_empty() { placeholder } else { value };
    let cols = shown.chars().count() + 1;
    let lead = if unit.is_empty() { 1.0 } else { 0.0 };
    let unit_w = if unit.is_empty() { 0.0 } else { FIGURE_GAP + unit.chars().count() as f32 * screen::UNIT * MONO_ADVANCE };
    let size = FIGURE_SIZES
        .into_iter()
        .find(|s| (cols as f32 + lead) * s * MONO_ADVANCE + unit_w <= room)
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
        s * 1.4,
        s,
        MONO,
        if red { cp.red } else { cp.text },
        cp.text,
        cp.faint,
        focus,
    );
    let mut figure = row![
        Space::new().width(Length::Fixed(lead * advance)),
        container(input).width(Length::Fixed(cols as f32 * advance)),
    ]
    .align_y(Alignment::End);
    if !unit.is_empty() {
        figure = figure.push(Space::new().width(FIGURE_GAP * scale).boxed()).push(
            container(text(unit.to_string()).font(MONO).size(screen::UNIT * scale).color(cp.muted))
                .padding(Padding::new(0.0).bottom((size * 0.17).round() * scale))
                .boxed(),
        );
    }
    figure.boxed()
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
        None => grid::pane_link("Set destination tag", Message::TagEdit(TagPane::Receive), cp, scale),
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
        drow("Status", vec![(format!("\u{25cf} {word}"), dot)], 3.0, cp, scale),
        drow(
            "Ledger",
            vec![(node.ledger_index.map_or(NA.to_string(), |h| add_commas(h as i64)), cp.dim)],
            3.0,
            cp,
            scale,
        ),
        drow("Peers", vec![(node.peers.map_or(NA.to_string(), |n| n.to_string()), cp.dim)], 3.0, cp, scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill);
    if CHANNEL.xrp_exists() {
        match xrp_reserve() {
            Some(r) => {
                col = col
                    .push(drow(
                        "Reserved",
                        vec![(format!("{:.6}", r.total), cp.dim), (" XRP".to_string(), cp.faint)],
                        3.0,
                        cp,
                        scale,
                    ))
                    .push(drow(
                        "Available",
                        vec![(mask(format!("{:.6}", r.available)), cp.text), (" XRP".to_string(), cp.faint)],
                        3.0,
                        cp,
                        scale,
                    ));
            }
            None => {
                col = col
                    .push(drow("Reserved", vec![(NA.to_string(), cp.muted)], 3.0, cp, scale))
                    .push(drow("Available", vec![(NA.to_string(), cp.muted)], 3.0, cp, scale));
            }
        }
    } else {
        col = col.push(drow("Account", vec![("inactive".to_string(), cp.dim)], 3.0, cp, scale));
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
