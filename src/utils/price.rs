//! Central price lookups over the rates channel.
//!
//! The rates server publishes ONE USD price per asset (USD-numeraire model):
//! `CHANNEL.rates_rx` is keyed by asset code (`"XRP"`, `"BTC"`, `"EUR"`,
//! `"SGD"`, `"AUD"`, `"USDC"`), each mapping to that asset's USD price. Every
//! cross A/B is derived here as `usd(A) / usd(B)` — no pair matrix is ever sent
//! or stored, so this scales to any number of tokens.
//!
//! All rate consumers go through [`cross`] / [`usd`]; nothing reads pair keys
//! like `"XRP/USD"` directly anymore.

use crate::channel::CHANNEL;
use crate::utils::tokens::rate_key;

/// USD price of one unit of `asset`. Accepts a token code or an asset code
/// (`"RLUSD"` → USD, `"XRP"` → XRP, …). Returns 1.0 for USD and 0.0 when the
/// asset isn't priced yet.
pub fn usd(asset: &str) -> f64 {
    let key = rate_key(asset);
    if key == "USD" {
        return 1.0;
    }
    CHANNEL.rates_rx.borrow().get(key).copied().unwrap_or(0.0) as f64
}

/// Cross rate `base/quote` = `usd(base) / usd(quote)`. Accepts token or asset
/// codes. Returns 0.0 if either leg is unpriced (same sentinel the old
/// pair-keyed lookups used).
pub fn cross(base: &str, quote: &str) -> f64 {
    let b = usd(base);
    let q = usd(quote);
    if b > 0.0 && q > 0.0 {
        b / q
    } else {
        0.0
    }
}
