//! **Is this market tradeable?** — answered by measurement, never by a list.
//!
//! Decided 2026-09-04 from our own node's books: only XRP/RLUSD is a real
//! market; AUDD is one market maker with a 10k-XRP cliff behind a 0.40%
//! spread; USDC is a thin book beside a 222k-XRP pool; EURØP and XSGD have
//! no market at all (20% or dust spreads, pools under 200 XRP). So **spread is
//! the wrong metric** — a dust bid makes any spread, and a tight one can hide
//! a cliff. The metric is **depth at 1% of mid, per side**: funded CLOB XRP
//! plus what the pool trades for a 1% move, as bookd measures it every ledger
//! ([`crate::channel::MarketSummary`]). Three states from one constant set:
//!
//! | state | rule | today |
//! |---|---|---|
//! | [`Depth::Live`] | ≥ [`LIVE_XRP`] within 1% on BOTH sides | RLUSD, AUDD |
//! | [`Depth::Thin`] | ≥ [`THIN_XRP`] within 5% on both sides | USDC |
//! | [`Depth::None`] | less than that — the words go amber; nothing is withheld | EURØP, XSGD |
//!
//! A cross pair takes its **weakest leg**: XRP is the bridge, so a bridged fill
//! inherits both legs' costs and cannot manufacture liquidity — a dead
//! XRP/EURØP leg makes RLUSD/EURØP dead at any size. A token becomes tradeable
//! by itself the day a maker shows up, and stops the hour it leaves; nothing
//! here is a hardcoded allow-list, and the registry itself only carries tokens
//! with an XRP market to measure.

use crate::channel::{MarketSummary, CHANNEL};

/// XRP within 1% of mid, per side, for a market to read as live.
pub const LIVE_XRP: f64 = 5_000.0;
/// XRP within 5% of mid, per side, for a market to read as thin rather than
/// absent.
pub const THIN_XRP: f64 = 500.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    Live,
    Thin,
    None,
}

impl Depth {
    /// The weaker of two — what a bridged pair inherits.
    pub fn min(self, other: Depth) -> Depth {
        use Depth::*;
        match (self, other) {
            (None, _) | (_, None) => None,
            (Thin, _) | (_, Thin) => Thin,
            _ => Live,
        }
    }
}

/// One market as the picker reads it, in the caller's `(base, quote)`
/// orientation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Market {
    pub depth: Depth,
    /// Quote per base at the top of the book; `0.0` when a side is empty.
    pub mid: f64,
    /// Top-of-book spread, percent of mid. A cross pair reports the two legs
    /// combined, which is what crossing both books costs.
    pub spread_pct: f64,
    /// Roughly how much XRP a taker can move inside 5% — the number a thin
    /// market is described by, since a tag says less than a size.
    pub walkable_xrp: f64,
    /// STABILITY, the second axis (2026-09-15): whether the last ~20
    /// readings held together. A market can be liquid and unstable — one
    /// maker cancelling and re-posting leaves a one-ledger hole every cycle
    /// (XRP/AUDD) — and the hole count says so where the depth cannot. A
    /// cross pair is stable only if both legs are.
    pub stable: bool,
    pub holes: u32,
}

/// Holes in the window a market may carry and still read stable: one is a
/// single missed reading, two is a pattern.
pub const STABLE_HOLES: u32 = 1;

/// The reading the figures are taken from: the window's medians when bookd
/// sent a window, the instantaneous frame otherwise. Every classification
/// and every picker figure reads THIS, so a one-ledger hole moves nothing.
pub fn eff(s: &MarketSummary) -> MarketSummary {
    if s.win_n == 0 {
        return *s;
    }
    MarketSummary {
        mid: s.win_mid,
        spread_pct: s.win_spread_pct,
        bid1: s.win_bid1,
        ask1: s.win_ask1,
        bid5: s.win_bid5,
        ask5: s.win_ask5,
        amm1: s.win_amm1,
        ..*s
    }
}

/// Classify one XRP-leg summary — over its window (see [`eff`]).
pub fn classify(s: &MarketSummary) -> Depth {
    let s = eff(s);
    if s.mid <= 0.0 {
        return Depth::None;
    }
    let (b1, a1) = (s.bid1 + s.amm1, s.ask1 + s.amm1);
    if b1 >= LIVE_XRP && a1 >= LIVE_XRP {
        return Depth::Live;
    }
    let (b5, a5) = (s.bid5 + s.amm1, s.ask5 + s.amm1);
    if b5 >= THIN_XRP && a5 >= THIN_XRP {
        return Depth::Thin;
    }
    Depth::None
}

/// XRP reachable inside 5% on the thinner side, over the window.
pub fn walkable(s: &MarketSummary) -> f64 {
    let s = eff(s);
    (s.bid5 + s.amm1).min(s.ask5 + s.amm1)
}

/// Whether the window held together: at most [`STABLE_HOLES`] holes.
pub fn stable(s: &MarketSummary) -> bool {
    s.holes <= STABLE_HOLES
}

/// bookd's summary of the `XRP/{token}` leg, if it has arrived.
pub fn leg(token: &str) -> Option<MarketSummary> {
    CHANNEL.markets_rx.borrow().get(&format!("XRP/{token}")).copied()
}

/// The market for `(base, quote)`. `None` when a leg's summary has not landed
/// — the picker then draws the row as unmeasured rather than as dead.
pub fn market(base: &str, quote: &str) -> Option<Market> {
    if base == quote {
        return None;
    }
    match (base, quote) {
        ("XRP", t) => {
            let raw = leg(t)?;
            let s = eff(&raw);
            Some(Market {
                depth: classify(&raw),
                mid: s.mid,
                spread_pct: s.spread_pct,
                walkable_xrp: walkable(&raw),
                stable: stable(&raw),
                holes: raw.holes,
            })
        }
        (t, "XRP") => {
            let raw = leg(t)?;
            let s = eff(&raw);
            Some(Market {
                depth: classify(&raw),
                mid: if s.mid > 0.0 { 1.0 / s.mid } else { 0.0 },
                spread_pct: s.spread_pct,
                walkable_xrp: walkable(&raw),
                stable: stable(&raw),
                holes: raw.holes,
            })
        }
        (b, q) => {
            let (rb, rq) = (leg(b)?, leg(q)?);
            let (lb, lq) = (eff(&rb), eff(&rq));
            let mid = if lb.mid > 0.0 && lq.mid > 0.0 { lq.mid / lb.mid } else { 0.0 };
            Some(Market {
                depth: classify(&rb).min(classify(&rq)),
                mid,
                spread_pct: lb.spread_pct + lq.spread_pct,
                walkable_xrp: walkable(&rb).min(walkable(&rq)),
                stable: stable(&rb) && stable(&rq),
                holes: rb.holes.max(rq.holes),
            })
        }
    }
}

/// The market in words, on its two axes, over its last ~20 ledgers:
/// liquidity (`liquid` / `thin` / `illiquid`, the depth classification) and
/// stability (`stable` / `unstable`, the hole count) — `liquid · unstable`
/// for AUDD's one maker leaving a hole each cycle, `illiquid · stable` for a
/// dust book that at least holds still. The ONE vocabulary the picker and
/// the ticket's `Book` row share (user, 2026-09-15): words anyone can read,
/// not a percentage and a size to guess from — those live on the book and
/// depth panes. `measuring…` until bookd's summary has landed.
pub fn label(base: &str, quote: &str) -> String {
    let Some(m) = market(base, quote) else { return "measuring\u{2026}".to_string() };
    let depth = match m.depth {
        Depth::Live => "liquid",
        Depth::Thin => "thin",
        Depth::None => "illiquid",
    };
    format!("{} \u{b7} {}", depth, if m.stable { "stable" } else { "unstable" })
}

/// The market's one-bit verdict for an ink: `Some(true)` is liquid AND
/// stable — the market worth the name, and the only one that earns green
/// (user, 2026-09-15) — `Some(false)` anything less, `None` not yet
/// measured. The pair pane's rows and the ticket's `Book` row read the same
/// bit, so the two can never disagree on which markets are green.
///
/// This is all the classification decides now. It used to gate the Market
/// contract (`tradeable`, struck 2026-09-16): a windowed statistic standing
/// in front of a walk that measures the user's exact size. The rule is
/// *warn, never decide* — the words go amber, the button stays theirs.
pub fn healthy(base: &str, quote: &str) -> Option<bool> {
    market(base, quote).map(|m| m.depth == Depth::Live && m.stable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(bid1: f64, ask1: f64, bid5: f64, ask5: f64, amm1: f64) -> MarketSummary {
        MarketSummary { ledger: 1, mid: 1.42, spread_pct: 0.11, bid1, ask1, bid5, ask5, amm_xrp: 0.0, amm1, ..Default::default() }
    }

    /// A hole in the instantaneous frame does not move a classification
    /// taken over the window: the AUDD cycle, one ledger of junk-only asks.
    #[test]
    fn a_one_ledger_hole_does_not_demote_a_windowed_market() {
        let mut hole = s(0.0, 0.0, 0.0, 0.0, 5.8);
        hole.mid = 21.0;
        hole.win_n = 20;
        hole.win_mid = 2.005;
        hole.win_bid1 = 9990.0;
        hole.win_ask1 = 13480.0;
        hole.win_bid5 = 9990.0;
        hole.win_ask5 = 13480.0;
        hole.win_amm1 = 5.8;
        hole.holes = 2;
        assert_eq!(classify(&hole), Depth::Live);
        assert!(!stable(&hole), "two holes in twenty is a pattern");
        hole.holes = 1;
        assert!(stable(&hole));
        // No window (old bookd, first frame): the frame speaks for itself.
        hole.win_n = 0;
        assert_eq!(classify(&hole), Depth::None);
    }

    /// The 2026-09-04 measurements, as recorded, land in the states the
    /// decision was made on.
    #[test]
    fn the_measured_books_classify_as_decided() {
        // XRP/RLUSD: 42k / 41k within 1%, an 8.2k-per-1% pool.
        assert_eq!(classify(&s(42_000.0, 41_000.0, 72_000.0, 99_000.0, 8_200.0)), Depth::Live);
        // XRP/AUDD: 10.0k / 13.4k within 1%, one maker — live today, a cliff behind it.
        assert_eq!(classify(&s(10_000.0, 13_400.0, 10_000.0, 13_400.0, 5.0)), Depth::Live);
        // XRP/USDC: 2.1k / 2.7k CLOB but a 1.1k-per-1% pool — thin.
        assert_eq!(classify(&s(2_100.0, 2_700.0, 2_400.0, 8_400.0, 1_100.0)), Depth::Thin);
        // XRP/EURØP: nothing inside 5%, a 1-XRP pool.
        assert_eq!(classify(&s(0.0, 0.0, 0.0, 0.0, 1.0)), Depth::None);
        // A tight spread with nothing behind it is not a market.
        let mut dust = s(0.0, 0.0, 0.0, 0.0, 0.0);
        dust.spread_pct = 0.05;
        assert_eq!(classify(&dust), Depth::None);
        // No mid, no market — whatever the depth fields claim.
        let mut one_sided = s(50_000.0, 50_000.0, 50_000.0, 50_000.0, 0.0);
        one_sided.mid = 0.0;
        assert_eq!(classify(&one_sided), Depth::None);
    }

    /// A pool alone can carry a market: the pool's 1% capacity counts on
    /// both sides, because constant product is symmetric at that scale.
    #[test]
    fn a_pool_alone_can_be_live() {
        assert_eq!(classify(&s(0.0, 0.0, 0.0, 0.0, 6_000.0)), Depth::Live);
        assert_eq!(classify(&s(0.0, 0.0, 0.0, 0.0, 600.0)), Depth::Thin);
    }

    /// A bridge is only as deep as its shallower leg.
    #[test]
    fn a_cross_takes_the_weakest_leg() {
        assert_eq!(Depth::Live.min(Depth::Thin), Depth::Thin);
        assert_eq!(Depth::Live.min(Depth::None), Depth::None);
        assert_eq!(Depth::Thin.min(Depth::None), Depth::None);
        assert_eq!(Depth::Live.min(Depth::Live), Depth::Live);
    }
}
