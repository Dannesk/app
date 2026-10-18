
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
) -> (Element<'a, Message>, usize) {
    plain_field_grid_ink_focus(field, value, cols, min_rows, placeholder, on_submit, advance, line_height, size, font, text, caret, placeholder_color, None, false)
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

    (input.boxed(), rows)
}
