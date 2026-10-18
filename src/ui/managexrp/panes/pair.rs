
use iced::Widget as _;
use iced::widget::{button, column, container, mouse_area, opaque, row, svg, text, Column, Space};
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

const LABEL: f32 = 10.0;
pub const PANE_LINE: f32 = 9.5;
const MID: f32 = 12.5;
const SECOND: f32 = 10.5;

const CONTROL_H: f32 = 22.0;
const RADIUS: f32 = 5.0;
const CHIP_PAD_H: f32 = 9.0;
const FIELD_W: f32 = 206.0;
const FIELD_PAD_H: f32 = 8.0;
const FIELD_GAP: f32 = 7.0;
const GLYPH: f32 = 11.0;
const PANEL_W: f32 = 324.0;
const PANEL_LEFT: f32 = 45.0;
const PANEL_TOP: f32 = 39.0;
const PANEL_RADIUS: f32 = 10.0;
const PANEL_PAD_H: f32 = 12.0;
const PANEL_MAX_H: f32 = 380.0;
pub const PANE_GAP: f32 = 8.0;

fn no_pair(state: &AppState) -> bool {
    state.trade_pay_asset.is_empty() || state.trade_receive_asset.is_empty()
}

fn field_shown(state: &AppState) -> bool {
    no_pair(state) || state.trade_pair_search_open
}

pub fn label(state: &AppState) -> Option<String> {
    if no_pair(state) {
        return None;
    }
    let (base, quote) = trade_market_pair(state);
    Some(format!("{} / {}", disp(base), disp(quote)))
}

pub fn control<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if field_shown(state) {
        field(state, cp, scale)
    } else {
        chip(label(state).unwrap_or_default(), cp, scale)
    }
}

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
    .boxed()
}

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
        None,
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
    .boxed()
}

pub fn layer<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Option<Element<'a, Message>> {
    let searching = !state.trade_pair_query.trim().is_empty();
    if !state.trade_pair_search_open && !searching {
        return None;
    }
    let content: Element<'a, Message> = if searching {
        opaque(panel(state, cp, scale)).boxed()
    } else {
        Space::new().boxed()
    };
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
            .on_release(Message::Grid(GridMsg::PairSearchDismissed)),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed(),
    )
}

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

    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    if rows.is_empty() {
        list = list.push(
            container(text("no market found").size(super::ISSUER * scale).color(cp.muted))
                .width(Length::Fill)
                .padding(Padding::new(14.0 * scale))
                .boxed(),
        );
    }
    for (b, q) in rows {
        list = list.push(market_row(b, q, b == cur_base && q == cur_quote, cp, scale));
    }

    container(
        column![
            head,
            compact::hairline(cp.rule),
            container(scroller_hug(list.boxed(), cp, scale)).height(Length::Fit.max(PANEL_MAX_H * scale)),
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
    .boxed()
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
    let (mid_str, mid_ink) = match market {
        Some(m) if m.mid > 0.0 => (fmt_price(m.mid), cp.text),
        _ => (NA.to_string(), cp.faint),
    };
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

    let body = token_row(&format!("{} / {}", disp(base), disp(quote)), issuer, right.boxed(), false, cp, scale);
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
        .boxed()
}

