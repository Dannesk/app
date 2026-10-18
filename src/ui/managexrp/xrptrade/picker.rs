
use crate::channel::CHANNEL;
use crate::utils::tokens;

const RESTING_ROWS: usize = 4;

pub fn all_markets() -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = tokens::TOKENS.iter().map(|t| ("XRP", t.code)).collect();
    for (i, a) in tokens::TOKENS.iter().enumerate() {
        for b in &tokens::TOKENS[i + 1..] {
            out.push((a.code, b.code));
        }
    }
    out
}

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

    #[test]
    fn a_symbol_returns_every_market_touching_it() {
        assert!(matches("XRP", "RLUSD", "rlusd"));
        assert!(matches("RLUSD", "EUROP", "rlusd"));
        assert!(!matches("XRP", "AUDD", "rlusd"));
        assert!(matches("XRP", "EUROP", "eurøp"));
    }

    #[test]
    fn pair_syntax_narrows_to_one_market() {
        assert!(matches("XRP", "RLUSD", "xrp/rl"));
        assert!(matches("XRP", "RLUSD", "xrp rl"));
        assert!(matches("XRP", "RLUSD", "rl/xrp"));
        assert!(!matches("XRP", "AUDD", "xrp/rl"));
    }

    #[test]
    fn an_issuer_name_matches_and_an_address_does_not() {
        assert!(matches("XRP", "USDC", "circle"));
        assert!(matches("XRP", "RLUSD", "ripple"));
        assert!(!matches("XRP", "RLUSD", "rMxCK"));
    }

    #[test]
    fn the_registry_makes_fifteen_markets() {
        let n = tokens::TOKENS.len();
        assert_eq!(all_markets().len(), n + n * (n - 1) / 2);
    }

}
