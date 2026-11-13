
use iced::widget::Column;
use iced::Widget as _;
use iced::widget::{column, container, responsive, row, text, Space};
use iced::{Alignment, Color, Element, Length, Padding, Size};

use crate::channel::CHANNEL;
use crate::controller::app_state::AppState;
use crate::controller::message::{Message, SecureField};
use crate::controller::xrp::{xrp_available, xrp_fee, xrp_reserve};
use crate::ui::components::grid::{self, HINT, ROW_KEY};
use crate::ui::components::signing::SignFields;
use crate::utils::money;
use crate::ui::managexrp::tokens::short_issuer;
use crate::ui::managexrp::xrpdashboard::{drow, group_rule, ROW_VALUE};
use crate::utils::fonts::MONO;
use crate::utils::theme::CompactPalette;
use crate::utils::{add_commas, format_token_amount, price, tokens};

use super::{button_pair, danger_button, empty, fee_row, pressable, primary, quiet_button, scroller, sign_block, sign_mode, token_row, NA, SUB, VAL};

const MASK: &str = "\u{2022}\u{2022}\u{2022}\u{2022}";

pub fn available_title(state: &AppState) -> String {
    if state.show_enable && let Some(t) = state.xrp_token_tab.code().and_then(tokens::by_code) {
        format!("enable {}", t.display.to_lowercase())
    } else {
        "available tokens".to_string()
    }
}

pub fn held_title(state: &AppState) -> String {
    if let Some(t) = state.disable_token.and_then(tokens::by_code) {
        format!("remove {}", t.display.to_lowercase())
    } else if let Some(t) = state.token_details.and_then(tokens::by_code) {
        t.display.to_lowercase()
    } else {
        "tokens".to_string()
    }
}

fn issuer_line(t: &tokens::TokenDef) -> String {
    format!("{} \u{b7} {}", t.issuer_name, short_issuer(t.issuer))
}

fn issuer_row<'a>(t: &tokens::TokenDef, top: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    drow(
        "Issuer",
        vec![(t.issuer_name.to_string(), cp.text), (format!("  \u{b7}  {}", short_issuer(t.issuer)), cp.muted)],
        top,
        cp,
        scale,
    )
}

fn reserve_runs(cp: &'static CompactPalette) -> Vec<(String, Color)> {
    match xrp_reserve() {
        Some(r) => vec![(format_token_amount(r.per_object, 6), cp.text), (" XRP".to_string(), cp.faint)],
        None => vec![(NA.to_string(), cp.faint)],
    }
}

pub(crate) fn limit_text(limit: f64) -> String {
    if limit.fract() == 0.0 && limit.abs() < 9.0e15 {
        add_commas(limit as i64)
    } else {
        format_token_amount(limit, 6)
    }
}

pub fn held<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if let Some(t) = state.disable_token.and_then(tokens::by_code) {
        return remove_face(state, t, cp, scale);
    }
    if let Some(t) = state.token_details.and_then(tokens::by_code) {
        return details_face(state, t, cp, scale);
    }
    let ccy = state.base_currency.code();
    let hide = state.hide_balance;
    let held: Vec<&'static tokens::TokenDef> = tokens::TOKENS.iter().filter(|t| CHANNEL.token(t.code).1).collect();
    if held.is_empty() {
        return empty("No tokens to show", cp, scale);
    }
    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for t in held {
        let balance = CHANNEL.token(t.code).0;
        let rate = price::cross(t.rate_key, ccy);
        let (bal_str, fiat_str) = if hide {
            (MASK.to_string(), MASK.to_string())
        } else if rate > 0.0 {
            (format_token_amount(balance, 6), format!("\u{2248} {} {}", money(balance * rate), ccy))
        } else {
            (format_token_amount(balance, 6), NA.to_string())
        };
        let figures = column![
            text(bal_str).font(MONO).size(VAL * scale).color(cp.text),
            Space::new().height(3.0 * scale),
            text(fiat_str).font(MONO).size(SUB * scale).color(cp.muted),
        ]
        .align_x(Alignment::End);
        let line = token_row(t.display, issuer_line(t), figures.boxed(), false, cp, scale);
        list = list.push(pressable(line, Some(Message::TokenDetailsClicked(t.code)), cp));
    }
    scroller(list.boxed(), cp, scale)
}

pub fn available<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.show_enable && let Some(t) = state.xrp_token_tab.code().and_then(tokens::by_code) {
        return enable_face(state, t, cp, scale);
    }
    let available: Vec<&'static tokens::TokenDef> = tokens::TOKENS.iter().filter(|t| !CHANNEL.token(t.code).1).collect();
    if available.is_empty() {
        return empty("No tokens available", cp, scale);
    }
    let can_enable = xrp_reserve().is_some_and(|r| r.affords_new_object(xrp_fee()));
    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for t in available {
        let chevron = text("\u{203a}").font(MONO).size(VAL * scale).color(cp.faint).boxed();
        let line = token_row(t.display, issuer_line(t), chevron, !can_enable, cp, scale);
        list = list.push(pressable(line, can_enable.then_some(Message::EnableTokenClicked(t.code)), cp));
    }
    scroller(list.boxed(), cp, scale)
}

fn enable_face<'a>(state: &'a AppState, t: &'static tokens::TokenDef, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let fields = SignFields {
            mode: sign_mode(),
            secret: SecureField::EnablePassphrase,
            secret_buf: &state.enable_passphrase,
            secret_reveal: state.enable_passphrase_reveal,
            phrase: SecureField::EnableSeed,
            phrase_buf: &state.enable_seed_text,
            phrase_reveal: state.enable_seed_reveal,
            bip39: SecureField::EnableBip39,
            bip39_buf: &state.enable_bip39,
            bip39_reveal: state.enable_bip39_reveal,
            line: &state.phrase_line,
            on_submit: Message::EnableSubmitClicked,
        };
        let armed = state.enable_fee.is_some();
        let buttons = button_pair(
            quiet_button("Cancel", Message::EnableDismissed, cp, scale),
            primary("Sign & enable", armed && fields.can_submit(), Message::EnableSubmitClicked, cp, scale),
            scale,
        );
        let sign = sign_block(&fields, &state.enable_passphrase, &state.enable_seed_text, &state.enable_bip39, armed, buttons, inner, cp, scale);
        let body = column![
            issuer_row(t, 0.0, cp, scale),
            drow("Reserve", reserve_runs(cp), 3.0, cp, scale),
            fee_row(state.enable_fee, 3.0, cp, scale),
            sign,
            Space::new().height(2.0 * scale),
        ]
        .width(Length::Fill);
        scroller(body.boxed(), cp, scale)
    })
    .boxed()
}

fn remove_face<'a>(state: &'a AppState, t: &'static tokens::TokenDef, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let fields = SignFields {
            mode: sign_mode(),
            secret: SecureField::DisablePassphrase,
            secret_buf: &state.disable_passphrase,
            secret_reveal: state.disable_passphrase_reveal,
            phrase: SecureField::DisableSeed,
            phrase_buf: &state.disable_seed_text,
            phrase_reveal: state.disable_seed_reveal,
            bip39: SecureField::DisableBip39,
            bip39_buf: &state.disable_bip39,
            bip39_reveal: state.disable_bip39_reveal,
            line: &state.phrase_line,
            on_submit: Message::DisableSubmitClicked,
        };
        let armed = CHANNEL.token(t.code).0 == 0.0 && state.disable_fee.is_some();
        let buttons = button_pair(
            quiet_button("Cancel", Message::DisableDismissed, cp, scale),
            danger_button("Sign & remove", armed && fields.can_submit(), Message::DisableSubmitClicked, cp, scale),
            scale,
        );
        let sign = sign_block(&fields, &state.disable_passphrase, &state.disable_seed_text, &state.disable_bip39, armed, buttons, inner, cp, scale);
        let body = column![
            issuer_row(t, 0.0, cp, scale),
            drow("Reserve returned", reserve_runs(cp), 3.0, cp, scale),
            fee_row(state.disable_fee, 3.0, cp, scale),
            sign,
            Space::new().height(2.0 * scale),
        ]
        .width(Length::Fill);
        scroller(body.boxed(), cp, scale)
    })
    .boxed()
}

fn details_face<'a>(state: &'a AppState, t: &'static tokens::TokenDef, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let ccy = state.base_currency.code();
    let hide = state.hide_balance;
    let (balance, _, limit, _) = CHANNEL.token(t.code);
    let rate = price::cross(t.rate_key, ccy);
    let bal_str = if hide { MASK.to_string() } else { format_token_amount(balance, 6) };
    let fiat_str = if hide {
        MASK.to_string()
    } else if rate > 0.0 {
        format!("\u{2248} {} {}", money(balance * rate), ccy)
    } else {
        NA.to_string()
    };
    let hero = grid::hero(bal_str, t.display.to_uppercase(), cp, scale);
    let own = CHANNEL.wallet_balance_rx.borrow().1.clone();
    let reserve_word = match xrp_reserve() {
        Some(r) => format!("{} XRP reserve", format_token_amount(r.per_object, 6)),
        None => "reserve".to_string(),
    };
    let note = if balance == 0.0 {
        format!("Removing it returns the {reserve_word}. You can enable {} again any time.", t.ticker)
    } else {
        format!(
            "A trustline can only be removed at zero balance. Send or trade the {} {} first, then the {reserve_word} comes back.",
            format_token_amount(balance, 6),
            t.ticker
        )
    };
    let can_pay = xrp_available().is_some_and(|a| a >= xrp_fee());
    let live = balance == 0.0 && can_pay;

    let mut body = column![
        hero,
        text(fiat_str).font(MONO).size(SUB * scale).color(cp.muted),
        group_rule(cp, scale),
        drow("Issuer", vec![(t.issuer_name.to_string(), cp.text)], 0.0, cp, scale),
        address_fact("Issuer address", t.issuer, state.xrp_issuer_copied, Message::CopyIssuerAddress(t.code), cp, scale),
    ]
    .width(Length::Fill);
    if let Some(own) = own {
        body = body.push(address_fact("Your address", &own, state.xrp_wallet_copy_feedback, Message::WalletCopyAddress, cp, scale));
    }
    body = body
        .push(drow(
            "Limit",
            match limit {
                Some(l) => vec![(limit_text(l), cp.text), (format!(" {}", t.ticker), cp.faint)],
                None => vec![(NA.to_string(), cp.faint)],
            },
            3.0,
            cp,
            scale,
        ))
        .push(drow("Reserve", reserve_runs(cp), 3.0, cp, scale))
        .push(Space::new().height(8.0 * scale).boxed())
        .push(text(note).font(MONO).size(HINT * scale).color(cp.dim).boxed())
        .push(Space::new().height(12.0 * scale).boxed())
        .push(button_pair(
            quiet_button("Cancel", Message::TokenDetailsDismissed, cp, scale),
            danger_button("Remove trustline", live, Message::DisableTokenClicked(t.code), cp, scale),
            scale,
        ))
        .push(Space::new().height(2.0 * scale).boxed());
    scroller(body.boxed(), cp, scale)
}

fn address_fact<'a>(key: &str, address: &str, copied: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(
        row![
            text(key.to_string()).size(ROW_KEY * scale).color(cp.dim),
            Space::new().width(Length::Fill),
            text(grid::elide_addr(address)).font(MONO).size(ROW_VALUE * scale).color(cp.text),
            Space::new().width(6.0 * scale),
            grid::copy_glyph(copied, msg, cp, scale),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(3.0 * scale).bottom(3.0 * scale))
    .boxed()
}
