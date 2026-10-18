
use std::ops::Range;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::text::{self, Renderer as _};
use iced::advanced::widget::{self, tree, Tree, Widget};
use iced::advanced::{clipboard, mouse, Shell};
use iced::time::{Duration, Instant};
use iced::{
    alignment, keyboard, window, Border, Color, Event, Font, Length, Pixels, Point, Rectangle,
    Size,
};

struct State {
    focused: bool,
    cursor: usize,
    paste_pending: bool,
    window_focused: bool,
    now: Instant,
    blink_anchor: Instant,
}

impl Default for State {
    fn default() -> Self {
        let now = Instant::now();
        State {
            focused: false,
            cursor: 0,
            paste_pending: false,
            window_focused: true,
            now,
            blink_anchor: now,
        }
    }
}

impl State {
    const BLINK_MS: u128 = 500;

    fn caret_on(&self) -> bool {
        self.focused
            && self.window_focused
            && ((self.now - self.blink_anchor).as_millis() / Self::BLINK_MS).is_multiple_of(2)
    }

    fn touch(&mut self) {
        self.blink_anchor = Instant::now();
    }
}

#[derive(Clone, Copy)]
struct Grid {
    advance: f32,
    line_height: f32,
    font: Font,
}

fn wrap_rows(mask: &[bool], len: usize, cols: usize) -> Vec<Range<usize>> {
    let greedy = greedy_rows(mask, len, cols);

    if greedy.len() < 2 || !mask.iter().any(|&sep| sep) {
        return greedy;
    }

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
            i = i.max(start);
            last_sep = None;
            continue;
        }
        i += 1;
    }
    rows.push(start..len);
    rows
}

pub fn grid_rows(mask: &[bool], len: usize, cols: usize) -> usize {
    wrap_rows(mask, len, cols).len()
}

#[derive(Clone, Copy)]
struct Cells {
    cols: usize,
    rows: usize,
    word_cols: usize,
    index_cols: usize,
    gap_cols: usize,
    index_color: Color,
}

impl Cells {
    fn stride(&self) -> usize {
        self.index_cols + self.word_cols + self.gap_cols
    }

    fn block(&self, w: usize) -> (usize, usize) {
        let cells = self.cols * self.rows;
        if w < cells {
            (w / self.rows, w % self.rows)
        } else {
            let o = w - cells;
            (o % self.cols, self.rows + o / self.cols)
        }
    }

    fn slot(&self, w: usize, offset: usize) -> (usize, usize) {
        let (c, r) = self.block(w);
        (c * self.stride() + self.index_cols + offset, r)
    }
}

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

fn cell_layout_rows(mask: &[bool], len: usize, cells: Cells) -> usize {
    let words = word_runs(mask, len).len();
    if words <= cells.cols * cells.rows {
        cells.rows
    } else {
        let spill = words - cells.cols * cells.rows;
        cells.rows + spill.div_ceil(cells.cols)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Advance {
    Jump(usize),
    Open,
    Full,
}

fn advance_from(mask: &[bool], len: usize, cursor: usize, capacity: usize) -> Advance {
    let words = word_runs(mask, len);
    let here = words.iter().position(|r| cursor <= r.end);
    match here.and_then(|i| words.get(i + 1)) {
        Some(next) => Advance::Jump(next.start),
        None if words.len() >= capacity => Advance::Full,
        None => Advance::Open,
    }
}

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
    box_height: Option<f32>,
    pin_cells: Option<usize>,
    cell_bg: Color,
    cell_border: Color,
    digit_cap: Option<usize>,
    char_cap: Option<usize>,
    word_mask: Vec<bool>,
    word_gaps: bool,
    revealed: Option<&'a str>,
    grid: Option<Grid>,
    cells: Option<Cells>,
    placeholder: String,
    placeholder_color: Color,
    placeholder_font: Option<Font>,
    dot_color: Color,
    caret_color: Color,
    on_insert: Box<dyn Fn(usize, char) -> Message + 'a>,
    on_remove: Box<dyn Fn(usize) -> Message + 'a>,
    on_paste: Box<dyn Fn(usize, String) -> Message + 'a>,
    on_submit: Option<Message>,
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
            placeholder_font: None,
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

    pub fn multiline(mut self, height: f32) -> Self {
        self.box_height = Some(height);
        self
    }

    pub fn char_cap(mut self, n: usize) -> Self {
        self.char_cap = Some(n);
        self
    }

    pub fn word_mask(mut self, mask: Vec<bool>) -> Self {
        self.word_mask = mask;
        self
    }

    pub fn grid(mut self, advance: f32, line_height: f32, font: Font) -> Self {
        self.grid = Some(Grid { advance, line_height, font });
        self
    }

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

    pub fn revealed(mut self, text: Option<&'a str>) -> Self {
        self.revealed = text;
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn placeholder_color(mut self, color: Color) -> Self {
        self.placeholder_color = color;
        self
    }

    pub fn placeholder_font(mut self, font: Font) -> Self {
        self.placeholder_font = Some(font);
        self
    }

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

    fn grid_cols(&self, width: f32) -> usize {
        match self.grid {
            Some(g) => (width / g.advance + 1e-3).floor().max(1.0) as usize,
            None => 0,
        }
    }

    fn grid_scroll(&self, cols: usize, cursor: usize) -> usize {
        if self.box_height.is_some() {
            return 0;
        }
        cursor.saturating_sub(cols.saturating_sub(1))
    }

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
            let _ = n;
            return cell + gap;
        }
        if let Some(g) = self.grid {
            return g.advance;
        }
        let factor = if self.box_height.is_some() { 0.52 } else { 0.77 };
        self.text_size * factor
    }

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

        if self.char_len == 0 {
            if !self.placeholder.is_empty() {
                let shown: String = self.placeholder.chars().take(cols).collect();
                renderer.fill_text(
                    text::Text {
                        content: shown,
                        bounds: Size::new(bounds.width, g.line_height),
                        size: Pixels(self.text_size),
                        line_height: text::LineHeight::Relative(1.0),
                        font: self.placeholder_font.unwrap_or(g.font),
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
            if state.caret_on() {
                caret_quad(renderer, col_x(0), row_y(0));
            }
            return;
        }

        if let Some(cells) = self.cells {
            self.draw_cells(g, cells, state, renderer, bounds);
            return;
        }

        let first = self.grid_scroll(cols, cursor);

        let rows = if self.box_height.is_some() {
            wrap_rows(&self.word_mask, self.char_len, cols)
        } else {
            vec![first..(first + cols).min(self.char_len)]
        };

        let diameter = self.text_size * 0.45;
        let inset = (g.advance - diameter) / 2.0;
        let mut caret: Option<(f32, f32)> = None;

        for (r, range) in rows.iter().enumerate() {
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

        if state.caret_on() {
            let (x, cy) = caret.unwrap_or((col_x(0), row_y(0)));
            caret_quad(renderer, x, cy);
        }
    }

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

        if state.caret_on() {
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
      
        tree::State::new(State {
            focused: self.autofocus,
            cursor: if self.autofocus { self.char_len } else { 0 },
            ..State::default()
        })
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &iced::Renderer, limits: &layout::Limits) {
        let width = match self.pin_cells {
            Some(n) => Length::Fixed(self.pin_width(n)),
            None => self.width,
        };
        let height = match (self.pin_cells, self.box_height) {
            (Some(_), _) | (None, None) => self.widget_height(),
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
        let hint = renderer.hint_factor();
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        let multiline = self.box_height.is_some();

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
            if state.caret_on() {
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

        if let Some(plain) = &self.revealed {
            renderer.fill_text(
                text::Text {
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

        if state.caret_on() {
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
        state.cursor = state.cursor.min(self.char_len);

        match event {
            Event::Clipboard(clipboard::Event::Read(read)) => {
                if !std::mem::take(&mut state.paste_pending) || !state.focused {
                    return;
                }
                let Ok(content) = read else { return };
                let clipboard::Content::Text(pasted) = content.as_ref() else { return };
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
                    state.window_focused = true;
                    state.touch();
                    if self.box_height.is_some() {
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
                state.touch();

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
                                if let Some(cap) = self.digit_cap {
                                    if !c.is_ascii_digit() || self.char_len >= cap {
                                        return;
                                    }
                                }
                                if self.char_cap.is_some_and(|cap| self.char_len >= cap) {
                                    return;
                                }
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
            Event::Window(window::Event::Unfocused) => {
                state.window_focused = false;
                if state.focused {
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::Focused) => {
                state.window_focused = true;
                if state.focused {
                    state.touch();
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if state.focused && state.window_focused && self.pin_cells.is_none() {
                    state.now = *now;
                    let elapsed = (*now - state.blink_anchor).as_millis();
                    let until_flip = State::BLINK_MS - elapsed % State::BLINK_MS;
                    shell.request_redraw_at(*now + Duration::from_millis(until_flip as u64));
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

    const FIELD: usize = 80 - 17 - 3;

    const PHRASE: &str = "abandon ability able about above absent absorb abstract absurd \
abuse access accident account accuse achieve acid acoustic acquire across act action actor \
actress actual";

    fn mask(s: &str) -> Vec<bool> {
        s.chars().map(|c| c.is_whitespace()).collect()
    }

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

    #[test]
    fn worst_case_bip39_lands_in_four_rows() {
        let phrase = vec!["abstract"; 24].join(" ");
        assert_eq!(phrase.chars().count(), 215);
        let m = mask(&phrase);
        assert_eq!(grid_rows(&m, m.len(), FIELD), 4);
    }

    #[test]
    fn a_real_phrase_stops_at_four_rows() {
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

    #[test]
    fn a_word_wider_than_the_row_splits() {
        assert_eq!(wrap_rows(&vec![false; 25], 25, 10), vec![0..10, 10..20, 20..25]);
    }

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

    const CELLS: Cells = Cells {
        cols: 3,
        rows: 8,
        word_cols: 8,
        index_cols: 3,
        gap_cols: 2,
        index_color: Color::BLACK,
    };

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
        let open = "abandon ability ";
        let m = mask(open);
        assert_eq!(advance_from(&m, open.len(), open.len(), 24), Advance::Open);
        assert_eq!(advance_from(&m, open.len(), 10, 2), Advance::Full);
        assert_eq!(advance_from(&m, open.len(), open.len(), 2), Advance::Full);
        assert_eq!(advance_from(&m, open.len(), 2, 2), Advance::Jump(8));
    }

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
        let long = format!("{PHRASE} extra words here");
        let lm = mask(&long);
        assert_eq!(cell_layout_rows(&lm, lm.len(), CELLS), 9);
        assert_eq!(cell_layout_rows(&[], 0, CELLS), 8);
    }

    #[test]
    fn cell_positions_walk_words_and_separators() {
        let m = mask(PHRASE);
        let pos = cell_positions(&m, m.len(), CELLS);
        assert_eq!(pos.len(), m.len() + 1);

        let runs = word_runs(&m, m.len());
        assert_eq!(pos[runs[0].start], (CELLS.index_cols, 0));
        let sep = runs[0].end;
        assert_eq!(pos[sep], (CELLS.index_cols + runs[0].len(), 0));
        assert_eq!(pos[sep].0, pos[sep - 1].0 + 1);
        assert_eq!(pos[sep + 1], (CELLS.index_cols, 1));
        assert_eq!(pos[runs[8].start], (CELLS.stride() + CELLS.index_cols, 0));
        for (i, &(x, r)) in pos.iter().enumerate() {
            assert!(x >= CELLS.index_cols, "slot {i} at column {x} is inside a prefix");
            assert!(r < 8, "slot {i} on row {r} of an 8-row phrase");
        }
    }

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
