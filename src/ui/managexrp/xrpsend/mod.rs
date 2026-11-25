
use iced::widget::Column;
use iced::Widget as _;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::channel::CHANNEL;
use crate::controller::app_state::XrpTokenTab;
use crate::controller::message::Message;
use crate::utils::money;
use crate::ui::components::compact;
use crate::ui::components::send_screen as screen;
use crate::ui::managexrp::panes;
use crate::ui::managexrp::tokens as tokens_page;
use crate::ui::components::grid;
use crate::utils::{format_token_amount, theme, tokens};

pub(crate) const SELF_ERROR: &str = "You cannot send to yourself";

pub(crate) const OVER_AVAILABLE: &str = "More than available";
pub(crate) const NO_XRP_FOR_FEE: &str = "No XRP available for the network fee";
pub(crate) const PREFIX_ERROR: &str = "An XRP address starts with r, or X for an X-address";
pub(crate) const TAG_NONE: &str = "None";
pub(crate) const X_TAG_NOTE: &str = "Tag from the X-address";
pub(crate) const CHECKSUM_ERROR: &str = "That address fails its checksum";

const TAG_DIGITS: usize = 11; // one past u32::MAX's ten, so a digit too many shows red

pub(crate) fn tag_typed(candidate: &str) -> Option<String> {
    if !candidate.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let digits = candidate.trim_start_matches('0');
    let tag = if digits.is_empty() && !candidate.is_empty() { "0" } else { digits };
    (tag.len() <= TAG_DIGITS).then(|| tag.to_string())
}

pub(crate) fn tag_over(tag: &str) -> bool {
    !tag.trim().is_empty() && tag.trim().parse::<u32>().is_err()
}

const CLASSIC_MIN: usize = 25;
const XADDRESS_MIN: usize = 46;

pub(crate) fn recipient_fault(addr: &str) -> Option<&'static str> {
    let addr = addr.trim();
    if addr.is_empty() {
        return None;
    }
    let n = addr.chars().count();
    let floor = if addr.starts_with('X') {
        XADDRESS_MIN
    } else if addr.starts_with('r') {
        CLASSIC_MIN
    } else {
        return Some(PREFIX_ERROR);
    };
    if n < floor {
        return None;
    }
    dannesk_xrpl_codec::xaddress::resolve(addr)
        .is_none()
        .then_some(CHECKSUM_ERROR)
}

pub(crate) fn asset_display(tab: &XrpTokenTab) -> &'static str {
    match *tab {
        XrpTokenTab::Xrp => "XRP",
        XrpTokenTab::Token(c) => tokens::by_code(c).map(|t| t.display).unwrap_or(c),
    }
}

const CHECK_W: f32 = 9.0;
const CHECK: f32 = 10.0;
const ROW_GAP: f32 = 14.0;
const ROW_RADIUS: f32 = 5.0;

pub(crate) struct Holding {
    tab:      XrpTokenTab,
    symbol:   &'static str,
    issuer:   String,
    balance:  f64,
    rate_key: &'static str,
}

pub(crate) fn holdings() -> Vec<Holding> {
    let (xrp_balance, _, _, _) = *CHANNEL.wallet_balance_rx.borrow();
    let mut out = vec![Holding {
        tab: XrpTokenTab::Xrp,
        symbol: "XRP",
        issuer: "Native asset".to_string(),
        balance: xrp_balance,
        rate_key: "XRP",
    }];
    for t in tokens::TOKENS.iter() {
        let (balance, _, _, _) = CHANNEL.token(t.code);
        if crate::controller::xrp::holds(t.code) {
            out.push(Holding {
                tab: XrpTokenTab::Token(t.code),
                symbol: t.display,
                issuer: format!("{} \u{b7} {}", t.issuer_name, tokens_page::short_issuer(t.issuer)),
                balance,
                rate_key: t.rate_key,
            });
        }
    }
    out
}

pub fn asset_list<'a>(
    selected: &XrpTokenTab,
    ccy: &'static str,
    cp: &'static theme::CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let held = holdings();
    let last = held.len().saturating_sub(1);
    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for (i, h) in held.iter().enumerate() {
        list = list.push(picker_row(h, &h.tab == selected, ccy, cp, scale));
        if i != last {
            list = list.push(compact::hairline(cp.rule));
        }
    }
    grid::scroller(list.boxed(), cp, scale)
}

fn picker_row<'a>(
    h:        &Holding,
    selected: bool,
    ccy:      &'static str,
    cp:       &'static theme::CompactPalette,
    scale:    f32,
) -> Element<'a, Message> {
    let bal_str = format_token_amount(h.balance, 6);
    let rate = crate::utils::price::cross(h.rate_key, ccy);
    let fiat_str = if rate > 0.0 {
        format!("\u{2248} {} {ccy}", money(h.balance * rate))
    } else {
        grid::NA.to_string()
    };
    let (left, right) =
        tokens_page::holding_columns(h.symbol, h.issuer.clone(), bal_str, fiat_str, cp, scale);

    let check: Element<'a, Message> = if selected {
        text("\u{2713}").size(CHECK * scale).color(cp.green).boxed()
    } else {
        Space::new().boxed()
    };

    let body = row![
        left,
        Space::new().width(Length::Fill),
        right,
        Space::new().width(ROW_GAP * scale),
        container(check)
            .width(Length::Fixed(CHECK_W * scale))
            .align_x(Alignment::End),
    ]
    .align_y(Alignment::Center);

    let row_button = button(body)
        .width(Length::Fill)
        .padding(
            Padding::new(0.0)
                .top(tokens_page::ROW_PAD_V * scale)
                .bottom(tokens_page::ROW_PAD_V * scale)
                .left(panes::TOKEN_ROW_PAD_H * scale)
                .right(panes::TOKEN_ROW_PAD_H * scale),
        )
        .on_press(Message::XrpTokenTabChanged(h.tab.clone()))
        .style(move |_, status| screen::row_style(true, status, ROW_RADIUS * scale, cp));
    container(row_button)
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(screen::ROW_INSET * scale).bottom(screen::ROW_INSET * scale))
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "rPEPPER7kfTD9w2To4CQk6UCfuHM9c6GDY";

    #[test]
    fn an_edited_character_is_named_as_a_checksum_failure() {
        assert_eq!(recipient_fault(GOOD), None, "a good address must stay quiet");
        let chars: Vec<char> = GOOD.chars().collect();
        for i in 1..chars.len() {
            let mut edited = chars.clone();
            edited[i] = if edited[i] == 'p' { 'q' } else { 'p' };
            let edited: String = edited.into_iter().collect();
            assert_eq!(
                recipient_fault(&edited),
                Some(CHECKSUM_ERROR),
                "editing slot {i} of a good address said nothing",
            );
        }
    }

    #[test]
    fn a_half_typed_address_says_nothing() {
        assert_eq!(recipient_fault(""), None);
        for n in 1..CLASSIC_MIN {
            let part: String = GOOD.chars().take(n).collect();
            assert_eq!(recipient_fault(&part), None, "spoke at {n} characters");
        }
        assert_eq!(recipient_fault(&"X".repeat(XADDRESS_MIN - 1)), None);
        assert_eq!(recipient_fault(&"X".repeat(XADDRESS_MIN)), Some(CHECKSUM_ERROR));
    }

    #[test]
    fn a_wrong_first_character_is_wrong_at_once() {
        assert_eq!(recipient_fault("b"), Some(PREFIX_ERROR));
        assert_eq!(recipient_fault("0x1234"), Some(PREFIX_ERROR));
        assert_eq!(recipient_fault("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"), Some(PREFIX_ERROR));
    }

    #[test]
    fn a_tag_is_typed_as_eleven_digits_with_its_zeros_collapsed() {
        assert_eq!(tag_typed(""), Some(String::new()));
        assert_eq!(tag_typed("0"), Some("0".to_string()), "0 is a tag");
        assert_eq!(tag_typed("000"), Some("0".to_string()));
        assert_eq!(tag_typed("0104857"), Some("104857".to_string()));
        assert_eq!(tag_typed("4294967295"), Some("4294967295".to_string()));
        assert_eq!(tag_typed("4294967296"), Some("4294967296".to_string()), "past a u32 is held, drawn red");
        assert_eq!(tag_typed("22222222222"), Some("22222222222".to_string()), "an eleventh digit is held, drawn red");
        assert_eq!(tag_typed("222222222222"), None, "a twelfth digit is refused");
        assert_eq!(tag_typed("00222222222222"), None);
        assert_eq!(tag_typed("12a"), None);
        assert_eq!(tag_typed("1.5"), None);
    }

    #[test]
    fn only_a_tag_past_a_u32_is_over() {
        for fits in ["", "0", "104857", "2222222222", "4294967295"] {
            assert!(!tag_over(fits), "{fits:?} drawn red");
        }
        for over in ["4294967296", "9999999999", "22222222222"] {
            assert!(tag_over(over), "{over:?} not drawn red");
        }
    }

    #[test]
    fn the_hint_never_argues_with_the_gate() {
        for addr in [GOOD, "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De"] {
            assert!(dannesk_xrpl_codec::xaddress::resolve(addr).is_some(), "{addr} is not a good address");
            assert_eq!(recipient_fault(addr), None);
        }
    }
    use crate::ui::components::send_screen::COMPOSE_W;
    use crate::ui::components::tui::MONO_ADVANCE;

    #[test]
    fn the_tag_fits_its_box() {
        let advance = compact::FIELD * MONO_ADVANCE;
        let cols = ((COMPOSE_W - 16.0) / advance).floor();
        assert!(u32::MAX.to_string().chars().count() as f32 <= cols);
    }
}
