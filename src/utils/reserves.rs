
use crate::channel::{XrpAccount, XrpNodeStats};

pub const DROPS: f64 = 1_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reserve {
    pub base: f64,
    pub per_object: f64,
    pub owner_count: u32,
    pub total: f64,
    pub available: f64,
}

impl Reserve {
    pub fn affords_new_object(&self, cost: f64) -> bool {
        self.available >= self.per_object + cost
    }
}

pub fn chain_reserve(
    total_xrp: f64,
    account_exists: bool,
    account: XrpAccount,
    node: &XrpNodeStats,
) -> Option<Reserve> {
    let base = node.reserve_base? as f64 / DROPS;
    let per_object = node.reserve_inc? as f64 / DROPS;

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

    #[test]
    fn a_missing_half_is_never_guessed() {
        assert!(chain_reserve(100.0, true, acct(Some(2)), &node(None, Some(200_000))).is_none());
        assert!(chain_reserve(100.0, true, acct(Some(2)), &node(Some(1_000_000), None)).is_none());
        assert!(chain_reserve(100.0, true, acct(None), &node(Some(1_000_000), Some(200_000))).is_none());
    }

    #[test]
    fn the_reserve_is_base_plus_owner_count() {
        let r = chain_reserve(10.0, true, acct(Some(2)), &node(1_000_000.into(), 200_000.into())).unwrap();
        assert_eq!(r.base, 1.0);
        assert_eq!(r.per_object, 0.2);
        assert!((r.total - 1.4).abs() < 1e-9);
        assert!((r.available - 8.6).abs() < 1e-9);
    }

    #[test]
    fn a_revote_moves_everything() {
        let r = chain_reserve(10.0, true, acct(Some(2)), &node(5_000_000.into(), 1_000_000.into())).unwrap();
        assert!((r.total - 7.0).abs() < 1e-9);
        assert!((r.available - 3.0).abs() < 1e-9);
        assert!(r.affords_new_object(0.000012));
        assert!(!r.affords_new_object(2.5));
    }

    #[test]
    fn a_fee_burned_account_clamps_to_zero() {
        let r = chain_reserve(0.999964, true, acct(Some(0)), &node(1_000_000.into(), 200_000.into())).unwrap();
        assert_eq!(r.available, 0.0);
        assert!(!r.affords_new_object(0.0));
    }

    #[test]
    fn an_inactive_account_reserves_nothing() {
        let r = chain_reserve(0.5, false, acct(None), &node(1_000_000.into(), 200_000.into())).unwrap();
        assert_eq!(r.total, 0.0);
        assert_eq!(r.available, 0.5);
    }
}
