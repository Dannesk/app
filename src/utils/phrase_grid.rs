use std::ops::Range;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::text::{self, Renderer as _};
use iced::advanced::widget::{self, tree, Tree, Widget};
use iced::advanced::{clipboard, mouse, Shell};
use iced::time::{Duration, Instant};
use iced::{alignment, keyboard, window, Border, Color, Event, Font, Length, Pixels, Point, Rectangle, Size};

use crate::controller::message::PhraseOp;
use crate::utils::phrase::{self, MASK, SLOTS, WORD_MAX};

const BASELINE: f32 = 0.36;

#[derive(Clone, Copy)]
pub struct Look {
    pub word: f32,
    pub index: f32,
    pub advance: f32,
    pub slot_h: f32,
    pub pad_top: f32,
    pub pad_side: f32,
    pub line_h: f32,
    pub index_w: f32,
    pub gap: f32,
    pub col_gap: f32,
    pub row_gap: f32,
    pub font: Font,
    pub text: Color,
    pub faint: Color,
    pub rule: Color,
    pub focus: Color,
    pub red: Color,
    pub pill: Color,
}

#[derive(Clone, Copy)]
enum Target {
    Next(usize),
    Fill,
}

struct State {
    focused: bool,
    slot: usize,
    offset: usize,
    selected: bool,
    pending: Option<Target>,
    sent: Option<Option<usize>>,
    jumps: u32,
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
            slot: 0,
            offset: 0,
            selected: false,
            pending: None,
            sent: None,
            jumps: 0,
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

pub struct PhraseGrid<'a, Message> {
    fields: Vec<Range<usize>>,
    revealed: Option<&'a str>,
    active: Option<usize>,
    lit: usize,
    shown: usize,
    near: usize,
    jump: (u32, usize),
    band: f32,
    autofocus: bool,
    look: Look,
    on_op: Box<dyn Fn(PhraseOp) -> Message + 'a>,
}

impl<'a, Message> PhraseGrid<'a, Message> {
    pub fn new(
        fields: Vec<Range<usize>>,
        revealed: Option<&'a str>,
        look: Look,
        on_op: impl Fn(PhraseOp) -> Message + 'a,
    ) -> Self {
        PhraseGrid {
            fields,
            revealed,
            active: None,
            lit: 0,
            shown: 0,
            near: 0,
            jump: (0, 0),
            band: 0.0,
            autofocus: false,
            look,
            on_op: Box::new(on_op),
        }
    }

    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }

    pub fn line(mut self, active: Option<usize>, lit: usize, shown: usize, near: usize) -> Self {
        self.active = active;
        self.lit = lit;
        self.shown = shown;
        self.near = near;
        self
    }

    pub fn jump(mut self, count: u32, from: usize) -> Self {
        self.jump = (count, from);
        self
    }

    pub fn band(mut self, height: f32) -> Self {
        self.band = height;
        self
    }

    pub fn columns(look: &Look, width: f32) -> usize {
        let min = 2.0 * look.pad_side + look.index_w + look.gap + (WORD_MAX as f32 + 0.5) * look.advance;
        [4usize, 3, 2]
            .into_iter()
            .find(|&c| c as f32 * min + (c - 1) as f32 * look.col_gap <= width + 0.5)
            .unwrap_or(2)
    }

    pub fn height(look: &Look, cols: usize) -> f32 {
        let rows = SLOTS / cols;
        rows as f32 * look.slot_h + (rows - 1) as f32 * look.row_gap
    }

    fn len(&self, k: usize) -> usize {
        self.fields.get(k).map_or(0, |r| r.len())
    }

    fn last(&self) -> usize {
        self.fields.len().saturating_sub(1)
    }

    fn word(&self, k: usize) -> Option<&'a str> {
        let text = self.revealed?;
        let r = self.fields.get(k)?;
        Some(phrase::word(text, r))
    }

    fn bad(&self, k: usize, caret: Option<usize>) -> bool {
        self.word(k).is_some_and(|w| {
            !w.is_empty() && !phrase::is_word(w) && (caret != Some(k) || phrase::matches(w).is_empty())
        })
    }

    fn ghost(&self, w: &str) -> Option<&'static str> {
        if self.shown == 0 || w.is_empty() {
            return None;
        }
        let m = phrase::matches(w);
        let pick = m.get(self.lit.min(self.shown - 1).min(m.len().checked_sub(1)?))?;
        pick.get(w.len()..)
    }

    fn matched(&self, k: usize) -> bool {
        self.word(k).is_some_and(|w| !w.is_empty() && !phrase::matches(w).is_empty())
    }

    fn next_from(&self, from: usize) -> Option<usize> {
        let n = self.fields.len();
        let order = || (1..n).map(move |i| (from + i) % n);
        order()
            .find(|&k| self.len(k) == 0)
            .or_else(|| order().find(|&k| self.word(k).is_some_and(|w| !phrase::is_word(w))))
    }

    fn resolve(&self, state: &mut State, target: Target) {
        state.selected = false;
        match target {
            Target::Next(from) => match self.next_from(from) {
                Some(k) => {
                    state.slot = k;
                    state.offset = self.len(k);
                }
                None => state.focused = false,
            },
            Target::Fill => {
                let bad = (0..self.fields.len()).any(|k| self.bad(k, None));
                match (0..self.fields.len()).find(|&k| self.len(k) == 0) {
                    Some(k) if !bad => {
                        state.slot = k;
                        state.offset = 0;
                    }
                    _ => state.focused = false,
                }
            }
        }
    }

    fn clamp(&self, state: &mut State) {
        state.slot = state.slot.min(self.last());
        state.offset = state.offset.min(self.len(state.slot));
        if self.len(state.slot) == 0 {
            state.selected = false;
        }
    }

    fn cell(&self, b: Rectangle, cols: usize, k: usize) -> Rectangle {
        let rows = SLOTS / cols;
        let col_w = (b.width - (cols - 1) as f32 * self.look.col_gap) / cols as f32;
        let (c, r) = (k / rows, k % rows);
        Rectangle {
            x: b.x + c as f32 * (col_w + self.look.col_gap),
            y: b.y + r as f32 * (self.look.slot_h + self.look.row_gap),
            width: col_w,
            height: self.look.slot_h,
        }
    }

    fn hit(&self, b: Rectangle, pos: Point) -> usize {
        let cols = Self::columns(&self.look, b.width);
        let rows = SLOTS / cols;
        let col_w = (b.width - (cols - 1) as f32 * self.look.col_gap) / cols as f32;
        let c = ((pos.x - b.x) / (col_w + self.look.col_gap)).floor().max(0.0) as usize;
        let r = ((pos.y - b.y) / (self.look.slot_h + self.look.row_gap)).floor().max(0.0) as usize;
        c.min(cols - 1) * rows + r.min(rows - 1)
    }

    fn publish(&self, shell: &mut Shell<'_, Message>, op: PhraseOp) {
        shell.publish((self.on_op)(op));
    }

    fn advance(&self, state: &mut State, shell: &mut Shell<'_, Message>, tab: bool) {
        let k = state.slot;
        state.selected = false;
        if self.len(k) == 0 {
            if tab {
                self.resolve(state, Target::Next(k));
            }
            return;
        }
        if self.revealed.is_some() {
            if self.matched(k) {
                self.publish(shell, PhraseOp::Take(k, self.lit, tab));
            } else if tab {
                self.publish(shell, PhraseOp::Advance);
            } else {
                return;
            }
        } else {
            self.publish(shell, PhraseOp::Advance);
        }
        state.pending = Some(Target::Next(k));
    }

    fn key(
        &self,
        state: &mut State,
        key: &keyboard::Key,
        text: Option<&str>,
        modifiers: keyboard::Modifiers,
        shell: &mut Shell<'_, Message>,
    ) -> bool {
        use keyboard::key::Named;
        let k = state.slot;
        let len = self.len(k);
        match key {
            keyboard::Key::Named(Named::Tab) => {
                if modifiers.shift() {
                    state.selected = false;
                    if k > 0 {
                        state.slot = k - 1;
                        state.offset = self.len(k - 1);
                    }
                } else {
                    self.advance(state, shell, true);
                }
            }
            keyboard::Key::Named(Named::Enter) | keyboard::Key::Named(Named::Space) => {
                self.advance(state, shell, false);
            }
            keyboard::Key::Named(Named::ArrowDown) | keyboard::Key::Named(Named::ArrowUp) => {
                if self.matched(k) && self.shown > 0 {
                    let lit = self.lit.min(self.shown - 1);
                    let down = matches!(key, keyboard::Key::Named(Named::ArrowDown));
                    let next = if down { (lit + 1) % self.shown } else { (lit + self.shown - 1) % self.shown };
                    self.publish(shell, PhraseOp::Lit(next));
                }
            }
            keyboard::Key::Named(Named::ArrowLeft) => {
                if state.selected {
                    state.selected = false;
                    state.offset = 0;
                } else if state.offset > 0 {
                    state.offset -= 1;
                } else if k > 0 {
                    state.slot = k - 1;
                    state.offset = self.len(k - 1);
                }
            }
            keyboard::Key::Named(Named::ArrowRight) => {
                if state.selected {
                    state.selected = false;
                    state.offset = len;
                } else if state.offset < len {
                    state.offset += 1;
                } else if k < self.last() {
                    state.slot = k + 1;
                    state.offset = 0;
                }
            }
            keyboard::Key::Named(Named::Home) => {
                state.selected = false;
                state.offset = 0;
            }
            keyboard::Key::Named(Named::End) => {
                state.selected = false;
                state.offset = len;
            }
            keyboard::Key::Named(Named::Backspace) => {
                if state.selected {
                    self.publish(shell, PhraseOp::Replace(k, None));
                    state.selected = false;
                    state.offset = 0;
                } else if state.offset > 0 {
                    self.publish(shell, PhraseOp::Erase(k, state.offset - 1));
                    state.offset -= 1;
                } else if k > 0 {
                    if len == 0 && k == self.last() {
                        self.publish(shell, PhraseOp::StepBack(k));
                    }
                    state.slot = k - 1;
                    state.offset = self.len(k - 1);
                }
            }
            keyboard::Key::Named(Named::Delete) => {
                if state.selected {
                    self.publish(shell, PhraseOp::Replace(k, None));
                    state.selected = false;
                    state.offset = 0;
                } else if state.offset < len {
                    self.publish(shell, PhraseOp::Erase(k, state.offset));
                }
            }
            keyboard::Key::Named(Named::Escape) => {
                state.focused = false;
                state.selected = false;
            }
            keyboard::Key::Character(c) if modifiers.command() && c.as_str() == "v" => {
                state.paste_pending = true;
                shell.read_clipboard(clipboard::Kind::Text);
            }
            _ => {
                if modifiers.command() || modifiers.alt() {
                    return false;
                }
                let Some(ch) = text.and_then(|t| t.chars().next()) else { return false };
                if ch == ' ' {
                    self.advance(state, shell, false);
                } else if let Some(d) = ch.to_digit(10).filter(|d| (1..=9).contains(d)) {
                    let n = d as usize - 1;
                    if let Some(w) = self.word(k).filter(|w| !w.is_empty()) {
                        let room = if phrase::matches(w).is_empty() { self.near } else { self.shown };
                        if n < room {
                            self.publish(shell, PhraseOp::Pick(k, n));
                            state.pending = Some(Target::Next(k));
                        }
                    }
                } else if ch.is_ascii_alphabetic() {
                    let c = ch.to_ascii_lowercase();
                    if state.selected {
                        self.publish(shell, PhraseOp::Replace(k, Some(c)));
                        state.selected = false;
                        state.offset = 1;
                    } else if len < WORD_MAX {
                        self.publish(shell, PhraseOp::Type(k, state.offset, c));
                        state.offset += 1;
                    }
                }
            }
        }
        true
    }

    fn sync(&self, state: &mut State, shell: &mut Shell<'_, Message>) {
        let want = state.focused.then_some(state.slot);
        if want == self.active {
            state.sent = None;
        } else if state.sent != Some(want) {
            self.publish(shell, PhraseOp::Caret(want));
            state.sent = Some(want);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn label(
        &self,
        renderer: &mut iced::Renderer,
        content: String,
        size: f32,
        at: Point,
        align_x: text::Alignment,
        color: Color,
        clip: Rectangle,
    ) {
        let hint = renderer.hint_factor();
        renderer.fill_text(
            text::Text {
                content,
                bounds: Size::new(clip.width, self.look.line_h),
                size: Pixels(size),
                line_height: text::LineHeight::Relative(1.0),
                font: self.look.font,
                align_x,
                align_y: alignment::Vertical::Center,
                shaping: text::Shaping::Advanced,
                ellipsis: text::Ellipsis::None,
                hint_factor: hint,
                wrapping: text::Wrapping::None,
            },
            at,
            color,
            clip,
        );
    }
}

impl<Message> widget::Meta for PhraseGrid<'_, Message> {}

impl<'a, Message> Widget<Message, iced::Theme, iced::Renderer> for PhraseGrid<'a, Message>
where
    Message: Clone,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fit)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        let slot = (0..self.fields.len()).find(|&k| self.len(k) == 0).unwrap_or(self.last());
        tree::State::new(State {
            focused: self.autofocus,
            slot,
            offset: self.len(slot),
            jumps: self.jump.0,
            ..State::default()
        })
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &iced::Renderer, limits: &layout::Limits) {
        {
            let state = tree.state.downcast_mut::<State>();
            if let Some(target) = state.pending.take() {
                self.resolve(state, target);
            }
            if state.jumps != self.jump.0 {
                state.jumps = self.jump.0;
                if state.focused {
                    self.resolve(state, Target::Next(self.jump.1));
                }
            }
            self.clamp(state);
        }
        let cols = Self::columns(&self.look, limits.max.width);
        let height = Self::height(&self.look, cols);
        tree.size = layout::atomic(limits, Length::Fill, Length::Fixed(height));
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
        let state = tree.state.downcast_ref::<State>();
        let b = layout.bounds();
        let look = self.look;
        let cols = Self::columns(&look, b.width);
        let caret = state.focused.then_some(state.slot);

        for k in 0..SLOTS {
            let cell = self.cell(b, cols, k);
            let len = self.len(k);
            let active = caret == Some(k);
            let bad = self.bad(k, caret);
            let mid = cell.y + look.pad_top + look.line_h / 2.0;
            let x = cell.x + look.pad_side + look.index_w + look.gap;

            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle { x: cell.x, y: cell.y + cell.height - 1.0, width: cell.width, height: 1.0 },
                    ..Default::default()
                },
                if bad { look.red } else if active { look.focus } else { look.rule },
            );

            self.label(
                renderer,
                format!("{:02}", k + 1),
                look.index,
                Point::new(cell.x + look.pad_side + look.index_w, mid + BASELINE * (look.word - look.index)),
                text::Alignment::Right,
                look.faint,
                cell,
            );

            if active && state.selected && len > 0 {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: x - 3.0,
                            y: mid - look.line_h / 2.0 - 1.0,
                            width: len as f32 * look.advance + 6.0,
                            height: look.line_h + 2.0,
                        },
                        border: Border { radius: 4.0.into(), ..Default::default() },
                        ..Default::default()
                    },
                    look.pill,
                );
            }

            match self.word(k) {
                Some(w) if !w.is_empty() => {
                    self.label(renderer, w.to_owned(), look.word, Point::new(x, mid), text::Alignment::Left, if bad { look.red } else { look.text }, cell);
                    if active && !bad && !state.selected {
                        if let Some(rest) = self.ghost(w) {
                            let gx = x + w.chars().count() as f32 * look.advance;
                            self.label(renderer, rest.to_owned(), look.word, Point::new(gx, mid), text::Alignment::Left, look.faint, cell);
                        }
                    }
                }
                Some(_) => {}
                None if len == 0 => {}
                None if active => {
                    self.label(renderer, "\u{b7}".repeat(len), look.word, Point::new(x, mid), text::Alignment::Left, look.text, cell);
                }
                None => {
                    self.label(renderer, MASK.to_owned(), look.word, Point::new(x, mid), text::Alignment::Left, look.faint, cell);
                }
            }

            if active && !state.selected && state.caret_on() {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: x + state.offset as f32 * look.advance,
                            y: mid - look.line_h * 0.45,
                            width: 1.5,
                            height: look.line_h * 0.9,
                        },
                        ..Default::default()
                    },
                    if bad { look.red } else { look.text },
                );
            }
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
        self.clamp(state);

        match event {
            Event::Clipboard(clipboard::Event::Read(read)) => {
                if std::mem::take(&mut state.paste_pending) && state.focused {
                    if let Ok(content) = read {
                        if let clipboard::Content::Text(pasted) = content.as_ref() {
                            self.publish(shell, PhraseOp::Fill(state.slot, pasted.clone()));
                            state.pending = Some(Target::Fill);
                            shell.capture_event();
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let b = layout.bounds();
                let band = Rectangle { y: b.y + b.height, height: self.band, ..b };
                let pos = cursor.position();
                let over = pos.filter(|p| b.contains(*p));
                if pos.is_some_and(|p| band.contains(p)) && over.is_none() {
                    self.sync(state, shell);
                    return;
                }
                state.focused = over.is_some();
                state.selected = false;
                if let Some(pos) = over {
                    state.window_focused = true;
                    state.touch();
                    let k = self.hit(b, pos);
                    if k < self.fields.len() {
                        state.slot = k;
                        state.offset = self.len(k);
                        state.selected = self.len(k) > 0;
                    } else {
                        let last = self.last();
                        state.slot = last;
                        state.offset = self.len(last);
                        if self.len(last) > 0 && self.fields.len() < SLOTS {
                            self.publish(shell, PhraseOp::Advance);
                            state.pending = Some(Target::Next(last));
                        }
                    }
                    shell.capture_event();
                }
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, text, modifiers, .. }) if state.focused => {
                state.touch();
                if self.key(state, key, text.as_deref(), *modifiers, shell) {
                    shell.capture_event();
                }
                shell.request_redraw();
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
                if state.focused && state.window_focused {
                    state.now = *now;
                    let elapsed = (*now - state.blink_anchor).as_millis();
                    let until_flip = State::BLINK_MS - elapsed % State::BLINK_MS;
                    shell.request_redraw_at(*now + Duration::from_millis(until_flip as u64));
                }
            }
            _ => {}
        }

        self.sync(state, shell);
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

