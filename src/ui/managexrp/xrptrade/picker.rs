//! The **pair picker** — search-first: the pair is the input.
//!
//! The sign stack's panel at the asset picker's width (384), the asset
//! picker's row geometry, and one flat list of **markets** — never a list of
//! assets with slots, because a ticket with a pair on top and pickers for its
//! halves is two models wearing one card. Resting, the list is `your pairs`:
//! the markets makeable from what is held, at most four rows, no scrollbar.
//! Typing reaches every market on the ledger the registry knows: a symbol
//! (`rlusd` — every market touching RLUSD, as whole pairs), pair syntax
//! (`xrp/rl`, `xrp rl`), or an issuer name (`circle`). Issuer addresses are
//! deliberately not matched.
//!
//! ## Tradeable is measured, never a list
//!
//! Every row's right column comes from [`crate::utils::liquidity`] — depth at
//! 1% of mid, funded book plus pool — not from the mock's spread rule (struck
//! 2026-09-04: a dust bid makes any spread, and a tight spread can hide a
//! cliff). Every measured row reads the same line — `spread · ~size XRP`,
//! the size being what a taker can move inside 5% of mid on the thinner
//! side — in `muted` when live and `amber` below that. Every row is
//! pickable (2026-09-15), and since 2026-09-16 depth gates nothing at all:
//! Market is offered on every XRP-leg pair and the ticket's `Market` row
//! prints what the walk finds for the size — a Limit order is the user's
//! price, and a resting order on an empty book is the only way a book
//! fills. `NO LIQUIDITY` is a book with no mid, a
//! side literally empty. A cross pair takes its weakest leg: the ledger
//! bridges token / token through XRP itself, so a cross is as liquid as the
//! shallower of its XRP legs and no more. Legs not yet measured read
//! `measuring…`.
//!
//! Order never moves under the cursor: held-first, then registry order.
//! Depth ticks every ledger and is never a sort key.
//!
//! Picking resolves the screen — no confirm. Dismiss: scrim, back, `Esc`
//! (which clears a typed query first). The pill that opened it stays lit.


use crate::channel::CHANNEL;
use crate::utils::tokens;


/// Rows the resting list shows.
const RESTING_ROWS: usize = 4;

/// Every market the registry can make: `XRP / token` for each token, then
/// token / token for each unordered pair — bridged through XRP by the ledger.
pub fn all_markets() -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = tokens::TOKENS.iter().map(|t| ("XRP", t.code)).collect();
    for (i, a) in tokens::TOKENS.iter().enumerate() {
        for b in &tokens::TOKENS[i + 1..] {
            out.push((a.code, b.code));
        }
    }
    out
}

/// Whether the wallet holds any of an asset — the registry's own figure,
/// total balance, no reserve arithmetic.
fn held(code: &str) -> bool {
    if code == "XRP" {
        CHANNEL.wallet_balance_rx.borrow().0 > 0.0
    } else {
        CHANNEL.token(code).0 > 0.0
    }
}

fn names(code: &str) -> Vec<String> {
    let mut v = vec![code.to_lowercase()];
    if let Some(t) = tokens::by_code(code) {
        v.push(t.display.to_lowercase());
        v.push(t.ticker.to_lowercase());
    }
    v
}

fn issuer_name(code: &str) -> String {
    tokens::by_code(code).map(|t| t.issuer_name.to_lowercase()).unwrap_or_default()
}

/// Does `(base, quote)` answer the query? Three matchers, all built:
/// symbol, pair syntax, issuer name.
pub fn matches(base: &str, quote: &str, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    let parts: Vec<&str> = q.split(|c| c == '/' || c == ' ').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 2 {
        let starts = |code: &str, p: &str| names(code).iter().any(|n| n.starts_with(p));
        return (starts(base, parts[0]) && starts(quote, parts[1]))
            || (starts(base, parts[1]) && starts(quote, parts[0]));
    }
    let sym = |code: &str| names(code).iter().any(|n| n.contains(&q));
    sym(base) || sym(quote) || issuer_name(base).contains(&q) || issuer_name(quote).contains(&q)
}

/// The rows to show: resting, the held pairs (capped); typing, every match,
/// held first. Both in registry order otherwise.
pub(crate) fn listing(query: &str) -> (Vec<(&'static str, &'static str)>, bool) {
    let all = all_markets();
    if query.trim().is_empty() {
        let mine: Vec<_> = all.into_iter().filter(|(b, q)| held(b) && held(q)).take(RESTING_ROWS).collect();
        return (mine, true);
    }
    let mut mine = Vec::new();
    let mut rest = Vec::new();
    for (b, q) in all {
        if !matches(b, q, query) {
            continue;
        }
        if held(b) || held(q) { mine.push((b, q)) } else { rest.push((b, q)) }
    }
    mine.extend(rest);
    (mine, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A symbol reaches every market touching it, as whole pairs.
    #[test]
    fn a_symbol_returns_every_market_touching_it() {
        assert!(matches("XRP", "RLUSD", "rlusd"));
        assert!(matches("RLUSD", "EUROP", "rlusd"));
        assert!(!matches("XRP", "AUDD", "rlusd"));
        // The display form counts too.
        assert!(matches("XRP", "EUROP", "eurøp"));
    }

    /// Pair syntax narrows to one market, either way round, either separator.
    #[test]
    fn pair_syntax_narrows_to_one_market() {
        assert!(matches("XRP", "RLUSD", "xrp/rl"));
        assert!(matches("XRP", "RLUSD", "xrp rl"));
        assert!(matches("XRP", "RLUSD", "rl/xrp"));
        assert!(!matches("XRP", "AUDD", "xrp/rl"));
    }

    /// An issuer's name reaches its markets; an issuer's address does not.
    #[test]
    fn an_issuer_name_matches_and_an_address_does_not() {
        assert!(matches("XRP", "USDC", "circle"));
        assert!(matches("XRP", "RLUSD", "ripple"));
        assert!(!matches("XRP", "RLUSD", "rMxCK"));
    }

    /// Five tokens make five XRP legs and ten crosses.
    #[test]
    fn the_registry_makes_fifteen_markets() {
        let n = tokens::TOKENS.len();
        assert_eq!(all_markets().len(), n + n * (n - 1) / 2);
    }

}
