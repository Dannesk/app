

use iced::widget::{Column, Row};
use iced::Widget as _;
use iced::widget::{button, column, container, row, svg, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::controller::message::{Message, SecureField, SecureOp};
use crate::secure::SecureString;
use crate::ui::components::tui::{Run, MONO_ADVANCE};
use crate::utils::entropy;
use crate::utils::fonts::MONO;
use crate::utils::secure_input::SecureInput;
use crate::utils::theme::CompactPalette;
use crate::utils::icons;


pub const EYEBROW: f32 = 8.5;
pub const TITLE: f32 = 13.0;
pub const ROW: f32 = 10.0;
pub const SEG: f32 = 11.5;
pub const FIELD: f32 = 12.5;
pub const METER: f32 = 9.5;

pub const NOTE: f32 = 9.5;
pub const EYE: f32 = 13.0;

pub const CORNER: f32 = 22.0;
pub const CORNER_GLYPH: f32 = 12.0;
pub const CORNER_TOP: f32 = 12.0;
pub const CORNER_LEFT: f32 = 14.0;

pub const METER_KEY_W: f32 = 62.0;
pub const METER_BAR_W: f32 = 66.0;
pub const METER_BAR_H: f32 = 8.0;



pub const WORDS_COLS: usize = 3;
pub const WORDS_ROWS: usize = 8;

pub const WORD_CHARS: usize = 8;
pub const INDEX_CHARS: usize = 3;
pub const GAP_CHARS: usize = 2;
pub const GRID_SPAN: usize = WORDS_COLS * (INDEX_CHARS + WORD_CHARS + GAP_CHARS) - GAP_CHARS;

const BOX_PAD_V: f32 = 5.0;
const BOX_PAD_H: f32 = 8.0;
const EYE_GAP: f32 = 6.0;


pub fn hairline<'a>(color: Color) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(move |_| container::Style { background: Some(color.into()), ..Default::default() })
        .boxed()
}

pub fn vrule<'a>(color: Color, height: f32) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fixed(1.0))
        .height(Length::Fixed(height))
        .style(move |_| container::Style { background: Some(color.into()), ..Default::default() })
        .boxed()
}

fn bare(_: &iced::Theme, _: button::Status) -> button::Style {
    button::Style {
        background: None,
        border: Border::default(),
        text_color: Color::TRANSPARENT,
        shadow: Shadow::default(),
        snap: false,
    }
}


pub fn minimum_hint(min: usize, unit: &str) -> String {
    format!("minimum {min} {unit}")
}


pub fn corner_button<'a>(
    icon:  svg::Handle,
    msg:   Message,
    p:     &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let d = CORNER * scale;
    let g = CORNER_GLYPH * scale;
    let glyph = container(
        svg(icon)
            .width(Length::Fixed(g))
            .height(Length::Fixed(g))
            .style(move |_, status| svg::Style {
                color: Some(match status {
                    svg::Status::Hovered => p.text,
                    _ => p.muted,
                }),
            }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center);

    button(glyph)
        .width(Length::Fixed(d))
        .height(Length::Fixed(d))
        .padding(0)
        .on_press(msg)
        .style(move |_, status| {
            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if hot { p.hover } else { Color::TRANSPARENT }.into()),
                border: Border { color: p.border_soft, width: 1.0, radius: (d / 2.0).into() },
                text_color: if hot { p.text } else { p.muted },
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}


pub fn back_chevron<'a>(msg: Message, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    corner_button(icons::CHEVRON_LEFT.clone(), msg, p, scale)
}


pub fn header_strip<'a>(
    title:     &str,
    shortcuts: &[(&str, &str)],
    p:         &'static CompactPalette,
    scale:     f32,
) -> Element<'a, Message> {
 
    let shortcuts: Element<'a, Message> = if shortcuts.is_empty() {
        Space::new().boxed()
    } else {
        let mut runs: Vec<Run> = vec![("SHORTCUTS".to_string(), p.faint)];
        for (key, action) in shortcuts {
            runs.push((format!("   {}", key.to_uppercase()), p.muted));
            runs.push((format!(" {}", action.to_uppercase()), p.faint));
        }
        mono_runs(runs, EYEBROW * scale)
    };
    column![
        container(
            row![
                text(title.to_string()).size(TITLE * scale).color(p.text),
                Space::new().width(Length::Fill),
                shortcuts,
            ]
            .align_y(Alignment::End),
        )
        .width(Length::Fill)
        .padding(Padding::new(0.0).left(16.0 * scale).right(16.0 * scale).bottom(8.0 * scale)),
        hairline(p.rule),
    ]
    .width(Length::Fill)
    .boxed()
}


pub fn eyebrow<'a>(
    left:  &str,
    right: &str,
    p:     &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    container(
        row![
            text(left.to_uppercase()).font(MONO).size(EYEBROW * scale).color(p.muted),
            Space::new().width(Length::Fill),
            text(right.to_uppercase()).font(MONO).size(EYEBROW * scale).color(p.muted),
        ]
        .align_y(Alignment::End),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).bottom(8.0 * scale))
    .boxed()
}


pub fn eyebrow_data<'a>(
    left:  &str,
    right: &str,
    p:     &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    container(
        row![
            text(left.to_uppercase()).font(MONO).size(EYEBROW * scale).color(p.muted),
            Space::new().width(Length::Fill),
            text(right.to_string()).font(MONO).size(EYEBROW * scale).color(p.muted),
        ]
        .align_y(Alignment::End),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).bottom(8.0 * scale))
    .boxed()
}


pub fn upper_link<'a>(label: &str, msg: Message, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(
        text(format!("{} \u{203a}", label.to_uppercase()))
            .font(MONO)
            .size(EYEBROW * scale)
            .color(p.dim),
    )
    .on_press(msg)
    .padding(Padding::new(1.0 * scale).left(5.0 * scale).right(5.0 * scale))
    .style(move |_, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if hot { p.hover } else { Color::TRANSPARENT }.into()),
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (4.0 * scale).into() },
            text_color: if hot { p.text } else { p.dim },
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}

pub fn eye<'a>(revealed: bool, msg: Message, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let handle = if revealed { icons::EYE_CLOSED.clone() } else { icons::EYE_OPEN.clone() };
    button(
        svg(handle)
            .width(Length::Fixed(EYE * scale))
            .height(Length::Fixed(EYE * scale))
            .style(move |_, status| svg::Style {
                color: Some(match status {
                    svg::Status::Hovered => p.text,
                    _ => p.muted,
                }),
            }),
    )
    .on_press(msg)
    .padding(Padding::ZERO)
    .style(bare)
    .boxed()
}


pub const TRACK_OPTION: f32 = 9.0;


pub fn track<'a>(
    options: Vec<(&'a str, bool, Message)>,
    p: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let mut track: Row<Element<'_, Message>> = row![].align_y(Alignment::Center);
    for (label, active, msg) in options {
        track = track.push(
            button(
                text(label.to_uppercase())
                    .font(MONO)
                    .size(TRACK_OPTION * scale)
                    .color(if active { p.text } else { p.muted }),
            )
            .on_press(msg)
            .padding(Padding::new(3.0 * scale).left(9.0 * scale).right(9.0 * scale))
            .style(move |_, status| {
                let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: Some(if active { p.pill } else { Color::TRANSPARENT }.into()),
                    border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (5.0 * scale).into() },
                    text_color: if active { p.text } else if hot { p.dim } else { p.muted },
                    shadow: Shadow::default(),
                    snap: false,
                }
            }).boxed(),
        );
    }

    container(track)
        .padding(Padding::new(2.0 * scale))
        .style(move |_| container::Style {
            border: Border { color: p.border_soft, width: 1.0, radius: (7.0 * scale).into() },
            ..Default::default()
        })
        .boxed()
}



pub fn segmented<'a>(
    left:  (&str, bool, Message),
    right: (&str, bool, Message),
    p:     &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    segmented_many(vec![left, right], p, scale)
}

pub fn segmented_many<'a>(
    items: Vec<(&str, bool, Message)>,
    p:     &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let seg = |label: &str, active: bool, msg: Message| -> Element<'a, Message> {
        button(
            text(label.to_string())
                .size(SEG * scale)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .color(if active { p.text } else { p.muted }),
        )
        .on_press(msg)
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(4.0 * scale).bottom(4.0 * scale))
        .style(move |_, status| {
            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if active { p.neutral } else { Color::TRANSPARENT }.into()),
                border: Border { color: p.border, width: 1.0, radius: (6.0 * scale).into() },
                text_color: if active {
                    p.text
                } else if hot {
                    p.dim
                } else {
                    p.muted
                },
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
    };

    let mut r: Row<Element<'_, Message>> = row![].spacing(4.0 * scale).width(Length::Fill);
    for (label, active, msg) in items {
        r = r.push(seg(label, active, msg));
    }
    r.boxed()
}


#[allow(clippy::too_many_arguments)]
pub fn boxed_secret<'a>(
    field:       SecureField,
    secret:      &'a SecureString,
    revealed:    bool,
    placeholder: &str,
    width:       f32,
    on_submit:   Option<Message>,
    p:           &'static CompactPalette,
    scale:       f32,
) -> Element<'a, Message> {
    boxed_at(FIELD, field, secret, revealed, placeholder, width, on_submit, p, scale)
}


#[allow(clippy::too_many_arguments)]
pub fn boxed_credential<'a>(
    field:       SecureField,
    secret:      &'a SecureString,
    revealed:    bool,
    placeholder: &str,
    width:       f32,
    on_submit:   Option<Message>,
    p:           &'static CompactPalette,
    scale:       f32,
) -> Element<'a, Message> {
    boxed_at(ROW, field, secret, revealed, placeholder, width, on_submit, p, scale)
}


#[allow(clippy::too_many_arguments)]
fn boxed_at<'a>(
    base:        f32,
    field:       SecureField,
    secret:      &'a SecureString,
    revealed:    bool,
    placeholder: &str,
    width:       f32,
    on_submit:   Option<Message>,
    p:           &'static CompactPalette,
    scale:       f32,
) -> Element<'a, Message> {
    let size = base * scale;
    let advance = size * MONO_ADVANCE;
    let cols = (((width - 2.0 * BOX_PAD_H - EYE - EYE_GAP) * scale) / advance).floor() as usize;

    let mut input = SecureInput::new(
        secret.char_len(),
        move |i, c| Message::SecureEdit(field, SecureOp::Insert(i, c)),
        move |i| Message::SecureEdit(field, SecureOp::Remove(i)),
        move |i, s| Message::SecureEdit(field, SecureOp::Paste(i, s)),
    )
    .size(size)
    .width(Length::Fill)
    .dot_color(p.text)
    .caret_color(p.focus)
    .placeholder_color(p.faint)
    .placeholder(placeholder.to_owned())
    .grid(advance, size * 1.6, MONO)
    .revealed(revealed.then(|| secret.as_str()));
    if let Some(cap) = field.char_cap() {
        input = input.char_cap(cap);
    }
    if let Some(msg) = on_submit {
        input = input.on_submit(msg);
    }

    let line = if secret.char_len() > 0 { p.focus } else { p.border };
    container(
        row![
            container(input).width(Length::Fixed(cols as f32 * advance)),
            Space::new().width(Length::Fill),
            eye(revealed, Message::SecureEdit(field, SecureOp::ToggleReveal), p, scale),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(
        Padding::new(0.0)
            .top(BOX_PAD_V * scale)
            .bottom(BOX_PAD_V * scale)
            .left(BOX_PAD_H * scale)
            .right(BOX_PAD_H * scale),
    )
    .style(move |_| container::Style {
        background: Some(p.field.into()),
        border: Border { color: line, width: 1.0, radius: (6.0 * scale).into() },
        ..Default::default()
    })
    .boxed()
}


pub fn tier_colour(tier: entropy::Tier, p: &'static CompactPalette) -> Color {
    match tier {
        entropy::Tier::Weak => p.red,
        entropy::Tier::Fair => p.amber,
        entropy::Tier::Strong => p.green,
    }
}


pub fn crack_runs(bits: f64, kdf: entropy::Kdf, p: &'static CompactPalette) -> (Vec<Run>, Color) {
    let v = entropy::crack_verdict(bits, kdf);
    let colour = tier_colour(v.tier, p);
    let mut runs = vec![(v.phrase, colour)];
    if v.numeric {
        runs.push(("  \u{b7}  ".to_string(), p.faint));
        runs.push((entropy::crack_cost_phrase(bits, kdf), p.dim));
    }
    (runs, colour)
}


pub fn meter_rows<'a>(bits: f64, kdf: entropy::Kdf, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let (crack, colour) = crack_runs(bits, kdf, p);
    let key = |s: &str| {
        container(text(s.to_string()).font(MONO).size(METER * scale).color(p.muted))
            .width(Length::Fixed(METER_KEY_W * scale))
    };

    let track = container(
        container(Space::new())
            .width(Length::Fixed(METER_BAR_W * entropy::bar_fill(bits) * scale))
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(colour.into()),
                border: Border { radius: 1.0.into(), ..Default::default() },
                ..Default::default()
            }),
    )
    .width(Length::Fixed(METER_BAR_W * scale))
    .height(Length::Fixed(METER_BAR_H * scale))
    .style(move |_| container::Style {
        background: Some(p.pill.into()),
        border: Border { radius: 1.0.into(), ..Default::default() },
        ..Default::default()
    });

    column![
        row![
            key("entropy"),
            track,
            Space::new().width(9.0 * scale),
            mono_runs(vec![(format!("{} bits", bits.round() as u64), colour)], METER * scale),
        ]
        .align_y(Alignment::Center),
        Space::new().height(8.0 * scale),
        row![key("crack time"), mono_runs(crack, METER * scale)].align_y(Alignment::Center),
    ].boxed()
}

pub fn mono_runs<'a>(runs: Vec<Run>, size: f32) -> Element<'a, Message> {
    use iced::widget::text::{Span, Wrapping};
    use iced::widget::{rich_text, span};
    let spans: Vec<Span<'static, ()>> = runs
        .into_iter()
        .map(|(s, c)| span(s).size(size).font(MONO).color(c))
        .collect();
    rich_text(spans).wrapping(Wrapping::None).boxed()
}


pub fn stat_rows<'a>(
    rows:  &[(&str, &str)],
    p:     &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let mut col = column![hairline(p.rule), Space::new().height(9.0 * scale)].spacing(0);
    let mut list: Column<Element<'_, Message>> = column![].spacing(3.0 * scale);
    for (k, v) in rows {
        list = list.push(
            row![
                text(k.to_string()).size(ROW * scale).color(p.dim),
                Space::new().width(Length::Fill),
                text(v.to_string()).font(MONO).size(ROW * scale).color(p.text),
            ]
            .align_y(Alignment::Center).boxed(),
        );
    }
    col = col.push(list.boxed());
    container(col)
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(11.0 * scale))
        .boxed()
}


pub fn cta<'a>(label: &str, enabled: bool, msg: Message, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(
        text(label.to_string())
            .size(SEG * scale)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .color(if enabled { p.text } else { p.faint }),
    )
    .on_press_maybe(enabled.then_some(msg))
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(6.0 * scale).bottom(6.0 * scale))
    .style(move |_, status| {
        if !enabled {
            return button::Style {
                background: None,
                border: Border { color: p.border_soft, width: 1.0, radius: (6.0 * scale).into() },
                text_color: p.faint,
                shadow: Shadow::default(),
                snap: false,
            };
        }
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if hot { p.hover } else { p.neutral }.into()),
            border: Border { color: p.border, width: 1.0, radius: (6.0 * scale).into() },
            text_color: p.text,
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}


#[allow(clippy::too_many_arguments)]
pub fn phrase_box<'a>(
    field:     SecureField,
    secret:    &'a SecureString,
    revealed:  bool,
    placeholder: &str,
    height:    f32,
    on_submit: Message,
    p:         &'static CompactPalette,
    scale:     f32,
) -> Element<'a, Message> {
    use iced::widget::stack;

    let size = ROW * scale;
    let advance = size * MONO_ADVANCE;
    let line = size * 1.6;
    let mask: Vec<bool> = secret.as_str().chars().map(|c| c.is_whitespace()).collect();

    let input = SecureInput::new(
        secret.char_len(),
        move |i, c| Message::SecureEdit(field, SecureOp::Insert(i, c)),
        move |i| Message::SecureEdit(field, SecureOp::Remove(i)),
        move |i, s| Message::SecureEdit(field, SecureOp::Paste(i, s)),
    )
    .size(size)
    .width(Length::Fill)
    .dot_color(p.text)
    .caret_color(p.focus)
    .placeholder_color(p.faint)
    .placeholder(placeholder.to_owned())
    .grid(advance, line, MONO)
    .word_cells(WORDS_COLS, WORDS_ROWS, WORD_CHARS, INDEX_CHARS, GAP_CHARS, p.faint)
    .word_mask(mask)
    .multiline(WORDS_ROWS as f32 * line)
    .on_submit(on_submit)
    .revealed(revealed.then(|| secret.as_str()));

    let border = if secret.char_len() > 0 { p.focus } else { p.border };
    let well = container(container(input).width(Length::Fixed(GRID_SPAN as f32 * advance)))
        .width(Length::Fill)
        .height(Length::Fixed(height * scale))
        .padding(
            Padding::new(0.0)
                .top(9.0 * scale)
                .bottom(9.0 * scale)
                .left(11.0 * scale)
                .right((11.0 + EYE + EYE_GAP) * scale),
        )
        .style(move |_| container::Style {
            background: Some(p.field.into()),
            border: Border { color: border, width: 1.0, radius: (6.0 * scale).into() },
            ..Default::default()
        });

    let toggle = container(eye(
        revealed,
        Message::SecureEdit(field, SecureOp::ToggleReveal),
        p,
        scale,
    ))
    .width(Length::Fill)
    .align_x(Alignment::End)
    .padding(Padding::new(0.0).top(9.0 * scale).right(9.0 * scale));

    stack![well, toggle].boxed()
}
