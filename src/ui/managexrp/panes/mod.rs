//! Phase 2 of the XRP pane grid — the remaining screens as **content in
//! boxes that already work** (design handoff `design_handoff_xrp_panes`,
//! 2026-09-09). Nine pane faces: `wallet` (+ `restore key`), `ticket` (+ the
//! pair search), `order book`, `depth`, `transactions`, `orders` (+ `cancel
//! offer`), `tokens`, `available tokens` (+ `enable trustline`).
//!
//! **The logic already exists.** Every face here draws an existing screen at
//! 299px through the same state, messages and controller paths that screen
//! uses — the ticket's two contracts, the picker's measured depth, the
//! transactions row model, the tokens page's reserve gate, key management's
//! one-click mode change. What the panes add is layout, and two things the
//! handoff calls genuinely new: the **pair search** (in the ticket at first;
//! in the top bar since 2026-09-11 — see [`pair`]) and the **empty-list
//! glyph**.
//!
//! ## Where this file and the handoff part ways (user, 2026-09-09)
//!
//! - Signing is **inside the owning pane** (the grid's rule since phase 1),
//!   and every form asks for what the wallet actually takes: the encryption
//!   key plus the optional 25th word under Standard, the 24 words plus the
//!   25th word under cold storage. The 25th word is never stored, so it is
//!   asked for every time.
//! - The ticket IS our trade logic — `xrptrade::ticket::form`, the same code
//!   the trade screen draws: amount is the base and total the quote on both
//!   sides, Market offered only where a book can be walked, the stats as the
//!   review. The pair search shows measured depth, not a spread rule. The
//!   mock's quick fills were dropped with the rest of its ticket (user: "the
//!   only good thing about the designer was the search").
//! - No default market. Nothing is held until a pair is picked in the top
//!   bar; a default pair would promote one stablecoin over another. The last
//!   pick is restored on relaunch.
//! - Neither wallet action confirms. `Remove key` and `Remove wallet` run
//!   on the press; the state change is the receipt.
//! - No entropy meter under the 25th word on restore — it is being recalled,
//!   not chosen. The encryption key's meter stays: that one is new.
//! - The orders pane keeps `open` (not `pending`) and our cancel lead; the
//!   detail block keeps our party and flag rules. Its type column reads
//!   `sell` / `buy` — in a list of nothing but offers, `trade` on every row
//!   said nothing.
//!
//! ## What is shared here
//!
//! The list scroller, the list row and its detail block, the empty state, the
//! inline sign block, and the three button voices (`neutral`, `pill`, and the
//! quiet/destructive outlines). The pane faces themselves live in the
//! sibling files, one per handoff section.

pub mod lists;
pub mod market;
pub mod pair;
pub mod ticket;
pub mod tokens;
pub mod wallet;

use iced::widget::text::{Span, Wrapping};
use iced::widget::{button, column, container, rich_text, row, span, svg, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::channel::CHANNEL;
use crate::controller::app_state::EnableInputMode;
use crate::controller::message::Message;
use crate::ui::components::compact;
use crate::ui::components::tx_panel::{Detail, Seg, Tail};
use crate::ui::managexrp::xrpdashboard::{self, STRIP};
use crate::utils::fonts::MONO;
use crate::utils::icons;
use crate::utils::theme::CompactPalette;

// The list scroller, the three button voices and the inline sign block
// moved to the shared chrome when BTC got its grid (2026-09-10); the pane
// files keep importing them from here.
pub use crate::ui::components::grid::{button_pair, danger_button, primary, primary_tinted, quiet_button, scroller, sign_block};

// ── Type scale (the handoff's, before `scale()`) ────────────────────────────

/// The state line — `Standard`, `Cold storage` (mono).
pub const BIG: f32 = 13.0;
/// A list row's caret (mono) and its width.
const CARET: f32 = 8.0;
const CARET_W: f32 = 6.0;
/// A list row's type word (Inter) and the column's two widths.
const TYPE: f32 = 10.0;
const TYPE_W: f32 = 42.0;
const SIDE_W: f32 = 24.0;
/// Amount · status · time (mono).
const CELL: f32 = 9.5;
const STATUS_W: f32 = 42.0;
const TIME_W: f32 = 66.0;
/// Between the row's cells; the row's padding.
const ROW_GAP: f32 = 5.0;
const ROW_PAD_V: f32 = 6.0;
const ROW_PAD_H: f32 = 11.0;
/// The detail block: key (Inter) and its width, value (mono), prose (Inter).
const KEY: f32 = 9.5;
const KEY_W: f32 = 44.0;
const VALUE: f32 = 10.0;
const DETAIL_INDENT: f32 = 12.0;
const DETAIL_ROW_PAD_V: f32 = 2.5;
/// Links (mono upper).
const LINK: f32 = 8.5;
/// The empty state's line (Inter) and glyph.
const EMPTY_LINE: f32 = 11.5;
const EMPTY_GLYPH: f32 = 22.0;
const EMPTY_GAP: f32 = 10.0;
/// A token row (Inter symbol / issuer, mono value / sub).
pub const SYM: f32 = 12.0;
pub const ISSUER: f32 = 10.5;
pub const VAL: f32 = 12.5;
pub const SUB: f32 = 10.5;
pub const TOKEN_ROW_PAD_V: f32 = 8.0;
pub const TOKEN_ROW_PAD_H: f32 = 12.0;

pub(crate) const NA: &str = xrpdashboard::NA;

// ── Lists ───────────────────────────────────────────────────────────────────

/// `No transactions to show` — the glyph over the line, centred in whatever
/// height the pane has. No call to action, no explanation: nothing is broken
/// and there is nothing to do about it.
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
    .into()
}

/// A list group head — `settled` — mono 8.5 upper `muted` on a `rule`.
/// Earned only when there are settled orders under live ones.
pub fn group_head<'a>(word: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    column![
        container(text(word.to_uppercase()).font(MONO).size(STRIP * scale).color(cp.muted))
            .width(Length::Fill)
            .padding(Padding::new(0.0).top(9.0 * scale).bottom(6.0 * scale).left(ROW_PAD_H * scale).right(ROW_PAD_H * scale)),
        compact::hairline(cp.rule),
    ]
    .width(Length::Fill)
    .into()
}

/// One list row, the shipped transactions row narrowed: `▸` · type · amount ·
/// status · timestamp, every column kept. `side` draws the type column at the
/// orders pane's width. An expanded row drops its rule and hangs its detail.
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
        container(el).width(Length::Fixed(w * scale)).align_x(align).into()
    };

    let line = row![
        cell(text(if open { "\u{25be}" } else { "\u{25b8}" }).font(MONO).size(CARET * scale).color(quiet).into(), CARET_W, Alignment::Start),
        cell(text(r.kind).size(TYPE * scale).color(cp.text).wrapping(Wrapping::None).into(), type_w, Alignment::Start),
        container(runs(r.amount, CELL * scale, CELL * scale)).width(Length::Fill).align_x(Alignment::End),
        cell(text(r.status.0).font(MONO).size(CELL * scale).color(r.status.1).wrapping(Wrapping::None).into(), STATUS_W, Alignment::Start),
        cell(text(r.time).font(MONO).size(CELL * scale).color(time_ink).wrapping(Wrapping::None).into(), TIME_W, Alignment::End),
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
    col.into()
}

pub fn inset_rule(cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    container(compact::hairline(cp.rule))
        .width(Length::Fill)
        .padding(Padding::new(0.0).left(ROW_PAD_H * scale).right(ROW_PAD_H * scale))
        .into()
}

/// The detail dropdown, unchanged from the transactions spec: key/value
/// lines indented under the type column, the tail link, one rule under the
/// group. It scrolls with everything else — a dropdown never resizes the pane.
pub fn detail_block(d: Detail, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let mut col = column![].width(Length::Fill);
    for (key, segs) in d.lines {
        col = col.push(
            row![
                container(text(key).size(KEY * scale).color(cp.dim)).width(Length::Fixed(KEY_W * scale)),
                container(runs(segs, VALUE * scale, KEY * scale)).width(Length::Fill),
            ]
            .spacing(10.0 * scale)
            .align_y(Alignment::Center)
            .padding(Padding::new(0.0).top(DETAIL_ROW_PAD_V * scale).bottom(DETAIL_ROW_PAD_V * scale).left(DETAIL_INDENT * scale)),
        );
    }
    let tail: Element<'static, Message> = match d.tail {
        Tail::Link { label, color, msg } => link(label, color, msg, cp, scale),
        Tail::Prose { s, color } => text(s).size(KEY * scale).color(color).into(),
    };
    col = col.push(container(tail).padding(Padding::new(0.0).top(7.0 * scale).left(DETAIL_INDENT * scale)));

    column![
        container(col)
            .width(Length::Fill)
            .padding(Padding::new(0.0).bottom(9.0 * scale).left(TOKEN_ROW_PAD_H * scale).right(TOKEN_ROW_PAD_H * scale)),
        container(compact::hairline(cp.rule))
            .width(Length::Fill)
            .padding(Padding::new(0.0).left(TOKEN_ROW_PAD_H * scale).right(TOKEN_ROW_PAD_H * scale)),
    ]
    .width(Length::Fill)
    .into()
}

/// A link — `copy hash ›`, `cancel offer ›`, `enable`: mono 8.5 upper in a
/// caller-chosen ink, `neutral` wash and `text` on hover; the destructive one
/// keeps its red and washes with the ask tint.
pub fn link(label: &str, color: Color, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let word = format!("{} \u{203a}", label.to_uppercase());
    link_word(word, color, Some(msg), cp, scale)
}

/// [`link`] without the `›` — the row-end `enable`. Dead when `msg` is `None`.
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
        .into()
}

/// A value's runs as one rich text: mono segs at `mono_size`, word segs at
/// `word_size`, sharing a baseline.
pub fn runs(segs: Vec<Seg>, mono_size: f32, word_size: f32) -> Element<'static, Message> {
    let spans: Vec<Span<'static, ()>> = segs
        .into_iter()
        .map(|seg| {
            let s = span(seg.s).color(seg.color);
            if seg.mono { s.size(mono_size).font(MONO) } else { s.size(word_size) }
        })
        .collect();
    rich_text(spans).wrapping(Wrapping::None).into()
}

/// A token-style row: symbol over issuer left, whatever the caller puts
/// right, a `rule` under it, the pane's gutters.
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
    .into()
}

// ── Signing, inline ─────────────────────────────────────────────────────────

/// The credential a signing form asks for, resolved from the wallet: the key
/// under Standard, the phrase under cold storage. Detected, never chosen.
pub fn sign_mode() -> EnableInputMode {
    let w = CHANNEL.wallet_balance_rx.borrow();
    EnableInputMode::detect(w.2, w.3)
}

/// The fact rows above a signing form share the dashboard's row.
pub fn fact<'a>(key: &str, runs: Vec<(String, Color)>, top: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    xrpdashboard::drow(key, runs, top, cp, scale)
}

/// The network fee row every signing form ends its facts with: the open-ledger
/// fee in drops, `dim`; `—` before the node frame lands.
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
