//! A memory-hardened, masked single-line input widget.
//!
//! Goal: keep typed secrets (passphrase, BIP39 word) out of iced's plaintext
//! churn. The stock `text_input` rebuilds a `Value(Vec<String>)` of the real
//! characters every frame and ships a full plaintext `String` in its `on_input`
//! message per keystroke — none of which we can lock.
//!
//! This widget owns no secret. The secret lives in a
//! [`crate::secure::SecureString`] in app state (mlocked + zeroizing). The
//! widget only knows the *length* (to draw that many mask dots) and tracks the
//! caret position; edits are emitted as granular, positional messages — insert
//! a `char` at index, remove the char at index, or paste a `String` at index —
//! which the update loop applies to the `SecureString`. The renderer only ever
//! sees dots, never the real characters.
//!
//! Supports: typing/insert at caret, backspace/delete, Left/Right/Home/End,
//! click-to-position, and paste (Ctrl/Cmd+V). No selection yet.

use std::ops::Range;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::text::{self, Renderer as _};
use iced::advanced::widget::{self, tree, Tree, Widget};
use iced::advanced::{clipboard, mouse, Shell};
use iced::{
    alignment, keyboard, Border, Color, Event, Font, Length, Pixels, Point, Rectangle, Size,
};

#[derive(Default)]
struct State {
    focused: bool,
    cursor: usize,
    /// Ctrl+V asked the runtime for the clipboard; the text arrives later as
    /// an event that every widget sees. Only the field that asked takes it.
    paste_pending: bool,
}

/// Character-grid metrics, for a field that lives inside a `tui` box.
///
/// Everything else this widget draws is free to sit wherever it looks best; a
/// field inside a box is not. Its slots share columns with the framing `│`, so
/// an advance that is a fraction of a pixel off doesn't look slightly wrong, it
/// pushes the closing bar out of column and staircases the panel.
#[derive(Clone, Copy)]
struct Grid {
    /// Exactly one column, i.e. `tui::char_w(scale)`.
    advance: f32,
    /// Exactly one framed row, i.e. `BOX_SIZE * scale * BOX_LINE_HEIGHT`.
    line_height: f32,
    /// The box font. Must be the monospace the grid was measured from —
    /// revealed text renders in it, so a proportional face here would ragged
    /// the right edge the moment the user hits `reveal ›`.
    font: Font,
}

/// Break `len` slots into rows of at most `cols`, preferring word boundaries.
///
/// Masked and revealed text **must** break identically. The frame around this
/// field draws a fixed number of `│` rows, so a reveal that re-wrapped would
/// visibly re-shape the box mid-interaction. Both modes therefore break here,
/// from the same `mask`, and revealed text is sliced by these ranges rather
/// than handed to the renderer's own word wrapping.
///
/// A separator slot chosen as the break point is dropped from both rows — it
/// *is* the line break. A word longer than `cols` is split hard; nothing else
/// would fit.
fn wrap_rows(mask: &[bool], len: usize, cols: usize) -> Vec<Range<usize>> {
    let greedy = greedy_rows(mask, len, cols);

    // Greedy filling packs every row to the brim and drops whatever is left on
    // the last one, which is how 24 words become 7 / 7 / 7 / 3 — or, worse, how
    // the twenty-fourth word ends up alone on a row of its own. A phrase read
    // back for verification is a block of text, and a block with a stub on the
    // end reads as a layout that broke rather than as a phrase that ended.
    //
    // So: keep the row count greedy chose, then find the narrowest width that
    // still fits in that many rows. Narrowing cannot remove a row, and the last
    // width that holds is the one that spreads the words as evenly as the break
    // points allow — 6 / 6 / 6 / 6 instead of 7 / 7 / 7 / 3.
    //
    // Only for text with word breaks. A secret with no separators splits hard
    // wherever the column runs out, and "evening out" an arbitrary split is a
    // rearrangement of nothing.
    if greedy.len() < 2 || !mask.iter().any(|&sep| sep) {
        return greedy;
    }

    // Row count is monotonically non-increasing in width, so the narrowest
    // width holding `rows` is a binary search rather than a walk.
    let rows = greedy.len();
    let (mut lo, mut hi) = (1usize, cols);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if greedy_rows(mask, len, mid).len() <= rows {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let balanced = greedy_rows(mask, len, lo);
    if balanced.len() == rows { balanced } else { greedy }
}

/// [`wrap_rows`] before balancing: fill each row until the next slot would
/// overflow, then break at the last separator.
fn greedy_rows(mask: &[bool], len: usize, cols: usize) -> Vec<Range<usize>> {
    if cols == 0 || len == 0 {
        return vec![0..len];
    }
    let mut rows = Vec::new();
    let mut start = 0usize;
    let mut last_sep: Option<usize> = None;
    let mut i = 0usize;
    while i < len {
        if mask.get(i).copied().unwrap_or(false) {
            last_sep = Some(i);
        }
        if i - start == cols {
            let (end, next) = match last_sep {
                Some(b) if b > start => (b, b + 1),
                _ => (i, i),
            };
            rows.push(start..end);
            start = next;
            // Usually `i` is re-tested against the row it now belongs to. The
            // exception is a break taken *on* slot `i` — the separator that
            // filled the row is consumed by the break, so stepping over it is
            // what keeps `i - start` from running backwards.
            i = i.max(start);
            last_sep = None;
            continue;
        }
        i += 1;
    }
    rows.push(start..len);
    rows
}

/// Rows a grid field of `len` characters occupies at `cols` columns — the same
/// count [`wrap_rows`] produces, for a caller sizing the frame around it.
pub fn grid_rows(mask: &[bool], len: usize, cols: usize) -> usize {
    wrap_rows(mask, len, cols).len()
}

// ── Word cells ──────────────────────────────────────────────────────────────
//
// The compact import screen's numbered grid: words don't flow, they LAND —
// word 0 in the top-left cell, 01–08 down the first column, column-major,
// each cell prefixed by a faint index the widget draws as decoration. The
// buffer underneath is exactly the paragraph's (chars + separator slots), so
// every edit message, the caps and the mask machinery are untouched; only
// where a character sits on screen changes.
//
// Draw, caret and click all read ONE table ([`cell_positions`]) so the three
// cannot disagree about where a slot is — the same discipline the wrap grid
// keeps with `wrap_rows`.

/// Geometry of the word-cell layout — see [`SecureInput::word_cells`].
#[derive(Clone, Copy)]
struct Cells {
    /// Word columns × rows (3 × 8 on the import screen), column-major.
    cols: usize,
    rows: usize,
    /// Character columns a word's glyphs get. A longer (illegal) word draws
    /// on into the gap rather than truncating — a truncated secret is a lie.
    word_cols: usize,
    /// Character columns of the `NN ` index prefix.
    index_cols: usize,
    /// Character columns between blocks.
    gap_cols: usize,
    /// The index decoration's ink.
    index_color: Color,
}

impl Cells {
    /// Character columns one block occupies, gap included.
    fn stride(&self) -> usize {
        self.index_cols + self.word_cols + self.gap_cols
    }

    /// Which (block column, row) word `w` lands in. Column-major inside the
    /// grid; words past the last cell — an over-long paste, mid-edit — spill
    /// row-major into extra rows BELOW it rather than off the right edge,
    /// so nothing ever draws outside the widget's width.
    fn block(&self, w: usize) -> (usize, usize) {
        let cells = self.cols * self.rows;
        if w < cells {
            (w / self.rows, w % self.rows)
        } else {
            let o = w - cells;
            (o % self.cols, self.rows + o / self.cols)
        }
    }

    /// (character column, row) of a caret `offset` characters into word `w`.
    fn slot(&self, w: usize, offset: usize) -> (usize, usize) {
        let (c, r) = self.block(w);
        (c * self.stride() + self.index_cols + offset, r)
    }
}

/// Maximal runs of non-separator slots — the words, in buffer order.
fn word_runs(mask: &[bool], len: usize) -> Vec<Range<usize>> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for i in 0..len {
        let sep = mask.get(i).copied().unwrap_or(false);
        match (sep, start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                runs.push(s..i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        runs.push(s..len);
    }
    runs
}

/// The position table: `(character column, row)` for every caret index
/// `0..=len`. Index `i` is where the caret sits *before* slot `i`, which is
/// also where slot `i`'s own glyph or dot draws. A separator slot sits where
/// the caret after its word's last character sits; the slot after it is the
/// next word's first column.
fn cell_positions(mask: &[bool], len: usize, cells: Cells) -> Vec<(usize, usize)> {
    let mut pos = Vec::with_capacity(len + 1);
    let mut word = 0usize;
    let mut offset = 0usize;
    let mut in_word = false;
    for i in 0..len {
        let sep = mask.get(i).copied().unwrap_or(false);
        if !sep && !in_word {
            in_word = true;
            offset = 0;
        }
        pos.push(cells.slot(word, offset));
        if sep {
            if in_word {
                in_word = false;
                word += 1;
                offset = 0;
            }
        } else {
            offset += 1;
        }
    }
    pos.push(cells.slot(word, offset));
    pos
}

/// Rows the cell layout needs for `mask`/`len` — the full grid, plus any
/// spill rows an over-long phrase adds below it.
fn cell_layout_rows(mask: &[bool], len: usize, cells: Cells) -> usize {
    let words = word_runs(mask, len).len();
    if words <= cells.cols * cells.rows {
        cells.rows
    } else {
        let spill = words - cells.cols * cells.rows;
        cells.rows + spill.div_ceil(cells.cols)
    }
}

/// Where a word-cell field's caret goes on `Enter`, or on a click in a cell
/// with nothing in it yet.
#[derive(Debug, PartialEq, Eq)]
enum Advance {
    /// To the start of the next filled word.
    Jump(usize),
    /// To a fresh cell after the last word — opened with a separator if the
    /// buffer does not already end on one.
    Open,
    /// Every cell is taken and the caret is in or past the last word: there
    /// is no next word, so `Enter` means submit.
    Full,
}

/// The caret's next word from `cursor`: the word after the one the caret is
/// in (or just after), else a fresh cell, else — at `capacity` words — full.
fn advance_from(mask: &[bool], len: usize, cursor: usize, capacity: usize) -> Advance {
    let words = word_runs(mask, len);
    let here = words.iter().position(|r| cursor <= r.end);
    match here.and_then(|i| words.get(i + 1)) {
        Some(next) => Advance::Jump(next.start),
        None if words.len() >= capacity => Advance::Full,
        None => Advance::Open,
    }
}

/// Whether the word the caret is in (or just after) already holds `cap`
/// characters. A caret past every word — in a cell nothing has been typed
/// into — is in no word, so never full.
fn word_full(mask: &[bool], len: usize, cursor: usize, cap: usize) -> bool {
    word_runs(mask, len)
        .iter()
        .find(|r| cursor <= r.end)
        .is_some_and(|r| r.len() >= cap)
}

pub struct SecureInput<'a, Message> {
    char_len: usize,
    width: Length,
    text_size: f32,
    /// When `Some(h)`, render as a fixed-height `h` box with dots wrapping
    /// across lines (for long secrets like a seed phrase). When `None`, a
    /// single line.
    box_height: Option<f32>,
    /// When `Some(n)`, render as `n` discrete PIN cells instead of a flowing
    /// row of dots: one bordered box per slot, a filled dot once typed, the
    /// caret box outlined. Input is restricted to ASCII digits and capped at
    /// `n`. The secret still lives in the same `SecureString` — this is purely
    /// a render/entry mode, so it composes with the existing edit plumbing.
    pin_cells: Option<usize>,
    /// Fill + border for an empty PIN cell (only used in `pin_cells` mode).
    cell_bg: Color,
    cell_border: Color,
    /// When `Some(n)`, accept only ASCII digits and never more than `n` of
    /// them. Separate from `pin_cells` because it is about what the field
    /// *accepts*, not how it is drawn — a PIN on the character grid is still a
    /// PIN, and must not quietly start taking letters because it lost its boxes.
    digit_cap: Option<usize>,
    /// When `Some(n)`, never hold more than `n` characters of any kind — the
    /// length half of [`Self::digit_cap`] without the digit filter. A
    /// keystroke past it is dropped, a paste is cut to what fits.
    char_cap: Option<usize>,
    /// Per-character "is this a word separator" flags, derived from the buffer
    /// by the view. Used for two things: choosing line breaks, and — when
    /// [`Self::word_gaps`] is set — leaving the slot blank so masked words read
    /// as separate clusters. Empty = every slot is a dot.
    word_mask: Vec<bool>,
    /// Whether a separator slot renders as a gap.
    ///
    /// **Off for recovery phrases, deliberately.** Visible word boundaries let
    /// anyone who can see the screen read off each word's length, and BIP39
    /// lengths are not uniform — the list has 2048 words but only 103 of three
    /// letters and 88 of eight, so a length is worth ~2.34 bits and a 24-word
    /// phrase leaks ~56 of its 256. That still leaves 2^200, so this is not a
    /// break; it is free to give away and free to withhold, which is the whole
    /// argument for withholding it.
    ///
    /// The mask is still honoured for *wrapping* either way, so masked and
    /// revealed keep breaking in the same places and the frame doesn't
    /// re-shape under `reveal ›`. What is left is where the lines break — a
    /// few cumulative sums rather than all 24 lengths.
    word_gaps: bool,
    /// When `Some`, the field is revealed: the plaintext is rendered as real
    /// glyphs instead of dots. The view passes this only while the user holds
    /// the reveal toggle on — that's the one window where plaintext reaches the
    /// renderer (an accepted, user-initiated tradeoff).
    ///
    /// **Borrowed, never owned.** It used to be an `Option<String>`, which the
    /// view refilled from `SecureString::as_str().to_owned()` on every single
    /// frame — a full plaintext copy of the secret, on the ordinary heap, out
    /// of reach of the `mlock` and dropped without a wipe, sixty times a
    /// second for as long as the toggle was held. Borrowing costs nothing and
    /// leaves exactly one copy per frame: the `String` that
    /// [`iced::advanced::text::Renderer::fill_text`] takes by value, which is
    /// the renderer's and not ours to avoid. See the note in [`Self::draw`].
    revealed: Option<&'a str>,
    /// When `Some`, lay out on the character grid — see [`Grid`]. Opt-in: every
    /// field outside a `tui` box keeps the proportional layout below.
    grid: Option<Grid>,
    /// When `Some`, place words in the numbered cell grid instead of flowing
    /// them — see the module's "Word cells" section. Only meaningful together
    /// with [`Self::grid`] and [`Self::multiline`].
    cells: Option<Cells>,
    /// Hint shown (in `placeholder_color`) when the buffer is empty. Non-secret.
    placeholder: String,
    placeholder_color: Color,
    dot_color: Color,
    caret_color: Color,
    on_insert: Box<dyn Fn(usize, char) -> Message + 'a>,
    on_remove: Box<dyn Fn(usize) -> Message + 'a>,
    on_paste: Box<dyn Fn(usize, String) -> Message + 'a>,
    on_submit: Option<Message>,
    /// Start focused when the widget first mounts (no click needed). Focus is
    /// internal to this widget (not iced's Focusable op), so this seeds the
    /// initial state — used by the gate screen's PIN entry.
    autofocus: bool,
}

impl<'a, Message> SecureInput<'a, Message> {
    pub fn new(
        char_len: usize,
        on_insert: impl Fn(usize, char) -> Message + 'a,
        on_remove: impl Fn(usize) -> Message + 'a,
        on_paste: impl Fn(usize, String) -> Message + 'a,
    ) -> Self {
        SecureInput {
            char_len,
            width: Length::Fill,
            text_size: 15.0,
            box_height: None,
            pin_cells: None,
            cell_bg: Color::TRANSPARENT,
            cell_border: Color::from_rgb(0.6, 0.6, 0.6),
            digit_cap: None,
            char_cap: None,
            word_mask: Vec::new(),
            word_gaps: true,
            grid: None,
            cells: None,
            revealed: None,
            placeholder: String::new(),
            placeholder_color: Color::from_rgb(0.5, 0.5, 0.5),
            dot_color: Color::from_rgb(0.6, 0.6, 0.6),
            caret_color: Color::from_rgb(0.3, 0.55, 1.0),
            on_insert: Box::new(on_insert),
            on_remove: Box::new(on_remove),
            on_paste: Box::new(on_paste),
            on_submit: None,
            autofocus: false,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.text_size = size;
        self
    }

    /// Render as a fixed-height box with dots wrapping across lines.
    pub fn multiline(mut self, height: f32) -> Self {
        self.box_height = Some(height);
        self
    }

    /// Accept at most `n` characters, of any kind.
    pub fn char_cap(mut self, n: usize) -> Self {
        self.char_cap = Some(n);
        self
    }

    /// Per-character word-separator flags: line-break points, and — unless
    /// [`Self::word_gaps`] is cleared — where the mask leaves a gap.
    pub fn word_mask(mut self, mask: Vec<bool>) -> Self {
        self.word_mask = mask;
        self
    }

    /// Lay this field out on the character grid of a `tui` box: one slot per
    /// column of `advance`, one row per `line_height`, revealed text in `font`.
    ///
    /// Pass `tui::char_w(scale)` and `BOX_SIZE * scale * BOX_LINE_HEIGHT` —
    /// the same numbers the surrounding frame is drawn from, never re-derived.
    pub fn grid(mut self, advance: f32, line_height: f32, font: Font) -> Self {
        self.grid = Some(Grid { advance, line_height, font });
        self
    }

    /// Place words in a numbered cell grid — `cols × rows` word cells,
    /// column-major, each `word_cols` characters wide behind an `index_cols`
    /// `NN ` prefix drawn in `index_color`, `gap_cols` between blocks. The
    /// import screen's editable word grid. Requires [`Self::grid`] metrics and
    /// [`Self::multiline`]; the caret, clicks and glyphs all live on the same
    /// cell arithmetic.
    pub fn word_cells(
        mut self,
        cols: usize,
        rows: usize,
        word_cols: usize,
        index_cols: usize,
        gap_cols: usize,
        index_color: Color,
    ) -> Self {
        self.cells = Some(Cells { cols, rows, word_cols, index_cols, gap_cols, index_color });
        self
    }

    /// Reveal the plaintext (rendered as glyphs) when `Some`.
    ///
    /// Pass a borrow of the live buffer — `Some(secret.as_str())`. Never an
    /// owned copy; see the field's own note for why.
    pub fn revealed(mut self, text: Option<&'a str>) -> Self {
        self.revealed = text;
        self
    }

    /// Hint shown when the buffer is empty.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn placeholder_color(mut self, color: Color) -> Self {
        self.placeholder_color = color;
        self
    }

    /// Mount focused, caret blinking, no click needed — the top bar's pair
    /// search opens this way when its chip is pressed.
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn dot_color(mut self, color: Color) -> Self {
        self.dot_color = color;
        self
    }

    pub fn caret_color(mut self, color: Color) -> Self {
        self.caret_color = color;
        self
    }

    pub fn on_submit(mut self, message: Message) -> Self {
        self.on_submit = Some(message);
        self
    }

    fn line_height(&self) -> f32 {
        match self.grid {
            Some(g) => g.line_height,
            None => self.text_size * 1.4,
        }
    }

    /// Columns that fit in `width`. Floored, never zero — a partial column is
    /// not a column, and rounding up would put a slot under the closing `│`.
    ///
    /// The epsilon is not slop. The caller sizes this field at exactly
    /// `cols × advance`, and that product divided back by `advance` can land a
    /// hair under a whole number in f32 — floored, the field would quietly give
    /// up its last column and wrap a row early.
    fn grid_cols(&self, width: f32) -> usize {
        match self.grid {
            Some(g) => (width / g.advance + 1e-3).floor().max(1.0) as usize,
            None => 0,
        }
    }

    /// First visible slot of a single-line grid field.
    ///
    /// A key can be longer than its frame, and a box has no room to grow
    /// sideways, so the window tracks the caret instead: the field scrolls
    /// rather than spilling dots over the `│`.
    fn grid_scroll(&self, cols: usize, cursor: usize) -> usize {
        if self.box_height.is_some() {
            return 0;
        }
        cursor.saturating_sub(cols.saturating_sub(1))
    }

    /// (cell, gap, dot) sizes for PIN mode, derived from the text size so cells
    /// scale with the field. At `text_size == 16` this yields ~40px cells, ~9px
    /// gaps and dots — matching the established PIN box look.
    fn pin_metrics(&self) -> (f32, f32, f32) {
        let cell = self.text_size * 2.5;
        let gap = self.text_size * 0.56;
        let dot = self.text_size * 0.56;
        (cell, gap, dot)
    }

    fn pin_width(&self, n: usize) -> f32 {
        let (cell, gap, _) = self.pin_metrics();
        n as f32 * cell + n.saturating_sub(1) as f32 * gap
    }

    fn widget_height(&self) -> f32 {
        if self.pin_cells.is_some() {
            return self.pin_metrics().0;
        }
        self.box_height.unwrap_or_else(|| self.line_height())
    }

    fn step(&self) -> f32 {
        if let Some(n) = self.pin_cells {
            let (cell, gap, _) = self.pin_metrics();
            // Uniform per-cell advance; the final cell has no trailing gap but
            // hit-testing past the last cell clamps anyway.
            let _ = n;
            return cell + gap;
        }
        if let Some(g) = self.grid {
            return g.advance;
        }
        // Masked dots advance a uniform step. In multiline (seed) mode pack them
        // tighter so 24 words occupy roughly the same width as the revealed
        // proportional text — at the single-line `0.77` factor, uniform dots are
        // ~1.5× wider than the equivalent glyphs and wrap into far more rows.
        let factor = if self.box_height.is_some() { 0.52 } else { 0.77 };
        self.text_size * factor
    }

    /// The character-grid draw path.
    ///
    /// Masked dots, revealed glyphs and the caret are all placed from the row
    /// ranges [`wrap_rows`] returns — never measured independently — so the
    /// three can't disagree about where a column is. That is the whole reason
    /// revealed text is drawn a row at a time with wrapping switched **off**
    /// rather than handed to the renderer as one paragraph: the renderer would
    /// re-wrap it on its own terms and the field would re-shape under `reveal ›`.
    fn draw_grid(&self, g: Grid, state: &State, renderer: &mut iced::Renderer, bounds: Rectangle) {
        let hint = renderer.hint_factor();
        let cols = self.grid_cols(bounds.width);
        let cursor = state.cursor.min(self.char_len);
        let col_x = |c: usize| bounds.x + c as f32 * g.advance;
        let row_y = |r: usize| bounds.y + r as f32 * g.line_height + g.line_height / 2.0;

        let caret_quad = |renderer: &mut iced::Renderer, x: f32, cy: f32| {
            let w = (self.text_size * 0.08).max(1.0);
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x,
                        y: cy - self.text_size / 2.0,
                        width: w,
                        height: self.text_size,
                    },
                    ..Default::default()
                },
                self.caret_color,
            );
        };

        // Empty: the placeholder is non-secret, but it still has to respect the
        // frame — a hint wider than the box would run under the closing `│`.
        if self.char_len == 0 {
            if !self.placeholder.is_empty() {
                let shown: String = self.placeholder.chars().take(cols).collect();
                renderer.fill_text(
                    text::Text {
                        content: shown,
                        bounds: Size::new(bounds.width, g.line_height),
                        size: Pixels(self.text_size),
                        line_height: text::LineHeight::Relative(1.0),
                        font: g.font,
                        align_x: text::Alignment::Left,
                        align_y: alignment::Vertical::Center,
                        shaping: text::Shaping::Advanced,
                        ellipsis: text::Ellipsis::None,
                        hint_factor: hint,
                        wrapping: text::Wrapping::None,
                    },
                    Point::new(col_x(0), row_y(0)),
                    self.placeholder_color,
                    bounds,
                );
            }
            if state.focused {
                caret_quad(renderer, col_x(0), row_y(0));
            }
            return;
        }

        if let Some(cells) = self.cells {
            self.draw_cells(g, cells, state, renderer, bounds);
            return;
        }

        let first = self.grid_scroll(cols, cursor);

        // A single-line field **scrolls**; it does not wrap. Running the wrap
        // here would draw the overflow below a widget one row tall — which
        // reads as characters falling off the top of the field as you type,
        // not as a field that has run out of room.
        let rows = if self.box_height.is_some() {
            wrap_rows(&self.word_mask, self.char_len, cols)
        } else {
            vec![first..(first + cols).min(self.char_len)]
        };

        let diameter = self.text_size * 0.45;
        let inset = (g.advance - diameter) / 2.0;
        let mut caret: Option<(f32, f32)> = None;

        for (r, range) in rows.iter().enumerate() {
            // Multiline never scrolls (it grows instead), so `first` is 0 there
            // and the origin is just the row's own start.
            let origin = range.start.max(first);
            let end = range.end.min(origin + cols);

            if caret.is_none() && cursor <= range.end {
                caret = Some((col_x(cursor.saturating_sub(origin).min(cols)), row_y(r)));
            }

            match &self.revealed {
                Some(plain) => {
                    let shown: String =
                        plain.chars().skip(origin).take(end.saturating_sub(origin)).collect();
                    if !shown.is_empty() {
                        renderer.fill_text(
                            text::Text {
                                content: shown,
                                bounds: Size::new(bounds.width, g.line_height),
                                size: Pixels(self.text_size),
                                // One grid row, drawn exactly the way every
                                // other mono row in the app is drawn: a
                                // line-height box with the glyphs centred
                                // inside it by the shaper, anchored at the
                                // row's TOP edge.
                                //
                                // It used to be `Relative(1.0)` anchored at the
                                // row's centre, which asks the renderer to
                                // centre a box whose height it measures itself
                                // — and a measured height that comes back even
                                // slightly short pulls the row up out of its
                                // slot. The masked path never had the problem
                                // because `fill_quad` places dots by
                                // arithmetic and cannot be measured wrong; the
                                // two paths now agree because neither one is
                                // asking a question any more.
                                line_height: text::LineHeight::Relative(
                                    g.line_height / self.text_size,
                                ),
                                font: g.font,
                                align_x: text::Alignment::Left,
                                align_y: alignment::Vertical::Top,
                                shaping: text::Shaping::Advanced,
                                ellipsis: text::Ellipsis::None,
                                hint_factor: hint,
                                wrapping: text::Wrapping::None,
                            },
                            Point::new(col_x(0), bounds.y + r as f32 * g.line_height),
                            self.dot_color,
                            bounds,
                        );
                    }
                }
                None => {
                    for s in origin..end {
                        // A separator slot owns its column either way; whether
                        // it is left blank is what decides if word lengths are
                        // legible over someone's shoulder.
                        if self.word_gaps && self.word_mask.get(s).copied().unwrap_or(false) {
                            continue;
                        }
                        let cy = row_y(r);
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle {
                                    x: col_x(s - origin) + inset,
                                    y: cy - diameter / 2.0,
                                    width: diameter,
                                    height: diameter,
                                },
                                border: Border {
                                    radius: (diameter / 2.0).into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                            self.dot_color,
                        );
                    }
                }
            }
        }

        if state.focused {
            let (x, cy) = caret.unwrap_or((col_x(0), row_y(0)));
            caret_quad(renderer, x, cy);
        }
    }

    /// The word-cell draw path — see the module's "Word cells" section.
    ///
    /// Every cell's index is drawn first (decoration, all 24, so the grid
    /// states its shape while you type), then each word's dots or glyphs at
    /// its cell, then the caret — glyph runs are one `fill_text` per word,
    /// pinned to the cell, never handed to the renderer's own wrapping. Dots,
    /// glyphs, caret and clicks all read the same [`cell_positions`] table.
    fn draw_cells(&self, g: Grid, cells: Cells, state: &State, renderer: &mut iced::Renderer, bounds: Rectangle) {
        let hint = renderer.hint_factor();
        let cursor = state.cursor.min(self.char_len);
        let col_x = |c: usize| bounds.x + c as f32 * g.advance;
        let row_y = |r: usize| bounds.y + r as f32 * g.line_height + g.line_height / 2.0;

        let cell_text = |content: String| text::Text {
            content,
            bounds: Size::new(bounds.width, g.line_height),
            size: Pixels(self.text_size),
            line_height: text::LineHeight::Relative(1.0),
            font: g.font,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Center,
            shaping: text::Shaping::Advanced,
            ellipsis: text::Ellipsis::None,
            hint_factor: hint,
            wrapping: text::Wrapping::None,
        };

        for w in 0..cells.cols * cells.rows {
            let (c, r) = cells.block(w);
            renderer.fill_text(
                cell_text(format!("{:02}", w + 1)),
                Point::new(col_x(c * cells.stride()), row_y(r)),
                cells.index_color,
                bounds,
            );
        }

        let positions = cell_positions(&self.word_mask, self.char_len, cells);

        match &self.revealed {
            Some(plain) => {
                for (w, run) in word_runs(&self.word_mask, self.char_len).iter().enumerate() {
                    // The per-word transient the renderer needs — same class
                    // of copy the paragraph path's per-row slices make.
                    let shown: String =
                        plain.chars().skip(run.start).take(run.len()).collect();
                    let (x_cols, r) = cells.slot(w, 0);
                    renderer.fill_text(
                        cell_text(shown),
                        Point::new(col_x(x_cols), row_y(r)),
                        self.dot_color,
                        bounds,
                    );
                }
            }
            None => {
                let diameter = self.text_size * 0.45;
                let inset = (g.advance - diameter) / 2.0;
                for i in 0..self.char_len {
                    // A separator draws nothing: the cell gap already is the
                    // word boundary, so there is no length left to hide.
                    if self.word_mask.get(i).copied().unwrap_or(false) {
                        continue;
                    }
                    let (x_cols, r) = positions[i];
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle {
                                x: col_x(x_cols) + inset,
                                y: row_y(r) - diameter / 2.0,
                                width: diameter,
                                height: diameter,
                            },
                            border: Border {
                                radius: (diameter / 2.0).into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        self.dot_color,
                    );
                }
            }
        }

        if state.focused {
            let (x_cols, r) = positions[cursor];
            let w = (self.text_size * 0.08).max(1.0);
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: col_x(x_cols),
                        y: row_y(r) - self.text_size / 2.0,
                        width: w,
                        height: self.text_size,
                    },
                    ..Default::default()
                },
                self.caret_color,
            );
        }
    }

    /// Map a click inside a word-cell field to a caret index: the block from
    /// x, the row from y, the word from both (column-major, spill rows below),
    /// the offset from what's left of x — clamped into that word. `None` for
    /// a cell with nothing in it yet: the caller opens the next free cell,
    /// which is what a click on `02` while typing word 1 is asking for.
    fn cell_click_index(&self, cells: Cells, g: Grid, bounds: Rectangle, pos: Point) -> Option<usize> {
        let rel_cols = ((pos.x - bounds.x) / g.advance).max(0.0);
        let row = (((pos.y - bounds.y) / g.line_height).max(0.0)).floor() as usize;
        let stride = cells.stride();
        let block = ((rel_cols / stride as f32) as usize).min(cells.cols.saturating_sub(1));
        let w = if row < cells.rows {
            block * cells.rows + row
        } else {
            cells.cols * cells.rows + (row - cells.rows) * cells.cols + block
        };
        let words = word_runs(&self.word_mask, self.char_len);
        let run = words.get(w)?;
        let within = rel_cols - (block * stride + cells.index_cols) as f32;
        let offset = (within + 0.5).max(0.0).floor() as usize;
        Some(run.start + offset.min(run.len()))
    }

    /// Put the caret in the first empty cell. A buffer ending on a separator
    /// already has that cell open; otherwise one separator is appended to
    /// open it — the same keystroke the space bar would have made.
    fn open_next_cell(&self, state: &mut State, shell: &mut Shell<'_, Message>) {
        let ends_open = self.char_len == 0
            || self.word_mask.get(self.char_len - 1).copied().unwrap_or(false);
        if ends_open || self.char_cap.is_some_and(|cap| self.char_len >= cap) {
            state.cursor = self.char_len;
        } else {
            shell.publish((self.on_insert)(self.char_len, ' '));
            state.cursor = self.char_len + 1;
        }
        shell.request_redraw();
    }
}

impl<Message> widget::Meta for SecureInput<'_, Message> {}

impl<'a, Message> Widget<Message, iced::Theme, iced::Renderer> for SecureInput<'a, Message>
where
    Message: Clone,
{
    fn size(&self) -> Size<Length> {
        let width = match self.pin_cells {
            Some(n) => Length::Fixed(self.pin_width(n)),
            None => self.width,
        };
        // A grown grid field reports `Fit`, not a number.
        //
        // `size()` has no width to work from, so the only height it could name
        // is `box_height` — the *reserve*, which is the floor and not the
        // height once the content wraps past it. A parent that believed that
        // number allocated four rows for a field drawing five and clipped the
        // last one, which is what put a row hard against the one above it: the
        // rows were evenly placed all along, the box just ended early.
        //
        // `Fit` makes the parent defer to `layout`, which knows the resolved
        // width and therefore the real row count. Same shape iced's own
        // `TextEditor` uses for exactly this reason.
        let height = match (self.pin_cells, self.box_height, self.grid) {
            (None, Some(_), Some(_)) => Length::Fit,
            _ => Length::Fixed(self.widget_height()),
        };
        Size::new(width, height)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State { focused: self.autofocus, ..State::default() })
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &iced::Renderer, limits: &layout::Limits) {
        let width = match self.pin_cells {
            Some(n) => Length::Fixed(self.pin_width(n)),
            None => self.width,
        };
        // Multiline grows to fit its wrapped content: the number of masked rows
        // depends on the resolved width (known here via `limits`), so a fixed
        // height would clip/overflow at narrow widths. `box_height` is the floor.
        let height = match (self.pin_cells, self.box_height) {
            (Some(_), _) | (None, None) => self.widget_height(),
            // On the grid, rows come from the same layout the draw pass uses —
            // the cell arithmetic in cells mode, the word-wrap otherwise.
            // Sizing by `ceil(len / cols)` would disagree with either the
            // moment a word wraps early, and the frame would be a row short.
            (None, Some(min_h)) if self.grid.is_some() => {
                let rows = match self.cells {
                    Some(cells) => cell_layout_rows(&self.word_mask, self.char_len, cells),
                    None => {
                        let cols = self.grid_cols(limits.max.width);
                        grid_rows(&self.word_mask, self.char_len, cols)
                    }
                };
                (rows as f32 * self.line_height()).max(min_h)
            }
            (None, Some(min_h)) => {
                let avail = limits.max.width;
                let cols = (avail / self.step()).floor().max(1.0);
                let rows = (self.char_len.max(1) as f32 / cols).ceil().max(1.0);
                (rows * self.line_height()).max(min_h)
            }
        };
        tree.size = layout::atomic(limits, width, Length::Fixed(height));
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        // What iced's own text widget passes, so the dots and glyphs land on
        // the same pixel grid as every other label.
        let hint = renderer.hint_factor();
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        let multiline = self.box_height.is_some();

        // PIN mode: discrete bordered cells, one filled dot per typed digit, the
        // active cell outlined in the caret colour. Never renders real glyphs.
        if let Some(n) = self.pin_cells {
            let (cell, gap, dot) = self.pin_metrics();
            let cursor = state.cursor.min(n);
            for i in 0..n {
                let x = bounds.x + i as f32 * (cell + gap);
                let filled = i < self.char_len;
                let is_cursor = state.focused && i == cursor;
                let border_col = if is_cursor { self.caret_color } else { self.cell_border };
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle { x, y: bounds.y, width: cell, height: cell },
                        border: Border {
                            color: border_col,
                            width: 1.0,
                            radius: (cell * 0.2).into(),
                        },
                        ..Default::default()
                    },
                    self.cell_bg,
                );
                let cx = x + cell / 2.0;
                let cy = bounds.y + cell / 2.0;
                if filled {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle {
                                x: cx - dot / 2.0,
                                y: cy - dot / 2.0,
                                width: dot,
                                height: dot,
                            },
                            border: Border { radius: (dot / 2.0).into(), ..Default::default() },
                            ..Default::default()
                        },
                        self.dot_color,
                    );
                } else if is_cursor {
                    let caret_w = (self.text_size * 0.09).max(1.5);
                    let caret_h = self.text_size;
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle {
                                x: cx - caret_w / 2.0,
                                y: cy - caret_h / 2.0,
                                width: caret_w,
                                height: caret_h,
                            },
                            ..Default::default()
                        },
                        self.caret_color,
                    );
                }
            }
            return;
        }

        if let Some(g) = self.grid {
            self.draw_grid(g, state, renderer, bounds);
            return;
        }

        // Empty: show the placeholder hint (non-secret) + caret if focused.
        if self.char_len == 0 {
            if !self.placeholder.is_empty() {
                renderer.fill_text(
                    text::Text {
                        content: self.placeholder.clone(),
                        bounds: Size::new(bounds.width, bounds.height),
                        size: Pixels(self.text_size),
                        line_height: text::LineHeight::Relative(1.3),
                        font: renderer.font(),
                        align_x: text::Alignment::Left,
                        align_y: alignment::Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        ellipsis: text::Ellipsis::None,
                        hint_factor: hint,
                        wrapping: if multiline {
                            text::Wrapping::Word
                        } else {
                            text::Wrapping::None
                        },
                    },
                    bounds.position(),
                    self.placeholder_color,
                    bounds,
                );
            }
            if state.focused {
                let caret_w = (self.text_size * 0.08).max(1.0);
                let caret_h = self.text_size;
                let cy = if multiline {
                    bounds.y + self.line_height() / 2.0
                } else {
                    bounds.y + bounds.height / 2.0
                };
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: bounds.x,
                            y: cy - caret_h / 2.0,
                            width: caret_w,
                            height: caret_h,
                        },
                        ..Default::default()
                    },
                    self.caret_color,
                );
            }
            return;
        }

        // Revealed: render the real plaintext as glyphs (no dots, no caret).
        if let Some(plain) = &self.revealed {
            renderer.fill_text(
                text::Text {
                    // The one plaintext copy per frame that cannot be
                    // avoided: `fill_text` takes `Text<String>` by value, so
                    // the renderer needs its own allocation and drops it
                    // unwiped when the frame is done. Everything above this
                    // line holds the secret by reference.
                    content: (*plain).to_owned(),
                    bounds: Size::new(bounds.width, bounds.height),
                    size: Pixels(self.text_size),
                    line_height: text::LineHeight::Relative(1.3),
                    font: renderer.font(),
                    align_x: text::Alignment::Left,
                    align_y: alignment::Vertical::Top,
                    shaping: text::Shaping::Advanced,
                    ellipsis: text::Ellipsis::None,
                    hint_factor: hint,
                    wrapping: if multiline {
                        text::Wrapping::Word
                    } else {
                        text::Wrapping::None
                    },
                },
                bounds.position(),
                self.dot_color,
                bounds,
            );
            return;
        }

        let diameter = self.text_size * if multiline { 0.34 } else { 0.45 };
        let step = self.step();
        let line_h = self.line_height();
        let cursor = state.cursor.min(self.char_len);

        // Walk dot slots left-to-right, wrapping in multiline mode. Track where
        // the caret falls as we go.
        let mut x = bounds.x;
        let mut cy = if multiline {
            bounds.y + line_h / 2.0
        } else {
            bounds.y + bounds.height / 2.0
        };
        let mut caret = (x, cy);

        for i in 0..self.char_len {
            if multiline && x + diameter > bounds.x + bounds.width {
                x = bounds.x;
                cy += line_h;
            }
            if i == cursor {
                caret = (x, cy);
            }
            // A word-separator slot is left blank so masked words read as
            // separate clusters; it still occupies one step (uniform caret math).
            let is_gap = self.word_mask.get(i).copied().unwrap_or(false);
            if !is_gap {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x,
                            y: cy - diameter / 2.0,
                            width: diameter,
                            height: diameter,
                        },
                        border: Border {
                            radius: (diameter / 2.0).into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    self.dot_color,
                );
            }
            x += step;
        }
        if cursor >= self.char_len {
            if multiline && x + diameter > bounds.x + bounds.width {
                x = bounds.x;
                cy += line_h;
            }
            caret = (x, cy);
        }

        if state.focused {
            let caret_w = (self.text_size * 0.08).max(1.0);
            let caret_h = self.text_size;
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: caret.0,
                        y: caret.1 - caret_h / 2.0,
                        width: caret_w,
                        height: caret_h,
                    },
                    ..Default::default()
                },
                self.caret_color,
            );
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        // Keep the caret within bounds if the buffer changed underneath us.
        state.cursor = state.cursor.min(self.char_len);

        match event {
            Event::Clipboard(clipboard::Event::Read(read)) => {
                if !std::mem::take(&mut state.paste_pending) || !state.focused {
                    return;
                }
                let Ok(content) = read else { return };
                let clipboard::Content::Text(pasted) = content.as_ref() else { return };
                // One pass from iced's buffer to the one copy handed on, which
                // the update loop wipes: digit-restricted fields keep only
                // digits, and a capped field takes only what fits. Sized up
                // front so growing it never leaves an unwiped buffer behind.
                let digits_only = self.digit_cap.is_some();
                let room = [self.digit_cap, self.char_cap]
                    .into_iter()
                    .flatten()
                    .map(|cap| cap.saturating_sub(self.char_len))
                    .min()
                    .unwrap_or(usize::MAX);
                let mut kept = String::with_capacity(pasted.len());
                kept.extend(pasted.chars().filter(|c| !digits_only || c.is_ascii_digit()).take(room));
                let pasted = kept;
                let count = pasted.chars().count();
                if count > 0 {
                    shell.publish((self.on_paste)(state.cursor, pasted));
                    state.cursor += count;
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let bounds = layout.bounds();
                let over = cursor.is_over(bounds);
                if state.focused != over {
                    state.focused = over;
                }
                if over {
                    if self.box_height.is_some() {
                        // Word cells map a click to its word and column — the
                        // whole point of the numbered grid is that word 17 is
                        // findable AND fixable. The wrapped paragraph still
                        // isn't mapped and drops the caret at the end.
                        match (self.cells, self.grid, cursor.position()) {
                            (Some(cells), Some(g), Some(pos)) => {
                                match self.cell_click_index(cells, g, bounds, pos) {
                                    Some(i) => state.cursor = i,
                                    None => self.open_next_cell(state, shell),
                                }
                            }
                            _ => state.cursor = self.char_len,
                        }
                    } else if let Some(pos) = cursor.position() {
                        let rel = (pos.x - bounds.x).max(0.0);
                        let idx = ((rel + self.step() / 2.0) / self.step()) as usize;
                        // A scrolled grid field shows a window onto the buffer,
                        // so the click lands on the slot under the pointer —
                        // not on the same-numbered slot.
                        let first = if self.grid.is_some() {
                            self.grid_scroll(
                                self.grid_cols(bounds.width),
                                state.cursor.min(self.char_len),
                            )
                        } else {
                            0
                        };
                        state.cursor = (first + idx).min(self.char_len);
                    }
                    shell.capture_event();
                }
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                text,
                modifiers,
                ..
            }) => {
                if !state.focused {
                    return;
                }

                use keyboard::key::Named;
                match key {
                    keyboard::Key::Named(Named::Backspace) => {
                        if state.cursor > 0 {
                            state.cursor -= 1;
                            shell.publish((self.on_remove)(state.cursor));
                            shell.capture_event();
                        }
                    }
                    keyboard::Key::Named(Named::Delete) => {
                        if state.cursor < self.char_len {
                            shell.publish((self.on_remove)(state.cursor));
                            shell.capture_event();
                        }
                    }
                    keyboard::Key::Named(Named::ArrowLeft) => {
                        state.cursor = state.cursor.saturating_sub(1);
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    keyboard::Key::Named(Named::ArrowRight) => {
                        state.cursor = (state.cursor + 1).min(self.char_len);
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    keyboard::Key::Named(Named::Home) => {
                        state.cursor = 0;
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    keyboard::Key::Named(Named::End) => {
                        state.cursor = self.char_len;
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    keyboard::Key::Named(Named::Enter) => {
                        // Word cells: Enter is `next word`, the way it is on
                        // any numbered form — submit only once every cell is
                        // taken. Every other field keeps Enter = submit.
                        if let Some(cells) = self.cells {
                            let capacity = cells.cols * cells.rows;
                            match advance_from(&self.word_mask, self.char_len, state.cursor, capacity) {
                                Advance::Jump(i) => {
                                    state.cursor = i;
                                    shell.capture_event();
                                    shell.request_redraw();
                                    return;
                                }
                                Advance::Open => {
                                    self.open_next_cell(state, shell);
                                    shell.capture_event();
                                    return;
                                }
                                Advance::Full => {}
                            }
                        }
                        if let Some(msg) = &self.on_submit {
                            shell.publish(msg.clone());
                            shell.capture_event();
                        }
                    }
                    keyboard::Key::Character(s)
                        if modifiers.command() && s.as_str() == "v" =>
                    {
                        state.paste_pending = true;
                        shell.read_clipboard(clipboard::Kind::Text);
                        shell.capture_event();
                    }
                    _ => {
                        if let Some(t) = text {
                            if let Some(c) = t.chars().next().filter(|c| !c.is_control()) {
                                // Digit-restricted: digits only, never past the cap.
                                if let Some(cap) = self.digit_cap {
                                    if !c.is_ascii_digit() || self.char_len >= cap {
                                        return;
                                    }
                                }
                                // Length-capped: full is full.
                                if self.char_cap.is_some_and(|cap| self.char_len >= cap) {
                                    return;
                                }
                                // Word cells: a word is capped at its cell's
                                // width, which is the wordlist's longest word
                                // — a ninth letter is never right, and taking
                                // it would draw on into the next cell. The
                                // separator itself is always taken: it opens
                                // the next word.
                                if let Some(cells) = self.cells {
                                    if !c.is_whitespace()
                                        && word_full(&self.word_mask, self.char_len, state.cursor, cells.word_cols)
                                    {
                                        return;
                                    }
                                }
                                shell.publish((self.on_insert)(state.cursor, c));
                                state.cursor += 1;
                                shell.capture_event();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The recovery-phrase field's width, restated from
    /// `ui::components::wallet_setup` (`INNER - GUTTER - TRAIL`). Restated
    /// rather than imported so this module stays pure wrap arithmetic — and so
    /// the two rows below fail loudly if the grid ever moves under them.
    const FIELD: usize = 80 - 17 - 3;

    /// A real 24-word phrase; the wrap has to hold for the words people paste,
    /// not just for uniform filler.
    const PHRASE: &str = "abandon ability able about above absent absorb abstract absurd \
abuse access accident account accuse achieve acid acoustic acquire across act action actor \
actress actual";

    fn mask(s: &str) -> Vec<bool> {
        s.chars().map(|c| c.is_whitespace()).collect()
    }

    /// The two failures that matter: a row wider than the box (which pushes the
    /// closing `│` out of column) and a word that silently loses a character.
    #[test]
    fn wrapping_never_overflows_and_never_loses_a_word() {
        let m = mask(PHRASE);
        let chars: Vec<char> = PHRASE.chars().collect();

        for cols in [20usize, 30, 46, 60, 200] {
            let rebuilt: Vec<String> = wrap_rows(&m, m.len(), cols)
                .iter()
                .map(|r| chars[r.clone()].iter().collect())
                .collect();

            for row in &rebuilt {
                assert!(row.chars().count() <= cols, "cols={cols} overflowed: {row:?}");
            }
            assert_eq!(
                rebuilt.join(" ").split_whitespace().collect::<Vec<_>>(),
                PHRASE.split_whitespace().collect::<Vec<_>>(),
                "cols={cols} changed the phrase",
            );
        }
    }

    /// The ceiling the setup frame grows to. BIP39's longest words are 8
    /// characters, so 24 of them plus 23 spaces is the widest a recovery phrase
    /// can ever be — the phrase field starts at one row and grows, and this is
    /// how far growth can go.
    ///
    /// Not reachable from a wordlist: it takes 24 copies of one maximum-length
    /// word. See [`a_real_phrase_stops_at_four_rows`] for what a drawn phrase
    /// actually does.
    #[test]
    fn worst_case_bip39_lands_in_four_rows() {
        let phrase = vec!["abstract"; 24].join(" ");
        assert_eq!(phrase.chars().count(), 215);
        let m = mask(&phrase);
        assert_eq!(grid_rows(&m, m.len(), FIELD), 4);
    }

    /// The ceiling `wallet_setup::PHRASE_ROWS` rests on: the phrase field starts
    /// one row tall and grows, and four rows is as far as it can ever go — the
    /// worst case above is a hand-built phrase of 24 copies of one eight-letter
    /// word, and nothing a wordlist can produce exceeds it.
    ///
    /// Three rows is the ordinary outcome, not a guarantee. Wrapping is
    /// word-aware, so a row wastes whatever the next word will not fit into:
    /// over 200k randomly drawn phrases 99.96% land in three rows and the rest
    /// in four, at BIP39's mean word length of 5.4. The long-worded sample below
    /// is one of the rare four-row ones and is here to pin that it still fits.
    #[test]
    fn a_real_phrase_stops_at_four_rows() {
        // The BIP39 vector for 32 zero bytes, then a deliberately long-worded
        // phrase — the shape that costs the extra row.
        for phrase in [
            "abandon abandon abandon abandon abandon abandon abandon abandon \
             abandon abandon abandon abandon abandon abandon abandon abandon \
             abandon abandon abandon abandon abandon abandon abandon art",
            "shoulder culture abandon require between segment blossom exhibit \
             quantum village crumble athlete transfer another package pyramid \
             surprise october distance neutral wrestle sponsor language brave",
        ] {
            let m = mask(phrase);
            let rows = grid_rows(&m, m.len(), FIELD);
            assert!(rows <= 4, "{rows} rows for {} characters", phrase.chars().count());
        }
    }

    /// Nothing else fits, so it splits — the alternative is a row running under
    /// the frame.
    #[test]
    fn a_word_wider_than_the_row_splits() {
        assert_eq!(wrap_rows(&vec![false; 25], 25, 10), vec![0..10, 10..20, 20..25]);
    }

    /// A single-line grid field scrolls rather than wrapping, so its window is
    /// always exactly one row — however long the secret gets. This is the
    /// arithmetic behind that window; if it ever yields more than `cols` slots
    /// or loses the caret, the overflow draws below the field.
    #[test]
    fn a_scrolled_window_is_one_row_and_always_holds_the_caret() {
        let cols = 26usize;
        for len in [0usize, 1, 25, 26, 27, 80] {
            for cursor in [0usize, len / 2, len] {
                let first = cursor.saturating_sub(cols - 1);
                let window = first..(first + cols).min(len);
                assert!(window.len() <= cols, "len={len} cursor={cursor} window too wide");
                assert!(
                    cursor >= window.start && cursor <= window.start + cols,
                    "len={len} cursor={cursor} fell outside the window",
                );
            }
        }
    }

    #[test]
    fn degenerate_inputs_still_yield_one_row() {
        assert_eq!(wrap_rows(&[], 0, 46), vec![0..0]);
        assert_eq!(wrap_rows(&[], 5, 0), vec![0..5]);
        assert_eq!(wrap_rows(&mask("one two"), 7, 46), vec![0..7]);
    }

    /// The import screen's cell geometry, restated (3 × 8 word cells, `NN `
    /// prefix, two-column gaps).
    const CELLS: Cells = Cells {
        cols: 3,
        rows: 8,
        word_cols: 8,
        index_cols: 3,
        gap_cols: 2,
        index_color: Color::BLACK,
    };

    /// Words land column-major — 01–08 down the first column — and an
    /// over-long phrase spills row-major into rows BELOW the grid, never off
    /// its right edge.
    /// Enter (and a click on an empty cell) walks the words: from inside
    /// word 1 to the start of word 2, from the last word to a fresh cell,
    /// and — at capacity, with no word after the caret — to the submit.
    #[test]
    fn enter_walks_the_cells_and_submits_only_when_full() {
        let two = "abandon ability";
        let m = mask(two);
        let n = two.chars().count();
        assert_eq!(advance_from(&m, n, 3, 24), Advance::Jump(8), "mid word 1 -> start of word 2");
        assert_eq!(advance_from(&m, n, 7, 24), Advance::Jump(8), "end of word 1 -> start of word 2");
        assert_eq!(advance_from(&m, n, 8, 24), Advance::Open, "start of word 2 -> open cell 3");
        assert_eq!(advance_from(&m, n, n, 24), Advance::Open, "end of word 2 -> open cell 3");
        assert_eq!(advance_from(&[], 0, 0, 24), Advance::Open, "an empty box has nowhere else to go");
        // A trailing separator: cell 3 is already open, the caret sits in it.
        let open = "abandon ability ";
        let m = mask(open);
        assert_eq!(advance_from(&m, open.len(), open.len(), 24), Advance::Open);
        // Two words is the whole phrase for a 2-cell grid: full from the last
        // word on, still a jump from the first.
        assert_eq!(advance_from(&m, open.len(), 10, 2), Advance::Full);
        assert_eq!(advance_from(&m, open.len(), open.len(), 2), Advance::Full);
        assert_eq!(advance_from(&m, open.len(), 2, 2), Advance::Jump(8));
    }

    /// The per-word cap: an eighth letter is taken, a ninth is not, and a
    /// caret in a fresh cell — or in a shorter word — is never blocked.
    #[test]
    fn a_word_stops_at_its_cell_width() {
        let s = "abcdefgh ab";
        let m = mask(s);
        let n = s.len();
        assert!(word_full(&m, n, 8, 8), "end of an 8-letter word");
        assert!(word_full(&m, n, 3, 8), "inside an 8-letter word");
        assert!(!word_full(&m, n, 9, 8), "start of the short word");
        assert!(!word_full(&m, n, n, 8), "end of the short word");
        let open = "abcdefgh ";
        let m = mask(open);
        assert!(!word_full(&m, open.len(), open.len(), 8), "a fresh cell takes anything");
        assert!(!word_full(&[], 0, 0, 8), "so does an empty box");
    }

    #[test]
    fn cells_place_words_column_major_and_spill_downward() {
        assert_eq!(CELLS.block(0), (0, 0));
        assert_eq!(CELLS.block(7), (0, 7));
        assert_eq!(CELLS.block(8), (1, 0));
        assert_eq!(CELLS.block(23), (2, 7));
        assert_eq!(CELLS.block(24), (0, 8));
        assert_eq!(CELLS.block(26), (2, 8));
        assert_eq!(CELLS.block(27), (0, 9));

        let m = mask(PHRASE);
        assert_eq!(word_runs(&m, m.len()).len(), 24);
        assert_eq!(cell_layout_rows(&m, m.len(), CELLS), 8);
        // 27 words: three spill into one extra row.
        let long = format!("{PHRASE} extra words here");
        let lm = mask(&long);
        assert_eq!(cell_layout_rows(&lm, lm.len(), CELLS), 9);
        // An empty buffer still lays out the full grid.
        assert_eq!(cell_layout_rows(&[], 0, CELLS), 8);
    }

    /// The position table: every caret index has a slot, a separator sits one
    /// column past its word's last character, and the slot after it is the
    /// next word's first glyph column — in the next cell down.
    #[test]
    fn cell_positions_walk_words_and_separators() {
        let m = mask(PHRASE);
        let pos = cell_positions(&m, m.len(), CELLS);
        assert_eq!(pos.len(), m.len() + 1);

        let runs = word_runs(&m, m.len());
        // Word 0 ("abandon") starts behind its index prefix, row 0.
        assert_eq!(pos[runs[0].start], (CELLS.index_cols, 0));
        // The separator after it sits one column past its last character…
        let sep = runs[0].end;
        assert_eq!(pos[sep], (CELLS.index_cols + runs[0].len(), 0));
        assert_eq!(pos[sep].0, pos[sep - 1].0 + 1);
        // …and the next slot is word 1's first column, one row down.
        assert_eq!(pos[sep + 1], (CELLS.index_cols, 1));
        // Word 8 tops the second column.
        assert_eq!(pos[runs[8].start], (CELLS.stride() + CELLS.index_cols, 0));
        // Nothing draws left of its cell's glyph area or on a phantom row.
        for (i, &(x, r)) in pos.iter().enumerate() {
            assert!(x >= CELLS.index_cols, "slot {i} at column {x} is inside a prefix");
            assert!(r < 8, "slot {i} on row {r} of an 8-row phrase");
        }
    }

    /// Messy typing states stay well-defined: leading, trailing and doubled
    /// separators produce the same words a clean phrase does, and the table
    /// still has one slot per caret index.
    #[test]
    fn cell_positions_survive_messy_separators() {
        for messy in [" alpha beta", "alpha  beta", "alpha beta ", "  "] {
            let m = mask(messy);
            let runs = word_runs(&m, m.len());
            assert!(runs.len() <= 2, "{messy:?} produced {} words", runs.len());
            let pos = cell_positions(&m, m.len(), CELLS);
            assert_eq!(pos.len(), m.len() + 1, "{messy:?} lost a caret slot");
        }
        assert_eq!(word_runs(&mask(" a  b "), 6), vec![1..2, 4..5]);
        assert!(word_runs(&[], 0).is_empty());
    }
}
