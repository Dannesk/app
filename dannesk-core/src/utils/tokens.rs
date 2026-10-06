//! Single source of truth for every issued (non-XRP) token the wallet supports.
//!
//! Adding a token is meant to be a one-entry change: append a [`TokenDef`] to
//! [`TOKENS`] (and have the relay publish its balance/trustline fields; no
//! logo — the app draws no token-provider marks anywhere since v3). Everything else — the channel map, the segmented
//! pills, issuer lookups for payment/trustset/offer blobs, live-balance and
//! trustline routing, and cached/import balance parsing — reads from this table,
//! so there are no other per-token edits to hunt down.
//!
//! Token identity is the canonical [`TokenDef::code`] (e.g. `"RLUSD"`), which is
//! also the key in the `tokens` channel map and the payload of
//! `XrpTokenTab::Token`.

pub struct TokenDef {
    /// Canonical code: channel-map key, `XrpTokenTab::Token` payload, and the
    /// asset string carried in signing commands. e.g. `"RLUSD"`.
    pub code: &'static str,
    /// Pill / label text. May differ from `code` for typography, e.g. `"EURØP"`.
    pub display: &'static str,
    /// Compact ticker shown next to the balance and in the trustline meter on
    /// the detail screen. Kept separate from `display` (which may carry
    /// typographic glyphs like "EURØP") and from `rate_key` (the fiat key used
    /// for price lookups), so display can be tuned without touching either.
    pub ticker: &'static str,
    /// Human issuer name shown on the enable screen and trade picker.
    pub issuer_name: &'static str,
    /// Issuer account (r-address).
    pub issuer: &'static str,
    /// 160-bit currency code, hex (40 chars), as used on-chain.
    pub currency_hex: &'static str,
    /// Asset key for rate lookups / USD conversion, e.g. `"EUR"`, `"SGD"`.
    /// For fiat-pegged tokens with no listed pair (EUROP/XSGD/AUDD) this is the
    /// *fiat* proxy (we assume token ≈ fiat). RLUSD and USDC are the exceptions:
    /// we have real Kraken RLUSD/USD and USDC/USD feeds, so their keys are their
    /// own assets, never `"USD"` — we price them, we don't assume the peg.
    ///
    /// BBRL (Braza Bank) was DROPPED 2026-09-04 with USDC taking its slot: its
    /// XRP book measured as no market at all (dust bid, ask at 400, a 19-XRP
    /// pool), and a token with no XRP market has no place in a registry that
    /// feeds the trade picker.
    pub rate_key: &'static str,
    /// Relay pub/sub command that delivers this token's live balance.
    pub balance_cmd: &'static str,
    /// JSON field carrying the balance in cached/import payloads.
    pub balance_field: &'static str,
    /// JSON bool field: whether the wallet holds this trustline.
    pub has_field: &'static str,
    /// Relay command that delivers this token's trustline limit.
    pub trustline_cmd: &'static str,
    /// JSON field carrying the trustline limit.
    pub trustline_limit_field: &'static str,
}

/// Every supported issued token, in display order. Append here to add one.
pub static TOKENS: &[TokenDef] = &[
    TokenDef {
        code: "RLUSD",
        display: "RLUSD",
        ticker: "RLUSD",
        issuer_name: "Ripple",
        issuer: "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De",
        currency_hex: "524C555344000000000000000000000000000000",
        rate_key: "RLUSD", // real Kraken RLUSD/USD rate — never assume the 1.0 peg
        balance_cmd: "get_rlusd_balance",
        balance_field: "rlusd_balance",
        has_field: "has_rlusd",
        trustline_cmd: "get_trustline_limit",
        trustline_limit_field: "trustline_limit",
    },
    TokenDef {
        code: "EUROP",
        display: "EURØP",
        ticker: "EUROP",
        issuer_name: "Schuman Financial",
        issuer: "rMkEuRii9w9uBMQDnWV5AA43gvYZR9JxVK",
        currency_hex: "4555524F50000000000000000000000000000000",
        rate_key: "EUR",
        balance_cmd: "get_euro_balance",
        balance_field: "euro_balance",
        has_field: "has_euro",
        trustline_cmd: "get_trustline_euro_limit",
        trustline_limit_field: "trustline_euro_limit",
    },
    TokenDef {
        code: "XSGD",
        display: "XSGD",
        ticker: "XSGD",
        issuer_name: "StraitsX",
        issuer: "rK67JczCpaYXVtfw3qJVmqwpSfa1bYTptw",
        currency_hex: "5853474400000000000000000000000000000000",
        rate_key: "SGD",
        balance_cmd: "get_xsgd_balance",
        balance_field: "xsgd_balance",
        has_field: "has_xsgd",
        trustline_cmd: "get_trustline_sgd_limit",
        trustline_limit_field: "trustline_xsgd_limit",
    },
    TokenDef {
        code: "AUDD",
        display: "AUDD",
        ticker: "AUDD",
        issuer_name: "AUDC Pty Ltd",
        issuer: "rUN5Zxt3K1AnMRJgEWywDJT8QDMMeLH5ok",
        currency_hex: "4155444400000000000000000000000000000000",
        rate_key: "AUD",
        balance_cmd: "get_audd_balance",
        balance_field: "audd_balance",
        has_field: "has_audd",
        trustline_cmd: "get_trustline_audd_limit",
        trustline_limit_field: "trustline_audd_limit",
    },
    TokenDef {
        code: "USDC",
        display: "USDC",
        ticker: "USDC",
        issuer_name: "Circle",
        issuer: "rGm7WCVp9gb4jZHWTEtGUr4dd74z2XuWhE",
        currency_hex: "5553444300000000000000000000000000000000",
        rate_key: "USDC", // real Kraken USDC/USD rate — like RLUSD, never assume the 1.0 peg
        balance_cmd: "get_usdc_balance",
        balance_field: "usdc_balance",
        has_field: "has_usdc",
        trustline_cmd: "get_trustline_usdc_limit",
        trustline_limit_field: "trustline_usdc_limit",
    },
];

/// Look up a token by its canonical code (`"RLUSD"`).
pub fn by_code(code: &str) -> Option<&'static TokenDef> {
    TOKENS.iter().find(|t| t.code == code)
}

/// Look up the token whose live-balance command this is.
pub fn by_balance_cmd(cmd: &str) -> Option<&'static TokenDef> {
    TOKENS.iter().find(|t| t.balance_cmd == cmd)
}

/// Look up the token whose trustline-limit command this is.
pub fn by_trustline_cmd(cmd: &str) -> Option<&'static TokenDef> {
    TOKENS.iter().find(|t| t.trustline_cmd == cmd)
}

/// Fiat rate key for a code, falling back to the code itself so non-token
/// symbols (e.g. `"XRP"`) pass through unchanged.
pub fn rate_key(code: &str) -> &str {
    by_code(code).map(|t| t.rate_key).unwrap_or(code)
}
