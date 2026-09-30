//! What the grid says about the Bitcoin node and its fee market, read off
//! the node frame (`CHANNEL.btc_node_rx`, measured by indexd and forwarded
//! verbatim): the status word, the fee severity bands the block train is
//! tinted by, the tip's age, and the rate formatter every pane shares.
//! Every field of the frame is an `Option` and a `None` renders `—`, never
//! a zero — the relay clears the whole frame when it loses indexd rather
//! than let a last good reading sit on screen looking current.
//!
//! These lived in the pre-grid balance screen (`btcbalance.rs`, deleted
//! 2026-09-10); the panes kept them.

use iced::Color;

use crate::channel::{BtcNodeStats as NodeFrame, CHANNEL};
use crate::utils::theme::CompactPalette;

/// Fee severity bands: `(upper bound in sat/vB, word)`.
///
/// The range spans the measured 0.68→4.86 sat/vB of a calm half-year and the
/// 300+ of the 2023-24 regime, so it has to stay legible across three orders of
/// magnitude — which is why the bands are ratios, not even steps. Five bands
/// onto three colours.
pub(crate) const FEE_BANDS: [(f32, &str); 5] = [
    (2.0,           "quiet"),
    (10.0,          "calm"),
    (50.0,          "busy"),
    (150.0,         "congested"),
    (f32::INFINITY, "extreme"),
];

/// How long the node's headers must run ahead of its blocks before the word
/// is `behind`. Fetching and validating a block takes seconds on this box
/// and indexd's tick can land inside that window; a minute cannot be that.
///
/// This REPLACED a clock (2026-09-10). The old rule read `stalled` when the
/// tip was older than three average intervals, floored at 45 minutes — and a
/// 45-minute gap is ordinary Poisson variance (about 1 % of blocks, once or
/// twice a day), so it fired on a perfectly healthy node during a quiet
/// stretch of the network. Age cannot tell a quiet chain from a deaf node.
/// Evidence can: a node that is behind knows of a block it has not applied.
pub(crate) const BEHIND_FLOOR_SECS: u64 = 60;

/// The severity ramp — green, amber, red, the same three steps XRP uses.
pub(crate) fn band_color(band: usize, p: &'static CompactPalette) -> Color {
    match band {
        0 | 1 => p.green,
        2 => p.amber,
        _ => p.red,
    }
}

/// Which band a rate falls in — the index into [`FEE_BANDS`].
pub(crate) fn fee_band(rate: f32) -> usize {
    FEE_BANDS.iter().position(|(hi, _)| rate < *hi).unwrap_or(FEE_BANDS.len() - 1)
}

impl NodeFrame {
    pub(crate) fn current() -> Self {
        *CHANNEL.btc_node_rx.borrow()
    }

    /// What the status dot says. One word, and the only claim the grid makes
    /// about the node. Every word is evidence the node reported about itself;
    /// none is inferred from how long the chain has been quiet.
    pub(crate) fn status(&self, p: &'static CompactPalette) -> (&'static str, Color) {
        // Nothing measured at all: the relay dropped the frame because it lost
        // indexd. Say so rather than draw a green dot over a row of dashes.
        if self.sync.is_none() && self.peers.is_none() && self.tip_height.is_none() {
            return ("offline", p.red);
        }
        // Nobody can reach this node, so nothing it shows can be current.
        if self.peers == Some(0) {
            return ("no peers", p.amber);
        }
        // The node has known of a newer block for over a minute and has not
        // applied it: behind the network. See [`BEHIND_FLOOR_SECS`].
        if self.behind_secs.is_some_and(|s| s >= BEHIND_FLOOR_SECS) {
            return ("behind", p.amber);
        }
        match self.sync {
            // The index is behind the node. Queries fall back or are refused
            // while this is true, so it is worth a word of its own.
            Some(s) if s < 0.9999 => ("syncing", p.amber),
            Some(_) => ("synced", p.green),
            None => ("unknown", p.muted),
        }
    }
}

/// Seconds since indexd stamped the tip's arrival.
///
/// Saturating, and `None` if the stamp is in our future by more than a slack
/// margin: this is the SERVER's clock, so a workstation running behind would
/// otherwise render a wildly negative age as a huge one. A block that looks a
/// few seconds early is clock skew and clamps to zero.
pub(crate) fn tip_age(tip_at: Option<u64>) -> Option<u64> {
    let at = tip_at.filter(|t| *t > 0)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    if at > now + 300 { None } else { Some(now.saturating_sub(at)) }
}

/// A fee rate with trailing zeros trimmed. Block-average feerate swung 7× in
/// one calm half-year and the 2023-24 regime hit 300+, so this has to stay
/// readable across three orders of magnitude.
pub(crate) fn trim_rate(v: f32) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() { "0".to_string() } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::theme;

    /// The band boundaries are what keep the block tint honest.
    #[test]
    fn the_band_word_matches_its_rate() {
        for (rate, want) in [
            (0.62, "quiet"), (1.99, "quiet"), (2.0, "calm"), (9.9, "calm"),
            (10.0, "busy"), (49.0, "busy"), (50.0, "congested"), (149.0, "congested"),
            (150.0, "extreme"), (900.0, "extreme"),
        ] {
            assert_eq!(FEE_BANDS[fee_band(rate)].1, want, "wrong word at {rate}");
        }
    }

    /// Five words, three colours, and the ramp only ever climbs.
    #[test]
    fn the_fee_ramp_only_climbs() {
        let p = &theme::COMPACT_OBSIDIAN;
        for rate in [0.1, 1.0, 5.0, 9.9] {
            assert_eq!(band_color(fee_band(rate), p), p.green, "at {rate}");
        }
        assert_eq!(band_color(fee_band(30.0), p), p.amber);
        for rate in [100.0, 900.0, 5000.0] {
            assert_eq!(band_color(fee_band(rate), p), p.red, "at {rate}");
        }
    }

    /// An empty frame is what the relay publishes when it loses indexd. It must
    /// read as `offline`, never as a green dot over a row of dashes.
    #[test]
    fn an_empty_frame_reads_as_offline() {
        let p = &theme::COMPACT_OBSIDIAN;
        let (word, color) = NodeFrame::default().status(p);
        assert_eq!(word, "offline");
        assert_eq!(color, p.red);
    }

    #[test]
    fn a_behind_index_reads_as_syncing_not_synced() {
        let p = &theme::COMPACT_OBSIDIAN;
        let behind = NodeFrame { sync: Some(0.87), peers: Some(10), tip_height: Some(900_000), ..Default::default() };
        assert_eq!(behind.status(p).0, "syncing");

        let live = NodeFrame { sync: Some(1.0), peers: Some(10), tip_height: Some(962_512), ..Default::default() };
        assert_eq!(live.status(p).0, "synced");
        assert_eq!(live.status(p).1, p.green);
    }

    /// A long gap is not a stall: with no newer header known, a tip fifty
    /// minutes old still reads `synced` (the 2026-09-10 false positive). The
    /// word `behind` needs evidence held for a minute, and `no peers`
    /// outranks everything but offline.
    #[test]
    fn a_quiet_chain_is_synced_and_behind_needs_evidence() {
        let p = &theme::COMPACT_OBSIDIAN;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let quiet = NodeFrame {
            sync: Some(1.0),
            peers: Some(10),
            tip_height: Some(966_298),
            tip_at: Some(now - 50 * 60),
            avg_interval: Some(548.0),
            node_height: Some(966_298),
            headers: Some(966_298),
            behind_secs: Some(0),
            ..Default::default()
        };
        assert_eq!(quiet.status(p), ("synced", p.green));
        // A block being fetched: headers ahead for seconds is still synced.
        let fetching = NodeFrame { headers: Some(966_299), behind_secs: Some(12), ..quiet };
        assert_eq!(fetching.status(p).0, "synced");
        let behind = NodeFrame { headers: Some(966_299), behind_secs: Some(61), ..quiet };
        assert_eq!(behind.status(p), ("behind", p.amber));
        let isolated = NodeFrame { peers: Some(0), ..behind };
        assert_eq!(isolated.status(p), ("no peers", p.amber));
        // A frame without the new fields (an older indexd) is judged on what
        // it has: never behind.
        let older = NodeFrame { node_height: None, headers: None, behind_secs: None, ..quiet };
        assert_eq!(older.status(p).0, "synced");
    }

    #[test]
    fn tip_age_clamps_skew_and_refuses_the_future() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        assert_eq!(tip_age(Some(now - 90)), Some(90));
        assert_eq!(tip_age(Some(now + 30)), Some(0), "a few seconds early is skew");
        assert_eq!(tip_age(Some(now + 3600)), None, "an hour early is a broken clock");
        assert_eq!(tip_age(Some(0)), None);
        assert_eq!(tip_age(None), None);
    }

    #[test]
    fn trim_rate_spans_three_orders_of_magnitude() {
        assert_eq!(trim_rate(0.10), "0.1");
        assert_eq!(trim_rate(0.35), "0.35");
        assert_eq!(trim_rate(1.40), "1.4");
        assert_eq!(trim_rate(300.0), "300");
    }
}
