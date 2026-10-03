//! The `block intervals` pane (handoff `btc-panes`, 2026-09-10): the gap
//! between the last twelve blocks against the ten-minute target, the gap in
//! progress, and where the difficulty epoch stands. Read-only; every value
//! is on the node frame already. Hover names the bar — its block and its
//! gap — because a bar with no number under the pointer is a shape, not a
//! fact (user, 2026-09-10).
//!
//! **Thirteen bars, one ink.** Twelve settled gaps, oldest at the left,
//! then the gap in progress — hollow, dashed, still counting, and in no
//! statistic. Height is the gap; the dashed rule is the target, computed
//! from the chart's own scale (`h × (1 − 600 / max)`), so a bar over the
//! rule is a slow block. The scale is the slowest settled gap, or the
//! current one if it has already run longer — the hollow bar never
//! overflows. `average` and `slowest` are over the twelve settled gaps, and
//! the axis says so: `last 12 blocks` is a property of the x-axis, not a
//! title. **Twelve is a small sample** — a run of fast blocks reads well
//! under the epoch's pace (5m 52s against 9m 42s on 2026-09-10; the
//! standard error of twelve ten-minute draws is near three minutes) — which
//! is why the axis names the window and the `retarget` row carries the
//! epoch's own average.
//!
//! The gaps come from the train's arrival stamps — indexd's own clock for
//! blocks it watched land, header times clamped to now for the rest — and
//! are clamped at zero, since header times are not monotonic. Right after
//! an indexd restart every stamp is a header time and a 20-second bar is
//! miner clock noise; the ring firms up as watched blocks replace them. The
//! hollow bar moves on the redraw cadence the pane already has (frames,
//! rates, keys): no timer, per the 2026-09-10 minutes ruling — a pixel is
//! ~20 s on a 24-minute scale anyway.
//!
//! `difficulty epoch` is `tip % 2016 + 1` of 2016. `retarget` estimates
//! the coming adjustment from the epoch so far — the epoch's first header
//! against the tip's, over the blocks between them — clamped to the ±4×
//! consensus bounds, with the ETA at the epoch's own pace. Noisy early in
//! an epoch, which is a fact, not a bug.

use iced::Widget as _;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame, LineDash, Path, Stroke};
use iced::widget::{column, container, Space};
use iced::{Color, Element, Length, Padding, Pixels, Point, Rectangle, Size};
use iced::alignment::{Horizontal, Vertical};

use crate::channel::{BtcBlock, BtcNodeStats as NodeFrame, BLOCK_TRAIN};
use crate::controller::app_state::AppState;
use crate::controller::message::Message;
use crate::ui::components::grid::{self, drow, NA};
use crate::ui::managebtc::node::tip_age;
use crate::ui::managebtc::panes::mempool::{axis_row, readout, slot_at, track, BarHover, AXIS, LABEL_PAD, MONO_ADVANCE};
use crate::utils::add_commas;
use crate::utils::fonts;
use crate::utils::theme::CompactPalette;

/// The handoff's chart geometry: `padding 2px 0 10px`, bars `gap 4`,
/// radius 1, the label 11px above the rule.
const CHART_PAD_TOP: f32 = 2.0;
const CHART_PAD_BOTTOM: f32 = 10.0;
const BAR_GAP: f32 = 4.0;
const BAR_RADIUS: f32 = 1.0;
const LABEL_LIFT: f32 = 11.0;
/// Settled gaps the chart shows — the train less one stamp.
const GAPS: usize = BLOCK_TRAIN - 1;
/// The target, and the epoch.
const TARGET_SECS: u64 = 600;
const EPOCH: u64 = 2016;
/// The least the pane fills before it scrolls: legible bars, the axis, the
/// four rows.
const MIN_H: f32 = 170.0;

/// One settled bar: the block, and how long it took to find it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Gap {
    height: u64,
    secs: u64,
}

pub fn view<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    grid::fill_or_scroll(MIN_H, move || body(state, cp, scale), cp, scale)
}

fn body<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let _ = state;
    let node = NodeFrame::current();
    let gaps = gaps(&node.blocks);
    let current = tip_age(node.tip_at);
    let stats = Stats::of(&gaps);

    let chart: Element<'a, Message> = if gaps.is_empty() {
        Space::new().width(Length::Fill).height(Length::Fill).boxed()
    } else {
        let max = stats.map_or(0, |s| s.slowest).max(current.unwrap_or(0)).max(1);
        Canvas::new(Intervals {
            bars: gaps.clone(),
            current,
            max,
            rule: rule_fraction(max),
            ink: cp.dim,
            hot: cp.text,
            faint: cp.faint,
            bg: cp.window,
            scale,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed()
    };

    let (left, right) = if gaps.is_empty() {
        (String::new(), String::new())
    } else {
        (format!("last {} blocks", gaps.len()), "now".to_string())
    };

    let na = || vec![(NA.to_string(), cp.faint)];
    let average = stats.map_or_else(na, |s| vec![(duration_word(s.average), cp.text)]);
    let slowest = stats.map_or_else(na, |s| vec![(duration_word(s.slowest), cp.dim)]);
    let epoch = node.tip_height.map_or_else(na, |h| {
        vec![(format!("{} / {}", add_commas((h % EPOCH + 1) as i64), add_commas(EPOCH as i64)), cp.dim)]
    });
    let retarget = match retarget(node.tip_height, node.tip_time, node.epoch_start_time) {
        Some(r) => vec![(format!("{:+.1}%", r.pct), cp.dim), (format!("  in {}", eta_word(r.eta_secs)), cp.faint)],
        None => na(),
    };

    column![
        container(chart)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::new(0.0).top(CHART_PAD_TOP * scale).bottom(CHART_PAD_BOTTOM * scale)),
        axis_row(left, right, cp, scale),
        drow("average", average, 0.0, cp, scale),
        drow("slowest", slowest, 3.0, cp, scale),
        drow("difficulty epoch", epoch, 3.0, cp, scale),
        drow("retarget", retarget, 3.0, cp, scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

// ── Derivations ─────────────────────────────────────────────────────────────

/// The settled gaps, oldest first, from the train's newest-first stamps:
/// each block's arrival less the one before it, clamped at zero, filed
/// under the block it took to find. Stops at the first empty slot — the
/// train is contiguous from the tip.
fn gaps(blocks: &[Option<BtcBlock>; BLOCK_TRAIN]) -> Vec<Gap> {
    let stamps: Vec<(u64, u64)> = blocks.iter().map_while(|b| b.map(|b| (b.height, b.at))).collect();
    let mut out: Vec<Gap> = stamps
        .windows(2)
        .map(|w| Gap { height: w[0].0, secs: w[0].1.saturating_sub(w[1].1) })
        .take(GAPS)
        .collect();
    out.reverse();
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Stats {
    average: u64,
    slowest: u64,
}

impl Stats {
    fn of(gaps: &[Gap]) -> Option<Self> {
        if gaps.is_empty() {
            return None;
        }
        let sum: u64 = gaps.iter().map(|g| g.secs).sum();
        Some(Self { average: sum / gaps.len() as u64, slowest: gaps.iter().map(|g| g.secs).max().unwrap_or(0) })
    }
}

/// The target's height as a fraction of the chart from the top — `None`
/// when the target is off the top, i.e. every gap so far was under it.
fn rule_fraction(max: u64) -> Option<f32> {
    if max <= TARGET_SECS {
        None
    } else {
        Some(1.0 - TARGET_SECS as f32 / max as f32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Retarget {
    pct: f64,
    eta_secs: u64,
}

/// The coming adjustment, from the epoch so far: blocks since the epoch's
/// first block against the header time elapsed, clamped to consensus's
/// ±4×. `None` at the epoch's first block or without both headers.
fn retarget(tip: Option<u64>, tip_time: Option<u64>, epoch_start_time: Option<u64>) -> Option<Retarget> {
    let (tip, now, start) = (tip?, tip_time?, epoch_start_time?);
    let n = tip % EPOCH;
    if n == 0 || now <= start {
        return None;
    }
    let avg = (now - start) as f64 / n as f64;
    let pct = ((TARGET_SECS as f64 / avg - 1.0) * 100.0).clamp(-75.0, 300.0);
    let eta_secs = ((EPOCH - n) as f64 * avg).round() as u64;
    Some(Retarget { pct, eta_secs })
}

/// `9m 38s`, `1h 03m` — a statistic, so seconds stay.
fn duration_word(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}m {:02}s", secs / 60, secs % 60)
    }
}

/// `6d` from a day up, `14h` under.
fn eta_word(secs: u64) -> String {
    if secs >= 86_400 {
        format!("{}d", secs / 86_400)
    } else {
        format!("{}h", secs / 3600)
    }
}

/// The hover readout for slot `i`: a settled bar names its block and gap,
/// the hollow one reads `next` and how long it has been running.
fn bar_word(bars: &[Gap], current: Option<u64>, i: usize) -> Option<String> {
    if let Some(g) = bars.get(i) {
        return Some(format!("{} \u{b7} {}", add_commas(g.height as i64), duration_word(g.secs)));
    }
    (i == bars.len()).then(|| format!("next \u{b7} {}", current.map_or(NA.to_string(), duration_word)))
}

// ── The chart ───────────────────────────────────────────────────────────────

/// The settled bars filled, the current one hollow and dashed, all at one
/// pitch; the target rule dashed across, its label above its right end.
/// The hovered bar in `hot`, its readout over everything.
struct Intervals {
    bars: Vec<Gap>,
    current: Option<u64>,
    max: u64,
    rule: Option<f32>,
    ink: Color,
    hot: Color,
    faint: Color,
    bg: Color,
    scale: f32,
}

impl<Message> canvas::Program<Message> for Intervals {
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
        let n = self.bars.len() + 1;
        let gap = BAR_GAP * s;
        let slot = ((w - gap * (n as f32 - 1.0)).max(0.0) / n as f32).round().max(1.0);
        let max = self.max.max(1) as f32;
        let height_of = |secs: u64| (h * secs as f32 / max).clamp(1.0, h).round();
        let x_of = |i: usize| (i as f32 * (slot + gap)).round();
        let hovered = state.cursor.and_then(|c| slot_at(c.x, w, n, gap));

        for (i, g) in self.bars.iter().enumerate() {
            let bh = height_of(g.secs);
            let bar = Path::new(|b| {
                b.rounded_rectangle(Point::new(x_of(i), h - bh), Size::new(slot, bh), iced::border::Radius::from(BAR_RADIUS * s));
            });
            frame.fill(&bar, if hovered == Some(i) { self.hot } else { self.ink });
        }

        // The gap in progress: the same slot, a dashed outline inset by
        // half a pixel so the stroke sits on whole pixels.
        if let Some(secs) = self.current {
            let bh = height_of(secs);
            let outline = Path::new(|b| {
                b.rounded_rectangle(
                    Point::new(x_of(self.bars.len()) + 0.5, h - bh + 0.5),
                    Size::new((slot - 1.0).max(1.0), (bh - 1.0).max(1.0)),
                    iced::border::Radius::from(BAR_RADIUS * s),
                );
            });
            let dash = [2.0 * s, 2.0 * s];
            let ink = if hovered == Some(self.bars.len()) { self.hot } else { self.faint };
            frame.stroke(
                &outline,
                Stroke {
                    line_dash: LineDash { segments: &dash, offset: 0 },
                    ..Stroke::default().with_color(ink).with_width(1.0)
                },
            );
        }

        if let Some(f) = self.rule {
            let y = (h * f).round() + 0.5;
            let dash = [3.0 * s, 2.0 * s];
            frame.stroke(
                &Path::line(Point::new(0.0, y), Point::new(w, y)),
                Stroke {
                    line_dash: LineDash { segments: &dash, offset: 0 },
                    ..Stroke::default().with_color(self.faint).with_width(1.0)
                },
            );
            let size = AXIS * s;
            let pad = LABEL_PAD * s;
            let word = "10M";
            let tw = word.chars().count() as f32 * size * MONO_ADVANCE;
            let top = (y - LABEL_LIFT * s).max(0.0);
            frame.fill_rectangle(Point::new(w - tw - 2.0 * pad, top), Size::new(tw + 2.0 * pad, size + 2.0), self.bg);
            frame.fill_text(canvas::Text {
                content: word.to_string(),
                position: Point::new(w - pad, top),
                color: self.faint,
                size: Pixels(size),
                font: fonts::MONO,
                align_x: Horizontal::Right.into(),
                align_y: Vertical::Top.into(),
                ..canvas::Text::default()
            });
        }

        if let Some(word) = hovered.and_then(|i| bar_word(&self.bars, self.current, i)) {
            readout(&mut frame, &word, self.hot, self.bg, s);
        }

        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn train(stamps: &[u64]) -> [Option<BtcBlock>; BLOCK_TRAIN] {
        let mut out = [None; BLOCK_TRAIN];
        for (i, (slot, &at)) in out.iter_mut().zip(stamps).enumerate() {
            *slot = Some(BtcBlock { height: 1000 - i as u64, at, txs: 1, weight: 4_000_000, feerate: 1.0 });
        }
        out
    }

    fn secs(g: &[Gap]) -> Vec<u64> {
        g.iter().map(|g| g.secs).collect()
    }

    /// Twelve gaps from thirteen stamps, oldest first and filed under the
    /// block they took to find; a non-monotonic header pair clamped at
    /// zero; a short train giving fewer bars.
    #[test]
    fn gaps_are_oldest_first_clamped_and_capped() {
        let stamps: Vec<u64> = (0..13).rev().map(|i| 10_000 - i * 600 + if i == 5 { 900 } else { 0 }).collect();
        // Newest first as the frame carries them.
        let mut newest_first = stamps.clone();
        newest_first.reverse();
        let g = gaps(&train(&newest_first));
        assert_eq!(g.len(), GAPS);
        // Oldest first: the first gap is between the two oldest stamps, and
        // belongs to the newer of the two (height 1000 − 11).
        assert_eq!(g[0], Gap { height: 989, secs: 600 });
        assert_eq!(g[GAPS - 1].height, 1000, "the last settled gap is the tip's");
        // The bumped stamp: the gap into it is 1500, the gap out of it 0.
        assert!(secs(&g).contains(&1500));
        assert!(secs(&g).contains(&0), "a header ahead of its child is a zero gap, not a negative one");

        assert_eq!(secs(&gaps(&train(&[5000, 4400, 3700]))), vec![700, 600]);
        assert!(gaps(&train(&[5000])).is_empty());
        assert!(gaps(&train(&[])).is_empty());
    }

    /// `slowest` is the tallest bar, `average` is over the settled gaps only,
    /// and the rule sits at `1 − 600 / max` from the top — the mock's 48px
    /// of 90 at a 24m 06s scale.
    #[test]
    fn the_rule_is_computed_from_the_scale() {
        let g: Vec<Gap> = [47u64 * 15, 13 * 15, 100 * 15].iter().map(|&s| Gap { height: 1, secs: s }).collect();
        let s = Stats::of(&g).unwrap();
        assert_eq!(s.slowest, 1500);
        assert_eq!(s.average, 800);
        assert!(Stats::of(&[]).is_none());
        let f = rule_fraction(1446).unwrap();
        assert!((f * 78.0 - 45.6).abs() < 0.1, "{}", f * 78.0);
        assert_eq!(rule_fraction(500), None, "every gap under target: no rule to draw");
    }

    /// The epoch so far at exactly the target pace is a 0% retarget; twice
    /// as fast doubles the difficulty; a stalled epoch is clamped, not
    /// infinite; the first block of an epoch has nothing to say.
    #[test]
    fn retarget_reads_the_epoch_so_far() {
        let start = 1_000_000;
        let tip = 2016 * 400 + 1000;
        let on_pace = retarget(Some(tip), Some(start + 1000 * 600), Some(start)).unwrap();
        assert!(on_pace.pct.abs() < 1e-9);
        assert_eq!(on_pace.eta_secs, 1016 * 600);
        let fast = retarget(Some(tip), Some(start + 1000 * 300), Some(start)).unwrap();
        assert!((fast.pct - 100.0).abs() < 1e-9);
        let slow = retarget(Some(tip), Some(start + 1000 * 60_000), Some(start)).unwrap();
        assert_eq!(slow.pct, -75.0);
        assert!(retarget(Some(2016 * 400), Some(start + 1), Some(start)).is_none());
        assert!(retarget(Some(tip), Some(start), Some(start)).is_none());
        assert!(retarget(None, Some(1), Some(0)).is_none());
    }

    #[test]
    fn durations_and_readouts_read_as_the_rows_show_them() {
        assert_eq!(duration_word(578), "9m 38s");
        assert_eq!(duration_word(1446), "24m 06s");
        assert_eq!(duration_word(3780), "1h 03m");
        assert_eq!(eta_word(6 * 86_400 + 3600), "6d");
        assert_eq!(eta_word(14 * 3600), "14h");
        let bars = vec![Gap { height: 966_318, secs: 575 }];
        assert_eq!(bar_word(&bars, Some(784), 0).as_deref(), Some("966,318 \u{b7} 9m 35s"));
        assert_eq!(bar_word(&bars, Some(784), 1).as_deref(), Some("next \u{b7} 13m 04s"));
        assert_eq!(bar_word(&bars, None, 1).as_deref(), Some("next \u{b7} \u{2014}"));
        assert_eq!(bar_word(&bars, None, 2), None);
    }
}
