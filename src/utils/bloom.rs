
use std::sync::OnceLock;
use std::time::Instant;

use iced::widget::canvas::{Frame, LineCap, Path, Stroke};
use iced::{Color, Point};

const LAYERS: usize = 30;
const STEPS: usize = 200;
const N_HARM: f32 = 6.0;
const M_HARM: f32 = 4.0;
const AMP: f32 = 0.17;
const AMP_2: f32 = 0.53;
pub const LOOP_SECS: f32 = 16.0;
const WIDTH: f32 = 0.9;
const R0_FACTOR: f32 = 0.05;
const SPREAD_FACTOR: f32 = 0.30;
const BASE_FACTOR: f32 = 1.15;

fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

pub fn clock_secs() -> f32 {
    start().elapsed().as_secs_f32()
}

fn lerp(a: Color, b: Color, t: f32, alpha: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: alpha,
    }
}

pub fn draw(
    frame: &mut Frame,
    center: Point,
    size: f32,
    t: f32,
    edge_in: Color,
    edge_out: Color,
    alpha: f32,
) {
    draw_cleared(frame, center, size, t, edge_in, edge_out, alpha, None);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_cleared(
    frame: &mut Frame,
    center: Point,
    size: f32,
    t: f32,
    edge_in: Color,
    edge_out: Color,
    alpha: f32,
    clear: Option<(f32, f32)>,
) {
    let theta = t / LOOP_SECS * std::f32::consts::TAU;
    let breathe = 1.0 + 0.13 * theta.sin();
    let base = size * BASE_FACTOR;
    let r0 = base * R0_FACTOR;
    let spread = base * SPREAD_FACTOR;

    for l in 0..LAYERS {
        let f = l as f32 / (LAYERS - 1) as f32;
        let rb = r0 + spread * f;
        let amp_a = rb * AMP * breathe;
        let amp_b = rb * AMP * AMP_2;
        let phase = theta + f * std::f32::consts::PI * 1.6;

        let path = Path::new(|b| {
            let mut pen = false;
            for s in 0..=STEPS {
                let a = s as f32 / STEPS as f32 * std::f32::consts::TAU;
                let r = rb + amp_a * (N_HARM * a + phase).cos() + amp_b * (M_HARM * a - phase).cos();
                let (dx, dy) = (r * (a + theta).cos(), r * (a + theta).sin());
                let inside = clear.is_some_and(|(rx, ry)| (dx / rx).powi(2) + (dy / ry).powi(2) < 1.0);
                if inside {
                    pen = false;
                    continue;
                }
                let p = Point::new(center.x + dx, center.y + dy);
                if pen { b.line_to(p) } else { b.move_to(p) }
                pen = true;
            }
            if clear.is_none() {
                b.close();
            }
        });

        frame.stroke(
            &path,
            Stroke {
                style: iced::widget::canvas::stroke::Style::Solid(lerp(edge_in, edge_out, f, alpha)),
                width: WIDTH,
                line_cap: LineCap::Round,
                ..Default::default()
            },
        );
    }
}
