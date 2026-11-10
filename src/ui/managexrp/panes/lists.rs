
use iced::widget::Column;
use iced::Widget as _;
use iced::widget::{column, container, responsive, text, Space};
use iced::{Element, Length, Size};

use crate::channel::{CHANNEL, HistoryList, TransactionStatus};
use crate::controller::app_state::AppState;
use crate::controller::message::{Message, SecureField};
use crate::controller::xrp::xrp_reserve;
use crate::ui::components::grid::{self, group_rule, HINT};
use crate::ui::components::signing::SignFields;
use crate::ui::components::tx_panel::Fact;
use crate::ui::managebtc::btctransactions::full_stamp;
use crate::ui::managexrp::xrpdashboard::{drow, STRIP};
use crate::ui::managexrp::xrptransactions::{self as tx, six, Kind, Tx};
use crate::utils::fonts::MONO;
use crate::utils::orderbook::fmt_price;
use crate::utils::theme::CompactPalette;

use super::{
    button_pair, danger_button, empty, end_row, fact, fact_row, fee_row, group_head, history_row, quiet_button, scroller,
    sign_block, sign_mode, HistoryRow, NA, SUB,
};

pub fn title(state: &AppState) -> &'static str {
    if state.cancel_offer_sequence.is_some() {
        "cancel order"
    } else if selected(state).is_some() {
        "details"
    } else {
        "transactions"
    }
}

pub fn meta<'a>(state: &AppState, cp: &'static CompactPalette, scale: f32) -> Option<Element<'a, Message>> {
    if state.cancel_offer_sequence.is_some() {
        return None;
    }
    let t = selected(state)?;
    Some(text(t.state_label()).font(MONO).size(STRIP * scale).color(t.state_color(cp)).boxed())
}

fn own() -> String {
    CHANNEL.wallet_balance_rx.borrow().1.clone().unwrap_or_default()
}

fn reserve() -> Option<f64> {
    xrp_reserve().map(|r| r.per_object)
}

fn selected(state: &AppState) -> Option<Tx> {
    let hash = state.xrp_tx_selected.as_deref()?;
    let own = own();
    CHANNEL.transactions_rx.borrow().transactions.get(hash).map(|t| Tx::from(t, &own))
}

pub fn view<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.cancel_offer_sequence.is_some() {
        return cancel(state, cp, scale);
    }
    if let Some(t) = selected(state) {
        return details(state, &t, cp, scale);
    }

    let (open, settled) = tx::split(&own());
    if open.is_empty() && settled.is_empty() {
        return empty("No transactions to show", cp, scale);
    }
    let reserve = reserve();
    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    if !open.is_empty() {
        let mut band: Column<Element<'_, Message>> = column![group_head("open", cp, scale)].width(Length::Fill);
        for (i, t) in open.iter().enumerate() {
            band = band.push(row(t, reserve, i + 1 < open.len(), cp, scale));
        }
        list = list.push(
            container(band)
                .width(Length::Fill)
                .style(move |_| container::Style { background: Some(cp.panel.into()), ..Default::default() })
                .boxed(),
        );
        list = list.push(group_head("history", cp, scale));
    }
    for t in &settled {
        list = list.push(row(t, reserve, true, cp, scale));
    }
    let end = {
        let txs = CHANNEL.transactions_rx.borrow();
        end_row(&txs.page, txs.held(), HistoryList::XrpTransactions, cp, scale)
    };
    if let Some(end) = end {
        list = list.push(end);
    }
    scroller(list.boxed(), cp, scale)
}

fn row(t: &Tx, reserve: Option<f64>, rule: bool, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let (figure, figure_ink, unit) = match t.amount_parts(reserve) {
        Some((f, u)) => (f, cp.text, u),
        None => (NA.to_string(), cp.faint, String::new()),
    };
    let row = HistoryRow {
        word: t.word(),
        status: (t.state_label(), t.state_color(cp)),
        figure,
        figure_ink,
        unit,
        stamp: t.closed_at.map_or(NA.to_string(), tx::stamp),
        stamp_ink: if t.closed_at.is_some() { cp.muted } else { cp.faint },
        on_press: Message::XrpTxSelected(t.hash.clone()),
    };
    history_row(row, rule, cp, scale)
}

fn details<'a>(state: &'a AppState, t: &Tx, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let reserve = reserve();
    let copied = state.xrp_detail_copied.as_deref();
    let hero = match t.amount_parts(reserve) {
        Some((f, u)) => grid::hero_ink(f, u, if t.status == TransactionStatus::Failed { cp.dim } else { cp.text }, cp, scale),
        None => grid::hero_ink(NA.to_string(), String::new(), cp.faint, cp, scale),
    };
    let (stamp, stamp_ink) = t.closed_at.map_or((NA.to_string(), cp.faint), |s| (full_stamp(s), cp.dim));

    let mut body = column![hero, text(stamp).font(MONO).size(SUB * scale).color(stamp_ink), group_rule(cp, scale)]
        .width(Length::Fill);
    for f in tx::facts(t, reserve, cp) {
        body = body.push(fact_line(f, copied, cp, scale));
    }
    if let Some(note) = tx::note(t, reserve) {
        body = body
            .push(Space::new().height(8.0 * scale).boxed())
            .push(text(note).font(MONO).size(HINT * scale).color(cp.dim).boxed());
    }

    let back = quiet_button("Back", Message::XrpTxBack, cp, scale);
    let buttons = if t.open() {
        let seq = t.cancellable();
        let cancel = danger_button("Cancel order", seq.is_some(), Message::CancelOrderClicked(seq.unwrap_or(0)), cp, scale);
        button_pair(back, cancel, scale)
    } else {
        back
    };
    column![scroller(body.boxed(), cp, scale), Space::new().height(12.0 * scale), buttons]
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed()
}

fn fact_line(f: Fact, copied: Option<&str>, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let copy = f.copy.map(|full| {
        let done = copied == Some(full.as_str());
        grid::copy_glyph(done, Message::XrpCopyDetail(full), cp, scale)
    });
    fact_row(f.key, None, f.segs, copy, cp, scale)
}

fn cancel<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let order = selected(state).filter(|t| t.kind == Kind::Offer);
        let lower = |u: &str| format!(" {}", u.to_lowercase());
        let (key, base_runs) = match &order {
            Some(t) => {
                let (v, unit) = t.base();
                let runs = match v {
                    Some(v) => vec![(six(v), cp.text), (lower(&unit), cp.faint)],
                    None => vec![(NA.to_string(), cp.faint)],
                };
                (t.side().key(), runs)
            }
            None => ("order", vec![(NA.to_string(), cp.faint)]),
        };
        let price_runs = match order.as_ref().and_then(|t| t.price().map(|p| (p, t.rate_unit()))) {
            Some((p, unit)) => {
                let mut runs = vec![(fmt_price(p), cp.text)];
                if !unit.is_empty() {
                    runs.push((lower(&unit), cp.faint));
                }
                runs
            }
            None => vec![(NA.to_string(), cp.faint)],
        };
        let reserve_runs = match reserve() {
            Some(r) => vec![(six(r), cp.text), (" xrp".to_string(), cp.faint)],
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
            line: &state.phrase_line,
            on_submit: Message::CancelSubmitClicked,
        };
        let armed = state.cancel_offer_sequence.is_some();
        let buttons = button_pair(
            quiet_button("Back", Message::CancelDismissed, cp, scale),
            danger_button("Sign & cancel", armed && fields.can_submit(), Message::CancelSubmitClicked, cp, scale),
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
            fact(key, base_runs, 0.0, cp, scale),
            drow("limit price", price_runs, 3.0, cp, scale),
            drow("reserve returned", reserve_runs, 3.0, cp, scale),
            fee_row(state.cancel_fee, 3.0, cp, scale),
            sign,
            Space::new().height(2.0 * scale),
        ]
        .width(Length::Fill);
        scroller(body.boxed(), cp, scale)
    })
    .boxed()
}
