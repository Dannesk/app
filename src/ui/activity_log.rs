
use iced::Widget as _;
use iced::widget::canvas::{self as cnvs};
use iced::widget::{Column, Row, Space, button, canvas, column, container, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Point, Radians, Rectangle, Shadow};
use iced::mouse;
use std::time::Instant;
use crate::channel::{ActivityLogState, ActivityStepState};
use crate::controller::message::Message;
use crate::utils::bloom;
use crate::utils::fonts::MONO;
use crate::utils::theme::{self, CompactPalette};

const BLOCK_W: f32 = 380.0;
const PAD_H: f32 = 16.0;


const REASSURE_SECS: f32 = 10.0;
const SPIN_SECS: f32 = 0.85;

const FILL_EASE_SECS: f32 = 0.5;

const TITLE: f32 = 13.0;
const COUNTER: f32 = 8.5;
const STEP_LABEL: f32 = 11.5;
const STEP_ELAPSED: f32 = 11.0;
const STEP_GLYPH: f32 = 9.0;
const NOTE: f32 = 10.0;
const BUTTON: f32 = 11.5;
const GLYPH_W: f32 = 10.0;
const SPINNER: f32 = 8.0;

fn ms_label(ms: u64) -> String {
    if ms == 0 {
        "<1 ms".to_string()
    } else if ms < 1000 {
        format!("{} ms", ms)
    } else {
        format!("{:.1} s", ms as f64 / 1000.0)
    }
}


fn fill_fraction(log: &ActivityLogState, tick: Instant) -> f32 {
    let total = log.steps.len().max(1) as f32;
    let done = log.steps.iter().filter(|s| matches!(s.state, ActivityStepState::Ok { .. })).count();
    if done == 0 {
        return 0.0;
    }
    let last_at = log
        .steps
        .iter()
        .filter_map(|s| match s.state {
            ActivityStepState::Ok { at, .. } => Some(at),
            _ => None,
        })
        .max();
    let eased = match last_at {
        Some(at) => {
            let t = (tick.saturating_duration_since(at).as_secs_f32() / FILL_EASE_SECS).clamp(0.0, 1.0);
            1.0 - (1.0 - t).powi(3)
        }
        None => 1.0,
    };
    ((done - 1) as f32 + eased) / total
}

pub fn render_activity_log<'a>(
    log: &'a ActivityLogState,
    tick: Instant,
    iced_theme: &iced::Theme,
    scale: f32,
) -> Element<'a, Message> {
    let cp = theme::compact(iced_theme);


    let link_down = !*crate::channel::CHANNEL.link_rx(log.health_key).borrow();

    let done = log.steps.iter().filter(|s| matches!(s.state, ActivityStepState::Ok { .. })).count();
    let total = log.steps.len();
    let failed = log.any_error();
    let terminal = log.is_terminal();


    let counter: Element<'_, Message> = if failed {
        counter_runs(vec![("failed \u{00b7} ", cp.muted), (&format!("{done} / {total}"), cp.dim)], scale)
    } else if terminal {
        let elapsed: u64 = log
            .steps
            .iter()
            .filter_map(|s| match s.state {
                ActivityStepState::Ok { ms, .. } => Some(ms),
                _ => None,
            })
            .sum();
        counter_runs(vec![("done \u{00b7} ", cp.muted), (&ms_label(elapsed), cp.dim)], scale)
    } else {
        counter_runs(vec![(&done.to_string(), cp.dim), (&format!(" / {total}"), cp.muted)], scale)
    };

    let header = container(
        Row::with_children(vec![
            text(log.title).size(TITLE * scale).color(cp.text).boxed(),
            Space::new().width(Length::Fill).boxed(),
            counter,
        ])
        .align_y(Alignment::End),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).left(PAD_H * scale).right(PAD_H * scale).bottom(8.0 * scale));

    let frac = fill_fraction(log, tick);
    let fill_colour = if failed { cp.red } else { cp.dim };
    let track = container(
        container(Space::new())
            .width(Length::Fixed((BLOCK_W * scale * frac).max(0.0)))
            .height(Length::Fixed(1.0))
            .style(move |_| container::Style {
                background: Some(fill_colour.into()),
                ..Default::default()
            }),
    )
    .width(Length::Fill)
    .height(Length::Fixed(1.0))
    .style(move |_| container::Style { background: Some(cp.rule.into()), ..Default::default() });


    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    for step in log.steps.iter() {
        let (glyph, label_colour, elapsed_colour): (Element<'_, Message>, Color, Color) = match &step.state {
            ActivityStepState::Pending => (
                text("\u{00b7}").font(MONO).size(STEP_GLYPH * scale).color(cp.faint).boxed(),
                cp.faint,
                cp.muted,
            ),
            ActivityStepState::Active { .. } => (
                canvas(Spinner { track: cp.border, arc: cp.dim })
                    .width(Length::Fixed(SPINNER * scale))
                    .height(Length::Fixed(SPINNER * scale))
                    .boxed(),
                cp.text,
                cp.muted,
            ),
        
            ActivityStepState::Ok { .. } => (
                text("\u{2713}").size(STEP_GLYPH * scale).color(cp.green).boxed(),
                cp.text,
                cp.muted,
            ),
          
            ActivityStepState::Error { .. } | ActivityStepState::Stalled { .. } => (
                text("\u{2715}").font(MONO).size(STEP_GLYPH * scale).color(cp.red).boxed(),
                cp.red,
                cp.red,
            ),
        };

  
        let elapsed = match &step.state {
            ActivityStepState::Ok { ms, .. } => ms_label(*ms),
            ActivityStepState::Error { .. } => String::new(),
            _ => String::new(),
        };

        let mut body: Column<Element<'_, Message>> = column![Row::with_children(vec![
            text(step.label).size(STEP_LABEL * scale).color(label_colour).boxed(),
            Space::new().width(Length::Fill).boxed(),
            text(elapsed).font(MONO).size(STEP_ELAPSED * scale).color(elapsed_colour).boxed(),
        ])
        .align_y(Alignment::End)];

    
        if let ActivityStepState::Active { since } = &step.state {
            let (line, colour) = if link_down {
                (Some("Network dropped \u{2014} retrying connection\u{2026}"), cp.amber)
            } else if tick.saturating_duration_since(*since).as_secs_f32() >= REASSURE_SECS {
                (Some("Still waiting for the network\u{2026}"), cp.muted)
            } else {
                (None, cp.muted)
            };
            if let Some(line) = line {
                body = body
                    .push(Space::new().height(3.0 * scale).boxed())
                    .push(text(line).size(NOTE * scale).color(colour).boxed());
            }
        }

        rows.push(
            container(
                Row::with_children(vec![
                    container(glyph)
                        .width(Length::Fixed(GLYPH_W * scale))
                        .align_x(Alignment::Center)
                        .boxed(),
                    Space::new().width(10.0 * scale).boxed(),
                    body.width(Length::Fill).boxed(),
                ])
                .align_y(Alignment::End),
            )
            .width(Length::Fill)
            .padding(Padding::new(0.0).top(5.5 * scale).bottom(5.5 * scale))
            .boxed(),
        );
    }

    let steps = container(column(rows))
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(9.0 * scale).left(PAD_H * scale).right(PAD_H * scale));


    let failure_message = log.steps.iter().find_map(|s| match &s.state {
        ActivityStepState::Error { message } | ActivityStepState::Stalled { message } => {
            Some(message.as_str())
        }
        _ => None,
    });

    let note: Element<'_, Message> = match (failure_message, log.note.as_deref()) {
        (Some(message), _) => note_block(Some(cp.red), message, cp, scale),
        (None, Some(outcome)) => note_block(None, outcome, cp, scale),
        (None, None) => Space::new().boxed(),
    };

  
    let footer = container(
        button(
            text("Continue")
                .size(BUTTON * scale)
                .color(if terminal { cp.text } else { cp.faint }),
        )
        .padding(Padding::new(0.0).top(5.0 * scale).bottom(5.0 * scale).left(18.0 * scale).right(18.0 * scale))
        .on_press_maybe(terminal.then_some(Message::ActivityDismiss))
        .style(move |_, status| {
            let hot = terminal && matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(Background::Color(match (terminal, hot) {
                    (false, _) => Color::TRANSPARENT,
                    (true, false) => cp.neutral,
                    (true, true) => cp.hover,
                })),
                text_color: if terminal { cp.text } else { cp.faint },
                border: Border {
                    color: if terminal { cp.border } else { cp.border_soft },
                    width: 1.0,
                    radius: (6.0 * scale).into(),
                },
                shadow: Shadow::default(),
                snap: false,
            }
        }),
    )
    .width(Length::Fill)
    .align_x(Alignment::End)
    .padding(Padding::new(0.0).top(13.0 * scale).left(PAD_H * scale).right(PAD_H * scale));

    container(
        column![header, track, steps, note, footer].width(Length::Fixed(BLOCK_W * scale)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container::Style { background: Some(cp.window.into()), ..Default::default() })
    .center(Length::Fill)
    .boxed()
}


fn counter_runs<'a>(runs: Vec<(&str, Color)>, scale: f32) -> Element<'a, Message> {
    Row::with_children(
        runs.into_iter()
            .map(|(s, c)| {
                text(s.to_uppercase())
                    .font(MONO)
                    .size(COUNTER * scale)
                    .color(c)
                    .boxed()
            })
            .collect::<Vec<_>>(),
    )
    .align_y(Alignment::End)
    .boxed()
}


fn note_block<'a>(
    marker: Option<Color>,
    body: &'a str,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let mut r: Row<Element<'_, Message>> = Row::new().align_y(Alignment::Start);
    if let Some(colour) = marker {
        r = r
            .push(text("!").font(MONO).size(NOTE * scale).color(colour).boxed())
            .push(Space::new().width(6.0 * scale).boxed());
    }
    r = r.push(
        text(body)
            .size(NOTE * scale)
            .line_height(iced::widget::text::LineHeight::Relative(1.5))
            .color(cp.muted).boxed(),
    );

    container(
        column![
            crate::ui::components::compact::hairline(cp.rule),
            Space::new().height(9.0 * scale),
            r,
        ]
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(10.0 * scale).left(PAD_H * scale).right(PAD_H * scale))
    .boxed()
}



struct Spinner {
    track: Color,
    arc: Color,
}

impl cnvs::Program<Message> for Spinner {
    type State = ();
    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<cnvs::Geometry<iced::Renderer>> {
        let mut frame = cnvs::Frame::new(renderer, bounds.size());
        let c = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let r = (bounds.width.min(bounds.height) / 2.0 - 0.5).max(0.5);
        let stroke = |color: Color| cnvs::Stroke {
            style: cnvs::stroke::Style::Solid(color),
            width: 1.0,
            line_cap: cnvs::stroke::LineCap::Round,
            line_join: cnvs::stroke::LineJoin::Round,
            line_dash: cnvs::stroke::LineDash::default(),
        };

        frame.stroke(&cnvs::Path::circle(c, r), stroke(self.track));

        let a0 = bloom::clock_secs() / SPIN_SECS * std::f32::consts::TAU;
        let arc = cnvs::Path::new(|b| {
            b.arc(cnvs::path::Arc {
                center: c,
                radius: r,
                start_angle: Radians(a0),
                end_angle: Radians(a0 + std::f32::consts::FRAC_PI_2),
            });
        });
        frame.stroke(&arc, stroke(self.arc));

        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::ActivityStep;
    use std::time::Duration;

    fn log_with(states: Vec<ActivityStepState>) -> ActivityLogState {
        ActivityLogState {
            title: "Import XRP wallet",
            health_key: None,
            steps: states
                .into_iter()
                .enumerate()
                .map(|(i, state)| ActivityStep {
                    id: "s",
                    label: ["Deriving keys", "Subscribing", "Awaiting response", "Import successful"][i],
                    state,
                })
                .collect(),
            note: None,
        }
    }

    #[test]
    fn an_untouched_run_shows_no_fill() {
        let log = log_with(vec![ActivityStepState::Pending; 4]);
        assert_eq!(fill_fraction(&log, Instant::now()), 0.0);
    }

    #[test]
    fn a_settled_bar_is_done_over_total() {
        let then = Instant::now() - Duration::from_secs(5);
        let log = log_with(vec![
            ActivityStepState::Ok { ms: 2200, at: then },
            ActivityStepState::Ok { ms: 0, at: then },
            ActivityStepState::Active { since: then },
            ActivityStepState::Pending,
        ]);
        assert!((fill_fraction(&log, Instant::now()) - 0.5).abs() < 1e-6);
    }

  
    #[test]
    fn the_fill_eases_from_the_previous_step() {
        let now = Instant::now();
        let log = log_with(vec![
            ActivityStepState::Ok { ms: 2200, at: now - Duration::from_secs(5) },
            ActivityStepState::Ok { ms: 0, at: now },
            ActivityStepState::Pending,
            ActivityStepState::Pending,
        ]);
        let f = fill_fraction(&log, now);
        assert!((f - 0.25).abs() < 1e-6, "the bar jumped instead of easing: {f}");
    }

    #[test]
    fn a_finished_run_fills_completely() {
        let then = Instant::now() - Duration::from_secs(5);
        let log = log_with(vec![ActivityStepState::Ok { ms: 10, at: then }; 4]);
        assert!((fill_fraction(&log, Instant::now()) - 1.0).abs() < 1e-6);
    }
}
