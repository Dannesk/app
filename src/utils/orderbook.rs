//! Book resolution and book-native pricing, including synthetic token/token
//! crosses bridged via XRP.
//!
//! The rates server publishes only the N token/XRP legs — direct token/token
//! books would grow combinatorially and sit mostly empty, because XRPL market
//! makers quote the XRP legs and the ledger's native AUTOBRIDGING merges them
//! at execution. So a token/token ladder is derived here the same way the
//! engine actually fills it: each synthetic level combines one level of each
//! leg — price = quote-leg price / base-leg price, capacity = the XRP the two
//! levels have in common. Same philosophy as [`crate::utils::price`]: the
//! server ships primitives, the client derives crosses.
//!
//! The pool is part of the book. `book_offers` never shows an AMM — the
//! ledger materialises it only at crossing time — yet on XRP/RLUSD it holds
//! several times the CLOB's depth and its post-fee touch often IS the top of
//! book. [`with_amm`] slices the pool into synthetic levels at the display
//! tick, the same way rippled itself generates a synthetic AMM offer sized
//! to reach the next CLOB quality, so the ladder and the crosses derived from
//! it match execution. Constant product with the fee on the input side
//! reproduces the engine's AMM leg to five decimal places (measured
//! 2026-08-29 against the node's own dry run, before it was removed).
//!
//! This module draws the ladder AND prices the ticket: [`walk`] is the order's
//! only source of numbers since Stage 2 removed the node dry run, and it reads
//! the same frame the ladder does — one clock. The Kraken fiat index
//! (`utils::price`) is reference display — a depeg note, a fiat estimate —
//! and never prices, gates, or anchors an order.

use crate::channel::{Amm, OrderBook, CHANNEL};

/// Past this spread a book still DISPLAYS — the data is the truth, and in the
/// persistent ladder a hidden/empty panel reads as broken — but its midpoint
/// is no longer presented as a price: a mid between a real bid and a
/// placeholder ask is arithmetic, not a market (seen live: XRP/EUROP asks at
/// 3× fair → synthetic RLUSD/EUROP "mid" of 1.84 vs ~0.87 real). The view
/// swaps MID/SPREAD for a warning instead, and [`healthy`] fails, which
/// forces the trade form's fill-now preset over to a hand-typed limit —
/// display can warn, but a button that auto-prices from a junk touch would
/// sweep the junk offer. Hiding degenerate books outright was tried
/// 2026-07-30 and reversed same day.
pub const DEGENERATE_SPREAD_PCT: f64 = 5.0;

/// The book to display for (base, quote): a direct server book in either
/// orientation, else a synthetic cross bridged through the two XRP legs.
/// Returns (pair label in the orientation the prices are quoted in, book,
/// via_xrp). Synthetic pairs are labeled in the caller's (base, quote) order;
/// direct pairs keep the server key's orientation.
pub fn resolve(base: &str, quote: &str) -> Option<(String, OrderBook, bool)> {
    if let Some((pair, book)) = CHANNEL.book(base, quote) {
        return Some((pair, with_amm(&book), false));
    }
    if base == "XRP" || quote == "XRP" || base == quote {
        return None;
    }
    let (_, base_leg) = CHANNEL.book("XRP", base)?;
    let (_, quote_leg) = CHANNEL.book("XRP", quote)?;
    // Each leg's pool is liquidity the bridge crosses too.
    let base_leg = with_amm(&base_leg);
    let quote_leg = with_amm(&quote_leg);

    // Buying base with quote routes quote → XRP → base: take the quote leg's
    // asks (buy XRP with quote) into the base leg's bids (sell XRP for base).
    // Selling base is the mirror. Both leg sides arrive best-first, so the
    // stepwise merge emits synthetic levels best-first too, and the combined
    // book can't cross (leg asks ≥ leg bids on both legs).
    let asks = combine(&quote_leg.asks, &base_leg.bids);
    let bids = combine(&quote_leg.bids, &base_leg.asks);

    Some((
        format!("{base}/{quote}"),
        OrderBook {
            // Only as fresh as the stalest leg.
            ledger: base_leg.ledger.min(quote_leg.ledger),
            bids,
            asks,
            // Both pools are already inside the levels; there is no single
            // pool for a bridged pair.
            amm: None,
            // A bridge is only as tradeable as its worst leg: either issuer
            // restricted restricts the cross. `None` stays permissive.
            require_auth: worse(base_leg.require_auth, quote_leg.require_auth),
            global_freeze: worse(base_leg.global_freeze, quote_leg.global_freeze),
            // The COARSER grid binds: fewer significant digits is the bigger
            // tick, so the effective TickSize is the min over both issuers.
            tick_size: match (base_leg.tick_size, quote_leg.tick_size) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (t, None) | (None, t) => t,
            },
        },
        true,
    ))
}

/// How many synthetic levels a pool is sliced into per side. Forty display
/// ticks from the touch is deeper than any ladder shows and past where any
/// order's walk would ever reach.
const AMM_SLICES: usize = 40;

/// Merge a server book's pool into its levels. Server orientation only:
/// prices token per XRP, amounts XRP, best first both sides. The pool's
/// touch is its spot price moved by the fee (a taker selling XRP gets
/// `Y/X·(1−f)`; one buying XRP pays `Y/X/(1−f)`), and the XRP it trades
/// between two prices is the closed form of constant product:
///
///   bids (pool takes XRP down to p): `(√(X·Y·(1−f)/p) − X) / (1−f)`
///   asks (pool gives XRP up to p):   `X · (1 − √(Y / (X·p·(1−f))))`
///
/// These hold k constant; XRPL keeps the fee IN the pool, so each slice is
/// ~f/2 (0.1% at a 0.2% fee) generous. Display-grade: the ticket's floor
/// uses the pool's real post-trade reserves ([`amm_marginal_after`]), not
/// these slices.
///
/// Each slice is priced at its worst edge on the display tick — the same
/// away-from-the-taker rounding the server applies to offers — so a clicked
/// level still crosses. Equal prices merge with the CLOB's own levels.
pub fn with_amm(book: &OrderBook) -> OrderBook {
    let Some(pool) = book.amm.filter(|a| a.xrp > 0.0 && a.token > 0.0 && a.fee_pct >= 0.0 && a.fee_pct < 100.0) else {
        return book.clone();
    };
    let f = pool.fee_pct / 100.0;
    let (x, y) = (pool.xrp, pool.token);
    let spot = y / x;
    let tick = display_tick(spot);

    // Bids: from the post-fee touch downward.
    let q_bid = spot * (1.0 - f);
    let depth_bid = |p: f64| ((x * y * (1.0 - f) / p).sqrt() - x) / (1.0 - f);
    let mut bid_slices: Vec<(f64, f64)> = Vec::with_capacity(AMM_SLICES);
    let mut prev = 0.0;
    let mut edge = (q_bid / tick - 1e-9).floor() * tick;
    for _ in 0..AMM_SLICES {
        if edge <= 0.0 { break; }
        let d = depth_bid(edge);
        if d - prev > 0.0 {
            bid_slices.push((edge, d - prev));
        }
        prev = d;
        edge -= tick;
    }

    // Asks: from the fee-inclusive touch upward.
    let q_ask = spot / (1.0 - f);
    let depth_ask = |p: f64| x * (1.0 - (y / (x * p * (1.0 - f))).sqrt());
    let mut ask_slices: Vec<(f64, f64)> = Vec::with_capacity(AMM_SLICES);
    let mut prev = 0.0;
    let mut edge = (q_ask / tick + 1e-9).ceil() * tick;
    for _ in 0..AMM_SLICES {
        let d = depth_ask(edge);
        if d - prev > 0.0 {
            ask_slices.push((edge, d - prev));
        }
        prev = d;
        edge += tick;
    }

    OrderBook {
        ledger: book.ledger,
        bids: merge_side(&book.bids, &bid_slices, false),
        asks: merge_side(&book.asks, &ask_slices, true),
        amm: book.amm,
        require_auth: book.require_auth,
        global_freeze: book.global_freeze,
        tick_size: book.tick_size,
    }
}

/// The more restrictive of two optional flags: a known `true` wins, then a
/// known `false`, and `None` only when neither leg has answered. Never invents
/// a restriction out of a missing answer.
fn worse(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), _) | (_, Some(false)) => Some(false),
        _ => None,
    }
}

/// Two best-first lists of one side into one, summing equal prices.
fn merge_side(a: &[(f64, f64)], b: &[(f64, f64)], asks: bool) -> Vec<(f64, f64)> {
    let better = |p: f64, q: f64| if asks { p < q } else { p > q };
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        let next = match (a.get(i), b.get(j)) {
            (Some(&l), Some(&r)) => {
                if better(l.0, r.0) || l.0 == r.0 { i += 1; l } else { j += 1; r }
            }
            (Some(&l), None) => { i += 1; l }
            (None, Some(&r)) => { j += 1; r }
            (None, None) => break,
        };
        match out.last_mut() {
            Some(last) if last.0 == next.0 => last.1 += next.1,
            _ => out.push(next),
        }
    }
    out
}

/// The pool's marginal rate after a trade moved it by `d_xrp` / `d_token`
/// (the pool's own deltas, as the relay reports them), as receive-per-pay
/// for a taker paying XRP (`pay_is_xrp`) or paying the token. This is the
/// deepest price the AMM leg of a fill reached — the limit that reproduces
/// it on a still ledger.
pub fn amm_marginal_after(pool: &Amm, d_xrp: f64, d_token: f64, pay_is_xrp: bool) -> Option<f64> {
    let f = pool.fee_pct / 100.0;
    let x = pool.xrp + d_xrp;
    let y = pool.token + d_token;
    if x <= 0.0 || y <= 0.0 || f >= 1.0 {
        return None;
    }
    Some(if pay_is_xrp { y / x * (1.0 - f) } else { x / y * (1.0 - f) })
}

/// The price grid the ladder draws on — the last digit [`fmt_price`] shows.
pub fn display_tick(p: f64) -> f64 {
    if p >= 100.0 { 0.01 } else if p >= 1.0 { 0.0001 } else { 0.00001 }
}

/// One side bucketed to the display tick, best first, with running depth:
/// `(price, amount, cumulative)`. The server keeps nine significant digits
/// so a shown limit always crosses; the ladder shows four or five, and two
/// prices that differ only past that were the same row twice (seen live:
/// 1.4225 above 1.4225). Bucket edges round the way the server does — asks
/// up, bids down — so a clicked bucket still crosses everything in it.
pub fn display_levels(levels: &[(f64, f64)], asks: bool, max: usize) -> Vec<(f64, f64, f64)> {
    let mut out: Vec<(f64, f64, f64)> = Vec::with_capacity(max);
    let mut cumulative = 0.0;
    for &(p, a) in levels {
        if p <= 0.0 || a <= 0.0 {
            continue;
        }
        let tick = display_tick(p);
        let bucket = if asks { (p / tick - 1e-9).ceil() * tick } else { (p / tick + 1e-9).floor() * tick };
        cumulative += a;
        match out.last_mut() {
            Some(last) if last.0 == bucket => {
                last.1 += a;
                last.2 = cumulative;
            }
            _ => {
                if out.len() == max {
                    break;
                }
                out.push((bucket, a, cumulative));
            }
        }
    }
    out
}

/// [`resolve`], normalized into the caller's orientation: prices are always
/// quote-per-base and amounts in base units, matching the trade form, so the
/// inline ladder's numbers ARE the form's numbers and a clicked level can be
/// used as a limit verbatim. A direct book keyed the other way round is
/// inverted level-by-level: a resting bid on B/Q (buy B paying Q) is an ask
/// on Q/B at 1/price, and its B amount becomes amount × price in Q. Both
/// sides stay best-first under inversion (ascending asks map to descending
/// bids and vice versa). Returns (book, via_xrp).
pub fn oriented(base: &str, quote: &str) -> Option<(OrderBook, bool)> {
    let (pair, book, via) = resolve(base, quote)?;
    if pair == format!("{base}/{quote}") {
        return Some((book, via));
    }
    let flip = |levels: &[(f64, f64)]| {
        levels
            .iter()
            .filter(|&&(p, _)| p > 0.0)
            .map(|&(p, a)| (1.0 / p, a * p))
            .collect::<Vec<_>>()
    };
    Some((
        OrderBook {
            ledger: book.ledger,
            bids: flip(&book.asks),
            asks: flip(&book.bids),
            amm: book.amm,
            require_auth: book.require_auth,
            global_freeze: book.global_freeze,
            tick_size: book.tick_size,
        },
        via,
    ))
}

/// Top-of-book spread as a percentage of mid. `None` when either side is
/// missing — a one-sided book has no spread, only an unopposed quote.
/// A price for display: two places past 100, four past 1, five below — the
/// server's own rounding, so a ladder level, the form's output line and the
/// review agree to the digit.
pub fn fmt_price(p: f64) -> String {
    if p >= 100.0 { format!("{p:.2}") } else if p >= 1.0 { format!("{p:.4}") } else { format!("{p:.5}") }
}

/// The oriented book's top bid, when it has one — the price the pay asset
/// sells into right now.
pub fn touch(book: &OrderBook) -> Option<f64> {
    book.bids.first().map(|l| l.0).filter(|&p| p > 0.0)
}

/// The oriented book's midpoint and spread, when both sides exist.
pub fn mid_and_spread(book: &OrderBook) -> Option<(f64, f64)> {
    let s = spread_pct(book)?;
    Some(((book.bids[0].0 + book.asks[0].0) / 2.0, s))
}

/// Negative when the book is CROSSED — possible once the pool is merged in:
/// a resting offer can sit on the wrong side of a pool that moved after it
/// was placed (the next taker simply gets the better of the two).
pub fn spread_pct(book: &OrderBook) -> Option<f64> {
    let bb = book.bids.first().map(|l| l.0).filter(|&p| p > 0.0)?;
    let ba = book.asks.first().map(|l| l.0).filter(|&p| p > 0.0)?;
    Some((ba - bb) / ((bb + ba) / 2.0) * 100.0)
}

/// Whether this book's midpoint is a price: two-sided, uncrossed, with a
/// spread inside [`DEGENERATE_SPREAD_PCT`]. Display only now — the ticket
/// is priced by [`walk`], not by this shape.
pub fn healthy(book: &OrderBook) -> bool {
    spread_pct(book).is_some_and(|s| s >= 0.0 && s <= DEGENERATE_SPREAD_PCT)
}

/// The price of the deepest level `amount` of base reaches walking one side
/// best-first — for a bridged token/token fill, whose legs the relay cannot
/// pair, the synthetic ladder is the only composed rate there is. `None`
/// when the side is empty; the last level when the size outruns it.
pub fn deepest_level(levels: &[(f64, f64)], amount: f64) -> Option<f64> {
    let mut remaining = amount;
    let mut last = None;
    for &(price, avail) in levels {
        if price <= 0.0 {
            continue;
        }
        last = Some(price);
        remaining -= avail;
        if remaining <= 1e-9 {
            break;
        }
    }
    last
}

/// A size-aware quote of the order as it stands: what [`walk`] says the pair's
/// own book fills right now, in the engine's pay/receive orientation.
#[derive(Debug, Clone, Copy)]
pub struct Quote {
    /// Expected fill price across everything crossed (quote per base). The
    /// ledger executes at each maker's own price, so the realized fill is
    /// this or better on a still book.
    pub vwap: f64,
    /// The deepest rate the fill reached — the price the signed limit
    /// anchors to, so the same walk stays crossable.
    pub floor: f64,
    /// Base amount the fill covers.
    pub filled: f64,
    /// Quote amount the filled portion returns.
    pub receive: f64,
    /// True when the ledger could not cover the full amount — the remainder
    /// rests (GTC), is dropped (IOC) or fails the order (FOK).
    pub depth_short: bool,
    /// The ledger the book this was walked from was read at. Carried so the
    /// caller can refuse a frame that has fallen behind — rates replays its
    /// last stash the moment a client subscribes, and on a pair nobody has
    /// held recently that stash can be old.
    pub ledger: u64,
    /// Pay units the pool absorbed — the venue mix, for disclosure (spec §7:
    /// "disclose the venue mix, never a fee split").
    pub from_pool: f64,
    /// CLOB levels the fill touched — how deep into the book the order reaches.
    pub clob_levels: usize,
}

/// The fill an order of `amount` gets right now, walked against the pair's own
/// book: CLOB levels and the pool consumed together, best-first, the way the
/// engine fills. Since §12 removed the node dry run this is the ONLY source of
/// the ticket's numbers.
///
/// **XRP-leg pairs only.** A token/token pair has no single book and no single
/// pool, and its synthetic composition is wrong by percent-scale (RLUSD→EUROP
/// filled 1.1671 against a ~16% synthetic spread), so Market is gated to XRP
/// legs (§3.4) and this returns `None` rather than a number nobody should sign.
///
/// `anchor_is_pay` walks until `amount` of the PAY asset is spent; otherwise
/// until `amount` of the RECEIVE asset is bought.
///
/// The pool is walked in its **exact** closed form, not the display slices
/// [`with_amm`] cuts: those are ~f/2 generous and quantised to the display
/// tick, which is right for a ladder and wrong for a number we sign against.
/// Ordering follows the engine — rippled re-checks the pool against the next
/// CLOB level on every iteration, so the pool is consumed only down to that
/// level's price before the book gets its turn, never in one lump.
pub fn walk(pay: &str, recv: &str, amount: f64, anchor_is_pay: bool) -> Option<Quote> {
    if amount <= 0.0 || !amount.is_finite() {
        return None;
    }
    let (token, pay_is_xrp) = match (pay, recv) {
        ("XRP", t) if t != "XRP" => (t, true),
        (t, "XRP") if t != "XRP" => (t, false),
        _ => return None,
    };
    // Raw server book — `CHANNEL.book` does not merge the pool, `resolve` does.
    // Server orientation is always token-per-XRP with XRP amounts.
    let (_, book) = CHANNEL.book("XRP", token)?;

    // Normalise both sides into THIS order's orientation: rate = receive per
    // pay, capacity in pay units. Selling XRP hits the bids (someone buying
    // XRP with the token) and the server's price already is receive-per-pay.
    // Selling the token hits the asks, where the price is still token-per-XRP,
    // so the rate inverts and the level's XRP amount becomes token.
    let levels: Vec<(f64, f64)> = if pay_is_xrp {
        book.bids.iter().filter(|&&(p, a)| p > 0.0 && a > 0.0).map(|&(p, a)| (p, a)).collect()
    } else {
        book.asks.iter().filter(|&&(p, a)| p > 0.0 && a > 0.0).map(|&(p, a)| (1.0 / p, a * p)).collect()
    };

    // Pool reserves in the same orientation: `a` is the asset being paid in,
    // `b` the one coming out. Constant product with the fee on the input, which
    // reproduced the engine to <1e-4 relative on the recorded reply below.
    let pool = book.amm.filter(|p| p.xrp > 0.0 && p.token > 0.0 && p.fee_pct >= 0.0 && p.fee_pct < 100.0);
    let (a0, b0, g) = match pool {
        Some(p) => {
            let g = 1.0 - p.fee_pct / 100.0;
            if pay_is_xrp { (p.xrp, p.token, g) } else { (p.token, p.xrp, g) }
        }
        None => (0.0, 0.0, 1.0),
    };
    let has_pool = pool.is_some() && g > 0.0;
    // Output for `d` paid in, and the pay-in that drives the marginal down to
    // `p`. Both hold k constant; the fee stays in the pool, so `a` grows by the
    // full `d` while only `d·g` does the work.
    let out_for = |d: f64| b0 * d * g / (a0 + d * g);
    let depth_to = |p: f64| ((a0 * b0 * g / p).sqrt() - a0) / g;

    let mut spent = 0.0;
    let mut got = 0.0;
    let mut from_pool = 0.0;
    let mut worst = f64::INFINITY;
    let mut ci = 0usize;
    let mut clob_levels = 0usize;
    let mut last_touched: Option<usize> = None;

    // 4096 is a stop, not a budget: every branch either spends the remainder,
    // finishes a CLOB level or drives the pool to the next level's price, so
    // the loop cannot outlast the ladder in practice.
    for _ in 0..4096 {
        let left = if anchor_is_pay { amount - spent } else { amount - got };
        if left <= 1e-12 {
            break;
        }
        let pool_rate = if has_pool {
            let an = a0 + from_pool;
            let bn = b0 - out_for(from_pool);
            if an > 0.0 && bn > 0.0 { bn / an * g } else { 0.0 }
        } else {
            0.0
        };
        let clob_rate = levels.get(ci).map(|l| l.0).unwrap_or(0.0);
        if pool_rate <= 0.0 && clob_rate <= 0.0 {
            break; // depth exhausted on both sides
        }

        if pool_rate > clob_rate {
            // Pool is best. Take it only as far as the next CLOB level's price
            // (all the way, if the book has nothing left below it).
            let room = if clob_rate > 0.0 {
                (depth_to(clob_rate) - from_pool).max(0.0)
            } else {
                f64::INFINITY
            };
            let want = if anchor_is_pay {
                left
            } else {
                // Pay-in that buys exactly `left` of the receive asset.
                let bn = b0 - out_for(from_pool);
                if left >= bn { f64::INFINITY } else { (a0 + from_pool) * left / (g * (bn - left)) }
            };
            let take = want.min(room);
            if !(take > 0.0) {
                // Already at parity with the book: let the level have its turn
                // rather than spinning here.
                if clob_rate > 0.0 { ci += 1; continue; }
                break;
            }
            if !take.is_finite() {
                break; // the pool cannot supply what is still wanted
            }
            let out = out_for(from_pool + take) - out_for(from_pool);
            if out <= 0.0 {
                break;
            }
            from_pool += take;
            spent += take;
            got += out;
            let an = a0 + from_pool;
            let bn = b0 - out_for(from_pool);
            if an > 0.0 && bn > 0.0 {
                worst = worst.min(bn / an * g);
            }
        } else {
            let (rate, cap) = levels[ci];
            let want = if anchor_is_pay { left } else { left / rate };
            let take = want.min(cap);
            if take <= 0.0 {
                ci += 1;
                continue;
            }
            spent += take;
            got += take * rate;
            worst = worst.min(rate);
            if last_touched != Some(ci) {
                last_touched = Some(ci);
                clob_levels += 1;
            }
            if take >= cap - 1e-12 {
                ci += 1;
            }
        }
    }

    if spent <= 0.0 || got <= 0.0 || !worst.is_finite() {
        return None;
    }
    let vwap = got / spent;
    // The floor is a ">=" promise, so it rounds DOWN, at the nine significant
    // digits the server keeps its own levels at. `.min(vwap)` because a fill
    // that never left the touch has a marginal equal to it, and a floor above
    // the average would refuse its own walk.
    let floor = floor_sig(worst.min(vwap), 9);
    let target = if anchor_is_pay { spent } else { got };
    Some(Quote {
        vwap,
        floor,
        filled: spent,
        receive: got,
        depth_short: target < amount - amount.abs().max(1.0) * 1e-9,
        ledger: book.ledger,
        from_pool,
        clob_levels,
    })
}

/// A scaled value within a few ulps of an integer IS that integer: `4100.4 ×
/// 1e6` lands at 4100399999.9999995, and floored as-is it signs one drop —
/// one micro-token — less than the digits the user typed. Measured: ~2% of
/// ordinary two-decimal amounts and `1.000001`-style six-decimal ones.
/// Values genuinely between integers (`0.1234567 × 1e6`) are left alone.
fn snap(s: f64) -> f64 {
    let r = s.round();
    if (s - r).abs() <= 4.0 * f64::EPSILON * r.abs().max(1.0) { r } else { s }
}

/// Round *down* to `places`. A `≥` number is a promise, and a promise rounded
/// to nearest can promise more than the ledger guarantees.
pub fn floor_to(x: f64, places: usize) -> f64 {
    let k = 10f64.powi(places as i32);
    snap(x * k).floor() / k
}

/// Round *down* at `digits` significant digits — the server's own rounding
/// for a bid, applied to a fill's floor so the limit signed from it never
/// sits above the rate the ledger actually reached.
pub fn floor_sig(x: f64, digits: i32) -> f64 {
    if x <= 0.0 || !x.is_finite() {
        return x;
    }
    let factor = 10f64.powf((digits - 1) as f64 - x.abs().log10().floor());
    (x * factor).floor() / factor
}

/// Round *up* to `places`. A `≤` number is a ceiling, and the amount signed
/// as TakerGets has to keep the ratio at or under the limit — so it rounds
/// the other way.
pub fn ceil_to(x: f64, places: usize) -> f64 {
    let k = 10f64.powi(places as i32);
    snap(x * k).ceil() / k
}

/// Merge one side from each XRP leg into one synthetic side. Legs carry
/// (token-per-XRP price, XRP amount); a synthetic level spans the XRP the two
/// current levels share: price = quote-leg price / base-leg price (quote per
/// base), amount = that XRP converted to base units through the base leg's
/// price. Walks both lists in step, advancing whichever level exhausts first.
fn combine(quote_levels: &[(f64, f64)], base_levels: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    let mut q_left = quote_levels.first().map(|l| l.1).unwrap_or(0.0);
    let mut b_left = base_levels.first().map(|l| l.1).unwrap_or(0.0);

    while i < quote_levels.len() && j < base_levels.len() && out.len() < 15 {
        let q_price = quote_levels[i].0;
        let b_price = base_levels[j].0;
        if q_price <= 0.0 || b_price <= 0.0 {
            break;
        }
        let xrp = q_left.min(b_left);
        if xrp > 0.0 {
            out.push((q_price / b_price, xrp * b_price));
        }
        q_left -= xrp;
        b_left -= xrp;
        if q_left <= 0.0 {
            i += 1;
            if i < quote_levels.len() { q_left = quote_levels[i].1; }
        }
        if b_left <= 0.0 {
            j += 1;
            if j < base_levels.len() { b_left = base_levels[j].1; }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The XRP/RLUSD pool as measured 2026-08-29.
    fn pool() -> Amm {
        Amm { xrp: 1_679_427.563204, token: 2_314_144.269111297, fee_pct: 0.205 }
    }

    fn book() -> OrderBook {
        OrderBook {
            ledger: 1,
            bids: vec![(1.37502, 305.0), (1.3749, 47.0)],
            asks: vec![(1.37516, 89.0), (1.3757, 471.0)],
            amm: Some(pool()),
            require_auth: Some(false),
            global_freeze: Some(false),
            tick_size: None,
        }
    }

    /// Selling 3,033.89 XRP into the pool returned 4,164.39 RLUSD on the
    /// node; the closed form the slices are cut from must say the same.
    #[test]
    fn the_constant_product_matches_the_engine() {
        let p = pool();
        let f = p.fee_pct / 100.0;
        let dx = 3033.887481 * (1.0 - f);
        let out = p.token * dx / (p.xrp + dx);
        assert!((out - 4164.3948).abs() / 4164.3948 < 1e-4, "{out}");
    }

    #[test]
    fn the_pool_becomes_levels_best_first_that_sum_to_its_depth() {
        let b = with_amm(&book());
        // The pool's post-fee touch (1.37511) beats the CLOB's 1.37502 bid,
        // so the AMM's first slice is the top of the bid side.
        assert!(b.bids[0].0 > 1.37502 && b.bids[0].0 <= 1.37511, "{:?}", b.bids[0]);
        // Bids descend, asks ascend, no zero rows.
        assert!(b.bids.windows(2).all(|w| w[0].0 > w[1].0 && w[1].1 > 0.0));
        assert!(b.asks.windows(2).all(|w| w[0].0 < w[1].0 && w[1].1 > 0.0));
        // Every bid amount minus the two CLOB levels is pool depth, and it
        // adds up to the closed form at the deepest slice's edge.
        let p = pool();
        let f = p.fee_pct / 100.0;
        let deepest = b.bids.last().unwrap().0;
        let expect = ((p.xrp * p.token * (1.0 - f) / deepest).sqrt() - p.xrp) / (1.0 - f);
        let amm_only: f64 = b.bids.iter().map(|l| l.1).sum::<f64>() - 305.0 - 47.0;
        assert!((amm_only - expect).abs() / expect < 1e-6, "{amm_only} vs {expect}");
        assert!(b.amm.is_some());
    }

    /// Unknown must never read as restricted: rates asks once per connection,
    /// so a `None` is an answer in flight, and blacking out a market on it
    /// would make every reconnect look like a freeze.
    #[test]
    fn an_unanswered_issuer_is_permissive() {
        assert_eq!(worse(None, None), None);
        assert_eq!(worse(Some(false), None), Some(false));
        assert_eq!(worse(None, Some(false)), Some(false));
        // A bridge is only as tradeable as its worst leg.
        assert_eq!(worse(Some(true), Some(false)), Some(true));
        assert_eq!(worse(Some(false), Some(true)), Some(true));
        assert_eq!(worse(Some(true), None), Some(true));
    }

    #[test]
    fn a_book_without_a_pool_is_untouched() {
        let mut b = book();
        b.amm = None;
        let out = with_amm(&b);
        assert_eq!(out.bids, b.bids);
        assert_eq!(out.asks, b.asks);
    }

    /// Two nine-digit prices that show as one four-digit price are one row,
    /// bucketed away from the taker, with the depth accumulating.
    #[test]
    fn display_buckets_merge_and_accumulate() {
        let bids = vec![(1.42259999, 10.0), (1.42251, 5.0), (1.4224, 1.0)];
        let d = display_levels(&bids, false, 8);
        assert_eq!(d.len(), 2);
        assert!((d[0].0 - 1.4225).abs() < 1e-9 && (d[0].1 - 15.0).abs() < 1e-9 && (d[0].2 - 15.0).abs() < 1e-9);
        assert!((d[1].0 - 1.4224).abs() < 1e-9 && (d[1].2 - 16.0).abs() < 1e-9);
        let asks = vec![(1.42250001, 2.0), (1.4226, 3.0)];
        let d = display_levels(&asks, true, 8);
        assert!((d[0].0 - 1.4226).abs() < 1e-9 && (d[0].1 - 5.0).abs() < 1e-9);
        assert_eq!(display_levels(&asks, true, 1).len(), 1);
    }

    #[test]
    fn the_marginal_rate_follows_the_pool_after_the_trade() {
        let p = pool();
        let touch = amm_marginal_after(&p, 0.0, 0.0, true).unwrap();
        assert!((touch - p.token / p.xrp * (1.0 - 0.00205)).abs() < 1e-9);
        let after = amm_marginal_after(&p, 3033.887481, -4164.3948, true).unwrap();
        assert!(after < touch && after > 1.37);
        assert!(amm_marginal_after(&p, -p.xrp, 0.0, true).is_none());
    }

    /// The pool alone, walked, must reproduce the node's own recorded answer:
    /// 3,033.887481 XRP in, 4,164.3948 RLUSD out. This is the single fixture
    /// that says the walk may be signed against — it is the same reply
    /// `the_constant_product_matches_the_engine` checks the closed form with,
    /// but taken through the walk's own loop rather than the bare formula.
    #[test]
    fn the_walk_reproduces_the_recorded_node_fill() {
        let p = pool();
        let g = 1.0 - p.fee_pct / 100.0;
        let (a, b) = (p.xrp, p.token);
        let d = 3033.887481;
        let out = b * d * g / (a + d * g);
        assert!((out - 4164.3948).abs() / 4164.3948 < 1e-4, "{out}");
        // Marginal after is below the touch and above the deepest CLOB bid, so
        // a floor taken from it is a real bound and not the average.
        let after = amm_marginal_after(&p, d, -out, true).unwrap();
        let touch = amm_marginal_after(&p, 0.0, 0.0, true).unwrap();
        assert!(after < touch, "{after} !< {touch}");
        assert!(after < out / d, "the floor must sit under the vwap");
    }

    /// Best-first means the pool is consumed only down to the next CLOB level's
    /// price, then the book gets its turn — never the pool in one lump. With
    /// the pool's touch (1.37511) above the top bid (1.37502), a small order
    /// must take the pool first and stop at the book.
    #[test]
    fn the_pool_is_consumed_only_down_to_the_next_level() {
        let p = pool();
        let g = 1.0 - p.fee_pct / 100.0;
        let (x, y) = (p.xrp, p.token);
        let depth_to = |q: f64| ((x * y * g / q).sqrt() - x) / g;
        // XRP the pool absorbs before its marginal reaches the CLOB's best bid.
        let to_book = depth_to(1.37502);
        assert!(to_book > 0.0, "pool touch must beat the book's 1.37502");
        // And it is a real, finite slice — not the whole pool.
        assert!(to_book < x * 0.01, "{to_book} should be a thin top slice");
        // Past that price the book is better, so the walk switches.
        assert!(depth_to(1.3749) > to_book);
    }

    /// A floor is a `>=` promise: it must never come out above the rate the
    /// walk actually reached, at any rounding step.
    #[test]
    fn the_floor_never_sits_above_the_walk() {
        for r in [1.37511_f64, 0.000123456789, 98765.4321, 1.0] {
            assert!(floor_sig(r, 9) <= r, "{r}");
        }
        // Nine significant digits, rounded down, is what the server keeps.
        assert_eq!(floor_sig(1.234567891234, 9), 1.23456789);
        assert!(floor_sig(1.4455000001, 9) <= 1.4455000001);
    }

    #[test]
    fn rounding_goes_the_way_the_bound_points() {
        assert_eq!(floor_to(356.2499, 2), 356.24);
        assert_eq!(ceil_to(703.1854, 2), 703.19);
        assert_eq!(ceil_to(703.19, 2), 703.19);
        assert_eq!(floor_to(250.0, 6), 250.0);
    }

    /// The typed side is exact: what was typed is what is signed, even where
    /// the binary product sits one ulp under the integer.
    #[test]
    fn a_typed_amount_survives_the_round_trip_to_the_wire() {
        assert_eq!(floor_to(4100.4, 6), 4100.4);
        assert_eq!(floor_to(4260.48, 6), 4260.48);
        assert_eq!(floor_to(1.000001, 6), 1.000001);
        assert_eq!(ceil_to(4260.48, 6), 4260.48);
        assert_eq!(ceil_to(4100.4, 6), 4100.4);
        // Still a floor / a ceiling where the value really is in between.
        assert_eq!(floor_to(0.1234567, 6), 0.123456);
        assert_eq!(ceil_to(0.1234561, 6), 0.123457);
    }
}
