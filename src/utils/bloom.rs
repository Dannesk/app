//! The guilloché bloom — the woven security rosette behind the PIN gate.
//!
//! Nested closed curves whose radius is modulated by two cosine harmonics, the whole
//! figure slowly rotating and "breathing" on a 16s cycle. One `Path` per layer, 200
//! segments each, one stroke pass per frame.
//!
//! ## What this replaced
//!
//! `utils/brand_flow.rs` — the launch splash, deleted entire. The splash was a second
//! of theatre in front of the gate, and the gate is now the thing worth looking at, so
//! the mark moved to where someone is actually waiting. Gone with it: the locked
//! brand-blue palette (`#7AA9FF` core, aliceblue rim), the `SPLASH`/`BAND` presets, the
//! fade-in `smoothstep`, and the activity log's progress band.
//!
//! **No blue.** The mark now draws in the theme's own ink — `faint` at the centre
//! running out to `text` at the rim — so it belongs to whichever theme is on rather
//! than being a brand moment pasted over one.
//!
//! ## Freezing it is fine
//!
//! The animation wants a redraw pump (`controller/subscription.rs` runs one at 16ms
//! while the gate is up). Drop the pump and the bloom sits still at `t = 0`, which
//! still looks right — it is a figure, not a progress indicator, and it promises
//! nothing by moving.

use std::sync::OnceLock;
use std::time::Instant;

use iced::widget::canvas::{Frame, LineCap, Path, Stroke};
use iced::{Color, Point};

/// Nested closed curves. More = denser weave, linearly more work per frame.
const LAYERS: usize = 30;
/// Samples per curve. Below ~150 the curves visibly polygonise.
const STEPS: usize = 200;
/// Primary lobe count.
const N_HARM: f32 = 6.0;
/// Secondary lobe count.
const M_HARM: f32 = 4.0;
/// Radial amplitude, as a fraction of the layer radius.
const AMP: f32 = 0.17;
/// The secondary harmonic's share of it.
const AMP_2: f32 = 0.53;
/// One full rotation + breathe cycle. Seamless, so no consumer cares about phase.
pub const LOOP_SECS: f32 = 16.0;
/// Stroke width. Hairline by intent — this sits behind six digits.
const WIDTH: f32 = 0.9;
/// Innermost radius and radial spread, as fractions of `base`.
const R0_FACTOR: f32 = 0.05;
const SPREAD_FACTOR: f32 = 0.30;
/// `base` is a touch wider than the box, so the outermost curve reaches the edge
/// rather than floating inside a margin.
const BASE_FACTOR: f32 = 1.15;

/// Process-global start instant. Never resets within a run.
fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

/// Seconds since the process epoch.
///
/// Public because the activity log's spinner needs a monotonic clock and has no
/// business owning a second one. Both loop seamlessly, so nothing depends on the
/// phase at which a consumer starts drawing — only that it advances at one rate.
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

/// Draw the bloom into `frame`, centred on `center` and sized off `size`.
///
/// `edge_in` is the centre ink and `edge_out` the rim — `faint` → `text` in every
/// theme, so the weave reads as one figure gaining weight outward rather than as two
/// colours meeting. `alpha` is per-theme: light needs more of it, because dark ink on
/// a near-white ground loses weight at 0.9px in a way pale ink on dark does not.
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

/// [`draw`] with an optional **clear**: an ellipse of half-axes `(rx, ry)`
/// about the centre inside which no line is drawn — the pen lifts on the way
/// in and lands again on the way out. The Balance screen's digits sit in it
/// (2026-09-11). Done analytically, in the path, rather than by painting a
/// `window`-filled gradient over the lines afterwards: iced's canvas
/// gradients are linear only and band at this size, and at a 0.9px hairline
/// a hard edge is invisible anyway.
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
            // `pen` is down while the last point was drawable; a point inside
            // the clear lifts it, and the next point outside lands it again.
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
            // `s == STEPS` lands on the `s == 0` point, so an uncut curve is
            // already closed; `close()` on a cut one would chord the gap.
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
