
use iced::Widget as _;
use iced::widget::{button, column, container, row, text, Column, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::controller::app_state::AppState;
use crate::controller::message::{Message, PlainField};
use crate::controller::panes::{GridMsg, PaneKind};
use crate::controller::xrp::trade_market_pair;
use crate::ui::components::compact;
use crate::ui::components::tui::MONO_ADVANCE;
use crate::ui::components::tx_panel::Seg;
use crate::ui::managexrp::tokens as tokens_page;
use crate::ui::managexrp::xrptrade::disp;
use crate::ui::managexrp::xrptrade::picker;
use crate::utils::fonts::{self, MONO};
use crate::utils::liquidity;
use crate::utils::orderbook::fmt_price;
use crate::utils::plain_field::plain_field_grid_ink_focus;
use crate::utils::theme::CompactPalette;
use crate::utils::tokens;

use super::{runs, NA};

pub const PANE_LINE: f32 = 9.5;
const GLYPH: f32 = 8.0;
const SYM: f32 = 12.0;
const ISSUER: f32 = 10.5;
const ADDR: f32 = 9.5;
const MID: f32 = 12.5;
const CLASS: f32 = 9.5;
const FIELD: f32 = 11.5;
const FIELD_PAD_H: f32 = 8.0;
const GUTTER: f32 = 9.0;
const ROW_PAD_V: f32 = 7.0;

pub fn label(state: &AppState) -> Option<String> {
    let (base, quote) = trade_market_pair(state);
    if base.is_empty() || quote.is_empty() {
        return None;
    }
    Some(format!("{} / {}", disp(base), disp(quote)))
}

pub fn door<'a>(state: &'a AppState, kind: PaneKind, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let word = label(state).unwrap_or_default();
    let lit = state.trade_pair_list == Some(kind);
    button(
        row![
            text(word).font(MONO).size(PANE_LINE * scale).color(cp.text),
            text("\u{25be}").font(MONO).size(GLYPH * scale).color(cp.faint),
        ]
        .spacing(5.0 * scale)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Grid(GridMsg::PairDoor(kind)))
    .padding(Padding::new(2.0 * scale).left(5.0 * scale).right(5.0 * scale))
    .style(move |_, status| {
        let hot = lit || matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if hot { cp.hover } else { Color::TRANSPARENT }.into()),
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (4.0 * scale).into() },
            text_color: cp.text,
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}

pub fn count_meta(state: &AppState) -> String {
    let query = state.trade_pair_query.trim();
    if query.is_empty() {
        let n = picker::all_markets().len();
        return format!("{n} MARKETS");
    }
    match picker::listing(query).0.len() {
        1 => "1 MATCH".to_string(),
        n => format!("{n} MATCHES"),
    }
}

pub fn class_words<'a>(base: &str, quote: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let (words, ink) = match liquidity::healthy(base, quote) {
        Some(good) => (liquidity::label(base, quote), if good { cp.green } else { cp.amber }),
        None => ("measuring\u{2026}".to_string(), cp.faint),
    };
    text(words.to_lowercase()).font(MONO).size(CLASS * scale).color(ink).boxed()
}

pub(crate) fn issuer_segs(base: &str, quote: &str, cp: &'static CompactPalette) -> Vec<Seg> {
    match (base, quote) {
        ("XRP", t) | (t, "XRP") => match tokens::by_code(t) {
            Some(t) => vec![
                Seg::word(t.issuer_name, cp.dim),
                Seg::word(" \u{b7} ", cp.faint),
                Seg::mono(tokens_page::short_issuer(t.issuer), cp.muted),
            ],
            None => Vec::new(),
        },
        (a, b) => vec![
            Seg::word(tokens::by_code(a).map(|t| t.issuer_name.to_string()).unwrap_or_else(|| a.to_string()), cp.dim),
            Seg::word(" \u{b7} ", cp.faint),
            Seg::word(tokens::by_code(b).map(|t| t.issuer_name.to_string()).unwrap_or_else(|| b.to_string()), cp.dim),
        ],
    }
}

fn pair_row<'a>(
    base: &'static str,
    quote: &'static str,
    selected: bool,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let (mid_str, mid_ink) = match liquidity::market(base, quote) {
        Some(m) if m.mid > 0.0 => (fmt_price(m.mid), cp.text),
        _ => (NA.to_string(), cp.faint),
    };
    let left = column![
        text(format!("{} / {}", disp(base), disp(quote))).size(SYM * scale).color(cp.text),
        Space::new().height(3.0 * scale),
        runs(issuer_segs(base, quote, cp), ADDR * scale, ISSUER * scale),
    ];
    let right = column![
        text(mid_str).font(MONO).size(MID * scale).color(mid_ink),
        Space::new().height(3.0 * scale),
        class_words(base, quote, cp, scale),
    ]
    .align_x(Alignment::End);
    let check: Element<'a, Message> = if selected {
        text("\u{2713}").font(MONO).size(CLASS * scale).color(cp.green).boxed()
    } else {
        Space::new().boxed()
    };

    let line = row![
        left,
        Space::new().width(Length::Fill),
        right,
        container(check).width(Length::Fixed(GUTTER * scale)).align_x(Alignment::End),
    ]
    .spacing(10.0 * scale)
    .align_y(Alignment::Center);

    column![
        compact::hairline(cp.rule),
        button(line)
            .width(Length::Fill)
            .padding(Padding::new(0.0).top(ROW_PAD_V * scale).bottom(ROW_PAD_V * scale))
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
            }),
    ]
    .width(Length::Fill)
    .boxed()
}

fn search_field<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let s = FIELD * scale;
    let advance = s * MONO_ADVANCE;
    let (input, _) = plain_field_grid_ink_focus(
        PlainField::TradePairQuery,
        state.trade_pair_query.as_str(),
        64,
        1,
        "Search symbol or issuer",
        None,
        advance,
        s * 1.6,
        s,
        MONO,
        cp.text,
        cp.text,
        cp.faint,
        Some(fonts::SANS),
        true,
    );
    container(input)
        .width(Length::Fill)
        .padding(Padding::new(5.0 * scale).left(FIELD_PAD_H * scale).right(FIELD_PAD_H * scale))
        .style(move |_| container::Style {
            background: Some(cp.field.into()),
            border: Border { color: cp.focus, width: 1.0, radius: (9.0 * scale).into() },
            ..Default::default()
        })
        .boxed()
}

pub fn face<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let query = state.trade_pair_query.trim();
    let (rows, _) = picker::listing(query);
    let (cur_base, cur_quote) = trade_market_pair(state);

    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    if rows.is_empty() {
        list = list.push(
            column![
                compact::hairline(cp.rule),
                container(text("No market found").size(ISSUER * scale).color(cp.muted))
                    .width(Length::Fill)
                    .padding(Padding::new(14.0 * scale)),
            ]
            .width(Length::Fill)
            .boxed(),
        );
    }
    for (b, q) in rows {
        list = list.push(pair_row(b, q, b == cur_base && q == cur_quote, cp, scale));
    }

    column![
        search_field(state, cp, scale),
        Space::new().height(8.0 * scale),
        super::scroller(list.boxed(), cp, scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}
