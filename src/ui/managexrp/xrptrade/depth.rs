//! The **depth** view: cumulative depth either side of the mid, inside a
//! window that FITS THE BOOK ([`window_pct`], 2026-09-15): the half-width
//! is the farther of the two outermost shipped levels, capped at
//! ±[`CAP_PCT`] so junk priced at a tenth of fair cannot take the axis. One
//! rule for every pair — a deep book draws its fraction of a percent, a
//! dust book draws its one bid and one ask ten percent out — and the footer
//! prints the width it used. It used to be a fixed ±1%, the band the market
//! is classified by, which drew an empty chart for exactly the books where
//! seeing the book matters (any pair is pickable since 2026-09-15).
//!
//! Two step areas from the mid line outward: bids left, asks right, drawn on
//! a [`canvas`] the way the sparkline is. A Limit ticket adds a dashed marker
//! at the typed price with a `limit …` tag over it. The footer is the
//! cumulative XRP inside the window on each side. The sentence that used to
//! read the chart for the user went 2026-09-09: how to read a depth chart is
//! the landing-page docs' subject.

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, LineJoin, Path, Stroke};
use iced::{mouse, Color, Pixels, Point, Rectangle};

use crate::controller::message::Message;
use crate::utils::fonts::{self};
use crate::utils::orderbook::fmt_price;

use super::{ASK_WASH, BID_WASH, EYEBROW};

/// The most the window will open, percent of mid either side. Past this a
/// level is junk for the chart's purposes: it still counts on the book pane,
/// it just does not get to set the axis.
pub const CAP_PCT: f64 = 25.0;
/// The window when nothing fits — a one-sided or empty book — so the axis
/// still exists and the typed limit can still land on it.
const FLOOR_PCT: f64 = 1.0;

/// Half-width of the price window, percent of `mid`: the farther of the two
/// outermost levels (best first, so the last of each side), and the typed
/// limit if there is one, capped at [`CAP_PCT`]. The one rule for every
/// pair.
pub fn window_pct(bids: &[(f64, f64)], asks: &[(f64, f64)], mid: f64, limit: Option<f64>) -> f64 {
    if mid <= 0.0 {
        return FLOOR_PCT;
    }
    let off = |p: f64| ((p - mid) / mid).abs() * 100.0;
    let outer = |levels: &[(f64, f64)]| levels.iter().rev().find(|&&(p, a)| p > 0.0 && a > 0.0).map(|&(p, _)| off(p));
    let mut w = outer(bids).unwrap_or(0.0).max(outer(asks).unwrap_or(0.0));
    if let Some(l) = limit.filter(|l| *l > 0.0) {
        w = w.max(off(l));
    }
    if w <= 0.0 { FLOOR_PCT } else { w.min(CAP_PCT) }
}

/// The two cumulative step areas.
pub(crate) struct DepthChart {
    /// `(price, cumulative)` best first, inside the window.
    pub(crate) bids: Vec<(f64, f64)>,
    pub(crate) asks: Vec<(f64, f64)>,
    pub(crate) lo: f64,
    pub(crate) hi: f64,
    pub(crate) limit: Option<f64>,
    pub(crate) bid: Color,
    pub(crate) ask: Color,
    pub(crate) base: Color,
    pub(crate) mid_line: Color,
    pub(crate) marker: Color,
    pub(crate) tag: Color,
    pub(crate) scale: f32,
}

impl canvas::Program<Message> for DepthChart {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;
        let top = 8.0 * self.scale;
        let baseline = h - 10.0 * self.scale;

        // Baseline and the mid line, whatever the data.
        frame.stroke(
            &Path::line(Point::new(0.0, baseline), Point::new(w, baseline)),
            Stroke::default().with_color(self.base).with_width(1.0),
        );
        let x_mid = w / 2.0;
        dashed(&mut frame, x_mid, top, baseline, 2.0, 3.0, self.mid_line, 1.0);

        let span = self.hi - self.lo;
        if span <= 0.0 {
            return vec![frame.into_geometry()];
        }
        let max = self.bids.last().map(|l| l.1).unwrap_or(0.0)
            .max(self.asks.last().map(|l| l.1).unwrap_or(0.0))
            .max(f64::MIN_POSITIVE);
        let x_of = |p: f64| (((p - self.lo) / span) as f32 * w).clamp(0.0, w);
        let y_of = |c: f64| baseline - (c / max) as f32 * (baseline - top);

        // A step area from the mid outward: horizontal to the level's price at
        // the previous depth, then up to the new depth; flat to the edge.
        let area = |levels: &[(f64, f64)], edge: f32| -> Option<Path> {
            if levels.is_empty() {
                return None;
            }
            Some(Path::new(|b| {
                b.move_to(Point::new(x_mid, baseline));
                let mut y = baseline;
                for &(p, c) in levels {
                    let x = x_of(p);
                    b.line_to(Point::new(x, y));
                    y = y_of(c);
                    b.line_to(Point::new(x, y));
                }
                b.line_to(Point::new(edge, y));
                b.line_to(Point::new(edge, baseline));
                b.close();
            }))
        };
        let stroke = |c: Color| Stroke::default().with_color(c).with_width(1.2).with_line_join(LineJoin::Round);
        if let Some(p) = area(&self.bids, 0.0) {
            frame.fill(&p, BID_WASH);
            frame.stroke(&p, stroke(self.bid));
        }
        if let Some(p) = area(&self.asks, w) {
            frame.fill(&p, ASK_WASH);
            frame.stroke(&p, stroke(self.ask));
        }

        if let Some(limit) = self.limit {
            let x = x_of(limit);
            dashed(&mut frame, x, top, baseline, 3.0, 3.0, self.marker, 1.0);
            frame.fill_text(canvas::Text {
                content: format!("limit {}", fmt_price(limit)),
                position: Point::new(x.clamp(28.0 * self.scale, w - 28.0 * self.scale), 1.0),
                color: self.tag,
                size: Pixels(EYEBROW * self.scale),
                font: fonts::MONO,
                align_x: Horizontal::Center.into(),
                align_y: Vertical::Top.into(),
                ..canvas::Text::default()
            });
        }

        vec![frame.into_geometry()]
    }
}

/// A vertical dashed line.
fn dashed(frame: &mut Frame, x: f32, y0: f32, y1: f32, dash: f32, gap: f32, color: Color, width: f32) {
    let path = Path::new(|b| {
        let mut y = y0;
        while y < y1 {
            b.move_to(Point::new(x, y));
            b.line_to(Point::new(x, (y + dash).min(y1)));
            y += dash + gap;
        }
    });
    frame.stroke(&path, Stroke::default().with_color(color).with_width(width));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window is the book's own width: a tight book stays tight, a
    /// dust book opens to its real levels, junk past the cap is cut.
    #[test]
    fn the_window_fits_the_book_and_stops_at_the_cap() {
        // RLUSD-shaped: forty levels inside 0.4%.
        let bids: Vec<(f64, f64)> = (1..=40).map(|i| (1.40 * (1.0 - 0.0001 * i as f64), 100.0)).collect();
        let asks: Vec<(f64, f64)> = (1..=40).map(|i| (1.40 * (1.0 + 0.0001 * i as f64), 100.0)).collect();
        let w = window_pct(&bids, &asks, 1.40, None);
        assert!((w - 0.4).abs() < 1e-9, "{w}");
        // EUROP-shaped: one bid 9.8% under, one ask 9.9% over, junk at 91% under.
        let bids = vec![(1.0417, 156.0), (0.1, 20.0)];
        let asks = vec![(1.269, 7411.0)];
        let w = window_pct(&bids, &asks, 1.155, None);
        assert!((w - CAP_PCT).abs() < 1e-9, "junk sets the width only up to the cap: {w}");
        let w = window_pct(&bids[..1], &asks, 1.155, None);
        assert!((w - 9.87).abs() < 0.01, "{w}");
        // A typed limit outside the levels opens the window to it.
        let w = window_pct(&bids[..1], &asks, 1.155, Some(1.30));
        assert!((w - 12.55).abs() < 0.01, "{w}");
        // Nothing on either side: the floor, so the axis exists.
        assert_eq!(window_pct(&[], &[], 1.155, None), 1.0);
        assert_eq!(window_pct(&[], &[], 0.0, None), 1.0);
    }
}
