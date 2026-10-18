
pub mod lists;
pub mod market;
pub mod pair;
pub mod ticket;
pub mod tokens;
pub mod wallet;

use iced::widget::Column;
use iced::Widget as _;
use iced::widget::text::{Span, Wrapping};
use iced::widget::{button, column, container, rich_text, row, span, svg, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::channel::{CHANNEL, HistoryList, HistoryPage, PageStatus};
use crate::controller::app_state::EnableInputMode;
use crate::controller::message::Message;
use crate::ui::components::{compact, grid};
use crate::ui::components::tx_panel::{Detail, Seg, Tail};
use crate::ui::managexrp::xrpdashboard::{self, STRIP};
use crate::utils::fonts::MONO;
use crate::utils::icons;
use crate::utils::theme::CompactPalette;

pub use crate::ui::components::grid::{button_pair, danger_button, primary, primary_tinted, quiet_button, scroller, sign_block};

pub const BIG: f32 = 13.0;
const CARET: f32 = 8.0;
const CARET_W: f32 = 6.0;
const TYPE: f32 = 10.0;
const TYPE_W: f32 = 42.0;
const SIDE_W: f32 = 24.0;
const CELL: f32 = 9.5;
const STATUS_W: f32 = 42.0;
const TIME_W: f32 = 66.0;
const ROW_GAP: f32 = 5.0;
const ROW_PAD_V: f32 = 6.0;
const ROW_PAD_H: f32 = 11.0;
const KEY: f32 = 9.5;
const KEY_W: f32 = 50.0;
const VALUE: f32 = 10.0;
const DETAIL_INDENT: f32 = 12.0;
const DETAIL_ROW_PAD_V: f32 = 2.5;
const LINK: f32 = 8.5;
const EMPTY_LINE: f32 = 11.5;
const EMPTY_GLYPH: f32 = 22.0;
const EMPTY_GAP: f32 = 10.0;
pub const SYM: f32 = 12.0;
pub const ISSUER: f32 = 10.5;
pub const VAL: f32 = 12.5;
pub const SUB: f32 = 10.5;
pub const TOKEN_ROW_PAD_V: f32 = 8.0;
pub const TOKEN_ROW_PAD_H: f32 = 12.0;

pub(crate) const NA: &str = xrpdashboard::NA;

pub fn empty<'a>(line: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(
        column![
            svg(icons::EMPTY_LIST.clone())
                .width(Length::Fixed(EMPTY_GLYPH * scale))
                .height(Length::Fixed(EMPTY_GLYPH * scale))
                .style(move |_, _| svg::Style { color: Some(cp.faint) }),
            Space::new().height(EMPTY_GAP * scale),
            text(line.to_string()).size(EMPTY_LINE * scale).color(cp.muted),
        ]
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .boxed()
}

pub fn group_head<'a>(word: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    column![
        container(text(word.to_uppercase()).font(MONO).size(STRIP * scale).color(cp.muted))
            .width(Length::Fill)
            .padding(Padding::new(0.0).top(9.0 * scale).bottom(6.0 * scale).left(ROW_PAD_H * scale).right(ROW_PAD_H * scale)),
        compact::hairline(cp.rule),
    ]
    .width(Length::Fill)
    .boxed()
}

pub struct ListRow {
    pub kind: &'static str,
    pub side: bool,
    pub amount: Vec<Seg>,
    pub status: (&'static str, Color),
    pub time: String,
    pub on_press: Message,
    pub detail: Option<Detail>,
}

pub fn list_row(r: ListRow, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let open = r.detail.is_some();
    let quiet = if open { cp.dim } else { cp.faint };
    let time_ink = if open { cp.dim } else { cp.muted };
    let type_w = if r.side { SIDE_W } else { TYPE_W };

    let cell = |el: Element<'static, Message>, w: f32, align: Alignment| -> Element<'static, Message> {
        container(el).width(Length::Fixed(w * scale)).align_x(align).boxed()
    };

    let line = row![
        cell(text(if open { "\u{25be}" } else { "\u{25b8}" }).font(MONO).size(CARET * scale).color(quiet).boxed(), CARET_W, Alignment::Start),
        cell(text(r.kind).size(TYPE * scale).color(cp.text).wrapping(Wrapping::None).boxed(), type_w, Alignment::Start),
        container(runs(r.amount, CELL * scale, CELL * scale)).width(Length::Fill).align_x(Alignment::End),
        cell(text(r.status.0).font(MONO).size(CELL * scale).color(r.status.1).wrapping(Wrapping::None).boxed(), STATUS_W, Alignment::Start),
        cell(text(r.time).font(MONO).size(CELL * scale).color(time_ink).wrapping(Wrapping::None).boxed(), TIME_W, Alignment::End),
    ]
    .spacing(ROW_GAP * scale)
    .align_y(Alignment::Center);

    let hit = button(line)
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(ROW_PAD_V * scale).bottom(ROW_PAD_V * scale).left(ROW_PAD_H * scale).right(ROW_PAD_H * scale))
        .style(move |_, status| button::Style {
            background: Some(match status {
                button::Status::Hovered | button::Status::Pressed => cp.hover,
                _ => Color::TRANSPARENT,
            }
            .into()),
            border: Border::default(),
            text_color: cp.text,
            shadow: Shadow::default(),
            snap: false,
        })
        .on_press(r.on_press);

    let mut col = column![hit].width(Length::Fill);
    col = match r.detail {
        Some(d) => col.push(detail_block(d, cp, scale)),
        None => col.push(inset_rule(cp, scale)),
    };
    col.boxed()
}

pub fn inset_rule(cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    container(compact::hairline(cp.rule))
        .width(Length::Fill)
        .padding(Padding::new(0.0).left(ROW_PAD_H * scale).right(ROW_PAD_H * scale))
        .boxed()
}

pub fn detail_block(d: Detail, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let line = move |key: &'static str, value: Element<'static, Message>| -> Element<'static, Message> {
        row![
            container(text(key).size(KEY * scale).color(cp.dim).wrapping(Wrapping::None)).width(Length::Fixed(KEY_W * scale)),
            container(value).width(Length::Fill),
        ]
        .spacing(10.0 * scale)
        .align_y(Alignment::Center)
        .padding(Padding::new(0.0).top(DETAIL_ROW_PAD_V * scale).bottom(DETAIL_ROW_PAD_V * scale).left(DETAIL_INDENT * scale)).boxed()
    };
    let id = row![
        runs(vec![Seg::mono(d.id.value, cp.text)], VALUE * scale, KEY * scale),
        grid::copy_glyph(d.id.copied, d.id.copy, cp, scale),
    ]
    .spacing(10.0 * scale)
    .align_y(Alignment::Center)
    .boxed();
    let mut col: Column<Element<'_, Message>> = column![].width(Length::Fill);
    col = col.push(line(d.id.key, id));
    for (key, segs) in d.lines {
        col = col.push(line(key, runs(segs, VALUE * scale, KEY * scale)));
    }
    if let Some(tail) = d.tail {
        let tail: Element<'static, Message> = match tail {
            Tail::Link { label, color, msg } => link(label, color, msg, cp, scale),
            Tail::Prose { s, color } => text(s).size(KEY * scale).color(color).boxed(),
        };
        col = col.push(container(tail).padding(Padding::new(0.0).top(7.0 * scale).left(DETAIL_INDENT * scale)).boxed());
    }

    column![
        container(col)
            .width(Length::Fill)
            .padding(Padding::new(0.0).bottom(9.0 * scale).left(TOKEN_ROW_PAD_H * scale).right(TOKEN_ROW_PAD_H * scale)),
        container(compact::hairline(cp.rule))
            .width(Length::Fill)
            .padding(Padding::new(0.0).left(TOKEN_ROW_PAD_H * scale).right(TOKEN_ROW_PAD_H * scale)),
    ]
    .width(Length::Fill)
    .boxed()
}

pub fn end_row(
    page: &HistoryPage,
    held: usize,
    list: HistoryList,
    cp: &'static CompactPalette,
    scale: f32,
) -> Option<Element<'static, Message>> {
    let inner: Element<'static, Message> = match page.status {
        PageStatus::Fetching => quiet_word("LOADING...", cp.faint, scale),
        PageStatus::Failed => row![
            quiet_word("couldn\u{2019}t load", cp.red, scale),
            link("retry", cp.dim, Message::HistoryLoadMore(list), cp, scale),
        ]
        .spacing(10.0 * scale)
        .align_y(Alignment::Center)
        .boxed(),
        PageStatus::Idle if page.more_than(held) => {
            link("load 20 more", cp.dim, Message::HistoryLoadMore(list), cp, scale)
        }
        PageStatus::Idle if page.capped => quiet_word("older history isn\u{2019}t fetched", cp.faint, scale),
        PageStatus::Idle => return None,
    };
    Some(
        container(inner)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .padding(Padding::new(0.0).top(6.0 * scale).bottom(7.0 * scale).left(ROW_PAD_H * scale).right(ROW_PAD_H * scale))
            .boxed(),
    )
}

fn quiet_word(word: &str, color: Color, scale: f32) -> Element<'static, Message> {
    container(text(word.to_string()).font(MONO).size(LINK * scale).color(color))
        .padding(Padding::new(3.0 * scale).left(7.0 * scale).right(7.0 * scale))
        .boxed()
}

pub fn link(label: &str, color: Color, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let word = format!("{} \u{203a}", label.to_uppercase());
    link_word(word, color, Some(msg), cp, scale)
}

pub fn link_word(word: String, color: Color, msg: Option<Message>, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let destructive = color == cp.red;
    let live = msg.is_some();
    button(text(word).font(MONO).size(LINK * scale).color(color))
        .padding(Padding::new(3.0 * scale).left(7.0 * scale).right(7.0 * scale))
        .style(move |_, status| {
            let hot = live && matches!(status, button::Status::Hovered | button::Status::Pressed);
            let (bg, ink) = match (hot, destructive) {
                (true, true) => (Color { a: 0.11, ..cp.red }, cp.red),
                (true, false) => (cp.neutral, cp.text),
                (false, _) => (Color::TRANSPARENT, color),
            };
            button::Style {
                background: Some(bg.into()),
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (5.0 * scale).into() },
                text_color: ink,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .on_press_maybe(msg)
        .boxed()
}

pub fn runs(segs: Vec<Seg>, mono_size: f32, word_size: f32) -> Element<'static, Message> {
    let spans: Vec<Span<'static, ()>> = segs
        .into_iter()
        .map(|seg| {
            let s = span(seg.s).color(seg.color);
            if seg.mono { s.size(mono_size).font(MONO) } else { s.size(word_size) }
        })
        .collect();
    rich_text(spans).wrapping(Wrapping::None).boxed()
}

pub fn token_row<'a>(
    symbol: &str,
    issuer: String,
    right: Element<'a, Message>,
    dead: bool,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let left = column![
        text(symbol.to_string()).size(SYM * scale).color(if dead { cp.muted } else { cp.text }),
        Space::new().height(3.0 * scale),
        text(issuer).size(ISSUER * scale).color(if dead { cp.faint } else { cp.dim }).wrapping(Wrapping::None),
    ];
    column![
        container(
            row![left, Space::new().width(Length::Fill), right]
                .spacing(12.0 * scale)
                .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(TOKEN_ROW_PAD_V * scale).bottom(TOKEN_ROW_PAD_V * scale).left(TOKEN_ROW_PAD_H * scale).right(TOKEN_ROW_PAD_H * scale)),
        container(compact::hairline(cp.rule)).width(Length::Fill),
    ]
    .width(Length::Fill)
    .boxed()
}

pub fn sign_mode() -> EnableInputMode {
    let w = CHANNEL.wallet_balance_rx.borrow();
    EnableInputMode::detect(w.2, w.3)
}

pub fn fact<'a>(key: &str, runs: Vec<(String, Color)>, top: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    xrpdashboard::drow(key, runs, top, cp, scale)
}

pub fn fee_row<'a>(top: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let fee = CHANNEL.xrp_node_rx.borrow().open_ledger_fee;
    fact(
        "network fee",
        vec![(fee.map_or(NA.to_string(), |d| format!("{d} drops")), cp.dim)],
        top,
        cp,
        scale,
    )
}
