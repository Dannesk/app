//! Primitives of the **compact spec** — the 2026-08-31 design language the
//! import/create screens speak first and the send v3 / trade redesigns adopt
//! next: cardless panes divided by single hairlines, mono eyebrows instead of
//! headings, 8.5–12.5px type, and a neutral CTA with no brand blue anywhere.
//!
//! Everything here draws from [`CompactPalette`](crate::utils::theme::compact)
//! — never from `AppPalette`. The two vocabularies must not mix on one screen:
//! the compact ink ramp is a step lighter and its hairlines a step stronger
//! than the shipped ones, and a widget that borrows across the line lands
//! visibly off.
//!
//! Nothing in this module knows what a wallet is. The setup-screen assembly
//! (panes, gating, copy) lives in [`super::wallet_setup`]; these are the
//! bricks: eyebrow rows, segmented pickers, boxed secret fields, the pixel
//! entropy meter, stat rows, links and the CTA.

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

// ── Type scale ──────────────────────────────────────────────────────────────
// The compact screens sit below the app's usual 13/14.5 floor by design: they
// are data screens. Inter for anything read, JetBrains Mono strictly for
// working data — words, keys, counts, bits, money, eyebrows, units.

/// Mono eyebrows, header meta, and the status-row links.
pub const EYEBROW: f32 = 8.5;
/// The window title in the header strip (Inter).
pub const TITLE: f32 = 13.0;
/// Status rows, stat-block rows, radio prose (Inter or mono by content).
pub const ROW: f32 = 10.0;
/// Segmented controls and buttons (Inter).
pub const SEG: f32 = 11.5;
/// Single-line field values (mono).
pub const FIELD: f32 = 12.5;
/// The meter rows: entropy · crack time (mono).
pub const METER: f32 = 9.5;
/// The restore faces' error line (Inter). Was also `note()`'s size, until
/// the orbit replaced the import and create foot notes and that function went
/// with them (2026-09-12).
pub const NOTE: f32 = 9.5;
/// The reveal eye, rendered smaller here than on the old surfaces (was 16).
pub const EYE: f32 = 13.0;
/// The corner circle (Preferences, back): 22 across, glyph 12, sat at (14, 12)
/// from the window's top-left on every screen that draws one.
pub const CORNER: f32 = 22.0;
pub const CORNER_GLYPH: f32 = 12.0;
pub const CORNER_TOP: f32 = 12.0;
pub const CORNER_LEFT: f32 = 14.0;

/// The meter's key column (`entropy`, `crack time`).
pub const METER_KEY_W: f32 = 62.0;
/// The meter track.
pub const METER_BAR_W: f32 = 66.0;
pub const METER_BAR_H: f32 = 8.0;

// ── The 24-word cell grid ───────────────────────────────────────────────────
// One geometry for every face of it: the editable import grid (SecureInput's
// word-cells mode), the read-only create grid, and the tests that pin both.
// Three columns of eight, column-major — 01–08 down the first column.

pub const WORDS_COLS: usize = 3;
pub const WORDS_ROWS: usize = 8;
/// Characters a word's glyphs get — BIP39's longest word, which is also
/// the per-word cap the box types to. Pinned to the wordlist by test.
pub const WORD_CHARS: usize = 8;
/// The `NN ` index prefix.
pub const INDEX_CHARS: usize = 3;
/// Characters between blocks.
pub const GAP_CHARS: usize = 2;
/// Character columns the whole grid spans (the last block has no gap).
pub const GRID_SPAN: usize = WORDS_COLS * (INDEX_CHARS + WORD_CHARS + GAP_CHARS) - GAP_CHARS;

/// Boxed-field padding (`5px 8px` in the mock).
const BOX_PAD_V: f32 = 5.0;
const BOX_PAD_H: f32 = 8.0;
/// Gap between a field's text and its eye.
const EYE_GAP: f32 = 6.0;

// ── Rules ───────────────────────────────────────────────────────────────────

/// A 1px horizontal rule the full width of its parent.
pub fn hairline<'a>(color: Color) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(move |_| container::Style { background: Some(color.into()), ..Default::default() })
        .into()
}

/// A 1px vertical rule at a fixed height.
pub fn vrule<'a>(color: Color, height: f32) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fixed(1.0))
        .height(Length::Fixed(height))
        .style(move |_| container::Style { background: Some(color.into()), ..Default::default() })
        .into()
}

/// A button that is nothing but its content.
fn bare(_: &iced::Theme, _: button::Status) -> button::Style {
    button::Style {
        background: None,
        border: Border::default(),
        text_color: Color::TRANSPARENT,
        shadow: Shadow::default(),
        snap: false,
    }
}

/// The requirement, shown *in* the empty field. It clears on the first
/// keystroke like any placeholder.
///
/// The last survivor of `components::form`, which held the pre-compact
/// `Form` builder — its `.framed()` / `.split_row()` vocabulary went with the
/// screens send v3 and import v2 replaced.
pub fn minimum_hint(min: usize, unit: &str) -> String {
    format!("minimum {min} {unit}")
}

// ── Chrome ──────────────────────────────────────────────────────────────────

/// The corner control: a [`CORNER`] circle on a `border_soft` hairline holding
/// a line glyph in `muted`; a `hover` wash and `text` under the pointer.
/// Balance's Preferences and every back control draw this one, so the corner
/// reads the same whichever screen you are on.
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
        .into()
}

/// The back control, app-wide: the chevron alone in the [`corner_button`]
/// circle, the same object as Balance's Preferences — no `back` label.
///
/// Still `‹`, never `×`: this steps back, it does not dismiss.
pub fn back_chevron<'a>(msg: Message, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    corner_button(icons::CHEVRON_LEFT.clone(), msg, p, scale)
}

/// The header strip over the panes: title left (Inter 13), the box's
/// shortcuts right (mono 8.5 upper, none on a read-only flow), a `rule`
/// along the bottom that meets the pane divider in a T. The chain is not
/// named here — the pane's orbit already says which one this is.
pub fn header_strip<'a>(
    title:     &str,
    shortcuts: &[(&str, &str)],
    p:         &'static CompactPalette,
    scale:     f32,
) -> Element<'a, Message> {
    // The keys the box takes, top right: a `shortcuts`
    // label in `faint`, then each key in `muted` with what it does in
    // `faint` — the key is the thing to find, the action its caption. Empty
    // draws nothing.
    let shortcuts: Element<'a, Message> = if shortcuts.is_empty() {
        Space::new().into()
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
    .into()
}

/// A section eyebrow: two mono-caps reads, what-it-is left and a qualifier
/// right, `padding-bottom 8`. Every pane section opens with one, so content
/// across the divider starts on the same baseline.
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
    .into()
}

/// An eyebrow whose right slot is DATA and must not be upper-cased — the
/// derivation path under `recovery phrase`. `m/…` is a private path and `M/…`
/// a public one in BIP-32 notation, so [`eyebrow`]'s `to_uppercase` would
/// turn the guard into a different claim. Same size, same ink, same padding.
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
    .into()
}

/// A status-row link — `CLEAR ›`, `PASTE ALL ›` — mono caps in `dim`, a
/// `hover` wash and `text` on hover.
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
    .into()
}

/// The reveal toggle at the compact 13px: open eye while masked, closed while
/// revealed — the same pairing every other surface uses.
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
    .into()
}

// ── Segmented ───────────────────────────────────────────────────────────────

/// Options in a [`track`] (mono 9 upper).
pub const TRACK_OPTION: f32 = 9.0;

/// An N-option segmented TRACK — Settings' rows and the chart's range
/// control: `2px` pad, 1px `border_soft`, radius 7; each option `3px 9px`,
/// radius 5, mono 9 upper. Selected = `pill` fill + `text` ink; unselected
/// lifts to `dim` on hover. Distinct from [`segmented`] above (the 2-up boxed
/// pair a form uses): this is the quiet one for choosing a view, not a value.
pub fn track<'a>(
    options: Vec<(&'a str, bool, Message)>,
    p: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let mut track = row![].align_y(Alignment::Center);
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
            }),
        );
    }

    container(track)
        .padding(Padding::new(2.0 * scale))
        .style(move |_| container::Style {
            border: Border { color: p.border_soft, width: 1.0, radius: (7.0 * scale).into() },
            ..Default::default()
        })
        .into()
}


/// A 2-up segmented control: equal-width segments, gap 4, 1px `border`,
/// radius 6. Active = `neutral` fill + `text`; inactive = transparent +
/// `muted`, lifting to `dim` on hover. **Neutral, never colored.**
pub fn segmented<'a>(
    left:  (&str, bool, Message),
    right: (&str, bool, Message),
    p:     &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    segmented_many(vec![left, right], p, scale)
}

/// The same control with any number of segments — the address-type picker's
/// four. Two-up callers keep [`segmented`]; the style is defined once, here.
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
        .into()
    };

    let mut r = row![].spacing(4.0 * scale).width(Length::Fill);
    for (label, active, msg) in items {
        r = r.push(seg(label, active, msg));
    }
    r.into()
}

// ── Fields ──────────────────────────────────────────────────────────────────

/// A single-line secret in a compact box at the setup screens' [`FIELD`] 12.5.
///
/// Import and create draw this one. The dashboard's signing surfaces draw
/// [`boxed_credential`] instead — see there for why the two sizes exist.
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

/// The same box at [`ROW`] 10 — the size the 24-word grid is locked to.
///
/// Every dashboard signing surface stacks a key box and a 25th-word box
/// directly under (or over) a phrase grid, and that grid cannot move: 37 mono
/// characters have to fit the pane, which is what sets the pane's width floor.
/// At 12.5 the single-line boxes read a quarter larger than the grid beside
/// them, which is the mismatch the user called out (2026-09-12).
///
/// **Import and create keep 12.5 on purpose.** They are not signing screens —
/// their phrase grid sits in a different column from their key field, so the
/// two sizes never meet, and their type scale was settled separately. Do not
/// "unify" these two functions.
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

/// A single-line secret in a compact box: 1px border (`focus` once something
/// is typed — the stand-in for focus, which `SecureInput` doesn't surface),
/// radius 6, `5px 8px` padding, the value at `base` mono, the eye at its right.
///
/// `width` is the whole row before scale; the input takes what the padding,
/// the eye and its gap leave, rounded down to whole columns.
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
    .into()
}

// ── Meter ───────────────────────────────────────────────────────────────────

/// The severity colour of a ladder rung, in compact tokens.
pub fn tier_colour(tier: entropy::Tier, p: &'static CompactPalette) -> Color {
    match tier {
        entropy::Tier::Weak => p.red,
        entropy::Tier::Fair => p.amber,
        entropy::Tier::Strong => p.green,
    }
}

/// The `crack time` runs and the colour the reading wears — the same rule the
/// old surface's `crack_runs` applies: the cost rides after a `·` while the
/// time is a number; a comparison rung stands alone. One ladder
/// ([`entropy::crack_verdict`]) produces both words and colour, so the two
/// can never disagree.
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

/// The two live rows under a secret a person chose:
/// `entropy   [■■■░░░] 62 bits` and `crack time   12 years · ≈ $3.4M`.
/// The track is a 66×8 pixel bar on `pill`; the fill is a **solid** severity
/// colour, no alpha, at [`entropy::bar_fill`]'s fraction of never (2^128).
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
    ]
    .into()}

/// A run of mono spans at an already-scaled size — the compact data voice.
pub fn mono_runs<'a>(runs: Vec<Run>, size: f32) -> Element<'a, Message> {
    use iced::widget::text::{Span, Wrapping};
    use iced::widget::{rich_text, span};
    let spans: Vec<Span<'static, ()>> = runs
        .into_iter()
        .map(|(s, c)| span(s).size(size).font(MONO).color(c))
        .collect();
    rich_text(spans).wrapping(Wrapping::None).into()
}

// ── Stat block ──────────────────────────────────────────────────────────────

/// The facts block under a storage choice: a `rule` above, then key/value
/// rows — key Inter 10 `dim`, value mono 10 `text` pinned right.
pub fn stat_rows<'a>(
    rows:  &[(&str, &str)],
    p:     &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let mut col = column![hairline(p.rule), Space::new().height(9.0 * scale)].spacing(0);
    let mut list = column![].spacing(3.0 * scale);
    for (k, v) in rows {
        list = list.push(
            row![
                text(k.to_string()).size(ROW * scale).color(p.dim),
                Space::new().width(Length::Fill),
                text(v.to_string()).font(MONO).size(ROW * scale).color(p.text),
            ]
            .align_y(Alignment::Center),
        );
    }
    col = col.push(list);
    container(col)
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(11.0 * scale))
        .into()
}

// ── CTA ─────────────────────────────────────────────────────────────────────

/// The screen's one exit: full width, `neutral` fill, 1px `border`, Inter 11.5
/// `text`, radius 6. Disabled = transparent fill, `border_soft`, `faint` —
/// drawn dead rather than hidden, so the way forward is always visible.
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
    .into()
}

// ── Phrase box ──────────────────────────────────────────────────────────────

/// The compact recovery-phrase box: the **editable numbered word grid** — the
/// masked field in `SecureInput`'s word-cells mode at mono 10, in a
/// fixed-height box (the resting and the filled state are the same shape),
/// the eye overlaid top-right. Word 17 is findable AND fixable: a click lands
/// the caret in that word, at that column; a click on an empty cell opens the
/// next one, and Enter moves word to word — the widget
/// owns both, so the signing panes get them too. The reveal eye is ours, kept from
/// the shipped flow — the mock drops it, but a pasted phrase you cannot
/// unmask to verify is a phrase you re-type.
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

    stack![well, toggle].into()
}
