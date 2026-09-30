//! Balance — the app home (balance v4, 2026-09-11): one screen, one number.
//!
//! The total of both chains in the base currency, in Inter 300 at 104, with
//! the cents dropped to half size, a one-line Mono delta under it, and the
//! guilloché seal behind it. Two corner tools own the application from here
//! (Preferences now; Notices when the bell lands — not drawn until then, a
//! dead corner being exactly the kind of fake control the live-indicator
//! rule forbids). Then the dock. Nothing on this screen is a list, nothing
//! has a press state, and nothing moves.
//!
//! ## What this replaced
//!
//! v3 rev 2: the 44px hero, a 112×24 sparkline day line, two asset rows with
//! the tokens nested under XRP on an elbow, and `N wallets activated` in the
//! footer. All of it is on the XRP and BTC tabs already, in panes that state
//! it better and in more detail than a three-line summary did. Keeping a
//! worse copy on the home screen was the problem: you looked, learned
//! nothing, and clicked through. What Balance kept is the only thing it
//! uniquely has — the sum. The attic has v3 (`_attic/2026-09-11-balance-v4`).
//!
//! ## The engraving is still, and it is the gate's own figure
//!
//! The handoff seeds the guilloché from the account's public address as a
//! visual fingerprint. Dropped (user, 2026-09-11): Balance is the one screen
//! that spans two chains, so it has two addresses and no answer to "which
//! one draws it" — and the check is coarse enough that the designer's own
//! notes call it theatre past a point. So the seal is the gate's figure at
//! a constant phase: one drawing in the app, no `account_seed`, nothing to
//! explain in Preferences.
//!
//! It is drawn ONCE into a canvas [`Cache`] and redrawn only when its inputs
//! change — theme, scale, the no-rate alpha. No subscription, no redraw
//! pump, no self-driven loop: the enterpin gate runs the whole app at 16ms
//! while it is up, the no-wallet orbit redraws itself at 30fps while shown;
//! this screen, the one left open all day, costs what a still image costs.
//! The cache lives in the canvas widget's own tree state, so leaving the tab
//! drops it and returning draws it once more — no app state, no message.
//!
//! The clear behind the digits is cut in the path (`bloom::draw_cleared`),
//! not painted over the lines: iced's canvas gradients are linear only and
//! band at this size, and a lifted pen at a 0.9px hairline has no visible
//! edge.
//!
//! ## Registration
//!
//! The seal is centred on the HERO's rectangle, not the stage and not the
//! window. The stage reserves the delta row in every state — hidden drops the
//! figure, not the space — so the hero, and the seal with it, never move
//! between states. The one thing that shifts the whole stage is the no-rate
//! foot line, which takes its height off the middle.
//!
//! ## The delta has no colour
//!
//! The sign carries the direction. `green` on the light window is 3.86:1 at
//! Mono 12, under the bar for text, and the hue is a palette token, not this
//! screen's to darken — so Balance is the one screen in the app with no hue
//! at all, which suits it: green and red are outcomes, and nothing here is
//! one. Percentage only; a fiat delta beside it is a second figure competing
//! with the hero. It is dropped when the balance is hidden — a percentage is
//! a leak, it says how the hidden number moved.
//!
//! This is a [`CompactPalette`] screen. `green`, `red` and `focus` are unused.

use std::cell::Cell;
use std::hash::{Hash, Hasher};

use iced::mouse;
use iced::widget::canvas::{self, Cache, Canvas, Geometry};
use iced::widget::{column, container, row, stack, text, Space};
use iced::{Alignment, Color, Element, Length, Padding, Point, Rectangle};

use crate::channel::CHANNEL;
use crate::controller::app_state::AppState;
use crate::controller::message::Message;
use crate::ui::components::compact as ck;
use crate::utils::bloom;
use crate::utils::fonts::{LIGHT, MONO};
use crate::utils::money;
use crate::utils::sparkline;
use crate::utils::theme::{self, CompactPalette};
use crate::utils::{icons, price, tokens};

// ── Geometry ────────────────────────────────────────────────────────────────

/// The stage's `padding-bottom` — lifts the hero above true centre so it
/// reads as placed.
const STAGE_PAD_BOTTOM: f32 = 28.0;
/// Hero → delta.
const DELTA_GAP: f32 = 22.0;
/// The delta row's height (Mono 12 at the default line height), reserved in
/// every state so the hero never moves.
const DELTA_H: f32 = 16.0;
/// Between the delta figure and its period word.
const DELTA_SPACING: f32 = 9.0;
/// The foot line (no-rate only): `padding-bottom 15`.
const FOOT_PAD_BOTTOM: f32 = 15.0;
/// The seal: a 540 box centred on the hero.
const SEAL: f32 = 540.0;
/// The clear through the seal, half-axes. The handoff's ellipse is opaque to
/// ±246 × ±57 and fades out to ±300 × ±69; the pen lifts a little inside the
/// fade, and the numeral (±231 × ±52) clears on all four sides.
const CLEAR_RX: f32 = 250.0;
const CLEAR_RY: f32 = 58.0;
/// The seal's phase — the handoff's `data-phase="1.9"`, in radians. Held
/// constant: a seed, not a clock.
const SEAL_PHASE_RAD: f32 = 1.9;

// ── Type ────────────────────────────────────────────────────────────────────
// Inter 300 for the hero — display type, not ticking data. Mono for the rest.

const HERO: f32 = 104.0;
const HERO_CENTS: f32 = 48.0;
const HERO_UNIT: f32 = 11.0;
const HERO_UNIT_GAP: f32 = 15.0;
const HERO_MASK: f32 = 74.0;
const HERO_NA: f32 = 72.0;
/// The unit beside the dash: 17 right, centre-aligned — a dash has no
/// baseline to share.
const HERO_NA_UNIT_GAP: f32 = 17.0;
const DELTA: f32 = 12.0;
const PERIOD: f32 = 9.0;
const FOOT: f32 = 9.5;

const MASK: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";
const NA: &str = "\u{2014}";

// ── View ────────────────────────────────────────────────────────────────────

pub fn view(state: &AppState) -> Element<'_, Message> {
    let p = theme::compact(&state.theme);
    let scale = state.scale();
    let hide = state.hide_balance;
    let ccy = state.base_currency.code();

    let (xrp_amount, xrp_wallet, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
    let (btc_amount, btc_wallet, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
    // No wallet ⟹ the total is a literal 0.00, never a rates read (user,
    // 2026-09-20): there is nothing to price, and without a wallet this client
    // does not even ask for prices. The dash is for a wallet whose prices
    // cannot be had — "something is held and I cannot value it" — which with
    // no wallet is simply false.
    let no_wallet = xrp_wallet.is_none() && btc_wallet.is_none();
    let held: Vec<&tokens::TokenDef> =
        tokens::TOKENS.iter().filter(|t| CHANNEL.token(t.code).1).collect();

    // Fiat in the base. `cross` is 0.0 for an unpriced leg, and one unpriced
    // leg means no total — a figure that silently left something out is a
    // lie in the one number this screen has.
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
    let total: Option<f64> = if no_wallet {
        Some(0.0)
    } else {
        match (xrp_fiat, btc_fiat, tokens_fiat) {
            (Some(x), Some(b), Some(t)) => Some(x + b + t),
            _ => None,
        }
    };

    // The day's change: what is held NOW at each of the last 24 hours'
    // prices (`total_series`) — price-driven, nothing stored. Only with a
    // rate, only once there is a day, never while hidden — and never with no
    // wallet: nothing held has no day.
    let pct: Option<f32> = total.filter(|_| !hide && !no_wallet).and_then(|_| {
        let mut holdings: Vec<(&str, f64)> = vec![("XRP", xrp_amount), ("BTC", btc_amount)];
        holdings.extend(held.iter().map(|t| (t.code, CHANNEL.token(t.code).0)));
        sparkline::change_pct(&sparkline::total_series(&holdings, ccy, &state.rate_history_long))
    });

    let no_rate = total.is_none();
    let alpha = seal_alpha(&state.theme, no_rate);

    // The seal, centred on the hero: the stage is centred in the middle and
    // its hero sits `LIFT` above the stage's centre, so the seal's box is
    // padded up by twice that.
    let lift = (DELTA_GAP + DELTA_H + STAGE_PAD_BOTTOM) / 2.0 * scale;
    let seal_layer = container(
        Canvas::new(Seal::new(p.faint, p.text, alpha, scale))
            .width(Length::Fixed(SEAL * scale))
            .height(Length::Fixed(SEAL * scale)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .padding(Padding::new(0.0).bottom(2.0 * lift))
    .clip(true);

    let stage_layer = container(stage(total, pct, hide, ccy, p, scale))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    let corner_layer = container(corner(p, scale))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::new(0.0).top(ck::CORNER_TOP * scale).left(ck::CORNER_LEFT * scale));

    let mid = stack![seal_layer, stage_layer, corner_layer]
        .width(Length::Fill)
        .height(Length::Fill);

    // The foot line exists in one state: with no rate the native amounts are
    // the only true thing left on the screen.
    let mut screen = column![mid].width(Length::Fill).height(Length::Fill);
    if no_rate {
        let native = if hide {
            format!("{MASK} XRP \u{b7} {MASK} BTC")
        } else {
            format!("{xrp_amount:.6} XRP \u{b7} {btc_amount:.8} BTC")
        };
        screen = screen.push(
            container(text(native).font(MONO).size(FOOT * scale).color(p.muted))
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .padding(Padding::new(0.0).bottom(FOOT_PAD_BOTTOM * scale)),
        );
    }
    screen.into()
}

// ── Stage ───────────────────────────────────────────────────────────────────

/// The hero and, under it, the delta — or `rate unavailable` with no rate, or
/// the reserved space while hidden or before there is a day.
fn stage<'a>(
    total: Option<f64>,
    pct:   Option<f32>,
    hide:  bool,
    ccy:   &'static str,
    p:     &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let unit = |gap: f32| {
        row![
            Space::new().width(gap * scale),
            text(ccy.to_uppercase()).font(MONO).size(HERO_UNIT * scale).color(p.muted),
        ]
    };

    let hero: Element<'a, Message> = match (total, hide) {
        (None, _) => row![
            text(NA).font(LIGHT).size(HERO_NA * scale).line_height(1.0).color(p.faint),
            unit(HERO_NA_UNIT_GAP),
        ]
        .align_y(Alignment::Center)
        .into(),
        (Some(_), true) => row![
            text(MASK).font(LIGHT).size(HERO_MASK * scale).line_height(1.0).color(p.dim),
            unit(HERO_UNIT_GAP),
        ]
        .align_y(Alignment::End)
        .into(),
        (Some(v), false) => {
            // Dollars first: the cents drop to half size in `muted`.
            let s = money(v);
            let (int, cents) = s.split_at(s.len() - 3);
            row![
                text(int.to_string()).font(LIGHT).size(HERO * scale).line_height(1.0).color(p.text),
                text(cents.to_string()).font(LIGHT).size(HERO_CENTS * scale).line_height(1.0).color(p.muted),
                unit(HERO_UNIT_GAP),
            ]
            .align_y(Alignment::End)
            .into()
        }
    };

    let under: Element<'a, Message> = match (total, pct) {
        (None, _) => text("rate unavailable").font(MONO).size(DELTA * scale).color(p.muted).into(),
        (Some(_), Some(pct)) => row![
            text(format!("{pct:+.2}%")).font(MONO).size(DELTA * scale).color(p.text),
            Space::new().width(DELTA_SPACING * scale),
            text("TODAY").font(MONO).size(PERIOD * scale).color(p.muted),
        ]
        .align_y(Alignment::End)
        .into(),
        (Some(_), None) => Space::new().into(),
    };

    column![
        hero,
        Space::new().height(DELTA_GAP * scale),
        container(under).height(Length::Fixed(DELTA_H * scale)).align_y(Alignment::Center),
    ]
    .align_x(Alignment::Center)
    .padding(Padding::new(0.0).bottom(STAGE_PAD_BOTTOM * scale))
    .into()
}

/// The seal's alpha: the handoff's per-theme values, lighter on the no-rate
/// window. Light needs a fraction more ink — dark-on-white loses weight at
/// 0.9px in a way pale-on-dark does not.
fn seal_alpha(t: &iced::Theme, no_rate: bool) -> f32 {
    if no_rate {
        0.11
    } else if theme::is_dark(t) {
        0.15
    } else {
        0.16
    }
}

// ── The seal ────────────────────────────────────────────────────────────────

/// The engraving, drawn once into the widget's own cache. Everything that
/// changes the drawing is folded into `key`; a key the cache was not drawn
/// under clears it, and the next frame draws again — once.
struct Seal {
    edge_in: Color,
    edge_out: Color,
    alpha: f32,
    scale: f32,
    key: u64,
}

impl Seal {
    fn new(edge_in: Color, edge_out: Color, alpha: f32, scale: f32) -> Self {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for c in [edge_in, edge_out] {
            for v in [c.r, c.g, c.b, c.a] {
                v.to_bits().hash(&mut h);
            }
        }
        alpha.to_bits().hash(&mut h);
        scale.to_bits().hash(&mut h);
        Self { edge_in, edge_out, alpha, scale, key: h.finish() }
    }
}

#[derive(Default)]
struct SealState {
    /// The key the cache was last drawn under; `0` = never.
    key: Cell<u64>,
    cache: Cache,
}

impl canvas::Program<Message> for Seal {
    type State = SealState;

    fn draw(
        &self,
        state: &SealState,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<iced::Renderer>> {
        if state.key.get() != self.key {
            state.cache.clear();
            state.key.set(self.key);
        }
        let geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            bloom::draw_cleared(
                frame,
                Point::new(bounds.width / 2.0, bounds.height / 2.0),
                bounds.width.min(bounds.height),
                SEAL_PHASE_RAD / std::f32::consts::TAU * bloom::LOOP_SECS,
                self.edge_in,
                self.edge_out,
                self.alpha,
                Some((CLEAR_RX * self.scale, CLEAR_RY * self.scale)),
            );
        });
        vec![geometry]
    }
}

// ── Corner ──────────────────────────────────────────────────────────────────

/// The Preferences control — [`ck::corner_button`] holding the sliders glyph,
/// the same circle every back control draws.
fn corner<'a>(p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    ck::corner_button(icons::SLIDERS.clone(), Message::OpenSettings, p, scale)
}
