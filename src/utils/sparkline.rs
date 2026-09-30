use iced::Color;
use std::collections::{HashMap, VecDeque};
use crate::controller::app_state::ChartPeriod;
use crate::utils::theme::CompactPalette;
use crate::utils::tokens::rate_key;
use chrono::{Local, TimeZone};

// ── Shared history helpers ────────────────────────────────────────────────────

// Raw per-asset USD series (keyed by asset code), 1h in-memory history.
fn raw_from_history(
    asset: &str,
    history_short: &HashMap<String, VecDeque<(u64, f32)>>,
) -> Vec<(u64, f32)> {
    if let Some(short) = history_short.get(asset) {
        if short.len() >= 2 {
            return short.iter().cloned().collect();
        }
    }
    vec![]
}

// Divide two (ts, price) series by matching timestamps: num[t] / den[t].
fn divide(num: Vec<(u64, f32)>, den: Vec<(u64, f32)>) -> Vec<(u64, f32)> {
    let dmap: HashMap<u64, f32> = den.into_iter().collect();
    num.into_iter()
        .filter_map(|(ts, n)| dmap.get(&ts).and_then(|&d| if d > 0.0 { Some((ts, n / d)) } else { None }))
        .collect()
}

// asset/base series as (ts, price), derived from per-asset USD series.
fn pair_raw(
    asset: &str,
    base: &str,
    history_short: &HashMap<String, VecDeque<(u64, f32)>>,
) -> Vec<(u64, f32)> {
    if base == "USD" {
        return raw_from_history(asset, history_short);
    }
    if asset == "USD" {
        return raw_from_history(base, history_short)
            .into_iter().map(|(t, p)| (t, if p > 0.0 { 1.0 / p } else { 0.0 })).collect();
    }
    divide(
        raw_from_history(asset, history_short),
        raw_from_history(base, history_short),
    )
}

// ── chart_data (the pane charts) ────────────────────────────────────────────

pub fn chart_data(
    asset: &str,
    ccy: &str,
    history_short: &HashMap<String, VecDeque<(u64, f32)>>,
) -> Vec<(u64, f32)> {
    pair_raw(asset, ccy, history_short)
}

// ── ChartInk (the pane charts) ────────────────────────

/// Every colour a chart canvas draws with, resolved by the screen from the
/// compact palette (chart v3, 2026-09-05; the canvas itself is now the pane
/// chart in `components::grid`, the old full-screen one went 2026-09-10). The line is `green` on an up range and
/// `red` on a down one — the only two signal colours the theme has — and the
/// fill is the line's own colour fading to nothing. No third hue exists here.
#[derive(Debug, Clone, Copy)]
pub struct ChartInk {
    /// `green` up / `red` down.
    pub line: Color,
    /// The wash's colour. The line's own on the chart screen and the trade
    /// pane; the dashboard's chart pane sets it to `dim` — a neutral wash,
    /// because there the direction is already stated twice.
    pub fill: Color,
    /// The fill's top-stop alpha: `.16` light / `.15` dark on an up range,
    /// `.15` / `.14` on a down one. Fades to `0` at the baseline.
    pub fill_top_a: f32,
    /// The dotted range-high guide — `border_soft`.
    pub guide: Color,
    /// The 1px foot of the plot — `rule`.
    pub baseline: Color,
    /// The high value above the guide — `dim`; the low under its point — `muted`.
    pub high_label: Color,
    pub low_label: Color,
    /// The scrub hairline — `dim`.
    pub hairline: Color,
    /// The scrub tooltip: `panel` on a 1px `border`, `text` figure, `muted` time.
    pub panel: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    /// The dot's inner fill — the ground the plot sits on.
    pub window: Color,
    /// The high label's right inset — the screen's own (24 on the chart page).
    pub inset_x: f32,
    pub scale: f32,
}

impl ChartInk {
    /// `dark` picks the alpha row; `up` picks the hue.
    pub fn new(cp: &CompactPalette, up: bool, dark: bool, inset_x: f32, scale: f32) -> Self {
        let fill_top_a = match (up, dark) {
            (true, false) => 0.16,
            (true, true) => 0.15,
            (false, false) => 0.15,
            (false, true) => 0.14,
        };
        let line = if up { cp.green } else { cp.red };
        Self {
            line,
            fill: line,
            fill_top_a,
            guide: cp.border_soft,
            baseline: cp.rule,
            high_label: cp.dim,
            low_label: cp.muted,
            hairline: cp.dim,
            panel: cp.panel,
            border: cp.border,
            text: cp.text,
            muted: cp.muted,
            window: cp.window,
            inset_x,
            scale,
        }
    }
}

pub(crate) fn fmt_hover_time(ts: u64, period: &ChartPeriod) -> String {
    if ts == 0 {
        return String::new();
    }
    // `ts` is epoch MILLISECONDS (rates stamps points with `as_millis()`), so this must be
    // timestamp_millis_opt — timestamp_opt takes seconds and silently renders a year-58000
    // date, which stays invisible because the patterns below only show %H:%M / %a %H:%M.
    match Local.timestamp_millis_opt(ts as i64) {
        chrono::LocalResult::Single(dt) => match period {
            ChartPeriod::OneHour   => dt.format("%H:%M").to_string(),
            ChartPeriod::OneDay    => dt.format("%a %H:%M").to_string(),
        },
        _ => String::new(),
    }
}

// ── Balance: the day line ─────────────────────────────────────────────────────

/// What is held NOW, valued at each of the last 24 hours' prices, in `base` —
/// the series under the Balance hero.
///
/// **Price-driven by decision (2026-09-03), not ledger-driven.** The other
/// reading — the account's actual balance sampled over the day — would need
/// the relay to keep a balance history per followed wallet in Redis, and would
/// paint a send you made this morning red. What this screen cannot otherwise
/// show is what the market did to the money; that is what the line says. It
/// also makes the Balance `24h` the holdings-weighted mean of the XRP and BTC
/// tabs' own `24h`, so the three figures reconcile. Nothing is stored for it:
/// the per-asset 24h series are already in state, seeded once at launch.
///
/// `holdings` are `(code, amount)` — token codes or asset codes, mapped through
/// `rate_key` here. Only buckets present in EVERY priced holding's series are
/// summed, so a series that skipped a tick (price 0 at the time) cannot dent
/// the total. A holding whose rate key IS the base (a USD-pegged token under a
/// USD base) has no series and contributes a constant. A priced holding with
/// no day yet means no day for the total — the line is omitted rather than
/// drawn short.
pub fn total_series(
    holdings: &[(&str, f64)],
    base: &str,
    history_long: &HashMap<String, VecDeque<(u64, f32)>>,
) -> Vec<f32> {
    let mut constant = 0.0f64;
    let mut series: Vec<(f64, HashMap<u64, f32>)> = Vec::new();
    for &(code, amount) in holdings {
        if amount <= 0.0 {
            continue;
        }
        let key = rate_key(code);
        if key == base {
            constant += amount;
            continue;
        }
        let s: HashMap<u64, f32> = pair_raw(key, base, history_long).into_iter().collect();
        if s.len() < 2 {
            return vec![];
        }
        series.push((amount, s));
    }
    let Some((_, first)) = series.first() else {
        return vec![];
    };
    let mut ts: Vec<u64> = first
        .keys()
        .copied()
        .filter(|t| series.iter().all(|(_, s)| s.contains_key(t)))
        .collect();
    ts.sort_unstable();
    if ts.len() < 2 {
        return vec![];
    }
    ts.iter()
        .map(|t| {
            let sum: f64 = series.iter().map(|(a, s)| a * s[t] as f64).sum();
            (sum + constant) as f32
        })
        .collect()
}

/// First against last, as a percentage. `None` with fewer than two points or a
/// zero start — the caller draws nothing rather than a confident `0.00%`.
pub fn change_pct(series: &[f32]) -> Option<f32> {
    match (series.first(), series.last()) {
        (Some(&first), Some(&last)) if series.len() >= 2 && first > 0.0 => {
            Some((last - first) / first * 100.0)
        }
        _ => None,
    }
}
