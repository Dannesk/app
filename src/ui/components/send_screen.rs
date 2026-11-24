

use iced::widget::Row;
use iced::Widget as _;
use iced::widget::text::{Ellipsis, LineHeight, Span, Wrapping};
use iced::widget::{button, column, container, mouse_area, rich_text, row, span, svg, text, Space};
use iced::{mouse, Alignment, Border, Color, Element, Length, Padding, Shadow, Vector};

use crate::controller::message::{Message, PlainField};
use crate::ui::components::compact;
use crate::ui::components::grid::{ROW_KEY, ROW_TOTAL};
use crate::ui::components::tui::MONO_ADVANCE;
use crate::utils::fonts::MONO;
use crate::utils::icons;
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

pub fn boxed_h(value: &str, width: f32, size: f32) -> f32 {
    let cols = ((width - 2.0 * BOX_PAD_H) / (size * MONO_ADVANCE)).floor().max(1.0) as usize;
    let n = value.chars().count();
    let rows = if n > cols { n.div_ceil(cols) } else { 1 };
    box_h(size) + (rows - 1) as f32 * size * 1.6
}

pub fn boxed_h_seg(value: &str, width: f32, size: f32, seg_w: f32) -> f32 {
    boxed_h(value, width - seg_w - SEG_INSET, size)
}

pub const GUTTER_W: f32 = 9.0;
pub const SETTING_ROW_H: f32 = 37.0;
const SETTING_NAME: f32 = 11.0;

pub const SEG_INSET: f32 = 2.0;
pub const ROW_INSET: f32 = 1.5;
pub const ROW_SLOT: f32 = SETTING_ROW_H + 2.0 * ROW_INSET;
const ROW_RADIUS: f32 = 5.0;
pub const SMALL_RADIUS: f32 = 4.5;
pub const REVIEW_W: f32 = 318.0;
const LOCK_SIZE: f32 = 10.0;



pub const ADDR_SIZE: f32 = 11.0;
pub const READOUT: f32 = 9.5;
pub const RADDR: f32 = 10.0;
pub const UNIT: f32 = 9.0;

const CHIP_PAD_H: f32 = 4.0;


pub fn chip_reserve(label: &str) -> f32 {
    label.chars().count() as f32 * UNIT * 0.6 + 2.0 * CHIP_PAD_H + 18.0
}


const REVIEW_EMPTY_ADDR: &str = "No recipient yet";


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
    boxed_field(field, value, placeholder, width, size, wrap, leading, lead_w, trailing, trail_w, on_submit, 0, None, cp, scale)
}

#[allow(clippy::too_many_arguments)]
pub fn boxed_address<'a>(
    field:       PlainField,
    value:       &'a str,
    placeholder: &str,
    width:       f32,
    size:        f32,
    tail:        usize,
    segment:     Option<(Element<'a, Message>, f32)>,
    cp:          &'static CompactPalette,
    scale:       f32,
) -> Element<'a, Message> {
    boxed_field(field, value, placeholder, width, size, true, None, 0.0, None, 0.0, None, tail, segment, cp, scale)
}

#[allow(clippy::too_many_arguments)]
fn boxed_field<'a>(
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
    tail:        usize,
    segment:     Option<(Element<'a, Message>, f32)>,
    cp:          &'static CompactPalette,
    scale:       f32,
) -> Element<'a, Message> {
    let s = size * scale;
    let advance = s * MONO_ADVANCE;
    let (segment, seg_w) = match segment {
        Some((e, w)) => (Some(e), w + SEG_INSET),
        None => (None, 0.0),
    };
    let cols = ((width - 2.0 * BOX_PAD_H - lead_w - trail_w - seg_w) * scale / advance)
        .floor()
        .max(1.0) as usize;

    let min_rows = if wrap && value.chars().count() > cols { 2 } else { 1 };
    let (input, rows) = plain_field_grid_ink(
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
        cp.text,
        cp.faint,
        tail,
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
    let padded = container(inner).width(Length::Fill).padding(
        Padding::new(0.0)
            .top(BOX_PAD_V * scale)
            .bottom(BOX_PAD_V * scale)
            .left(BOX_PAD_H * scale)
            .right(BOX_PAD_H * scale),
    );
    let body: Element<'a, Message> = match segment {
        None => padded.boxed(),
        Some(seg) => {
            let h = rows as f32 * s * 1.6 + 2.0 * (BOX_PAD_V - SEG_INSET) * scale;
            row![
                padded,
                container(seg).height(Length::Fixed(h)).align_y(Alignment::Center),
                Space::new().width(SEG_INSET * scale),
            ]
            .align_y(Alignment::Center)
            .boxed()
        }
    };
    container(body)
        .width(Length::Fixed(width * scale))
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

pub fn setting_row<'a>(
    name:    &str,
    caption: Option<&str>,
    value:   String,
    open:    bool,
    msg:     Option<Message>,
    cp:      &'static CompactPalette,
    scale:   f32,
) -> Element<'a, Message> {
    let mut left = column![text(name.to_string()).size(SETTING_NAME * scale).color(cp.text)].spacing(2.0 * scale);
    if let Some(c) = caption {
        left = left.push(
            text(c.to_string())
                .size(ROW_KEY * scale)
                .color(cp.dim)
                .wrapping(Wrapping::None)
                .ellipsis(Ellipsis::End)
                .boxed(),
        );
    }
    let mark: Element<'a, Message> = if msg.is_some() { chevron(open, cp, scale) } else { Space::new().boxed() };
    let body = container(
        row![
            container(left).width(Length::Fill).clip(true),
            text(value).font(MONO).size(ROW_TOTAL * scale).color(cp.dim),
            container(mark).width(Length::Fixed(GUTTER_W * scale)).align_x(Alignment::End),
        ]
        .spacing(10.0 * scale)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(SETTING_ROW_H * scale))
    .align_y(Alignment::Center)
    .boxed();
    row_button(body, msg, cp, scale)
}

pub fn row_button<'a>(
    body: Element<'a, Message>,
    msg: Option<Message>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let live = msg.is_some();
    container(
        button(body)
            .on_press_maybe(msg)
            .width(Length::Fill)
            .padding(Padding::new(0.0).left(4.0 * scale).right(4.0 * scale))
            .style(move |_, status| row_style(live, status, ROW_RADIUS * scale, cp)),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(ROW_INSET * scale).bottom(ROW_INSET * scale))
    .boxed()
}

pub fn row_style(live: bool, status: button::Status, radius: f32, cp: &'static CompactPalette) -> button::Style {
    let wash = match status {
        button::Status::Hovered if live => cp.hover,
        button::Status::Pressed if live => cp.pill,
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(wash.into()),
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: radius.into() },
        text_color: cp.text,
        shadow: Shadow::default(),
        snap: false,
    }
}

pub fn chevron<'a>(open: bool, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let (handle, ink) = if open {
        (icons::CHEVRON_DOWN.clone(), cp.accent)
    } else {
        (icons::CHEVRON_RIGHT.clone(), cp.faint)
    };
    let s = GUTTER_W * scale;
    svg(handle)
        .width(Length::Fixed(s))
        .height(Length::Fixed(s))
        .style(move |_, _| svg::Style { color: Some(ink) })
        .boxed()
}

pub fn lock<'a>(cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let s = LOCK_SIZE * scale;
    svg(icons::LOCK.clone())
        .width(Length::Fixed(s))
        .height(Length::Fixed(s))
        .style(move |_, _| svg::Style { color: Some(cp.muted) })
        .boxed()
}

pub const STAGE_PAD: f32 = 12.0;
pub const STAGE_MIN: f32 = 104.0;
pub const STATUS_H: f32 = 17.0;

pub fn stage_box<'a>(content: Element<'a, Message>, scale: f32) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .padding(Padding::new(STAGE_PAD * scale).left(0.0).right(0.0))
        .boxed()
}

pub fn focus_stage<'a>(content: Element<'a, Message>, field: PlainField, scale: f32) -> Element<'a, Message> {
    mouse_area(stage_box(content, scale))
        .on_press(Message::FocusField(field))
        .interaction(mouse::Interaction::Text)
        .boxed()
}

pub fn status_row<'a>(content: Element<'a, Message>, scale: f32) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fixed(STATUS_H * scale))
        .align_y(Alignment::Center)
        .boxed()
}

pub fn review_column<'a>(content: Element<'a, Message>, pad_v: f32, scale: f32) -> Element<'a, Message> {
    container(container(content).width(Length::Fill.max(REVIEW_W * scale)))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(Padding::new(pad_v * scale).left(0.0).right(0.0))
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
