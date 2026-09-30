//! The `mempool` pane (handoff `btc-panes`, 2026-09-10): what is queued at
//! each price, and where the next block's cut falls. A read-only
//! instrument — a histogram, an axis, three rows — plus a hover readout,
//! because a bar with no number under the pointer is a shape, not a fact
//! (user, 2026-09-10).
//!
//! **One ink for every bar** (`dim`); the cut is a dashed `faint` line,
//! never a colour. **The bands are indexd's, edges derived at walk time**
//! — log-spaced from the relay floor to the rate at its top read — because
//! fixed integer bands (`1 … 20+ sat/vB`, as the mock drew them) put a
//! whole 0.1–0.35 sat/vB mempool in the first bar. The axis therefore
//! reads the live edges: the floor at the left, `top+ sat/vB` at the right.
//!
//! **The cut is drawn where the `high` tier sits** (the fee ladder's
//! next-block read, 0.5 MB into the queue) rather than at the 1 MB edge the
//! handoff described, so the fees pane and this chart give one next-block
//! number. Its position on the log axis is interpolated between band edges,
//! not snapped to a band — and the fee ladder's bars are scaled on this
//! same axis ([`axis_position`]), so a rung's bar is where that rung sits
//! in the queue.
//!
//! **Hover** lifts the bar under the pointer to `text` and writes its band
//! and size at the top of the chart — `0.234–0.311 sat/vB · 2.67 vMB`.
//! Drawn inside the canvas on the canvas's own state: no message, no
//! controller round trip per mouse move.
//!
//! The three rows agree with the chart by construction: `pending` is the
//! vsize the bands sum to, `transactions` is the same walk's count,
//! `blocks to clear` is `ceil(pending / 1 MvB)`. Before the first frame the
//! chart is empty and every row reads `—`.

use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame, LineDash, Path, Stroke};
use iced::widget::{column, container, row, text, Space};
use iced::{Color, Element, Length, Padding, Pixels, Point, Rectangle, Size};
use iced::alignment::{Horizontal, Vertical};

use crate::channel::{BtcBand, BtcNodeStats as NodeFrame, BANDS};
use crate::controller::app_state::AppState;
use crate::controller::message::Message;
use crate::ui::components::grid::{self, drow, NA};
use crate::ui::managebtc::node::trim_rate;
use crate::utils::fonts::{self, MONO};
use crate::utils::theme::CompactPalette;
use crate::utils::add_commas;

/// The handoff's chart geometry: `padding 2px 0 8px`, bars `gap 3`, radius
/// 1; the axis row mono 8 `faint`, `padding-bottom 9`.
const CHART_PAD_TOP: f32 = 2.0;
const CHART_PAD_BOTTOM: f32 = 8.0;
const BAR_GAP: f32 = 3.0;
const BAR_RADIUS: f32 = 1.0;
pub(crate) const AXIS: f32 = 8.0;
const AXIS_PAD_BOTTOM: f32 = 9.0;
/// The hover readout (mono) at the top-left of a chart.
pub(crate) const READOUT: f32 = 9.0;
/// The threshold label's padding, filled with the window background.
pub(crate) const LABEL_PAD: f32 = 4.0;
/// JetBrains Mono's advance, for sizing a label's backing.
pub(crate) const MONO_ADVANCE: f32 = 0.6;
/// One block of vsize — what `blocks to clear` divides by.
const BLOCK_VSIZE: f64 = 1_000_000.0;
/// The least the pane fills before it scrolls: a legible histogram, the
/// axis, the three rows.
const MIN_H: f32 = 170.0;

pub fn view<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    grid::fill_or_scroll(MIN_H, move || body(state, cp, scale), cp, scale)
}

fn body<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let _ = state;
    let node = NodeFrame::current();
    let bands = node.bands;

    let chart: Element<'a, Message> = match bands {
        Some(b) => Canvas::new(Histogram {
            bands: b,
            cut: node.tiers.and_then(|t| cut_x(&b, t[3])),
            ink: cp.dim,
            hot: cp.text,
            faint: cp.faint,
            bg: cp.window,
            scale,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
        None => Space::new().width(Length::Fill).height(Length::Fill).into(),
    };

    let (left, right) = match bands {
        Some(b) => (trim_rate(b[0].from), format!("{}+ sat/vB", trim_rate(b[BANDS - 1].from))),
        None => (String::new(), String::new()),
    };

    let pending = node.pending_vsize.map(|v| v as f64 / BLOCK_VSIZE);
    let pending_row = match pending {
        Some(p) => vec![(format!("{p:.1} vMB"), cp.text)],
        None => vec![(NA.to_string(), cp.faint)],
    };
    let count_row = match node.mempool_txs {
        Some(n) => vec![(add_commas(n as i64), cp.text)],
        None => vec![(NA.to_string(), cp.faint)],
    };
    let clear_row = match pending {
        Some(p) => vec![(format!("~{}", blocks_to_clear(p)), cp.dim)],
        None => vec![(NA.to_string(), cp.faint)],
    };

    column![
        container(chart)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::new(0.0).top(CHART_PAD_TOP * scale).bottom(CHART_PAD_BOTTOM * scale)),
        axis_row(left, right, cp, scale),
        drow("pending", pending_row, 0.0, cp, scale),
        drow("transactions", count_row, 3.0, cp, scale),
        drow("blocks to clear", clear_row, 3.0, cp, scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// The axis under a chart: two mono 8 upper `faint` labels at the ends.
pub(crate) fn axis_row<'a>(left: String, right: String, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(
        row![
            text(left.to_uppercase()).font(MONO).size(AXIS * scale).color(cp.faint),
            Space::new().width(Length::Fill),
            text(right.to_uppercase()).font(MONO).size(AXIS * scale).color(cp.faint),
        ]
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).bottom(AXIS_PAD_BOTTOM * scale))
    .into()
}

/// `ceil(pending / 1 MvB)`, never under one while anything is queued.
fn blocks_to_clear(pending_vmb: f64) -> u64 {
    if pending_vmb <= 0.0 {
        0
    } else {
        (pending_vmb.ceil() as u64).max(1)
    }
}

/// Where a rate sits on the histogram's log axis, 0 at the floor and 1 at
/// the open band's edge, clamped. `None` when the axis has no span. The
/// fee ladder's bars are this too, so a rung and the cut line agree.
pub(crate) fn axis_position(bands: &[BtcBand; BANDS], rate: f32) -> Option<f32> {
    let left = bands[0].from;
    let top = bands[BANDS - 1].from;
    if !(left > 0.0) || !(top > left) {
        return None;
    }
    if rate <= left {
        return Some(0.0);
    }
    if rate >= top {
        return Some(1.0);
    }
    Some((rate / left).ln() / (top / left).ln())
}

/// [`axis_position`] as a fraction of the chart's width: the closed bands
/// take thirteen fourteenths, the open band the last.
fn cut_x(bands: &[BtcBand; BANDS], rate: f32) -> Option<f32> {
    axis_position(bands, rate).map(|p| p * (BANDS - 1) as f32 / BANDS as f32)
}

// ── Hover ───────────────────────────────────────────────────────────────────

/// Where the pointer is over a bar chart, kept by the canvas itself.
#[derive(Default, Clone)]
pub(crate) struct BarHover {
    pub cursor: Option<Point>,
}

/// The one event rule both bar charts share: track the pointer while it is
/// over the chart, forget it when it leaves, redraw on either change.
pub(crate) fn track<Message>(state: &mut BarHover, event: &canvas::Event, bounds: Rectangle) -> Option<canvas::Action<Message>> {
    match event {
        canvas::Event::Mouse(mouse::Event::CursorMoved { position }) => {
            let next = bounds.contains(*position).then(|| Point::new(position.x - bounds.x, position.y - bounds.y));
            if state.cursor != next {
                state.cursor = next;
                return Some(canvas::Action::request_redraw());
            }
            None
        }
        canvas::Event::Mouse(mouse::Event::CursorLeft) if state.cursor.is_some() => {
            state.cursor = None;
            Some(canvas::Action::request_redraw())
        }
        _ => None,
    }
}

/// The slot under `x` when `n` slots share `w` with `gap` between them —
/// a pointer in a gap belongs to no bar.
pub(crate) fn slot_at(x: f32, w: f32, n: usize, gap: f32) -> Option<usize> {
    if n == 0 || x < 0.0 || x > w {
        return None;
    }
    let pitch = (w + gap) / n as f32;
    let i = (x / pitch).floor() as usize;
    let within = x - i as f32 * pitch;
    (i < n && within <= pitch - gap).then_some(i)
}

/// The hover readout: mono 9 `text` at the chart's top-left on a `window`
/// backing, so it reads over a tall bar.
pub(crate) fn readout(frame: &mut Frame, word: &str, ink: Color, bg: Color, scale: f32) {
    let size = READOUT * scale;
    let pad = LABEL_PAD * scale;
    let tw = word.chars().count() as f32 * size * MONO_ADVANCE;
    frame.fill_rectangle(Point::new(0.0, 0.0), Size::new(tw + 2.0 * pad, size + 3.0), bg);
    frame.fill_text(canvas::Text {
        content: word.to_string(),
        position: Point::new(pad, 1.0),
        color: ink,
        size: Pixels(size),
        font: fonts::MONO,
        align_x: Horizontal::Left.into(),
        align_y: Vertical::Top.into(),
        ..canvas::Text::default()
    });
}

/// What the readout says for band `i`: its edges and what is queued in it.
fn band_word(bands: &[BtcBand; BANDS], i: usize) -> String {
    let span = if i + 1 < BANDS {
        format!("{}\u{2013}{} sat/vB", trim_rate(bands[i].from), trim_rate(bands[i + 1].from))
    } else {
        format!("{}+ sat/vB", trim_rate(bands[i].from))
    };
    format!("{span} \u{b7} {:.2} vMB", bands[i].vsize as f64 / BLOCK_VSIZE)
}

// ── The chart ───────────────────────────────────────────────────────────────

/// Fourteen bars, one ink, bottom-aligned, the tallest at the full height;
/// the cut as a dashed line from the top to the baseline with its label at
/// the top, to its right — or to its left when the right would run off.
/// The hovered bar in `hot`, its readout over everything.
struct Histogram {
    bands: [BtcBand; BANDS],
    /// Fraction of the width, from the left.
    cut: Option<f32>,
    ink: Color,
    hot: Color,
    faint: Color,
    bg: Color,
    scale: f32,
}

impl<Message> canvas::Program<Message> for Histogram {
    type State = BarHover;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        track(state, event, bounds)
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let s = self.scale;
        let w = bounds.width;
        let h = bounds.height;
        let mut frame = Frame::new(renderer, bounds.size());
        let n = BANDS;
        let gap = BAR_GAP * s;
        let slot = (w - gap * (n as f32 - 1.0)).max(0.0) / n as f32;
        let max = self.bands.iter().map(|b| b.vsize).max().unwrap_or(0).max(1) as f32;
        let hovered = state.cursor.and_then(|c| slot_at(c.x, w, n, gap));

        for (i, b) in self.bands.iter().enumerate() {
            if b.vsize == 0 {
                continue;
            }
            let bh = (h * b.vsize as f32 / max).max(1.0).round();
            let x = (i as f32 * (slot + gap)).round();
            let bar = Path::new(|p| {
                p.rounded_rectangle(
                    Point::new(x, h - bh),
                    Size::new(slot.round().max(1.0), bh),
                    iced::border::Radius::from(BAR_RADIUS * s),
                );
            });
            frame.fill(&bar, if hovered == Some(i) { self.hot } else { self.ink });
        }

        if let Some(f) = self.cut {
            let x = (w * f).round() + 0.5;
            let dash = [3.0 * s, 2.0 * s];
            frame.stroke(
                &Path::line(Point::new(x, 0.0), Point::new(x, h)),
                Stroke {
                    line_dash: LineDash { segments: &dash, offset: 0 },
                    ..Stroke::default().with_color(self.faint).with_width(1.0)
                },
            );
            let size = AXIS * s;
            let pad = LABEL_PAD * s;
            let word_w = |word: &str| word.chars().count() as f32 * size * MONO_ADVANCE;
            let (word, right_side) = if x + pad + word_w("NEXT BLOCK \u{203a}") + pad <= w {
                ("NEXT BLOCK \u{203a}", true)
            } else {
                ("\u{2039} NEXT BLOCK", false)
            };
            let tw = word_w(word);
            let box_w = tw + 2.0 * pad;
            let box_x = if right_side { x - 0.5 } else { x + 0.5 - box_w };
            frame.fill_rectangle(Point::new(box_x, 0.0), Size::new(box_w, size + 2.0), self.bg);
            frame.fill_text(canvas::Text {
                content: word.to_string(),
                position: Point::new(if right_side { x + pad } else { x - pad }, 1.0),
                color: self.faint,
                size: Pixels(size),
                font: fonts::MONO,
                align_x: if right_side { Horizontal::Left.into() } else { Horizontal::Right.into() },
                align_y: Vertical::Top.into(),
                ..canvas::Text::default()
            });
        }

        if let Some(i) = hovered {
            readout(&mut frame, &band_word(&self.bands, i), self.hot, self.bg, s);
        }

        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bands(left: f32, top: f32) -> [BtcBand; BANDS] {
        let ratio = top / left;
        std::array::from_fn(|i| BtcBand { from: left * ratio.powf(i as f32 / (BANDS - 1) as f32), vsize: 1 })
    }

    /// A rate sits on the log axis: at the floor it is 0, at the top edge 1,
    /// one octave up a four-octave axis a quarter; the cut line scales that
    /// to the closed bands' thirteen fourteenths.
    #[test]
    fn rates_sit_on_the_log_axis() {
        let b = bands(0.1, 1.6); // four octaves
        assert_eq!(axis_position(&b, 0.05), Some(0.0), "under the floor: the left edge");
        assert_eq!(axis_position(&b, 0.1), Some(0.0));
        assert!((axis_position(&b, 0.2).unwrap() - 0.25).abs() < 1e-5);
        assert_eq!(axis_position(&b, 1.6), Some(1.0));
        assert_eq!(axis_position(&b, 40.0), Some(1.0), "above the top: the open band");
        let closed = 13.0 / 14.0;
        assert!((cut_x(&b, 0.2).unwrap() - closed / 4.0).abs() < 1e-5);
        assert!((cut_x(&b, 1.6).unwrap() - closed).abs() < 1e-6);
        // No span, no line and no bar.
        let flat: [BtcBand; BANDS] = std::array::from_fn(|_| BtcBand { from: 0.1, vsize: 0 });
        assert_eq!(axis_position(&flat, 0.3), None);
        assert_eq!(cut_x(&flat, 0.3), None);
    }

    /// `pending` and `blocks to clear` agree: 3.9 vMB is `~4`, a sliver is
    /// still one block, nothing is none.
    #[test]
    fn blocks_to_clear_is_the_ceiling_of_pending() {
        assert_eq!(blocks_to_clear(3.9), 4);
        assert_eq!(blocks_to_clear(11.7), 12);
        assert_eq!(blocks_to_clear(0.02), 1);
        assert_eq!(blocks_to_clear(0.0), 0);
    }

    /// The pointer maps to the bar under it and to nothing in a gap or off
    /// the chart; the readout names the band's edges, the last one open.
    #[test]
    fn hover_finds_the_bar_and_names_its_band() {
        // 14 slots of 10 with 3 gaps: width 14×10 + 13×3 = 179.
        assert_eq!(slot_at(0.0, 179.0, 14, 3.0), Some(0));
        assert_eq!(slot_at(9.9, 179.0, 14, 3.0), Some(0));
        assert_eq!(slot_at(11.0, 179.0, 14, 3.0), None, "a gap");
        assert_eq!(slot_at(13.0, 179.0, 14, 3.0), Some(1));
        assert_eq!(slot_at(178.0, 179.0, 14, 3.0), Some(13));
        assert_eq!(slot_at(-1.0, 179.0, 14, 3.0), None);
        assert_eq!(slot_at(180.0, 179.0, 14, 3.0), None);
        let mut b = bands(0.1, 1.6);
        b[0].vsize = 4_277_889;
        b[13].vsize = 50_160;
        assert_eq!(band_word(&b, 0), "0.1\u{2013}0.12 sat/vB \u{b7} 4.28 vMB");
        assert_eq!(band_word(&b, 13), "1.6+ sat/vB \u{b7} 0.05 vMB");
    }
}
