
use iced::Widget as _;
use iced::widget::canvas::{self as cnvs};
use iced::widget::{Row, Space, canvas, column, container, stack, text};
use iced::{Alignment, Color, Element, Length, Padding, Point, Radians, Rectangle};
use iced::{mouse, window};
use std::time::Instant;
use crate::channel::{ActivityLogState, ActivityStepState, Socket, CHANNEL};
use crate::controller::message::Message;
use crate::ui::components::compact;
use crate::ui::components::orbit::{self, Chain, Status};
use crate::utils::bloom;
use crate::utils::fonts::{LIGHT, MONO};
use crate::utils::theme::{self, CompactPalette};

const BLOCK_W: f32 = 380.0;
const PAD_H: f32 = 16.0;

const SPIN_SECS: f32 = 0.85;

const FILL_EASE_SECS: f32 = 0.5;

const RING_D: f32 = 220.0;
const RING_W: f32 = 1.7;
const RING_GAP: f32 = 14.0;

const TITLE: f32 = 16.5;
const STEP_LABEL: f32 = 11.5;
const STEP_ELAPSED: f32 = 11.0;
const STEP_GLYPH: f32 = 9.0;
const NOTE: f32 = 10.0;
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


fn eased(tick: Instant, at: Instant) -> f32 {
    let t = (tick.saturating_duration_since(at).as_secs_f32() / FILL_EASE_SECS).clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn arcs(log: &ActivityLogState, tick: Instant) -> Vec<(f32, f32)> {
    let total = log.steps.len();
    let newest = log
        .steps
        .iter()
        .filter_map(|s| match s.state {
            ActivityStepState::Ok { at, .. } => Some(at),
            _ => None,
        })
        .max();
    log.steps
        .iter()
        .enumerate()
        .filter_map(|(i, step)| {
            let (start, full) = orbit::segment(i, total);
            let head = orbit::SEG_HEAD.min(full);
            let sweep = match step.state {
                ActivityStepState::Pending => return None,
                ActivityStepState::Active { .. } => head,
                ActivityStepState::Ok { at, .. } if Some(at) == newest => head + (full - head) * eased(tick, at),
                _ => full,
            };
            Some((start, sweep))
        })
        .collect()
}

fn link_words(key: Option<&str>) -> Status {
    let chain = match key.and_then(|k| k.split(':').next()) {
        Some("relay") => Some(Chain::Xrp),
        Some("btc") => Some(Chain::Btc),
        _ => None,
    };
    match chain {
        Some(chain) if *CHANNEL.proxy_ws_status_rx.borrow() => Status::Offline(Some(chain)),
        _ if matches!(orbit::socket(), Socket::Connecting { .. }) => Status::Connecting,
        _ => Status::Reconnecting,
    }
}

pub fn render_activity_log<'a>(
    log: &'a ActivityLogState,
    tick: Instant,
    iced_theme: &iced::Theme,
    scale: f32,
) -> Element<'a, Message> {
    let cp = theme::compact(iced_theme);

    let failed = log.any_error();
    let terminal = log.is_terminal();
    let down = !terminal && !*CHANNEL.link_rx(log.health_key).borrow();

    let (middle, ink) = if down {
        let status = link_words(log.health_key);
        (status.words(), if status.strong() { cp.text } else { cp.dim })
    } else {
        (log.title, cp.text)
    };
    let lit = if failed { cp.red } else { cp.green };
    let d = Length::Fixed(RING_D * scale);
    let ring = container(stack![
        canvas(Ring {
            arcs: arcs(log, tick),
            lit: Color { a: 0.85, ..lit },
            dim: Color { a: 0.85, ..cp.dim },
            track: Color { a: 0.32, ..cp.faint },
            down,
            scale,
        })
        .width(d)
        .height(d),
        container(text(middle).font(LIGHT).size(TITLE * scale).color(ink)).center(d),
    ])
    .center_x(Length::Fill);


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
            _ => String::new(),
        };

        rows.push(
            container(
                Row::with_children(vec![
                    container(glyph)
                        .width(Length::Fixed(GLYPH_W * scale))
                        .align_x(Alignment::Center)
                        .boxed(),
                    Space::new().width(10.0 * scale).boxed(),
                    Row::with_children(vec![
                        text(step.label).size(STEP_LABEL * scale).color(label_colour).boxed(),
                        Space::new().width(Length::Fill).boxed(),
                        text(elapsed).font(MONO).size(STEP_ELAPSED * scale).color(elapsed_colour).boxed(),
                    ])
                    .align_y(Alignment::End)
                    .width(Length::Fill)
                    .boxed(),
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


    let footer = container(compact::cta("Continue", terminal, Message::ActivityDismiss, cp, scale))
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(13.0 * scale).left(PAD_H * scale).right(PAD_H * scale));

    container(
        column![ring, Space::new().height(RING_GAP * scale), steps, note, footer]
            .width(Length::Fixed(BLOCK_W * scale)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container::Style { background: Some(cp.window.into()), ..Default::default() })
    .center(Length::Fill)
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
            compact::hairline(cp.rule),
            Space::new().height(9.0 * scale),
            r,
        ]
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(10.0 * scale).left(PAD_H * scale).right(PAD_H * scale))
    .boxed()
}


struct Ring {
    arcs: Vec<(f32, f32)>,
    lit: Color,
    dim: Color,
    track: Color,
    down: bool,
    scale: f32,
}

#[derive(Default)]
struct Laps {
    run: Option<(Instant, u32)>,
    now: Option<Instant>,
}

impl cnvs::Program<Message> for Ring {
    type State = Laps;

    fn update(
        &self,
        state: &mut Laps,
        event: &cnvs::Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<cnvs::Action<Message>> {
        let cnvs::Event::Window(window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        state.now = Some(*now);
        if self.down && orbit::turn(state.run, *now).is_none() {
            state.run = Some((*now, 1));
        }
        orbit::turn(state.run, *now).is_some().then(cnvs::Action::request_redraw)
    }

    fn draw(
        &self,
        state: &Laps,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<cnvs::Geometry<iced::Renderer>> {
        let mut frame = cnvs::Frame::new(renderer, bounds.size());
        let c = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let r = bounds.width.min(bounds.height) / 2.0 - RING_W * self.scale;
        if r <= 0.0 {
            return vec![frame.into_geometry()];
        }
        orbit::track(&mut frame, c, r, 2.0 * self.scale, 8.0 * self.scale, self.track);
        let (colour, spin) = match orbit::turn(state.run, state.now.unwrap_or_else(Instant::now)) {
            Some(t) => (self.dim, std::f32::consts::TAU * t),
            None => (self.lit, 0.0),
        };
        for &(start, sweep) in &self.arcs {
            orbit::arc(&mut frame, c, r, start + spin, sweep, colour, (RING_W * self.scale).max(1.0));
        }
        vec![frame.into_geometry()]
    }
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

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn an_untouched_run_lights_nothing() {
        let log = log_with(vec![ActivityStepState::Pending; 4]);
        assert!(arcs(&log, Instant::now()).is_empty());
    }

    #[test]
    fn a_settled_step_fills_its_segment_and_the_active_one_shows_the_head() {
        let then = Instant::now() - Duration::from_secs(5);
        let log = log_with(vec![
            ActivityStepState::Ok { ms: 2200, at: then },
            ActivityStepState::Active { since: then },
            ActivityStepState::Pending,
            ActivityStepState::Pending,
        ]);
        let lit = arcs(&log, Instant::now());
        assert_eq!(lit.len(), 2);
        assert!(close(lit[0].1, orbit::segment(0, 4).1));
        assert!(close(lit[1].0, orbit::segment(1, 4).0) && close(lit[1].1, orbit::SEG_HEAD));
    }

    #[test]
    fn the_newest_segment_grows_from_the_head() {
        let now = Instant::now();
        let log = log_with(vec![
            ActivityStepState::Ok { ms: 2200, at: now - Duration::from_secs(5) },
            ActivityStepState::Ok { ms: 0, at: now },
            ActivityStepState::Pending,
            ActivityStepState::Pending,
        ]);
        let lit = arcs(&log, now);
        assert!(close(lit[0].1, orbit::segment(0, 4).1));
        assert!(close(lit[1].1, orbit::SEG_HEAD), "the segment jumped instead of growing: {}", lit[1].1);
    }

    #[test]
    fn a_failed_step_fills_its_segment() {
        let then = Instant::now() - Duration::from_secs(5);
        let log = log_with(vec![
            ActivityStepState::Ok { ms: 10, at: then },
            ActivityStepState::Stalled { message: "Connection lost".to_string() },
            ActivityStepState::Pending,
            ActivityStepState::Pending,
        ]);
        let lit = arcs(&log, Instant::now());
        assert_eq!(lit.len(), 2);
        assert!(close(lit[1].1, orbit::segment(1, 4).1));
    }

    #[test]
    fn a_finished_run_fills_every_segment() {
        let then = Instant::now() - Duration::from_secs(5);
        let log = log_with(vec![ActivityStepState::Ok { ms: 10, at: then }; 4]);
        let lit = arcs(&log, Instant::now());
        assert_eq!(lit.len(), 4);
        assert!(lit.iter().all(|&(_, sweep)| close(sweep, orbit::segment(0, 4).1)));
    }
}
