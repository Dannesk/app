//! The `transactions` and `orders` panes — one list each, and the `cancel
//! offer` face the orders pane becomes.
//!
//! `transactions` holds payments, trustsets and cancels; `orders` holds
//! offers. The rows are the shipped transactions row model
//! ([`crate::ui::managexrp::xrptransactions`]) narrowed: every column
//! survives, the detail dropdown is unchanged, and it scrolls with the rest —
//! a dropdown never resizes the pane. **No tabs, no counts, no footer**: the
//! title says what the list is and the list says how long it is.
//!
//! The orders pane differs from the transactions spec in one place: the type
//! column reads `sell` / `buy` instead of `trade`, and a `settled` group head
//! appears only when there are settled orders under live ones. `open` stays
//! `open`. `cancel order ›` on an open row turns the pane into the cancel
//! form: the order and what is still resting, the fee, the sign block,
//! `Keep it` / `Cancel order`.

use iced::widget::Column;
use iced::Widget as _;
use iced::widget::{column, responsive, Space};
use iced::{Element, Length, Size};

use crate::channel::CHANNEL;
use crate::controller::app_state::AppState;
use crate::controller::message::{Message, SecureField};
use crate::ui::components::signing::SignFields;
use crate::ui::components::tx_panel::Detail;
use crate::ui::managexrp::xrptrade::disp;
use crate::ui::managexrp::xrptransactions::{self as tx, Kind, Tx};
use crate::ui::managexrp::xrpdashboard::drow;
use crate::utils::theme::CompactPalette;

use super::{button_pair, empty, fact, fee_row, group_head, list_row, primary, quiet_button, scroller, sign_block, sign_mode, ListRow, NA};

/// The orders pane's title: `cancel offer` while the form is up.
pub fn orders_title(state: &AppState) -> &'static str {
    if state.cancel_offer_sequence.is_some() { "cancel offer" } else { "orders" }
}

fn own() -> String {
    CHANNEL.wallet_balance_rx.borrow().1.clone().unwrap_or_default()
}

fn row_of(t: &Tx, side: bool, state: &AppState, cp: &'static CompactPalette) -> ListRow {
    let expanded = state.tx_expanded.as_deref() == Some(t.hash.as_str());
    ListRow {
        kind: if side { t.side_word() } else { t.kind.word() },
        side,
        amount: tx::amount_segs(t, cp),
        status: (t.state_label(), t.state_color(cp)),
        time: t.closed_at.map_or(NA.to_string(), tx::stamp),
        on_press: Message::TxCardToggled(t.hash.clone()),
        detail: expanded.then(|| Detail { lines: tx::detail_lines(t, cp), tail: tx::tail(t, cp) }),
    }
}

pub fn transactions<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let own = own();
    let (open, settled) = tx::split(&own);
    // Newest first, every kind but an offer; an offer never rests here.
    let rows: Vec<&Tx> = open.iter().chain(settled.iter()).filter(|t| t.kind != Kind::Offer).collect();
    if rows.is_empty() {
        return empty("No transactions to show", cp, scale);
    }
    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for t in rows {
        list = list.push(list_row(row_of(t, false, state, cp), cp, scale));
    }
    scroller(list.boxed(), cp, scale)
}

pub fn orders<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.cancel_offer_sequence.is_some() {
        return cancel(state, cp, scale);
    }
    let own = own();
    let (open, settled) = tx::split(&own);
    let live: Vec<&Tx> = open.iter().filter(|t| t.kind == Kind::Offer).collect();
    let done: Vec<&Tx> = settled.iter().filter(|t| t.kind == Kind::Offer).collect();
    if live.is_empty() && done.is_empty() {
        return empty("No orders to show", cp, scale);
    }
    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for t in &live {
        list = list.push(list_row(row_of(t, true, state, cp), cp, scale));
    }
    if !live.is_empty() && !done.is_empty() {
        list = list.push(group_head("settled", cp, scale));
    }
    for t in &done {
        list = list.push(list_row(row_of(t, true, state, cp), cp, scale));
    }
    scroller(list.boxed(), cp, scale)
}

/// The cancel form: the offer restated, the fee, the sign block. Same
/// component as `enable trustline`, two callers.
fn cancel<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let seq = state.cancel_offer_sequence;
        let own = own();
        let order = {
            let txs = CHANNEL.transactions_rx.borrow();
            txs.transactions
                .values()
                .find(|t| t.sequence.is_some() && t.sequence == seq)
                .map(|t| Tx::from(t, &own))
        };

        // `offer · sell 250.000000 XRP` — the side and what is still resting
        // on the book, which is the figure the cancel takes back.
        let offer_runs = match &order {
            Some(t) => {
                let mut runs = vec![(format!("{} ", t.side_word()), cp.text)];
                match t.amount_parts() {
                    Some((figure, unit)) => {
                        runs.push((figure, cp.text));
                        runs.push((format!(" {}", disp(unit).to_lowercase()), cp.faint));
                    }
                    None => runs.push((NA.to_string(), cp.faint)),
                }
                runs
            }
            None => vec![(seq.map_or(NA.to_string(), |s| format!("#{s}")), cp.text)],
        };
        let at_runs = match order.as_ref().and_then(|t| t.price.clone()) {
            Some(p) => vec![(p, cp.text)],
            None => vec![(NA.to_string(), cp.faint)],
        };

        let fields = SignFields {
            mode: sign_mode(),
            secret: SecureField::CancelPassphrase,
            secret_buf: &state.cancel_passphrase,
            secret_reveal: state.cancel_passphrase_reveal,
            phrase: SecureField::CancelSeed,
            phrase_buf: &state.cancel_seed_text,
            phrase_reveal: state.cancel_seed_reveal,
            bip39: SecureField::CancelBip39,
            bip39_buf: &state.cancel_bip39,
            bip39_reveal: state.cancel_bip39_reveal,
            on_submit: Message::CancelSubmitClicked,
        };
        let armed = seq.is_some();
        let live = armed && fields.can_submit();
        let buttons = button_pair(
            quiet_button("Keep it", Message::CancelDismissed, cp, scale),
            primary("Cancel order", live, Message::CancelSubmitClicked, cp, scale),
            scale,
        );
        let sign = sign_block(
            &fields,
            &state.cancel_passphrase,
            &state.cancel_seed_text,
            &state.cancel_bip39,
            armed,
            buttons,
            inner,
            cp,
            scale,
        );

        let body = column![
            fact("offer", offer_runs, 0.0, cp, scale),
            drow("at", at_runs, 3.0, cp, scale),
            fee_row(3.0, cp, scale),
            sign,
            Space::new().height(2.0 * scale),
        ]
        .width(Length::Fill);
        scroller(body.boxed(), cp, scale)
    })
    .boxed()
}
