

use iced::Color;

use crate::channel::{BtcNodeStats as NodeFrame, CHANNEL};
use crate::utils::theme::CompactPalette;


pub(crate) const FEE_BANDS: [(f32, &str); 5] = [
    (2.0,           "quiet"),
    (10.0,          "calm"),
    (50.0,          "busy"),
    (150.0,         "congested"),
    (f32::INFINITY, "extreme"),
];


pub(crate) const BEHIND_FLOOR_SECS: u64 = 60;

pub(crate) fn band_color(band: usize, p: &'static CompactPalette) -> Color {
    match band {
        0 | 1 => p.green,
        2 => p.amber,
        _ => p.red,
    }
}

pub(crate) fn fee_band(rate: f32) -> usize {
    FEE_BANDS.iter().position(|(hi, _)| rate < *hi).unwrap_or(FEE_BANDS.len() - 1)
}


pub(crate) trait NodeFrameView {
    fn current() -> Self;

    fn status(&self, p: &'static CompactPalette) -> (&'static str, Color);
}

impl NodeFrameView for NodeFrame {
    fn current() -> Self {
        *CHANNEL.btc_node_rx.borrow()
    }

    fn status(&self, p: &'static CompactPalette) -> (&'static str, Color) {
      
        if self.sync.is_none() && self.peers.is_none() && self.tip_height.is_none() {
            return ("offline", p.red);
        }
        if self.peers == Some(0) {
            return ("no peers", p.amber);
        }
      
        if self.behind_secs.is_some_and(|s| s >= BEHIND_FLOOR_SECS) {
            return ("behind", p.amber);
        }
        match self.sync {
        
            Some(s) if s < 0.9999 => ("syncing", p.amber),
            Some(_) => ("synced", p.green),
            None => ("unknown", p.muted),
        }
    }
}


pub(crate) fn tip_age(tip_at: Option<u64>) -> Option<u64> {
    let at = tip_at.filter(|t| *t > 0)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    if at > now + 300 { None } else { Some(now.saturating_sub(at)) }
}

pub(crate) fn trim_rate(v: f32) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() { "0".to_string() } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::theme;

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
        let fetching = NodeFrame { headers: Some(966_299), behind_secs: Some(12), ..quiet };
        assert_eq!(fetching.status(p).0, "synced");
        let behind = NodeFrame { headers: Some(966_299), behind_secs: Some(61), ..quiet };
        assert_eq!(behind.status(p), ("behind", p.amber));
        let isolated = NodeFrame { peers: Some(0), ..behind };
        assert_eq!(isolated.status(p), ("no peers", p.amber));
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
