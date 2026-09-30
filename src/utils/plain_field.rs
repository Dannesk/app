//! A **non-secret** input laid out on the character grid.
//!
//! ## Why this rides `SecureInput`
//!
//! A field inside a `tui` box has to place every character on a declared
//! column, wrap where the frame says rather than where the renderer would, and
//! draw its own caret — none of which iced's `text_input` can do, and all of
//! which [`crate::utils::secure_input::SecureInput`] already does. It also
//! cannot be a `text_input` for a blunter reason: a taproot `bc1p…` address is
//! 62 characters against a value column narrower than that, so the address
//! field has to **wrap**, and `text_input` is single-line by construction.
//!
//! `SecureInput` renders plaintext whenever `revealed` is set, which is what
//! this uses: a field that is permanently revealed and whose buffer is an
//! ordinary `String`. Nothing here is mlocked and nothing here is masked — an
//! address is public, and treating it as a secret would spend a finite resource
//! (locked pages) on a value that is about to be printed on screen anyway.
//!
//! The edits arrive as the same positional ops a secret field emits, applied by
//! `AppState::apply_plain_edit`, which is also where the per-field character
//! filter lives — see its docs for why the filter is not in the widget.

use iced::{Color, Element, Font, Length};
use crate::controller::message::{Message, PlainField, SecureOp};
use crate::utils::secure_input::{grid_rows, SecureInput};

/// A plain field on the grid, `cols` wide, wrapping to at least `min_rows`,
/// in the caller's three inks (`text`, `caret`, placeholder).
///
/// Returns the widget and the number of framed rows it occupies — the caller
/// needs the count *before* it draws, because the frame around the field has
/// to be that tall.
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
    plain_field_grid_ink_focus(field, value, cols, min_rows, placeholder, on_submit, advance, line_height, size, font, text, caret, placeholder_color, false)
}

/// [`plain_field_grid_ink`] with the focus seeded on mount — the field the
/// top bar swaps in for its pair chip, which nobody should have to click
/// twice.
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
    autofocus: bool,
) -> (Element<'a, Message>, usize) {
    let multiline = min_rows > 1;
    let len = value.chars().count();

    // Break on the same mask the widget will use, so the row count computed
    // here and the rows it actually draws cannot disagree. An address has no
    // spaces, so this is all-false and the wrap is a hard one at `cols` — which
    // is correct: an address split at an invented word boundary would read as
    // two values.
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
    // Permanently revealed: this is the whole difference between a plain field
    // and a secret one.
    .revealed(Some(value));

    if multiline {
        input = input.word_mask(mask).multiline(min_rows as f32 * line_height);
    }
    if let Some(msg) = on_submit {
        input = input.on_submit(msg);
    }
    if autofocus {
        input = input.autofocus();
    }

    (input.into(), rows)
}
