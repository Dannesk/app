

use iced::widget::Row;
use iced::Widget as _;
use iced::widget::text::{LineHeight, Span, Wrapping};
use iced::widget::{button, container, rich_text, row, span, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow, Vector};

use crate::controller::message::{Message, PlainField};
use crate::ui::components::compact;
use crate::ui::components::tui::MONO_ADVANCE;
use crate::utils::fonts::MONO;
use crate::utils::plain_field::plain_field_grid_ink;
use crate::utils::theme::CompactPalette;


const LEFT_W: f32 = 435.0;
const PANE_PAD_H: f32 = 16.0;
pub const COMPOSE_W: f32 = LEFT_W - 2.0 * PANE_PAD_H;

const BOX_PAD_V: f32 = 5.0;
const BOX_PAD_H: f32 = 8.0;


pub fn box_h(size: f32) -> f32 {
    size * 1.6 + 2.0 * BOX_PAD_V
}



pub const ADDR_SIZE: f32 = 11.0;
pub const READOUT: f32 = 9.5;
pub const RADDR: f32 = 10.0;
pub const UNIT: f32 = 9.0;

const CHIP_PAD_H: f32 = 4.0;


pub fn chip_reserve(label: &str) -> f32 {
    label.chars().count() as f32 * UNIT * 0.6 + 2.0 * CHIP_PAD_H + 18.0
}


const REVIEW_EMPTY_ADDR: &str = "no recipient yet";


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

    let mut inner: Row<Element<'_, Message>> = row![].align_y(Alignment::Center);
    if let Some(l) = leading {
        inner = inner.push(l);
    }
    inner = inner
        .push(container(input).width(Length::Fixed(cols as f32 * advance)).boxed())
        .push(Space::new().width(Length::Fill).boxed());
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
        .boxed()
}

pub fn unit<'a>(u: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    text(u.to_uppercase())
        .font(MONO)
        .size(UNIT * scale)
        .color(cp.muted)
        .boxed()
}


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
    .boxed()
}

pub fn mono_line<'a>(runs: Vec<(String, Color)>, size: f32, scale: f32) -> Element<'a, Message> {
    compact::mono_runs(runs, size * scale)
}


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
            .boxed();
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
    .boxed()
}

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
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn the_address_box_wraps_sanely() {
        let advance = ADDR_SIZE * MONO_ADVANCE;
        let cols = ((COMPOSE_W - 2.0 * BOX_PAD_H) / advance).floor();
        assert!(cols >= 35.0, "an r-address no longer fits one row ({cols} cols)");
        assert!((74.0 / cols).ceil() <= 2.0, "a long bech32 address outgrows two rows");
    }
}
