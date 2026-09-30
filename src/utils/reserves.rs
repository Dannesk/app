//! XRP reserves — **read off the ledger, never written down here.**
//!
//! The reserve is a validator-voted parameter, not a constant of nature: the
//! base moved 10 XRP → 1 XRP in December 2024 and the owner reserve moved with
//! it. Every number this module reports therefore comes from the `ledger`
//! stream the relay already forwards (`reserve_base` / `reserve_inc`, see
//! [`XrpNodeStats`](crate::channel::xrp::XrpNodeStats)) crossed with the
//! account's own `owner_count` — the two halves of the chain-exact answer.
//!
//! **A gate is never answered on a guess.** [`chain_reserve`] returns `None`
//! until both halves have landed, and every caller closes rather than
//! approximating: a gate computed from a stale or invented reserve either
//! blocks a send that would have worked or — the expensive direction — lights
//! a button on a transaction the ledger then refuses, after the fee is burned.
//!
//! `owner_count` is why the registry arithmetic this replaced was wrong rather
//! than merely imprecise. It counts EVERY owned object: trustlines, resting
//! offers, escrows, checks, tickets, NFT pages — including objects placed by
//! another client on the same account, which we cannot see and never could.
//! Counting our own registry always undercounts, so it always overstates what
//! is spendable.

use crate::channel::xrp::{XrpAccount, XrpNodeStats};

/// Drops per XRP. A protocol constant, unlike everything else in this file.
pub const DROPS: f64 = 1_000_000.0;

/// Balance-threshold INFERENCE of account existence — the fallback, not the truth,
/// and the one place a reserve figure is still written down.
///
/// The chain-asserted answer (relay `exists`, from `account_info`'s actNotFound and
/// validated tx meta) lives in the token map as `CHANNEL.xrp_exists()`; parse sites
/// fold this inference in at write time whenever an assertion is missing (legacy
/// cached hash, old relay), so view code reads the channel and never calls this on
/// a balance directly.
///
/// It survives the move to live values because it answers a different question —
/// *is there an account at all* — at a moment when there is no node frame to ask:
/// it runs inside the parse of a cached blob, before any stream is up. It is a
/// threshold test, never a spendable-balance computation, and nothing gates money
/// on it.
///
/// The inference has a known hole: transaction fees are EXEMPT from the reserve
/// check, so an existing account can burn below the base reserve (1.0 XRP minus a
/// few 12-drop fees ⇒ 0.999964) — this function would call that account
/// nonexistent, which is why the chain assertion supersedes it everywhere it's
/// available. The opposite direction holds: the XRPL rejects a funding payment
/// below the base reserve, so a balance at or above it does prove existence.
///
/// `managexrp/xrptransactions.rs` leans on the existence fact (via the channel)
/// for its empty-state split: a nonexistent account provably has NO history, and
/// an existing one provably has at least ONE transaction — the funding payment.
///
/// Mirrored by `BASE_RESERVE` in `android/.../ui/utils/XrpUtils.kt` — twins, keep synced.
pub const BASE_RESERVE: f64 = 1.0;

pub fn is_activated(total_xrp: f64) -> bool {
    total_xrp >= BASE_RESERVE
}

/// The account's reserve, as the ledger itself states it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reserve {
    /// `reserve_base` — what an account locks up merely to exist.
    pub base: f64,
    /// `reserve_inc` — the price of ONE owned object: a trustline, a resting
    /// offer, an escrow. What enabling a token costs.
    pub per_object: f64,
    /// Objects this account owns, per the ledger — not per our registry.
    pub owner_count: u32,
    /// `base + owner_count × per_object`.
    pub total: f64,
    /// Balance less the reserve, clamped at zero. Fees are exempt from the
    /// reserve check, so an account CAN sit below its own reserve; that is a
    /// zero here, never a negative.
    pub available: f64,
}

impl Reserve {
    /// Whether `cost` in XRP can be paid on top of one more owned object —
    /// the question every "can I enable / can this order rest" gate asks.
    pub fn affords_new_object(&self, cost: f64) -> bool {
        self.available >= self.per_object + cost
    }
}

/// The chain-exact reserve, or `None` while either half is still missing.
///
/// `None` means *we have not been told yet*, and the honest response to that is
/// a closed gate and a `—`, not a number we made up. It resolves within one
/// ledger of the node frame arriving.
pub fn chain_reserve(
    total_xrp: f64,
    account_exists: bool,
    account: XrpAccount,
    node: &XrpNodeStats,
) -> Option<Reserve> {
    let base = node.reserve_base? as f64 / DROPS;
    let per_object = node.reserve_inc? as f64 / DROPS;

    // No account on ledger: nothing is owned and nothing is reserved, so the
    // whole balance is "available" — the caller renders Inactive off the
    // existence flag, not off this number.
    let (owner_count, total) = if account_exists {
        let oc = account.owner_count?;
        (oc, base + oc as f64 * per_object)
    } else {
        (0, 0.0)
    };

    Some(Reserve {
        base,
        per_object,
        owner_count,
        total,
        available: (total_xrp - total).max(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(base: Option<u64>, inc: Option<u64>) -> XrpNodeStats {
        XrpNodeStats { reserve_base: base, reserve_inc: inc, ..Default::default() }
    }

    fn acct(oc: Option<u32>) -> XrpAccount {
        XrpAccount { sequence: None, owner_count: oc }
    }

    /// The whole point of the module: a missing half is `None`, not a default.
    /// A gate that fell back to a written-down reserve would light a button on
    /// a transaction the ledger refuses.
    #[test]
    fn a_missing_half_is_never_guessed() {
        assert!(chain_reserve(100.0, true, acct(Some(2)), &node(None, Some(200_000))).is_none());
        assert!(chain_reserve(100.0, true, acct(Some(2)), &node(Some(1_000_000), None)).is_none());
        assert!(chain_reserve(100.0, true, acct(None), &node(Some(1_000_000), Some(200_000))).is_none());
    }

    /// Chain-exact arithmetic, at today's voted values: 1 XRP base, 0.2 per
    /// object, two objects ⇒ 1.4 held, the rest spendable.
    #[test]
    fn the_reserve_is_base_plus_owner_count() {
        let r = chain_reserve(10.0, true, acct(Some(2)), &node(1_000_000.into(), 200_000.into())).unwrap();
        assert_eq!(r.base, 1.0);
        assert_eq!(r.per_object, 0.2);
        assert!((r.total - 1.4).abs() < 1e-9);
        assert!((r.available - 8.6).abs() < 1e-9);
    }

    /// A re-vote moves every number without a code change — the reason none of
    /// them are written down. Same account, a 5 XRP base and 1 XRP per object.
    #[test]
    fn a_revote_moves_everything() {
        let r = chain_reserve(10.0, true, acct(Some(2)), &node(5_000_000.into(), 1_000_000.into())).unwrap();
        assert!((r.total - 7.0).abs() < 1e-9);
        assert!((r.available - 3.0).abs() < 1e-9);
        // And the gate follows: one more object costs 1 XRP now, not 0.2.
        assert!(r.affords_new_object(0.000012));
        assert!(!r.affords_new_object(2.5));
    }

    /// Fees are exempt from the reserve check, so an account can sit below its
    /// own reserve. That is zero available, never negative.
    #[test]
    fn a_fee_burned_account_clamps_to_zero() {
        let r = chain_reserve(0.999964, true, acct(Some(0)), &node(1_000_000.into(), 200_000.into())).unwrap();
        assert_eq!(r.available, 0.0);
        assert!(!r.affords_new_object(0.0));
    }

    /// An unfunded address owns nothing and reserves nothing.
    #[test]
    fn an_inactive_account_reserves_nothing() {
        let r = chain_reserve(0.5, false, acct(None), &node(1_000_000.into(), 200_000.into())).unwrap();
        assert_eq!(r.total, 0.0);
        assert_eq!(r.available, 0.5);
    }
}
