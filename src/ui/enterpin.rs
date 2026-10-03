//! The launch gate — six digits under the brand mark.
//!
//! Three states, derived purely from existing state:
//!   * **choose a pin** — no gate registered, no draft
//!   * **confirm**      — a draft is held; the back circle restores it
//!   * **welcome back** — a gate exists; unlock
//!
//! The sixth digit commits. No button, no step counter: the PIN is
//! fixed-length, so the last digit *is* the submit, and a screen that then asks
//! you to press Unlock is asking you to confirm something you have finished
//! saying.
//!
//! ## What this gate is for, and what it is not
//!
//! It stops someone glancing at an unattended screen. That is the whole threat
//! model. Six digits is trivially brute-forced by anyone holding the file, and
//! breaking in buys nothing anyway — **no transaction can be signed from here**.
//! The seed is encrypted at rest and only the sign-time credential opens it.
//!
//! So the buffer is a plain `String` and the field is drawn by hand. The
//! memory-hardened [`SecureInput`](crate::utils::secure_input::SecureInput) and
//! `SecureString` exist for the seed, the 25th word and the encryption key —
//! things that actually protect value. Reaching for them here would cost real
//! complexity and, worse, imply a guarantee this screen does not make.
//!
//! **Masking stays.** Snooping is exactly what the dots are for.
//!
//! ## The mark, and why nothing here runs at 60fps any more
//!
//! `utils/brand_flow.rs` and its one-second launch animation are deleted. The
//! guilloché bloom that replaced it is gone too (user, 2026-09-15): a canvas
//! redrawn every frame, behind a 60fps pump that rebuilt this entire view 62
//! times a second — for a screen someone looks at for two seconds. The static
//! mark says the same thing at no cost and is the better brand beat for a
//! launch gate. `bloom::draw` is left unused; the clock in
//! [`crate::utils::bloom`] is NOT idle — the caret and the shake read it, and
//! so does the activity log's spinner, so nothing owns a second timebase.
//!
//! What still wants a pump: the caret blink and the wrong-PIN shake.
//! `controller::subscription` picks the rate off [`fast_pump`] — 60fps only
//! while the shake is actually running, otherwise [`IDLE_PUMP_MS`].
//!
//! ## Clicking the field does nothing, and that is the design
//!
//! There is no `text_input` on this screen. The field is drawn by hand (above)
//! and keystrokes arrive from a keyboard subscription that listens the whole
//! time the gate is up. So there is no focus to take and no cursor to place: a
//! click has nothing to land on, and typing works whether or not anyone clicks
//! first.
//!
//! **A blinking caret on the empty field was tried and REJECTED** (user,
//! 2026-09-15: "somehow worse"). The theory was that an empty field with no
//! caret reads as one waiting to be focused; in practice a caret sitting next
//! to a placeholder just looked wrong. The placeholder stands alone again, and
//! the caret appears with the first digit, as it always did. Do not re-add it.
//!
//! Do NOT "fix" the click by adding a text widget either. A fixed-length masked
//! PIN that submits itself has no meaningful cursor position, and a focusable
//! field can *lose* focus — turning a screen that always accepts typing into
//! one that sometimes silently does not.

use iced::widget::Row;
use iced::Widget as _;
use iced::widget::{Space, column, container, row, stack, svg, text};
use iced::{Alignment, Border, Color, Element, Length, Padding};

use crate::controller::app_state::AppState;
use crate::controller::message::Message;
use crate::gate::PIN_DIGITS;
use crate::ui::components::compact as ck;
use crate::utils::bloom;
use crate::utils::fonts::MONO;
use crate::utils::icons;
use crate::utils::theme::{self, CompactPalette};

// ── Geometry ────────────────────────────────────────────────────────────────
// One centred column, no card. Nothing in it moves between states: the message
// slot holds its height when empty so the field never shifts under someone
// typing blind.

/// The mark. The bloom it replaced was 230 — a soft figure that wanted the
/// room; a crisp one does not. This and `TITLE_GAP` are the two numbers to
/// nudge if the column sits wrong.
const LOGO: f32 = 76.0;
const TITLE_GAP: f32 = 14.0;
/// Title → field. Was 26 under the bloom, which stood 230 tall and gave the
/// column its air for free. The mark is a third of that now, so the gap has to
/// buy the separation itself or the field crowds the title (user, 2026-09-15).
const FIELD_GAP: f32 = 38.0;
const MSG_GAP: f32 = 14.0;

const FIELD_W: f32 = 208.0;
const FIELD_H: f32 = 44.0;
/// Reserved whether or not there is a message.
const MSG_H: f32 = 15.0;

/// 9px dot, 5.5px of margin each side — so 11px between neighbours.
const DOT: f32 = 9.0;
const DOT_GAP: f32 = 11.0;
const CARET_W: f32 = 1.5;
const CARET_H: f32 = 19.0;
const CARET_GAP: f32 = 4.0;

/// The shake needs room to travel in both directions, and padding cannot go
/// negative — so the column rests inset by this much and swings around it.
const SHAKE_BASE: f32 = 8.0;
const SHAKE_AMP: f32 = 6.0;
const SHAKE_SECS: f32 = 0.34;
/// How long a wrong PIN stays red before the field returns to resting.
const ERROR_SECS: f32 = 1.0;
/// Hard on/off, not a fade.
const BLINK_SECS: f32 = 1.05;

const TITLE_SIZE: f32 = 12.5;
const PLACEHOLDER_SIZE: f32 = 13.0;
const MSG_SIZE: f32 = 11.5;

const PLACEHOLDER: &str = "6 digits";

/// The wrong-PIN beat: a damped swing over 340ms that wants every frame.
pub const FAST_PUMP_MS: u64 = 16;
/// The caret: a hard on/off every `BLINK_SECS / 2` (~525ms), so this puts each
/// edge within an eighth of its own half-period — invisible — for 8 view
/// rebuilds a second. The bloom is what used to need all 62 of them.
pub const IDLE_PUMP_MS: u64 = 120;

/// How often the gate wants a redraw, or **`None` when nothing on it moves**.
///
/// Three cases, cheapest first:
///
///   * **nothing** — empty field, no error. The mark is static and the caret is
///     not drawn until the first digit, so there is no animation to serve: the
///     gate costs zero frames while someone reads it. The keystroke that ends
///     that state rebuilds the view by itself.
///   * [`IDLE_PUMP_MS`] — digits are showing, so the caret is blinking.
///   * [`FAST_PUMP_MS`] — the shake is running.
///
/// Lives here because the timings it reads are this screen's; a subscription
/// that guessed them would drift the moment one changed.
pub fn pump_ms(state: &AppState) -> Option<u64> {
    if state.gate_error_at.is_some_and(|t| t.elapsed().as_secs_f32() < ERROR_SECS) {
        Some(FAST_PUMP_MS)
    } else if state.gate_pin_input.is_empty() {
        None
    } else {
        Some(IDLE_PUMP_MS)
    }
}

pub fn render_gate(state: &AppState) -> Element<'_, Message> {
    let cp = theme::compact(&state.theme);
    let scale = state.scale();

    if state.gate_wiping {
        return wiping(cp, scale);
    }

    // ── State ────────────────────────────────────────────────────────────────
    let confirming = !state.gate_exists && state.gate_pin_draft.is_some();
    let title = if state.gate_exists {
        "welcome back"
    } else if confirming {
        "confirm"
    } else {
        "choose a pin"
    };

    // One timestamp drives both the shake and the red field, read off the
    // redraw pump. No tween state, no timer message.
    let since_error = state
        .gate_error_at
        .map(|t| t.elapsed().as_secs_f32())
        .filter(|e| *e < ERROR_SECS);
    let erroring = since_error.is_some();

    let typed = state.gate_pin_input.chars().count().min(PIN_DIGITS);
    let clock = bloom::clock_secs();

    // ── The column ───────────────────────────────────────────────────────────
    let rule_colour = if erroring { cp.red } else { cp.focus };

    let contents: Element<'_, Message> = if typed == 0 && !erroring {
        text(PLACEHOLDER)
            .font(MONO)
            .size(PLACEHOLDER_SIZE * scale)
            .color(cp.faint)
            .boxed()
    } else {
        let dot_colour = if erroring { cp.red } else { cp.text };
        let mut r: Row<Element<'_, Message>> = row![].align_y(Alignment::Center);
        for i in 0..typed {
            if i > 0 {
                r = r.push(Space::new().width(DOT_GAP * scale).boxed());
            }
            r = r.push(dot(dot_colour, scale));
        }
        // The caret rides the right edge of what has been typed and blinks hard
        // on/off — a fade reads as a glow at 1.5px.
        let lit = (clock / BLINK_SECS).fract() < 0.5;
        r = r
            .push(Space::new().width(CARET_GAP * scale).boxed())
            .push(caret(if lit { cp.dim } else { Color::TRANSPARENT }, scale));
        r.boxed()
    };

    let field = column![
        container(contents)
            .width(Length::Fixed(FIELD_W * scale))
            .height(Length::Fixed((FIELD_H - 1.0) * scale))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
        container(Space::new())
            .width(Length::Fixed(FIELD_W * scale))
            .height(Length::Fixed(1.0))
            .style(move |_| container::Style {
                background: Some(rule_colour.into()),
                ..Default::default()
            }),
    ];

    // The slot is always present; only its text comes and goes.
    // The red field and the shake are a one-second beat. The SENTENCE is not:
    // it stays until the next keystroke, because "1 attempt left" is not
    // something to flash for a second and take away.
    let message: Element<'_, Message> = match &state.gate_error {
        Some(msg) => text(msg.as_str())
            .font(MONO)
            .size(MSG_SIZE * scale)
            .color(cp.red)
            .boxed(),
        None => Space::new().boxed(),
    };

    let col = column![
        svg(icons::LOGO.clone())
            .width(Length::Fixed(LOGO * scale))
            .height(Length::Fixed(LOGO * scale))
            .style(move |_, _| svg::Style { color: Some(cp.text) }),
        Space::new().height(TITLE_GAP * scale),
        text(title).font(MONO).size(TITLE_SIZE * scale).color(cp.muted),
        Space::new().height(FIELD_GAP * scale),
        field,
        Space::new().height(MSG_GAP * scale),
        container(message).height(Length::Fixed(MSG_H * scale)),
    ]
    .align_x(Alignment::Center);

    // ── The shake ────────────────────────────────────────────────────────────
    // A damped swing over 340ms, derived from the same timestamp as the red.
    let offset = since_error
        .filter(|e| *e < SHAKE_SECS)
        .map_or(0.0, |e| {
            let t = e / SHAKE_SECS;
            SHAKE_AMP * (t * std::f32::consts::PI * 2.5).sin() * (1.0 - t)
        });

    let centred = container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(
            Padding::new(0.0)
                .left((SHAKE_BASE + offset) * scale)
                .right((SHAKE_BASE - offset) * scale),
        );

    // ── Back ─────────────────────────────────────────────────────────────────
    // Confirm only. Out of the column and pinned top-left, so the column's
    // geometry is the same in all three states.
    let back: Element<'_, Message> = if confirming {
        container(ck::back_chevron(Message::GateBack, cp, scale))
            .padding(Padding::new(0.0).top(ck::CORNER_TOP * scale).left(ck::CORNER_LEFT * scale))
            .boxed()
    } else {
        Space::new().boxed()
    };

    container(stack![centred, back].width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style { background: Some(cp.window.into()), ..Default::default() })
        .boxed()
}

// ── Parts ───────────────────────────────────────────────────────────────────

fn dot<'a>(colour: Color, scale: f32) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fixed(DOT * scale))
        .height(Length::Fixed(DOT * scale))
        .style(move |_| container::Style {
            background: Some(colour.into()),
            border: Border { radius: (DOT / 2.0 * scale).into(), ..Default::default() },
            ..Default::default()
        })
        .boxed()
}

fn caret<'a>(colour: Color, scale: f32) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fixed(CARET_W * scale))
        .height(Length::Fixed(CARET_H * scale))
        .style(move |_| container::Style {
            background: Some(colour.into()),
            ..Default::default()
        })
        .boxed()
}

/// Self-wipe in progress: the fifth consecutive miss has erased everything and
/// the app is about to quit (`Message::GateWipeExit`). No bloom — the mark is
/// for a screen someone is waiting at, and this one is over.
fn wiping<'a>(cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(
        column![
            text("that was the last attempt").font(MONO).size(TITLE_SIZE * scale).color(cp.red),
            Space::new().height(MSG_GAP * scale),
            text("every wallet on this device has been erased")
                .font(MONO)
                .size(MSG_SIZE * scale)
                .color(cp.muted),
        ]
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_| container::Style { background: Some(cp.window.into()), ..Default::default() })
    .boxed()
}
