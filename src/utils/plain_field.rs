
use iced::Widget as _;
use iced::{Color, Element, Font, Length};
use crate::controller::message::{Message, PlainField, SecureOp};
use crate::utils::secure_input::{grid_rows, SecureInput};

#[allow(clippy::too_many_arguments)]
pub fn plain_field_grid_ink<'a>(
    field: PlainField,
    value: &'a str,
    cols: usize,
    min_rows: usize,
    placeholder: &str,
    on_submit: Option<Message>,
    advance: f32,
    line_height: f32,
    size: f32,
    font: Font,
    text: Color,
    caret: Color,
    placeholder_color: Color,
    tail: usize,
) -> (Element<'a, Message>, usize) {
    grid_field(field, value, cols, min_rows, placeholder, on_submit, advance, line_height, size, font, text, caret, placeholder_color, None, false, None, tail)
}

#[allow(clippy::too_many_arguments)]
pub fn plain_field_grid_ink_focus<'a>(
    field: PlainField,
    value: &'a str,
    cols: usize,
    min_rows: usize,
    placeholder: &str,
    on_submit: Option<Message>,
    advance: f32,
    line_height: f32,
    size: f32,
    font: Font,
    text: Color,
    caret: Color,
    placeholder_color: Color,
    placeholder_font: Option<Font>,
    autofocus: bool,
) -> (Element<'a, Message>, usize) {
    grid_field(field, value, cols, min_rows, placeholder, on_submit, advance, line_height, size, font, text, caret, placeholder_color, placeholder_font, autofocus, None, 0)
}

#[allow(clippy::too_many_arguments)]
pub fn plain_field_grid_requested<'a>(
    field: PlainField,
    value: &'a str,
    cols: usize,
    placeholder: &str,
    on_submit: Option<Message>,
    advance: f32,
    line_height: f32,
    size: f32,
    font: Font,
    text: Color,
    caret: Color,
    placeholder_color: Color,
    focus: Option<u32>,
) -> Element<'a, Message> {
    grid_field(field, value, cols, 1, placeholder, on_submit, advance, line_height, size, font, text, caret, placeholder_color, None, false, focus, 0).0
}

#[allow(clippy::too_many_arguments)]
pub fn plain_field_grid_ink_requested<'a>(
    field: PlainField,
    value: &'a str,
    cols: usize,
    placeholder: &str,
    on_submit: Option<Message>,
    advance: f32,
    line_height: f32,
    size: f32,
    font: Font,
    text: Color,
    caret: Color,
    placeholder_color: Color,
    placeholder_font: Font,
    focus: Option<u32>,
) -> Element<'a, Message> {
    grid_field(field, value, cols, 1, placeholder, on_submit, advance, line_height, size, font, text, caret, placeholder_color, Some(placeholder_font), false, focus, 0).0
}

#[allow(clippy::too_many_arguments)]
fn grid_field<'a>(
    field: PlainField,
    value: &'a str,
    cols: usize,
    min_rows: usize,
    placeholder: &str,
    on_submit: Option<Message>,
    advance: f32,
    line_height: f32,
    size: f32,
    font: Font,
    text: Color,
    caret: Color,
    placeholder_color: Color,
    placeholder_font: Option<Font>,
    autofocus: bool,
    focus: Option<u32>,
    tail: usize,
) -> (Element<'a, Message>, usize) {
    let multiline = min_rows > 1;
    let len = value.chars().count();

    let mask: Vec<bool> = if multiline {
        value.chars().map(|c| c.is_whitespace()).collect()
    } else {
        Vec::new()
    };
    let rows = if multiline { grid_rows(&mask, len, cols).max(min_rows) } else { 1 };

    let mut input = SecureInput::new(
        len,
        move |i, c| Message::PlainEdit(field, SecureOp::Insert(i, c)),
        move |i| Message::PlainEdit(field, SecureOp::Remove(i)),
        move |i, s| Message::PlainEdit(field, SecureOp::Paste(i, s)),
    )
    .on_focus(Message::FieldClicked(field))
    .size(size)
    .width(Length::Fill)
    .dot_color(text)
    .caret_color(caret)
    .placeholder_color(placeholder_color)
    .placeholder(placeholder.to_owned())
    .grid(advance, line_height, font)
    .revealed(Some(value));

    if let Some(f) = placeholder_font {
        input = input.placeholder_font(f);
    }
    if multiline {
        input = input.word_mask(mask).multiline(min_rows as f32 * line_height);
    }
    if let Some(msg) = on_submit {
        input = input.on_submit(msg);
    }
    if autofocus {
        input = input.autofocus();
    }
    if let Some(token) = focus {
        input = input.focus_on(token);
    }
    if tail > 0 {
        input = input.underline_tail(tail);
    }

    (input.boxed(), rows)
}
