
use iced::Widget as _;
use iced::widget::scrollable::{Rail, Scroller};
use iced::widget::{column, text, Space};
use iced::{Alignment, Border, Color, Element};

use crate::controller::message::Message;
use crate::utils::fonts::MONO;
use crate::utils::theme::CompactPalette;

pub const ROW_PAD_V: f32 = 9.0;

pub const SCROLLER_W: f32 = 3.0;

pub const SYM: f32 = 12.0;
pub const ISSUER: f32 = 10.5;
pub const BALANCE: f32 = 12.5;
pub const FIAT: f32 = 10.5;

pub fn holding_columns<'a>(
    symbol:   &str,
    issuer:   String,
    bal_str:  String,
    fiat_str: String,
    cp:       &'static CompactPalette,
    scale:    f32,
) -> (Element<'a, Message>, Element<'a, Message>) {
    (
        column![
            text(symbol.to_string()).size(SYM * scale).color(cp.text),
            Space::new().height(3.0 * scale),
            text(issuer).size(ISSUER * scale).color(cp.dim),
        ].boxed(),
        column![
            text(bal_str).font(MONO).size(BALANCE * scale).color(cp.text),
            Space::new().height(3.0 * scale),
            text(fiat_str).font(MONO).size(FIAT * scale).color(cp.muted),
        ]
        .align_x(Alignment::End)
        .boxed(),
    )
}

pub fn thumb_only(thumb: Color) -> Rail {
    Rail {
        background: None,
        border: Border::default(),
        scroller: Scroller {
            background: thumb.into(),
            border: Border { radius: 2.0.into(), ..Border::default() },
        },
    }
}

pub fn short_issuer(addr: &str) -> String {
    let n = addr.chars().count();
    if n <= 12 {
        return addr.to_string();
    }
    let head: String = addr.chars().take(5).collect();
    let tail: String = addr.chars().skip(n - 4).collect();
    format!("{head}\u{2026}{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::reserves::Reserve;

    #[test]
    fn the_issuer_is_truncated_at_both_ends() {
        assert_eq!(short_issuer("rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De"), "rMxCK\u{2026}m5De");
        assert_eq!(short_issuer("rShort"), "rShort");
    }

    #[test]
    fn the_enable_gate_charges_the_reserve_and_the_fee() {
        let r = |available: f64| Reserve {
            base: 1.0,
            per_object: 0.2,
            owner_count: 0,
            total: 1.0,
            available,
        };
        assert!(r(0.5).affords_new_object(0.000012));
        assert!(!r(0.2).affords_new_object(0.000012), "the fee was not charged");
        assert!(r(0.2).affords_new_object(0.0));
    }
}
