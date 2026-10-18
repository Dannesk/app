

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

const LOGO: f32 = 76.0;
const TITLE_GAP: f32 = 14.0;

const FIELD_GAP: f32 = 38.0;
const MSG_GAP: f32 = 14.0;

const FIELD_W: f32 = 208.0;
const FIELD_H: f32 = 44.0;
const MSG_H: f32 = 15.0;

const DOT: f32 = 9.0;
const DOT_GAP: f32 = 11.0;
const CARET_W: f32 = 1.5;
const CARET_H: f32 = 19.0;
const CARET_GAP: f32 = 4.0;


const SHAKE_BASE: f32 = 8.0;
const SHAKE_AMP: f32 = 6.0;
const SHAKE_SECS: f32 = 0.34;
const ERROR_SECS: f32 = 1.0;
const BLINK_SECS: f32 = 1.05;

const TITLE_SIZE: f32 = 12.5;
const PLACEHOLDER_SIZE: f32 = 13.0;
const MSG_SIZE: f32 = 11.5;

const PLACEHOLDER: &str = "6 digits";

pub const FAST_PUMP_MS: u64 = 16;

pub const IDLE_PUMP_MS: u64 = 120;


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

    let confirming = !state.gate_exists && state.gate_pin_draft.is_some();
    let title = if state.gate_exists {
        "welcome back"
    } else if confirming {
        "confirm"
    } else {
        "choose a pin"
    };

  
    let since_error = state
        .gate_error_at
        .map(|t| t.elapsed().as_secs_f32())
        .filter(|e| *e < ERROR_SECS);
    let erroring = since_error.is_some();

    let typed = state.gate_pin_input.chars().count().min(PIN_DIGITS);
    let clock = bloom::clock_secs();

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
