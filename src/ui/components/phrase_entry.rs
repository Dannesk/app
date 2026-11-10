use iced::widget::text::{Span, Wrapping};
use iced::widget::{button, column, container, rich_text, row, span, text, Row, Space};
use iced::Widget as _;
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::controller::app_state::PhraseLine;
use crate::controller::message::{Message, PhraseOp, SecureField, SecureOp};
use crate::secure::SecureString;
use crate::ui::components::compact;
use crate::ui::components::tui::MONO_ADVANCE;
use crate::ui::components::wallet_setup::{mnemonic_checksum_ok, TARGET_WORDS};
use crate::utils::fonts::MONO;
use crate::utils::phrase;
use crate::utils::phrase_grid::{Look, PhraseGrid};
use crate::utils::theme::CompactPalette;

const CHIPS_MAX: usize = 5;

#[derive(Clone, Copy)]
pub struct Metrics {
    pub look: Look,
    pub line_top: f32,
    pub line_h: f32,
    pub text: f32,
    pub chip_word: f32,
    pub chip_num: f32,
    pub chip_h: f32,
    pub chip_pad_l: f32,
    pub chip_pad_r: f32,
    pub chip_gap: f32,
    pub right_w: f32,
    pub more_w: f32,
    pub clear: f32,
}

fn look(cp: &'static CompactPalette, word: f32, index: f32, s: f32) -> Look {
    Look {
        word: word * s,
        index: index * s,
        advance: word * s * MONO_ADVANCE,
        slot_h: 0.0,
        pad_top: 0.0,
        pad_side: 0.0,
        line_h: 0.0,
        index_w: 0.0,
        gap: 0.0,
        col_gap: 0.0,
        row_gap: 0.0,
        font: MONO,
        text: cp.text,
        faint: cp.faint,
        rule: cp.rule,
        focus: cp.focus,
        red: cp.red,
        pill: cp.pill,
    }
}

pub fn setup(cp: &'static CompactPalette, s: f32) -> Metrics {
    Metrics {
        look: Look {
            slot_h: 27.0 * s,
            pad_top: 9.0 * s,
            pad_side: 2.0 * s,
            line_h: 15.0 * s,
            index_w: 13.0 * s,
            gap: 7.0 * s,
            col_gap: 16.0 * s,
            row_gap: 2.0 * s,
            ..look(cp, 12.5, 9.0, s)
        },
        line_top: 10.0 * s,
        line_h: 26.0 * s,
        text: 10.0 * s,
        chip_word: 12.0 * s,
        chip_num: 8.5 * s,
        chip_h: 24.0 * s,
        chip_pad_l: 7.0 * s,
        chip_pad_r: 9.0 * s,
        chip_gap: 4.0 * s,
        right_w: 150.0 * s,
        more_w: 56.0 * s,
        clear: 10.5 * s,
    }
}

pub fn pane(cp: &'static CompactPalette, s: f32) -> Metrics {
    Metrics {
        look: Look {
            slot_h: 22.0 * s,
            pad_top: 7.0 * s,
            pad_side: 1.0 * s,
            line_h: 13.0 * s,
            index_w: 11.0 * s,
            gap: 5.0 * s,
            col_gap: 12.0 * s,
            row_gap: 1.0 * s,
            ..look(cp, 11.0, 8.0, s)
        },
        line_top: 6.0 * s,
        line_h: 22.0 * s,
        text: 9.5 * s,
        chip_word: 10.5 * s,
        chip_num: 7.5 * s,
        chip_h: 20.0 * s,
        chip_pad_l: 6.0 * s,
        chip_pad_r: 8.0 * s,
        chip_gap: 3.0 * s,
        right_w: 150.0 * s,
        more_w: 50.0 * s,
        clear: 9.5 * s,
    }
}

pub fn height(m: &Metrics, width: f32) -> f32 {
    let cols = PhraseGrid::<Message>::columns(&m.look, width);
    PhraseGrid::<Message>::height(&m.look, cols) + m.line_top + m.line_h
}

pub struct Entry<'a> {
    pub field: SecureField,
    pub seed: &'a SecureString,
    pub revealed: bool,
    pub line: &'a PhraseLine,
    pub width: f32,
    pub eye: bool,
    pub autofocus: bool,
}

pub fn view<'a>(e: Entry<'a>, m: &Metrics, cp: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let field = e.field;
    let text_all = e.seed.as_str();
    let fields = phrase::fields(text_all);
    let count = phrase::count(text_all);
    let line = (e.line.field == Some(field)).then_some(e.line);
    let active = line.and_then(|l| l.active).filter(|&k| k < fields.len());
    let prefix = active.filter(|_| e.revealed).map(|k| phrase::word(text_all, &fields[k]));
    let matches = prefix.map_or(&[][..], phrase::matches);
    let near = match prefix {
        Some(p) if !p.is_empty() && matches.is_empty() => phrase::nearest(p),
        _ => Vec::new(),
    };

    let room = e.width - m.right_w - m.more_w;
    let mut used = 0.0;
    let mut shown: usize = 0;
    for w in matches.iter().take(CHIPS_MAX) {
        let width = chip_width(w, m);
        if shown > 0 && used + width > room {
            break;
        }
        used += width;
        shown += 1;
    }
    let lit = line.map_or(0, |l| l.lit).min(shown.saturating_sub(1));

    let mut grid = PhraseGrid::new(fields, e.revealed.then_some(text_all), m.look, move |op| Message::Phrase(field, op))
        .line(active, lit, shown, near.len())
        .jump(line.map_or(0, |l| l.jumps), line.map_or(0, |l| l.jump_from))
        .band(m.line_top + m.line_h);
    if e.autofocus {
        grid = grid.autofocus();
    }

    let mut left: Row<Element<'a, Message>> = row![].spacing(m.chip_gap).align_y(Alignment::Center);
    match (active, prefix) {
        (Some(k), Some(p)) if !p.is_empty() => {
            if !matches.is_empty() {
                for (i, w) in matches.iter().copied().take(shown).enumerate() {
                    left = left.push(chip(i + 1, w, p.len(), i == lit, Message::Phrase(field, PhraseOp::Choose(k, i)), m, cp, s));
                }
                let more = matches.len() - shown;
                if more > 0 {
                    left = left.push(msg(format!("+{more} more"), cp.muted, m.text));
                } else if matches.len() == 1 {
                    left = left.push(msg("only match".into(), cp.muted, m.text));
                }
            } else {
                left = left.push(msg(format!("No word starts with \u{201c}{p}\u{201d}"), cp.red, m.text));
                if !near.is_empty() {
                    left = left.push(msg("did you mean".into(), cp.muted, m.text));
                    for (i, w) in near.iter().copied().enumerate() {
                        left = left.push(chip(i + 1, w, 0, false, Message::Phrase(field, PhraseOp::Choose(k, i)), m, cp, s));
                    }
                }
            }
        }
        (Some(_), _) => {
            if count == 0 {
                left = left.push(msg("Type or paste your 24 words".into(), cp.muted, m.text));
            }
        }
        (None, _) => {
            let bad = if e.revealed { phrase::bad_slots(text_all) } else { Vec::new() };
            let status = if let Some(over) = line.and_then(|l| l.over) {
                Some((format!("{over} words \u{2014} expected {TARGET_WORDS}"), cp.red))
            } else if let Some(first) = bad.first() {
                Some((format!("Word {} isn't in the word list", first + 1), cp.red))
            } else if count == TARGET_WORDS {
                if mnemonic_checksum_ok(text_all) {
                    Some(("\u{2713} checksum valid".to_string(), cp.green))
                } else if e.revealed {
                    Some(("Checksum doesn't match \u{2014} check the order".to_string(), cp.red))
                } else {
                    Some(("Checksum doesn't match".to_string(), cp.red))
                }
            } else if count == 0 {
                Some(("Type or paste your 24 words".to_string(), cp.muted))
            } else {
                None
            };
            if let Some((words, ink)) = status {
                left = left.push(msg(words, ink, m.text));
            }
            if let Some(n) = line.and_then(|l| l.pasted) {
                left = left.push(msg(format!("\u{b7} pasted {n} words"), cp.muted, m.text));
            }
        }
    }

    let mut right: Row<Element<'a, Message>> = row![
        count_text(count, Some(TARGET_WORDS), m.text, cp),
        Space::new().width(10.0 * s),
    ]
    .align_y(Alignment::Center);
    if e.eye {
        right = right
            .push(compact::eye(e.revealed, Message::SecureEdit(field, SecureOp::ToggleReveal), cp, s))
            .push(Space::new().width(8.0 * s).boxed());
    }
    right = right.push(word_button("Clear", (e.seed.char_len() > 0).then(|| Message::SecureClearRequested(field)), m.clear, cp, s));

    column![
        container(grid.boxed()).width(Length::Fill),
        Space::new().height(m.line_top),
        row![left, Space::new().width(Length::Fill), right]
            .align_y(Alignment::Center)
            .height(Length::Fixed(m.line_h)),
    ]
    .width(Length::Fill)
    .boxed()
}

pub fn msg<'a>(words: String, ink: Color, size: f32) -> Element<'a, Message> {
    text(words).size(size).wrapping(Wrapping::None).color(ink).boxed()
}

pub fn count_text<'a>(count: usize, of: Option<usize>, size: f32, cp: &'static CompactPalette) -> Element<'a, Message> {
    let figure = match of {
        Some(of) => format!("{count} / {of}"),
        None => count.to_string(),
    };
    row![
        text(figure).font(MONO).size(size).color(if count == 0 { cp.faint } else { cp.dim }),
        Space::new().width(size * 0.4),
        text("words").size(size).color(cp.dim),
    ]
    .align_y(Alignment::Center)
    .boxed()
}

fn word_button<'a>(label: &str, on: Option<Message>, size: f32, cp: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let live = on.is_some();
    button(text(label.to_string()).size(size))
        .padding(Padding::new(0.0).top(3.0 * s).bottom(3.0 * s).left(6.0 * s).right(6.0 * s))
        .on_press_maybe(on)
        .style(move |_, status| {
            let hot = live && matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if hot { cp.hover } else { Color::TRANSPARENT }.into()),
                border: Border { radius: (5.0 * s).into(), ..Default::default() },
                text_color: if !live { cp.faint } else if hot { cp.text } else { cp.dim },
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}

fn chip_width(word: &str, m: &Metrics) -> f32 {
    m.chip_pad_l
        + m.chip_pad_r
        + m.chip_num * MONO_ADVANCE
        + m.chip_num * 0.7
        + word.chars().count() as f32 * m.chip_word * MONO_ADVANCE
        + m.chip_gap
}

#[allow(clippy::too_many_arguments)]
fn chip<'a>(
    n:     usize,
    word:  &'static str,
    typed: usize,
    lit:   bool,
    on:    Message,
    m:     &Metrics,
    cp:    &'static CompactPalette,
    s:     f32,
) -> Element<'a, Message> {
    let typed = typed.min(word.len());
    let spans: [Span<'a, ()>; 2] = [
        span(&word[..typed]).font(MONO).size(m.chip_word).color(cp.muted),
        span(&word[typed..]).font(MONO).size(m.chip_word).color(cp.text),
    ];
    button(
        row![
            text(n.to_string()).font(MONO).size(m.chip_num).color(cp.muted),
            Space::new().width(m.chip_num * 0.7),
            rich_text(spans).wrapping(Wrapping::None),
        ]
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(m.chip_h))
    .padding(Padding::new(0.0).left(m.chip_pad_l).right(m.chip_pad_r))
    .on_press(on)
    .style(move |_, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if lit { cp.pill } else if hot { cp.hover } else { Color::TRANSPARENT }.into()),
            border: Border { radius: (6.0 * s).into(), ..Default::default() },
            text_color: cp.text,
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}
