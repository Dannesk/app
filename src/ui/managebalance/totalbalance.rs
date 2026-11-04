use iced::Widget as _;
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use iced::mouse;
use iced::widget::canvas::{self, Cache, Canvas, Event, Geometry};
use iced::widget::text::{Span, Wrapping};
use iced::widget::{column, container, rich_text, row, span, stack, text, Space};
use iced::{window, Alignment, Color, Element, Length, Padding, Point, Rectangle};

use crate::channel::{Socket, CHANNEL};
use crate::controller::app_state::AppState;
use crate::controller::message::Message;
use crate::ui::components::compact as ck;
use crate::ui::components::orbit::{self, Chain, Line, Middle, Node, Status};
use crate::ui::components::tui::MONO_ADVANCE;
use crate::utils::fonts::{LIGHT, MONO, SANS};
use crate::utils::sparkline;
use crate::utils::theme::{self, CompactPalette};
use crate::utils::{add_commas, icons, money, price, tokens};

const RING_R: f32 = 210.0;
const RING_PAD: f32 = 24.0;
const MARK_W: f32 = 11.0;
const MARK_GAP: f32 = 8.0;
const RING_LIFT: f32 = 10.0;
const TRACK_ALPHA: f32 = 0.32;
const TRACK_DASH: f32 = 2.0;
const TRACK_PITCH: f32 = 8.0;
const ARC_W: f32 = 1.7;
const ARC_DEG: f32 = 20.0;
const CATCH_DEG: f32 = 150.0;
const HOMES: [f32; 2] = [315.0, 135.0];

const HERO_SIZES: [f32; 3] = [80.0, 60.0, 48.0];
const HERO_FIT: f32 = 0.84;
const CENTS: f32 = 0.46;
const DIGIT_EM: f32 = 0.573; // mean Inter Light digit advance
const MARK_EM: f32 = 0.267; // Inter Light comma and period advance
const SANS_BASE: f32 = 0.136; // Inter baseline above the bottom of a 1.0 line box
const MONO_BASE: f32 = 0.14; // JetBrains Mono baseline above the bottom of a 1.0 line box
const UNIT: f32 = 11.0;
const UNIT_GAP: f32 = 12.0;
const HERO_MASK: f32 = 57.0;
const LINE_GAP: f32 = 20.0;
const LINE: f32 = 13.0;
const LINE_H: f32 = 16.0;
const COUNT: f32 = 12.0;
const DELTA: f32 = 12.0;
const PERIOD: f32 = 9.0;
const DELTA_SPACING: f32 = 9.0;
const STATUS: f32 = 28.0;
const STATUS_H: f32 = 34.0;
const FOOT: f32 = 9.5;
const FOOT_PAD_BOTTOM: f32 = 15.0;

const MASK: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

fn hero_size(int: &str, cents: &str, ccy: &str) -> f32 {
    let em = |s: &str| s.chars().map(|c| if c.is_ascii_digit() { DIGIT_EM } else { MARK_EM }).sum::<f32>();
    let unit = UNIT_GAP + ccy.chars().count() as f32 * UNIT * MONO_ADVANCE;
    HERO_SIZES
        .into_iter()
        .find(|s| em(int) * s + em(cents) * s * CENTS + unit <= HERO_FIT * 2.0 * RING_R)
        .unwrap_or(HERO_SIZES[HERO_SIZES.len() - 1])
}

pub fn view(state: &AppState) -> Element<'_, Message> {
    let p = theme::compact(&state.theme);
    let scale = state.scale();
    let hide = state.hide_balance;
    let ccy = state.base_currency.code();

    let (xrp_amount, xrp_wallet, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
    let (btc_amount, btc_wallet, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
    let no_wallet = xrp_wallet.is_none() && btc_wallet.is_none();
    let held: Vec<&tokens::TokenDef> =
        tokens::TOKENS.iter().filter(|t| CHANNEL.token(t.code).1).collect();

    let xrp_rate = price::cross("XRP", ccy);
    let btc_rate = price::cross("BTC", ccy);
    let xrp_fiat = (xrp_rate > 0.0).then(|| xrp_amount * xrp_rate);
    let btc_fiat = (btc_rate > 0.0).then(|| btc_amount * btc_rate);
    let tokens_fiat: Option<f64> = held.iter().try_fold(0.0, |acc, t| {
        let bal = CHANNEL.token(t.code).0;
        if bal <= 0.0 {
            return Some(acc);
        }
        let r = price::cross(t.code, ccy);
        (r > 0.0).then(|| acc + bal * r)
    });

    let loaded = *CHANNEL.loaded_rx.borrow();
    let ready = [
        loaded.wallets && (xrp_wallet.is_none() || loaded.xrp),
        loaded.wallets && (btc_wallet.is_none() || loaded.btc),
    ];
    let balances_in = ready.iter().all(|r| *r);
    let total: Option<f64> = if !balances_in {
        None
    } else if no_wallet {
        Some(0.0)
    } else {
        match (xrp_fiat, btc_fiat, tokens_fiat) {
            (Some(x), Some(b), Some(t)) => Some(x + b + t),
            _ => None,
        }
    };

    let socket = orbit::socket();
    let live = socket == Socket::Live;
    let owned = [xrp_wallet.is_some(), btc_wallet.is_some()];
    let every = Chain::ALL.map(|c| orbit::node(c, live));
    let nodes = [owned[0].then_some(every[0]), owned[1].then_some(every[1])];
    let middle = orbit::middle(socket, nodes, total.is_some(), ready, orbit::rates_pending());

    if !no_wallet {
        crate::startup_trace::stamp(match middle {
            Middle::Figure { .. } => "balance shows a total",
            Middle::Status(Status::Connecting) => "balance shows connecting",
            Middle::Status(_) => "balance shows a status",
        });
    }

    let pct: Option<f32> = total.filter(|_| !hide && !no_wallet).and_then(|_| {
        let mut holdings: Vec<(&str, f64)> = vec![("XRP", xrp_amount), ("BTC", btc_amount)];
        holdings.extend(held.iter().map(|t| (t.code, CHANNEL.token(t.code).0)));
        sparkline::change_pct(&sparkline::total_series(&holdings, ccy, &state.rate_history_long))
    });

    let arcs = Chain::ALL.map(|c| {
        let n = every[c as usize];
        Arc {
            chain: c,
            color: n.color(p),
            behind: match n {
                Node::CatchingUp(behind) => behind,
                _ => None,
            },
            ink: if owned[c as usize] { p.dim } else { p.faint },
        }
    });
    let ring_box = Length::Fixed((RING_R + RING_PAD) * 2.0 * scale);
    let ring = Orbit {
        arcs,
        peaks: [None, state.btc_behind_peak],
        laps: state.socket_laps,
        track: Color { a: TRACK_ALPHA, ..p.faint },
        dim: p.dim,
        alpha: orbit::arc_alpha(&state.theme),
        scale,
    };
    let centred = |content: Element<'static, Message>| {
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .padding(Padding::new(0.0).bottom(2.0 * RING_LIFT * scale))
            .clip(true)
    };

    let core: Element<'static, Message> = match middle {
        Middle::Figure { dim, line } => column![
            Space::new().height((LINE_GAP + LINE_H) * scale),
            hero(total.unwrap_or(0.0), dim, hide, ccy, p, scale),
            Space::new().height(LINE_GAP * scale),
            container(under(line, pct, p, scale))
                .height(Length::Fixed(LINE_H * scale))
                .align_y(Alignment::Center),
        ]
        .align_x(Alignment::Center)
        .boxed(),
        Middle::Status(status) => text(status.words())
            .font(LIGHT)
            .size(STATUS * scale)
            .line_height(STATUS_H / STATUS)
            .color(if status.strong() { p.text } else { p.dim })
            .boxed(),
    };

    let foot = balances_in && matches!(middle, Middle::Status(s) if s != Status::Connecting);
    let foot: Element<'static, Message> = if !foot {
        Space::new().boxed()
    } else {
        let native = if hide {
            format!("{MASK} XRP \u{b7} {MASK} BTC")
        } else {
            format!("{xrp_amount:.6} XRP \u{b7} {btc_amount:.8} BTC")
        };
        text(native).font(MONO).size(FOOT * scale).color(p.muted).boxed()
    };

    stack![
        centred(Canvas::new(ring).width(ring_box).height(ring_box).boxed()),
        centred(core),
        container(foot)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::End)
            .padding(Padding::new(0.0).bottom(FOOT_PAD_BOTTOM * scale)),
        container(ck::corner_button(icons::SLIDERS.clone(), Message::OpenSettings, p, scale))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::new(0.0).top(ck::CORNER_TOP * scale).left(ck::CORNER_LEFT * scale)),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

fn hero(
    total: f64,
    dim: bool,
    hide: bool,
    ccy: &'static str,
    p: &'static CompactPalette,
    scale: f32,
) -> Element<'static, Message> {
    let unit = |size: f32| {
        container(text(ccy.to_uppercase()).font(MONO).size(UNIT * scale).line_height(1.0).color(p.muted))
            .padding(Padding::new(0.0).bottom((SANS_BASE * size - MONO_BASE * UNIT) * scale))
    };
    if hide {
        return row![
            text(MASK).font(LIGHT).size(HERO_MASK * scale).line_height(1.0).color(p.dim),
            Space::new().width(UNIT_GAP * scale),
            unit(HERO_MASK),
        ]
        .align_y(Alignment::End)
        .boxed();
    }
    let s = money(total);
    let (int, cents) = s.split_at(s.len() - 3);
    let size = hero_size(int, cents, ccy);
    row![
        text(int.to_string())
            .font(LIGHT)
            .size(size * scale)
            .line_height(1.0)
            .color(if dim { p.dim } else { p.text }),
        container(text(cents.to_string()).font(LIGHT).size(size * CENTS * scale).line_height(1.0).color(p.muted))
            .padding(Padding::new(0.0).bottom(SANS_BASE * size * (1.0 - CENTS) * scale)),
        Space::new().width(UNIT_GAP * scale),
        unit(size),
    ]
    .align_y(Alignment::End)
    .boxed()
}

fn under(line: Line, pct: Option<f32>, p: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let name = |c: Chain| -> Span<'static, ()> { span(c.name()).font(SANS).size(LINE * scale).color(p.text) };
    let said = |s: &'static str| -> Span<'static, ()> { span(s).font(SANS).size(LINE * scale).color(p.dim) };
    let spans: Vec<Span<'static, ()>> = match line {
        Line::Delta => {
            return match pct {
                Some(pct) => row![
                    text(format!("{pct:+.2}%")).font(MONO).size(DELTA * scale).color(p.text),
                    Space::new().width(DELTA_SPACING * scale),
                    text("TODAY").font(MONO).size(PERIOD * scale).color(p.muted),
                ]
                .align_y(Alignment::End)
                .boxed(),
                None => Space::new().boxed(),
            };
        }
        Line::Offline(c, None) => vec![name(c), said(" node offline")],
        Line::Offline(c, Some(d)) => vec![name(c), said(" node offline \u{b7} "), name(d), said(" catching up")],
        Line::Connecting(c) => vec![said("Connecting to "), name(c), said(" node\u{2026}")],
        Line::NoPeers => vec![name(Chain::Btc), said(" node has no peers")],
        Line::CatchingUp(c, None) => vec![name(c), said(" node catching up")],
        Line::CatchingUp(c, Some(behind)) => vec![
            name(c),
            said(" node catching up \u{b7} "),
            span(add_commas(behind as i64)).font(MONO).size(COUNT * scale).color(p.text),
            said(c.behind()),
        ],
    };
    rich_text(spans).wrapping(Wrapping::None).boxed()
}

#[derive(Clone, Copy)]
struct Arc {
    chain: Chain,
    color: Color,
    behind: Option<u64>,
    ink: Color,
}

struct Orbit {
    arcs: [Arc; 2],
    peaks: [Option<u64>; 2],
    laps: Option<(Instant, u32)>,
    track: Color,
    dim: Color,
    alpha: f32,
    scale: f32,
}

#[derive(Default)]
struct OrbitMemory {
    now: Option<Instant>,
    key: Cell<u64>,
    track: Cache,
}

impl canvas::Program<Message> for Orbit {
    type State = OrbitMemory;

    fn update(
        &self,
        state: &mut OrbitMemory,
        event: &Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let Event::Window(window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        state.now = Some(*now);
        orbit::turn(self.laps, *now).map(|_| canvas::Action::request_redraw())
    }

    fn draw(
        &self,
        state: &OrbitMemory,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<iced::Renderer>> {
        let c = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let r = RING_R * self.scale;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for v in [self.track.r, self.track.g, self.track.b, self.track.a, self.scale] {
            v.to_bits().hash(&mut h);
        }
        if state.key.get() != h.finish() {
            state.track.clear();
            state.key.set(h.finish());
        }
        let track = state.track.draw(renderer, bounds.size(), |frame| {
            orbit::track(frame, c, r, TRACK_DASH * self.scale, TRACK_PITCH * self.scale, self.track);
        });

        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let lap = orbit::turn(self.laps, state.now.unwrap_or_else(Instant::now));
        for ((arc, home), peak) in self.arcs.iter().zip(HOMES).zip(self.peaks) {
            let (color, sweep, spin) = match lap {
                Some(t) => (self.dim, ARC_DEG, 360.0 * t),
                None => (arc.color, ARC_DEG + CATCH_DEG * orbit::progress(arc.behind, peak), 0.0),
            };
            let start = (home - ARC_DEG / 2.0 + spin - 90.0).to_radians();
            orbit::arc(
                &mut frame,
                c,
                r,
                start,
                sweep.to_radians(),
                Color { a: self.alpha, ..color },
                ARC_W * self.scale,
            );
            let at = (home + spin - 90.0).to_radians();
            let reach = r + (MARK_GAP + MARK_W / 2.0) * self.scale;
            orbit::mark(
                &mut frame,
                arc.chain,
                Point::new(c.x + reach * at.cos(), c.y + reach * at.sin()),
                MARK_W * self.scale,
                arc.ink,
            );
        }
        vec![track, frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_totals_step_down_to_stay_inside_the_ring() {
        assert_eq!(hero_size("17,943", ".53", "usd"), 80.0);
        assert_eq!(hero_size("1,284,630", ".07", "usd"), 60.0);
        assert_eq!(hero_size("400,000,000", ".00", "vnd"), 48.0);
    }
}
