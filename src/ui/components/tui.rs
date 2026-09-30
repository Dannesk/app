//! The mono character grid — what is left of the framed-TUI engine once the
//! last box-drawn surface (key management, 2026-09-01) went to the key stack.
//! The box assemblers were deleted 2026-09-05 (attic
//! `_attic/2026-09-05-update-screen/tui.rs`); what remains is the measured
//! fact every grid surface still depends on — `SecureInput`'s word cells, the
//! compact meter rows — plus the `Run` type the compact data voice is built
//! from.
//!
//! ## What makes the grid exact
//!
//! 1. The bundled `JetBrainsMono-Regular.ttf` covers U+2500–U+257F **completely**
//!    (128/128) plus all 32 block elements — verified against the file's own
//!    cmap. Inter has 0/128, so any glyph escaping to the app's default font
//!    renders tofu and ruins the grid. Everything here names [`MONO`].
//! 2. That font is a true monospace: unitsPerEm 1000 and *every* non-zero
//!    advance is exactly 600, i.e. [`MONO_ADVANCE`] em — verified against its
//!    `hmtx` table. A run of N characters is therefore always `N × size × 0.6`
//!    wide, which is what lets a row be split into separately-styled segments,
//!    some of them real buttons, without the right edge moving.
//!
//! **If the grid font ever changes, both of those must be re-measured.**

use iced::Color;

/// Advance width of one JetBrains Mono glyph, in em. Measured, not assumed.
pub const MONO_ADVANCE: f32 = 0.6;

/// One coloured run of characters. Lengths are always counted in **characters**.
pub type Run = (String, Color);

pub fn n_chars(runs: &[Run]) -> usize {
    runs.iter().map(|(s, _)| s.chars().count()).sum()
}

/// Pad to exactly `n` **characters**.
///
/// Never bytes: a grid is full of multi-byte glyphs and a byte-wise pad
/// silently under-fills every row containing one. A value already at or over
/// budget is returned whole — overflow eats the trailing padding, never the
/// number, because a truncated balance is worse than a ragged edge.
pub fn pad(s: &str, n: usize) -> String {
    let len = s.chars().count();
    if len >= n { s.to_string() } else { format!("{s}{}", " ".repeat(n - len)) }
}
