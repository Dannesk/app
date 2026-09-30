//! The `tokens` and `available tokens` panes — held vs. enableable, one list
//! each — and the two faces they become: `disable trustline` on the first,
//! `enable trustline` on the second.
//!
//! Held rows: symbol over issuer, balance over its fiat approximation in
//! `state.base_currency`, then `disable`, a link that is live only at an
//! exact zero balance: the ledger accepts a limit-0 TrustSet on any balance
//! but DELETES the line — and refunds its reserve — only at zero, so a live
//! link at any other balance would sign a transaction that refunds nothing.
//! No XRP row: the balance pane is two inches away.
//! Available rows end in `enable`, a link, dead until the ledger has said
//! what a trustline costs and the account can afford it — the tokens page's
//! own gate (`reserve_inc` + the open-ledger fee), never a number of ours.
//!
//! Either link swaps its pane for the form: trustline · reserve · network
//! fee, the sign block, `Cancel` / `Enable trustline` (or `Disable`). One
//! form, two callers, mirrored down to the row labels — the enable's reserve
//! is what the line will lock, the disable's is what comes back.

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

/// The available pane's title: `enable trustline` while the form is up.
pub fn available_title(state: &AppState) -> &'static str {
    if state.show_enable { "enable trustline" } else { "available tokens" }
}

/// The held pane's title: `disable trustline` while its form is up.
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
    // Registry order, never by balance: a list that reorders itself as
    // prices tick moves the row out from under the cursor.
    let held: Vec<&tokens::TokenDef> = tokens::TOKENS.iter().filter(|t| CHANNEL.token(t.code).1).collect();
    if held.is_empty() {
        return empty("No tokens to show", cp, scale);
    }
    // Removing a line costs the fee and nothing else — the reserve is what
    // comes back. Dead until the ledger has quoted the fee.
    let can_pay = xrp_available().is_some_and(|a| a >= xrp_fee());
    let mut list = column![].width(Length::Fill);
    for t in held {
        let balance = CHANNEL.token(t.code).0;
        let fiat = balance * price::cross(t.rate_key, ccy);
        let (bal_str, fiat_str) = if hide {
            (MASK.to_string(), MASK.to_string())
        } else {
            (format_token_amount(balance, 6), format!("\u{2248} {} {}", money(fiat), ccy.to_lowercase()))
        };
        let figures = column![
            text(bal_str).font(MONO).size(VAL * scale).color(cp.text),
            Space::new().height(3.0 * scale),
            text(fiat_str).font(MONO).size(SUB * scale).color(cp.muted),
        ]
        .align_x(Alignment::End);
        // Exact zero, not "rounds to zero": the relay writes the ledger's own
        // decimal string and the f64 it parses to is nonzero for any nonzero
        // ledger amount. A dust tail keeps the link dark until it is sent
        // or sold.
        let can_disable = can_pay && balance == 0.0;
        let ink = if can_disable { cp.dim } else { cp.faint };
        let link = link_word("DISABLE".to_string(), ink, can_disable.then_some(Message::DisableTokenClicked(t.code)), cp, scale);
        let right = row![figures, link].spacing(10.0 * scale).align_y(Alignment::Center);
        list = list.push(token_row(t.display, issuer_line(t), right.into(), false, cp, scale));
    }
    scroller(list.into(), cp, scale)
}

pub fn available<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.show_enable {
        return line_form(state, Face::Enable, state.xrp_token_tab.code().and_then(tokens::by_code), cp, scale);
    }
    let available: Vec<&'static tokens::TokenDef> = tokens::TOKENS.iter().filter(|t| !CHANNEL.token(t.code).1).collect();
    if available.is_empty() {
        return empty("No tokens available", cp, scale);
    }
    // One gate for every row: the owner reserve the new line locks, plus the
    // fee that posts it — both the ledger's numbers, dead until they land.
    let can_enable = xrp_reserve().is_some_and(|r| r.affords_new_object(xrp_fee()));
    let mut list = column![].width(Length::Fill);
    for t in available {
        let ink = if can_enable { cp.dim } else { cp.faint };
        let link = link_word("ENABLE".to_string(), ink, can_enable.then_some(Message::EnableTokenClicked(t.code)), cp, scale);
        list = list.push(token_row(t.display, issuer_line(t), link, false, cp, scale));
    }
    scroller(list.into(), cp, scale)
}

/// Which way the TrustSet goes. Everything that differs between the two
/// faces hangs off this: the buffers, the messages, the button's verb.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Enable,
    Disable,
}

/// The TrustSet form: the line, the reserve it locks or returns, the fee,
/// the sign block. `token` is `None` only if the face was raised with no
/// asset armed; the form then shows `n/a` and never goes live.
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
        // The disable's gate, held all the way to the button: a balance that
        // lands while the form is up darkens it rather than signing a
        // TrustSet the ledger would honour and not refund.
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

        // Enable: what the new line locks. Disable: what the deleted line
        // gives back. The number is the same ledger figure either way.
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
        scroller(body.into(), cp, scale)
    })
    .into()
}
