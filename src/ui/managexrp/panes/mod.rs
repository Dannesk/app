
pub mod lists;
pub mod market;
pub mod pair;
pub mod ticket;
pub mod tokens;
pub mod wallet;

use iced::widget::Row;
use iced::Widget as _;
use iced::widget::text::{Span, Wrapping};
use iced::widget::{button, column, container, rich_text, row, span, svg, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::channel::{CHANNEL, HistoryList, HistoryPage, PageStatus};
use crate::controller::app_state::EnableInputMode;
use crate::controller::message::Message;
use crate::ui::components::{compact, grid};
use crate::ui::components::tx_panel::Seg;
use crate::ui::managexrp::xrpdashboard::{self, STRIP};
use crate::utils::fonts::MONO;
use crate::utils::icons;
use crate::utils::theme::CompactPalette;

pub use crate::ui::components::grid::{button_pair, danger_button, primary, primary_tinted, quiet_button, scroller, sign_block};

pub const BIG: f32 = 13.0;
pub(crate) const ROW_PAD_H: f32 = 11.0;
pub(crate) const KEY: f32 = 9.5;
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

pub struct HistoryRow {
    pub word: &'static str,
    pub status: (&'static str, Color),
    pub figure: String,
    pub figure_ink: Color,
    pub unit: String,
    pub stamp: String,
    pub stamp_ink: Color,
    pub on_press: Message,
}

pub fn history_row(r: HistoryRow, rule: bool, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let left = column![
        text(r.word).size(SYM * scale).color(cp.text).wrapping(Wrapping::None),
        Space::new().height(3.0 * scale),
        text(r.status.0).font(MONO).size(SUB * scale).color(r.status.1).wrapping(Wrapping::None),
    ];
    let mut figure: Row<Element<'static, Message>> = row![
        text(r.figure).font(MONO).size(VAL * scale).color(r.figure_ink).wrapping(Wrapping::None),
    ]
    .align_y(Alignment::End);
    if !r.unit.is_empty() {
        figure = figure
            .push(Space::new().width(5.0 * scale).boxed())
            .push(text(r.unit).font(MONO).size(SUB * scale).color(cp.muted).boxed());
    }
    let right = column![
        figure,
        Space::new().height(3.0 * scale),
        text(r.stamp).font(MONO).size(SUB * scale).color(r.stamp_ink).wrapping(Wrapping::None),
    ]
    .align_x(Alignment::End);
    let line = container(row![left, Space::new().width(Length::Fill), right].spacing(12.0 * scale).align_y(Alignment::Center))
        .width(Length::Fill)
        .padding(
            Padding::new(0.0)
                .top(TOKEN_ROW_PAD_V * scale)
                .bottom(TOKEN_ROW_PAD_V * scale)
                .left(TOKEN_ROW_PAD_H * scale)
                .right(TOKEN_ROW_PAD_H * scale),
        );

    let mut col = column![pressable(line.boxed(), Some(r.on_press), cp)].width(Length::Fill);
    if rule {
        col = col.push(
            container(compact::hairline(cp.rule))
                .width(Length::Fill)
                .padding(Padding::new(0.0).left(TOKEN_ROW_PAD_H * scale).right(TOKEN_ROW_PAD_H * scale))
                .boxed(),
        );
    }
    col.boxed()
}

pub fn fact_row(
    key: &'static str,
    lead: Option<Element<'static, Message>>,
    segs: Vec<Seg>,
    copy: Option<Element<'static, Message>>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'static, Message> {
    let mut value: Row<Element<'static, Message>> = row![].spacing(6.0 * scale).align_y(Alignment::Center);
    if let Some(l) = lead {
        value = value.push(l);
    }
    value = value.push(runs(segs, grid::ROW_VALUE * scale, grid::ROW_KEY * scale));
    if let Some(c) = copy {
        value = value.push(c);
    }
    container(
        row![
            text(key).size(grid::ROW_KEY * scale).color(cp.dim).wrapping(Wrapping::None),
            Space::new().width(Length::Fill),
            value,
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(3.0 * scale).bottom(3.0 * scale))
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
        PageStatus::Fetching => quiet_word("Loading...", cp.faint, scale),
        PageStatus::Failed => row![
            quiet_word("Couldn\u{2019}t load", cp.red, scale),
            link_word("Retry".to_string(), cp.text, Some(Message::HistoryLoadMore(list)), cp, scale),
        ]
        .spacing(10.0 * scale)
        .align_y(Alignment::Center)
        .boxed(),
        PageStatus::Idle if page.more_than(held) => {
            link_word("Load 20 more".to_string(), cp.dim, Some(Message::HistoryLoadMore(list)), cp, scale)
        }
        PageStatus::Idle if page.capped => quiet_word("Older history isn\u{2019}t fetched", cp.faint, scale),
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
    let word = format!("{} \u{203a}", label);
    link_word(word, color, Some(msg), cp, scale)
}

pub fn link_word(word: String, color: Color, msg: Option<Message>, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    link_button(text(word).font(MONO).size(LINK * scale).color(color).boxed(), color, msg, cp, scale)
}

pub fn swap_word(word: String, color: Color, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let content = row![
        text(word).font(MONO).size(LINK * scale).color(color),
        Space::new().width(6.0 * scale),
        grid::swap_glyph(LINK * scale, color),
    ]
    .align_y(Alignment::Center);
    link_button(content.boxed(), color, Some(msg), cp, scale)
}

fn link_button(
    content: Element<'static, Message>,
    color: Color,
    msg: Option<Message>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'static, Message> {
    let destructive = color == cp.red;
    let live = msg.is_some();
    button(content)
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

pub fn pressable<'a>(content: Element<'a, Message>, msg: Option<Message>, cp: &'static CompactPalette) -> Element<'a, Message> {
    let live = msg.is_some();
    button(content)
        .on_press_maybe(msg)
        .padding(0)
        .width(Length::Fill)
        .style(move |_, status| {
            let hot = live && matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if hot { cp.hover } else { Color::TRANSPARENT }.into()),
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 0.0.into() },
                text_color: cp.text,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}

pub fn fee_row<'a>(fee: Option<u64>, top: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    fact(
        "Network fee",
        vec![(fee.map_or(NA.to_string(), |d| format!("{d} drops")), fee_ink(fee, cp))],
        top,
        cp,
        scale,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeeTone {
    Plain,
    High,
    Extreme,
}

pub const HIGH_FEE_MULTIPLE: u64 = 10;
pub const EXTREME_FEE_MULTIPLE: u64 = 100;

pub fn fee_tone(drops: u64, base: Option<u64>) -> FeeTone {
    match base {
        Some(b) if b > 0 && drops > b.saturating_mul(EXTREME_FEE_MULTIPLE) => FeeTone::Extreme,
        Some(b) if b > 0 && drops > b.saturating_mul(HIGH_FEE_MULTIPLE) => FeeTone::High,
        _ => FeeTone::Plain,
    }
}

pub fn fee_ink(drops: Option<u64>, cp: &'static CompactPalette) -> Color {
    let base = CHANNEL.xrp_node_rx.borrow().fee_base;
    match drops.map(|d| fee_tone(d, base)) {
        Some(FeeTone::Extreme) => cp.red,
        Some(FeeTone::High) => cp.amber,
        _ => cp.dim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fee_turns_amber_past_ten_times_the_base_and_red_past_a_hundred() {
        assert_eq!(fee_tone(10, Some(10)), FeeTone::Plain);
        assert_eq!(fee_tone(100, Some(10)), FeeTone::Plain);
        assert_eq!(fee_tone(101, Some(10)), FeeTone::High);
        assert_eq!(fee_tone(1000, Some(10)), FeeTone::High);
        assert_eq!(fee_tone(1001, Some(10)), FeeTone::Extreme);
    }

    #[test]
    fn no_base_fee_means_no_tone() {
        assert_eq!(fee_tone(5000, None), FeeTone::Plain);
        assert_eq!(fee_tone(5000, Some(0)), FeeTone::Plain);
    }
}
