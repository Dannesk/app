//! The **compose-field vocabulary** the send panes and the asset picker
//! share: the boxed plain input, the unit and asset chip that ride inside
//! it, the mono readout line, the review's address block, and the raised
//! `panel` card. Everything draws from [`CompactPalette`]; **no brand blue
//! anywhere** — the focused-field border, the address underline and links
//! are grays and severity colours only.
//!
//! This was the one-screen send surface (send v3, 2026-09-01): the framed
//! compose + live review + the floated sign stack. The frame, the review
//! rows and the stack went when both chains' sends became panes with the
//! sign block inline (XRP 2026-09-09, BTC 2026-09-10; the last caller, the
//! BTC old chain, was deleted 2026-09-10). What stayed is what the panes
//! still call.

use iced::widget::text::{LineHeight, Span, Wrapping};
use iced::widget::{button, container, rich_text, row, span, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow, Vector};

use crate::controller::message::{Message, PlainField};
use crate::ui::components::compact;
use crate::ui::components::tui::MONO_ADVANCE;
use crate::utils::fonts::MONO;
use crate::utils::plain_field::plain_field_grid_ink;
use crate::utils::theme::CompactPalette;

// ── Geometry ────────────────────────────────────────────────────────────────

/// The old compose pane's content width: 435 less 16 of padding a side.
/// The XRP send module still sizes its address box against it.
const LEFT_W: f32 = 435.0;
const PANE_PAD_H: f32 = 16.0;
pub const COMPOSE_W: f32 = LEFT_W - 2.0 * PANE_PAD_H;

/// Boxed-field padding (`5px 8px` in the mock).
const BOX_PAD_V: f32 = 5.0;
const BOX_PAD_H: f32 = 8.0;

/// A one-row [`boxed_plain`]'s height before scale — the row at its 1.6 line
/// height between the paddings — for a face that counts its content.
pub fn box_h(size: f32) -> f32 {
    size * 1.6 + 2.0 * BOX_PAD_V
}

// ── Type scale ──────────────────────────────────────────────────────────────
// On top of [`compact`]'s: the send screen's own sizes.

/// An address inside the compose box (mono).
pub const ADDR_SIZE: f32 = 11.0;
/// The fee readout line (mono).
pub const READOUT: f32 = 9.5;
/// The review address block (mono, line 1.55).
pub const RADDR: f32 = 10.0;
/// A boxed unit / the asset chip (mono upper).
pub const UNIT: f32 = 9.0;
/// The chip's own side padding — carried in both states so the lit chip is
/// exactly the size of the resting one.
const CHIP_PAD_H: f32 = 4.0;

/// What the asset chip reserves inside the amount box: its glyphs, its caret
/// and padding, plus the clearance the box leaves before the input's columns.
pub fn chip_reserve(label: &str) -> f32 {
    label.chars().count() as f32 * UNIT * 0.6 + 2.0 * CHIP_PAD_H + 18.0
}

// ── Copy ────────────────────────────────────────────────────────────────────

const REVIEW_EMPTY_ADDR: &str = "no recipient yet";

// ── Compose fields ──────────────────────────────────────────────────────────

/// A plain (non-secret) input in the compact box: 1px `border` (`focus` once
/// something is typed), radius 6, `5px 8px` padding, with optional runs at
/// either end — the `≈` glyph, a unit, the asset chip.
///
/// `width` is the whole box before scale; `lead_w`/`trail_w` are what those
/// runs reserve (their own width plus clearance), and the input takes what is
/// left, rounded down to whole columns. `wrap` lets an address break onto a
/// second row — mid-glyph, because an address is one unbroken token and where
/// its rows divide carries no meaning.
#[allow(clippy::too_many_arguments)]
pub fn boxed_plain<'a>(
    field:       PlainField,
    value:       &'a str,
    placeholder: &str,
    width:       f32,
    size:        f32,
    wrap:        bool,
    leading:     Option<Element<'a, Message>>,
    lead_w:      f32,
    trailing:    Option<Element<'a, Message>>,
    trail_w:     f32,
    on_submit:   Option<Message>,
    cp:          &'static CompactPalette,
    scale:       f32,
) -> Element<'a, Message> {
    let s = size * scale;
    let advance = s * MONO_ADVANCE;
    let cols = ((width - 2.0 * BOX_PAD_H - lead_w - trail_w) * scale / advance)
        .floor()
        .max(1.0) as usize;
    // The multiline switch only when the content needs it: an empty box rests
    // one row tall, and grows the moment a pasted address outruns the columns.
    let min_rows = if wrap && value.chars().count() > cols { 2 } else { 1 };
    let (input, _) = plain_field_grid_ink(
        field,
        value,
        cols,
        min_rows,
        placeholder,
        on_submit,
        advance,
        s * 1.6,
        s,
        MONO,
        cp.text,
        cp.focus,
        cp.faint,
    );

    let mut inner = row![].align_y(Alignment::Center);
    if let Some(l) = leading {
        inner = inner.push(l);
    }
    inner = inner
        .push(container(input).width(Length::Fixed(cols as f32 * advance)))
        .push(Space::new().width(Length::Fill));
    if let Some(t) = trailing {
        inner = inner.push(t);
    }

    let line = if value.trim().is_empty() { cp.border } else { cp.focus };
    container(inner)
        .width(Length::Fixed(width * scale))
        .padding(
            Padding::new(0.0)
                .top(BOX_PAD_V * scale)
                .bottom(BOX_PAD_V * scale)
                .left(BOX_PAD_H * scale)
                .right(BOX_PAD_H * scale),
        )
        .style(move |_| container::Style {
            background: Some(cp.field.into()),
            border: Border { color: line, width: 1.0, radius: (6.0 * scale).into() },
            ..Default::default()
        })
        .into()
}

/// A unit riding inside a box — `usd`, `btc`, `sats`: mono 9 upper `muted`.
pub fn unit<'a>(u: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    text(u.to_uppercase())
        .font(MONO)
        .size(UNIT * scale)
        .color(cp.muted)
        .into()
}

/// The asset chip in the crypto box: `xrp ▾`, mono 9 upper `dim` with a small
/// `faint` caret, lifting to `text` on hover. Opens the held-asset picker.
///
/// `open` is the lit state it wears **while the picker stack is up**: `pill`
/// fill, `text` ink, the caret a step brighter — so the origin of the stack
/// stays visible behind the scrim, which is also where the close affordance is
/// (the stack itself carries no close glyph). The padding is the same either
/// way, so lighting the chip never reflows the field around it.
pub fn asset_chip<'a>(
    label: &str,
    open:  bool,
    msg:   Message,
    cp:    &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    button(
        row![
            text(label.to_uppercase()).font(MONO).size(UNIT * scale),
            // The caret is JetBrains Mono's, like the panes' row carets;
            // Inter has no ▾.
            text("\u{25be}")
                .font(MONO)
                .size(7.0 * scale)
                .color(if open { cp.dim } else { cp.faint }),
        ]
        .spacing(4.0 * scale)
        .align_y(Alignment::Center),
    )
    .on_press(msg)
    .padding(Padding::new(1.0 * scale).left(CHIP_PAD_H * scale).right(CHIP_PAD_H * scale))
    .style(move |_, status| {
        let hot = open || matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if open { cp.pill } else { Color::TRANSPARENT }.into()),
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (4.0 * scale).into() },
            text_color: if hot { cp.text } else { cp.dim },
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .into()
}

/// A run of mono spans on one line — the hint / readout voice.
pub fn mono_line<'a>(runs: Vec<(String, Color)>, size: f32, scale: f32) -> Element<'a, Message> {
    compact::mono_runs(runs, size * scale)
}

/// The address, drawn in full at mono 10 / 1.55, wrapping mid-glyph, the last
/// `tail_len` characters underlined — the same tail the caution line names.
pub fn address_block<'a>(
    address:  Option<&str>,
    tail_len: usize,
    cp:       &'static CompactPalette,
    scale:    f32,
) -> Element<'a, Message> {
    let Some(addr) = address.filter(|a| !a.is_empty()) else {
        return text(REVIEW_EMPTY_ADDR)
            .font(MONO)
            .size(RADDR * scale)
            .color(cp.faint)
            .into();
    };
    let chars = addr.chars().count();
    let split = chars.saturating_sub(tail_len);
    let head: String = addr.chars().take(split).collect();
    let tail: String = addr.chars().skip(split).collect();
    let spans: Vec<Span<'static, ()>> = vec![
        span(head).size(RADDR * scale).font(MONO).color(cp.text),
        span(tail).size(RADDR * scale).font(MONO).color(cp.text).underline(true),
    ];
    container(
        rich_text(spans)
            .line_height(LineHeight::Relative(1.55))
            .wrapping(Wrapping::Glyph),
    )
    .width(Length::Fill)
    .into()
}

/// The raised card a stack wears: `panel` fill, 1px `border`, radius 10 and
/// the mock's one shadow, at whatever width and padding the stack needs —
/// the asset picker's card, and the old sign stack's before it.
pub fn panel<'a>(
    body:    Element<'a, Message>,
    width:   f32,
    padding: Padding,
    cp:      &'static CompactPalette,
    scale:   f32,
) -> Element<'a, Message> {
    container(body)
        .width(Length::Fixed(width * scale))
        .padding(padding)
        .style(move |_| container::Style {
            background: Some(cp.panel.into()),
            border: Border { color: cp.border, width: 1.0, radius: (10.0 * scale).into() },
            shadow: Shadow {
                color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.45 },
                offset: Vector::new(0.0, 26.0 * scale),
                blur_radius: 60.0 * scale,
            },
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The compose address box wraps rather than scrolls: an XRP r-address
    /// (35) fits one row, and the longest address either chain sends to — a
    /// 74-character bech32 — lands on the two rows the box grows to.
    #[test]
    fn the_address_box_wraps_sanely() {
        let advance = ADDR_SIZE * MONO_ADVANCE;
        let cols = ((COMPOSE_W - 2.0 * BOX_PAD_H) / advance).floor();
        assert!(cols >= 35.0, "an r-address no longer fits one row ({cols} cols)");
        assert!((74.0 / cols).ceil() <= 2.0, "a long bech32 address outgrows two rows");
    }
}
