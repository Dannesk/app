//! The XRP trade screen — **one screen plus the sign stack** (trade v3,
//! 2026-09-04, from the `design_handoff_trade_step1` mock and its pair-picker
//! companion).
//!
//! Three cardless panes divided by hairlines, centred over the dock: a **view
//! pane** on each side of the **ticket**. The view panes carry the same three
//! tabs — `book · depth · chart` — and each shows whichever one is clicked,
//! the other pane untouched (two depths is a choice, not a mistake). The
//! ticket is the order: pair
//! pill, Market/Limit, Buy/Sell, the fields, the stats, and the verb.
//!
//! **There is no review step.** The ticket's stats are the review, they are
//! live, and they sit behind the scrim when the sign stack rises — the same
//! stack send and token-enable sign on ([`send_screen::sign_stack_with`]), with
//! one lead line saying what is being authorised and, under Market, the bound
//! the order is signed at (spec §8). The pair picker is the other stack:
//! search-first, one flat list of markets, dismissed by scrim, back or `Esc`.
//!
//! Everything here draws from [`CompactPalette`] — no `AppPalette`, no brand
//! blue. The only colour is the ask/bid pair and the amber a thin market
//! carries. This is a data screen: it sits on the compressed type scale
//! (8.5–12.5) the mock sets, below the app's usual floor, on purpose.
//!
//! ## What the mock says that our logic overrides
//!
//! The mock's geometry, type and colour are followed. Its liquidity rules are
//! not: "illiquid by spread" and the `XRP / EURC` row were struck on
//! 2026-09-04 in favour of measured depth ([`crate::utils::liquidity`]), its
//! Market order on a cross pair contradicts spec §3.4 (Market is XRP-leg
//! only; a cross pair is Limit-only, so the Market segment is absent there),
//! and its `LastLedgerSequence · current + 3` is not what we sign
//! (`LAST_LEDGER_OFFSET` is 20, flat, every TIF).

pub mod depth;
pub mod picker;
pub mod ticket;

use iced::Color;

use crate::controller::app_state::AppState;
use crate::utils::tokens;

// ── Type scale (the mock's compressed scale) ────────────────────────────────

/// Eyebrows, column headers, pane tabs, footer keys (mono upper).
pub const EYEBROW: f32 = 8.5;
/// Book rows, stat values, footer values (mono).
pub const ROW: f32 = 10.5;
/// The mid value (mono).
pub const MID: f32 = 11.0;
/// Field values (mono).
pub const FIELD: f32 = 12.5;
/// Field labels and stat keys (Inter).
pub const LABEL: f32 = 10.5;
/// The pair pill (mono).
pub const PILL: f32 = 10.0;
/// Units (mono upper).
pub const UNIT: f32 = 9.0;
/// The depth note (Inter, line 1.5).
pub const NOTE: f32 = 9.5;

// ── The washes ──────────────────────────────────────────────────────────────
// The dark red/green at low alpha in ALL THREE themes — the mock never
// re-toned them per theme (handoff §Palettes), so they are not palette
// tokens; `theme.rs` is frozen and these are the handoff's own numbers.

const ASK_HUE: Color = Color { r: 224.0 / 255.0, g: 105.0 / 255.0, b: 107.0 / 255.0, a: 1.0 };
const BID_HUE: Color = Color { r: 95.0 / 255.0, g: 192.0 / 255.0, b: 138.0 / 255.0, a: 1.0 };

/// Depth bars and depth-chart fills.
pub const ASK_WASH: Color = Color { a: 0.13, ..ASK_HUE };
pub const BID_WASH: Color = Color { a: 0.13, ..BID_HUE };
/// The active Buy / Sell segment.
pub const BUY_FILL: Color = Color { a: 0.12, ..BID_HUE };
pub const BUY_BORDER: Color = Color { a: 0.42, ..BID_HUE };
pub const SELL_FILL: Color = Color { a: 0.12, ..ASK_HUE };
pub const SELL_BORDER: Color = Color { a: 0.42, ..ASK_HUE };

/// Display name for an asset code (XRP, or a token's display form like EURØP).
pub fn disp(code: &str) -> &str {
    if code == "XRP" { "XRP" } else { tokens::by_code(code).map(|t| t.display).unwrap_or(code) }
}

/// Whether the ticket is under Limit — the user's click and nothing else
/// (2026-09-15). The book never converts a Market ticket into a Limit one;
/// it can only dim Market and darken the button. Every pane reads this, so
/// the book's clickability, the depth note and the ticket's fields agree.
pub fn manual(state: &AppState) -> bool {
    crate::controller::xrp::trade_is_limit(state)
}
