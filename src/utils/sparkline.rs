use iced::Color;
use std::collections::{HashMap, VecDeque};
use crate::controller::app_state::ChartPeriod;
use crate::utils::theme::CompactPalette;
use crate::utils::tokens::rate_key;
use chrono::{Local, TimeZone};

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

fn divide(num: Vec<(u64, f32)>, den: Vec<(u64, f32)>) -> Vec<(u64, f32)> {
    let dmap: HashMap<u64, f32> = den.into_iter().collect();
    num.into_iter()
        .filter_map(|(ts, n)| dmap.get(&ts).and_then(|&d| if d > 0.0 { Some((ts, n / d)) } else { None }))
        .collect()
}

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

pub fn chart_data(
    asset: &str,
    ccy: &str,
    history_short: &HashMap<String, VecDeque<(u64, f32)>>,
) -> Vec<(u64, f32)> {
    pair_raw(asset, ccy, history_short)
}

#[derive(Debug, Clone, Copy)]
pub struct ChartInk {
    pub line: Color,
    pub fill: Color,
    pub fill_top_a: f32,
    pub guide: Color,
    pub baseline: Color,
    pub high_label: Color,
    pub low_label: Color,
    pub hairline: Color,
    pub panel: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub window: Color,
    pub inset_x: f32,
    pub scale: f32,
}

impl ChartInk {
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
    match Local.timestamp_millis_opt(ts as i64) {
        chrono::LocalResult::Single(dt) => match period {
            ChartPeriod::OneHour   => dt.format("%H:%M").to_string(),
            ChartPeriod::OneDay    => dt.format("%a %H:%M").to_string(),
        },
        _ => String::new(),
    }
}

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

pub fn change_pct(series: &[f32]) -> Option<f32> {
    match (series.first(), series.last()) {
        (Some(&first), Some(&last)) if series.len() >= 2 && first > 0.0 => {
            Some((last - first) / first * 100.0)
        }
        _ => None,
    }
}
