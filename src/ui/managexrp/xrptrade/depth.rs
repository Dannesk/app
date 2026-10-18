
use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, LineJoin, Path, Stroke};
use iced::{mouse, Color, Pixels, Point, Rectangle};

use crate::controller::message::Message;
use crate::utils::fonts::{self};
use crate::utils::orderbook::fmt_price;

use super::{ASK_WASH, BID_WASH, EYEBROW};

pub const CAP_PCT: f64 = 25.0;
const FLOOR_PCT: f64 = 1.0;

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

pub(crate) struct DepthChart {
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

    #[test]
    fn the_window_fits_the_book_and_stops_at_the_cap() {
        let bids: Vec<(f64, f64)> = (1..=40).map(|i| (1.40 * (1.0 - 0.0001 * i as f64), 100.0)).collect();
        let asks: Vec<(f64, f64)> = (1..=40).map(|i| (1.40 * (1.0 + 0.0001 * i as f64), 100.0)).collect();
        let w = window_pct(&bids, &asks, 1.40, None);
        assert!((w - 0.4).abs() < 1e-9, "{w}");
        let bids = vec![(1.0417, 156.0), (0.1, 20.0)];
        let asks = vec![(1.269, 7411.0)];
        let w = window_pct(&bids, &asks, 1.155, None);
        assert!((w - CAP_PCT).abs() < 1e-9, "junk sets the width only up to the cap: {w}");
        let w = window_pct(&bids[..1], &asks, 1.155, None);
        assert!((w - 9.87).abs() < 0.01, "{w}");
        let w = window_pct(&bids[..1], &asks, 1.155, Some(1.30));
        assert!((w - 12.55).abs() < 0.01, "{w}");
        assert_eq!(window_pct(&[], &[], 1.155, None), 1.0);
        assert_eq!(window_pct(&[], &[], 0.0, None), 1.0);
    }
}
