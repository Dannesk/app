
use iced::widget::Column;
use iced::Widget as _;
use iced::widget::{column, responsive, row, text, Space};
use iced::{Alignment, Element, Length, Size};

use crate::channel::CHANNEL;
use crate::controller::app_state::AppState;
use crate::controller::message::{Message, SecureField};
use crate::controller::xrp::{xrp_available, xrp_fee, xrp_reserve};
use crate::ui::components::signing::SignFields;
use crate::utils::money;
use crate::ui::managexrp::tokens::short_issuer;
use crate::ui::managexrp::xrpdashboard::drow;
use crate::utils::fonts::MONO;
use crate::utils::theme::CompactPalette;
use crate::utils::{format_token_amount, price, tokens};

use super::{button_pair, empty, fee_row, link_word, primary, quiet_button, scroller, sign_block, sign_mode, token_row, NA, SUB, VAL};

const MASK: &str = "\u{2022}\u{2022}\u{2022}\u{2022}";

pub fn available_title(state: &AppState) -> &'static str {
    if state.show_enable { "enable trustline" } else { "available tokens" }
}

pub fn held_title(state: &AppState) -> &'static str {
    if state.disable_token.is_some() { "disable trustline" } else { "tokens" }
}

fn issuer_line(t: &tokens::TokenDef) -> String {
    format!("{} \u{b7} {}", t.issuer_name, short_issuer(t.issuer))
}

pub fn held<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if let Some(code) = state.disable_token {
        return line_form(state, Face::Disable, tokens::by_code(code), cp, scale);
    }
    let ccy = state.base_currency.code();
    let hide = state.hide_balance;
    let held: Vec<&tokens::TokenDef> = tokens::TOKENS.iter().filter(|t| CHANNEL.token(t.code).1).collect();
    if held.is_empty() {
        return empty("No tokens to show", cp, scale);
    }
    let can_pay = xrp_available().is_some_and(|a| a >= xrp_fee());
    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for t in held {
        let balance = CHANNEL.token(t.code).0;
        let rate = price::cross(t.rate_key, ccy);
        let (bal_str, fiat_str) = if hide {
            (MASK.to_string(), MASK.to_string())
        } else if rate > 0.0 {
            (format_token_amount(balance, 6), format!("\u{2248} {} {}", money(balance * rate), ccy.to_lowercase()))
        } else {
            (format_token_amount(balance, 6), NA.to_string())
        };
        let figures = column![
            text(bal_str).font(MONO).size(VAL * scale).color(cp.text),
            Space::new().height(3.0 * scale),
            text(fiat_str).font(MONO).size(SUB * scale).color(cp.muted),
        ]
        .align_x(Alignment::End);
        let can_disable = can_pay && balance == 0.0;
        let ink = if can_disable { cp.dim } else { cp.faint };
        let link = link_word("DISABLE".to_string(), ink, can_disable.then_some(Message::DisableTokenClicked(t.code)), cp, scale);
        let right = row![figures, link].spacing(10.0 * scale).align_y(Alignment::Center);
        list = list.push(token_row(t.display, issuer_line(t), right.boxed(), false, cp, scale));
    }
    scroller(list.boxed(), cp, scale)
}

pub fn available<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.show_enable {
        return line_form(state, Face::Enable, state.xrp_token_tab.code().and_then(tokens::by_code), cp, scale);
    }
    let available: Vec<&'static tokens::TokenDef> = tokens::TOKENS.iter().filter(|t| !CHANNEL.token(t.code).1).collect();
    if available.is_empty() {
        return empty("No tokens available", cp, scale);
    }
    let can_enable = xrp_reserve().is_some_and(|r| r.affords_new_object(xrp_fee()));
    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for t in available {
        let ink = if can_enable { cp.dim } else { cp.faint };
        let link = link_word("ENABLE".to_string(), ink, can_enable.then_some(Message::EnableTokenClicked(t.code)), cp, scale);
        list = list.push(token_row(t.display, issuer_line(t), link, false, cp, scale));
    }
    scroller(list.boxed(), cp, scale)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Enable,
    Disable,
}

fn line_form<'a>(
    state: &'a AppState,
    face: Face,
    token: Option<&'static tokens::TokenDef>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let line_runs = match token {
            Some(t) => vec![(format!("{} ", t.display), cp.text), (t.issuer_name.to_string(), cp.faint)],
            None => vec![(NA.to_string(), cp.faint)],
        };
        let reserve_runs = match xrp_reserve() {
            Some(r) => vec![(format_token_amount(r.per_object, 6), cp.text), (" xrp".to_string(), cp.faint)],
            None => vec![(NA.to_string(), cp.faint)],
        };

        let (fields, secret, phrase, bip39, dismiss, verb) = match face {
            Face::Enable => (
                SignFields {
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
                    on_submit: Message::EnableSubmitClicked,
                },
                &state.enable_passphrase,
                &state.enable_seed_text,
                &state.enable_bip39,
                Message::EnableDismissed,
                "Enable trustline",
            ),
            Face::Disable => (
                SignFields {
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
                    on_submit: Message::DisableSubmitClicked,
                },
                &state.disable_passphrase,
                &state.disable_seed_text,
                &state.disable_bip39,
                Message::DisableDismissed,
                "Disable trustline",
            ),
        };
        let armed = match (face, token) {
            (Face::Enable, Some(_)) => true,
            (Face::Disable, Some(t)) => CHANNEL.token(t.code).0 == 0.0,
            (_, None) => false,
        };
        let live = armed && fields.can_submit();
        let buttons = button_pair(
            quiet_button("Cancel", dismiss, cp, scale),
            primary(verb, live, fields.on_submit.clone(), cp, scale),
            scale,
        );
        let sign = sign_block(&fields, secret, phrase, bip39, armed, buttons, inner, cp, scale);

        let reserve_key = match face {
            Face::Enable => "reserve",
            Face::Disable => "refund",
        };
        let body = column![
            drow("trustline", line_runs, 0.0, cp, scale),
            drow(reserve_key, reserve_runs, 3.0, cp, scale),
            fee_row(3.0, cp, scale),
            sign,
            Space::new().height(2.0 * scale),
        ]
        .width(Length::Fill);
        scroller(body.boxed(), cp, scale)
    })
    .boxed()
}
