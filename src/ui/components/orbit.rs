use std::time::{Duration, Instant};

use iced::advanced::svg::Svg;
use iced::advanced::text::Alignment as TextAlign;
use iced::alignment::Vertical;
use iced::widget::canvas::{path, Frame, LineCap, LineDash, Path, Stroke, Text};
use iced::widget::text::LineHeight;
use iced::{Color, Point, Radians, Rectangle};

use crate::channel::{BtcNodeStats, Health, Socket, BTC_INDEX, BTC_NODE, CHANNEL, XRP_NODE};
use crate::ui::components::wallet_gate::{GLYPH, XRP_MARK_H, XRP_MARK_W};
use crate::ui::managebtc::node::{BEHIND_FLOOR_SECS, SYNCED_FLOOR};
use crate::utils::fonts::LIGHT;
use crate::utils::icons;
use crate::utils::theme::CompactPalette;

pub const LAP: Duration = Duration::from_millis(1600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chain {
    Xrp,
    Btc,
}

impl Chain {
    pub const ALL: [Chain; 2] = [Chain::Xrp, Chain::Btc];

    pub fn name(self) -> &'static str {
        match self {
            Chain::Xrp => "XRP",
            Chain::Btc => "BTC",
        }
    }

    pub fn behind(self) -> &'static str {
        match self {
            Chain::Xrp => " ledgers behind",
            Chain::Btc => " blocks behind",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    Waiting,
    Live,
    CatchingUp(Option<u64>),
    NoPeers,
    Offline,
}

impl Node {
    pub fn up(self) -> bool {
        matches!(self, Node::Live | Node::CatchingUp(_) | Node::NoPeers)
    }

    pub fn color(self, p: &CompactPalette) -> Color {
        match self {
            Node::Waiting => p.dim,
            Node::Live => p.green,
            Node::CatchingUp(_) | Node::NoPeers => p.amber,
            Node::Offline => p.red,
        }
    }
}

pub fn socket() -> Socket {
    *CHANNEL.socket_rx.borrow()
}

pub fn arc_alpha(theme: &iced::Theme) -> f32 {
    if crate::utils::theme::is_dark(theme) { 0.85 } else { 1.0 }
}

pub fn xrp_state(live: bool, node: Health, synced: Option<bool>) -> Node {
    match node {
        _ if !live => Node::Waiting,
        Health::Unknown => Node::Waiting,
        Health::Down => Node::Offline,
        Health::Up if synced == Some(false) => Node::CatchingUp(None),
        Health::Up => Node::Live,
    }
}

pub fn btc_state(live: bool, health: [Health; 2], n: &BtcNodeStats) -> Node {
    if !live || (health.contains(&Health::Unknown) && !health.contains(&Health::Down)) {
        return Node::Waiting;
    }
    if health.contains(&Health::Down) {
        return Node::Offline;
    }
    if n.peers == Some(0) {
        Node::NoPeers
    } else if n.behind_secs.is_some_and(|s| s >= BEHIND_FLOOR_SECS) || n.sync.is_some_and(|s| s < SYNCED_FLOOR) {
        Node::CatchingUp(n.headers.zip(n.node_height).map(|(h, b)| h.saturating_sub(b)).filter(|b| *b > 0))
    } else {
        Node::Live
    }
}

pub fn xrp_node(live: bool) -> Node {
    xrp_state(live, CHANNEL.health(XRP_NODE), CHANNEL.xrp_node_rx.borrow().synced)
}

pub fn btc_node(live: bool) -> Node {
    btc_state(live, [CHANNEL.health(BTC_NODE), CHANNEL.health(BTC_INDEX)], &CHANNEL.btc_node_rx.borrow())
}

pub fn node(chain: Chain, live: bool) -> Node {
    match chain {
        Chain::Xrp => xrp_node(live),
        Chain::Btc => btc_node(live),
    }
}

pub fn rates_pending() -> bool {
    CHANNEL.rates_rx.borrow().is_empty() && !CHANNEL.launch_rx.borrow().over && *CHANNEL.rates_ws_status_rx.borrow()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Connecting,
    Reconnecting,
    Offline(Option<Chain>),
    NoRate,
}

impl Status {
    pub fn words(self) -> &'static str {
        match self {
            Status::Connecting => "Connecting\u{2026}",
            Status::Reconnecting => "Reconnecting\u{2026}",
            Status::Offline(None) => "Nodes offline",
            Status::Offline(Some(Chain::Xrp)) => "XRP node offline",
            Status::Offline(Some(Chain::Btc)) => "BTC node offline",
            Status::NoRate => "Rate unavailable",
        }
    }

    pub fn strong(self) -> bool {
        matches!(self, Status::Offline(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Delta,
    Offline(Chain, Option<Chain>),
    Connecting(Chain),
    NoPeers,
    CatchingUp(Chain, Option<u64>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Middle {
    Figure { dim: bool, line: Line },
    Status(Status),
}

pub fn middle(socket: Socket, nodes: [Option<Node>; 2], total: bool, ready: [bool; 2], rates_pending: bool) -> Middle {
    let held: Vec<(Chain, Node)> =
        Chain::ALL.into_iter().zip(nodes).filter_map(|(c, n)| n.map(|n| (c, n))).collect();
    let up = held.iter().any(|(_, n)| n.up());
    let balances_in = ready.iter().all(|r| *r);
    if total && (up || held.is_empty()) {
        let dim = held.iter().any(|&(_, n)| n != Node::Live);
        return Middle::Figure { dim, line: line(&held) };
    }
    Middle::Status(match socket {
        Socket::Connecting { .. } => Status::Connecting,
        Socket::Reconnecting { .. } => Status::Reconnecting,
        Socket::Live => {
            let down: Vec<Chain> = held
                .iter()
                .filter(|&&(c, n)| n == Node::Offline && (!up || !ready[c as usize]))
                .map(|&(c, _)| c)
                .collect();
            match down[..] {
                [c] => Status::Offline(Some(c)),
                [_, _] => Status::Offline(None),
                _ if !up && balances_in => Status::Reconnecting,
                _ if up && balances_in && !rates_pending => Status::NoRate,
                _ => Status::Connecting,
            }
        }
    })
}

fn line(held: &[(Chain, Node)]) -> Line {
    let find = |want: Node| held.iter().find(|&&(_, n)| n == want).map(|&(c, _)| c);
    let catching = held.iter().find_map(|&(c, n)| match n {
        Node::CatchingUp(behind) => Some((c, behind)),
        _ => None,
    });
    if let Some(c) = find(Node::Offline) {
        return Line::Offline(c, catching.map(|(c, _)| c));
    }
    if let Some(c) = find(Node::Waiting) {
        return Line::Connecting(c);
    }
    if find(Node::NoPeers).is_some() {
        return Line::NoPeers;
    }
    match catching {
        Some((c, behind)) => Line::CatchingUp(c, behind),
        None => Line::Delta,
    }
}

pub fn laps(run: Option<(Instant, u32)>, socket: Socket, now: Instant) -> Option<(Instant, u32)> {
    let (Socket::Connecting { attempt } | Socket::Reconnecting { attempt }) = socket else {
        return run;
    };
    if attempt == 0 {
        return run;
    }
    match run {
        Some((from, n)) if now < from + LAP * n => Some((from, n + 1)),
        _ => Some((now, 1)),
    }
}

pub fn turn(run: Option<(Instant, u32)>, now: Instant) -> Option<f32> {
    let (from, n) = run?;
    let t = now.saturating_duration_since(from);
    if t >= LAP * n {
        return None;
    }
    let f = (t.as_secs_f32() / LAP.as_secs_f32()).fract();
    Some(if f < 0.5 { 4.0 * f * f * f } else { 1.0 - (2.0 - 2.0 * f).powi(3) / 2.0 })
}

pub fn peak(prev: Option<u64>, node: Node) -> Option<u64> {
    match node {
        Node::CatchingUp(Some(behind)) => Some(prev.map_or(behind, |p| p.max(behind))),
        Node::Live => None,
        _ => prev,
    }
}

pub fn progress(behind: Option<u64>, peak: Option<u64>) -> f32 {
    match (behind, peak) {
        (Some(b), Some(p)) if p > 0 => 1.0 - b.min(p) as f32 / p as f32,
        _ => 0.0,
    }
}

pub fn track(frame: &mut Frame, c: Point, r: f32, dash: f32, pitch: f32, color: Color) {
    let circumference = std::f32::consts::TAU * r;
    let pitch = circumference / (circumference / pitch).round();
    let segments = [dash, pitch - dash];
    frame.stroke(
        &Path::circle(c, r),
        Stroke {
            line_dash: LineDash { segments: &segments, offset: 0 },
            line_cap: LineCap::Round,
            ..Stroke::default().with_color(color).with_width(1.0)
        },
    );
}

pub fn mark(frame: &mut Frame, chain: Chain, at: Point, width: f32, color: Color) {
    match chain {
        Chain::Xrp => {
            let height = width * XRP_MARK_H / XRP_MARK_W;
            frame.draw_svg(
                Rectangle { x: at.x - width / 2.0, y: at.y - height / 2.0, width, height },
                Svg::new(icons::XRP.clone()).color(color),
            );
        }
        Chain::Btc => frame.fill_text(Text {
            content: "\u{20bf}".to_string(),
            position: at,
            color,
            size: (width * GLYPH / XRP_MARK_W).into(),
            line_height: LineHeight::Relative(1.0),
            font: LIGHT,
            align_x: TextAlign::Center,
            align_y: Vertical::Center,
            ..Text::default()
        }),
    }
}

pub fn arc(frame: &mut Frame, c: Point, r: f32, start: f32, sweep: f32, color: Color, width: f32) {
    frame.stroke(
        &Path::new(|b| {
            b.arc(path::Arc { center: c, radius: r, start_angle: Radians(start), end_angle: Radians(start + sweep) })
        }),
        Stroke { line_cap: LineCap::Round, ..Stroke::default().with_color(color).with_width(width) },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_xrp_node_reads_the_socket_then_the_health_report() {
        assert_eq!(xrp_state(false, Health::Up, Some(true)), Node::Waiting);
        assert_eq!(xrp_state(true, Health::Unknown, Some(true)), Node::Waiting);
        assert_eq!(xrp_state(true, Health::Down, Some(true)), Node::Offline);
        assert_eq!(xrp_state(true, Health::Up, Some(false)), Node::CatchingUp(None));
        assert_eq!(xrp_state(true, Health::Up, Some(true)), Node::Live);
        assert_eq!(xrp_state(true, Health::Up, None), Node::Live);
    }

    #[test]
    fn the_btc_node_counts_the_blocks_it_is_behind() {
        let synced = BtcNodeStats { sync: Some(1.0), peers: Some(8), ..Default::default() };
        let up = [Health::Up, Health::Up];
        assert_eq!(btc_state(true, up, &synced), Node::Live);
        assert_eq!(btc_state(true, [Health::Up, Health::Down], &synced), Node::Offline);
        assert_eq!(btc_state(true, [Health::Unknown, Health::Down], &synced), Node::Offline);
        assert_eq!(btc_state(true, [Health::Up, Health::Unknown], &synced), Node::Waiting);
        assert_eq!(btc_state(false, up, &synced), Node::Waiting);
        let behind = BtcNodeStats {
            headers: Some(966_336),
            node_height: Some(966_300),
            behind_secs: Some(600),
            ..synced
        };
        assert_eq!(btc_state(true, up, &behind), Node::CatchingUp(Some(36)));
        assert_eq!(btc_state(true, up, &BtcNodeStats { peers: Some(0), ..synced }), Node::NoPeers);
    }

    #[test]
    fn each_dial_is_one_lap_and_a_dial_mid_lap_queues_the_next() {
        let t0 = Instant::now();
        let first = laps(None, Socket::Connecting { attempt: 1 }, t0);
        assert_eq!(first, Some((t0, 1)));
        assert_eq!(laps(first, Socket::Connecting { attempt: 2 }, t0 + LAP / 2), Some((t0, 2)));
        assert_eq!(laps(first, Socket::Reconnecting { attempt: 0 }, t0 + LAP / 2), first);
        assert_eq!(laps(first, Socket::Live, t0 + LAP / 2), first);
        let later = t0 + LAP * 3;
        assert_eq!(laps(first, Socket::Reconnecting { attempt: 1 }, later), Some((later, 1)));
    }

    #[test]
    fn the_orbit_is_still_once_its_laps_are_done() {
        let t0 = Instant::now();
        assert_eq!(turn(Some((t0, 1)), t0), Some(0.0));
        assert!(turn(Some((t0, 1)), t0 + LAP / 2).is_some_and(|t| (t - 0.5).abs() < 1e-3));
        assert_eq!(turn(Some((t0, 1)), t0 + LAP), None);
        assert!(turn(Some((t0, 2)), t0 + LAP).is_some());
        assert_eq!(turn(None, t0), None);
    }

    #[test]
    fn the_catch_up_arc_measures_against_the_widest_gap_until_the_node_is_live() {
        let p = peak(None, Node::CatchingUp(Some(36)));
        assert_eq!(p, Some(36));
        let p = peak(p, Node::CatchingUp(Some(12)));
        assert_eq!(p, Some(36));
        assert_eq!(peak(p, Node::Waiting), Some(36));
        assert_eq!(peak(p, Node::Live), None);
        assert_eq!(progress(Some(12), p), 1.0 - 12.0 / 36.0);
        assert_eq!(progress(Some(36), p), 0.0);
        assert_eq!(progress(None, p), 0.0);
    }

    #[test]
    fn no_live_node_means_no_figure() {
        let waiting = [Some(Node::Waiting), Some(Node::Waiting)];
        assert_eq!(middle(Socket::Connecting { attempt: 1 }, waiting, false, [false; 2], false), Middle::Status(Status::Connecting));
        assert_eq!(middle(Socket::Reconnecting { attempt: 3 }, waiting, true, [true; 2], false), Middle::Status(Status::Reconnecting));
        assert_eq!(middle(Socket::Live, waiting, true, [true; 2], false), Middle::Status(Status::Reconnecting));
        assert_eq!(middle(Socket::Live, waiting, false, [false; 2], false), Middle::Status(Status::Connecting));
    }

    #[test]
    fn a_node_down_behind_a_live_socket_is_named() {
        let live = Socket::Live;
        assert_eq!(
            middle(live, [Some(Node::Live), Some(Node::Offline)], true, [true; 2], false),
            Middle::Figure { dim: true, line: Line::Offline(Chain::Btc, None) },
        );
        assert_eq!(
            middle(live, [Some(Node::Offline), Some(Node::CatchingUp(Some(36)))], true, [true; 2], false),
            Middle::Figure { dim: true, line: Line::Offline(Chain::Xrp, Some(Chain::Btc)) },
        );
        assert_eq!(
            middle(live, [Some(Node::Offline), Some(Node::Live)], false, [false, true], false),
            Middle::Status(Status::Offline(Some(Chain::Xrp))),
        );
        assert_eq!(
            middle(live, [Some(Node::Offline), Some(Node::Offline)], true, [true; 2], false),
            Middle::Status(Status::Offline(None)),
        );
    }

    #[test]
    fn a_live_socket_without_a_total_waits_for_balances_then_prices() {
        let both = [Some(Node::Live), Some(Node::Live)];
        assert_eq!(middle(Socket::Live, both, false, [true, false], false), Middle::Status(Status::Connecting));
        assert_eq!(middle(Socket::Live, both, false, [true; 2], true), Middle::Status(Status::Connecting));
        assert_eq!(middle(Socket::Live, both, false, [true; 2], false), Middle::Status(Status::NoRate));
    }

    #[test]
    fn the_line_names_the_node_that_is_not_live() {
        assert_eq!(
            middle(Socket::Live, [Some(Node::Live), Some(Node::CatchingUp(Some(36)))], true, [true; 2], false),
            Middle::Figure { dim: true, line: Line::CatchingUp(Chain::Btc, Some(36)) },
        );
        assert_eq!(
            middle(Socket::Live, [Some(Node::Live), None], true, [true; 2], false),
            Middle::Figure { dim: false, line: Line::Delta },
        );
        assert_eq!(
            middle(Socket::Connecting { attempt: 0 }, [None, None], true, [true; 2], false),
            Middle::Figure { dim: false, line: Line::Delta },
        );
    }
}
