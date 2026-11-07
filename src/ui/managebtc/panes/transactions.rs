use iced::widget::Column;
use iced::widget::Row;
use iced::Widget as _;
use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::channel::{CHANNEL, HistoryList};
use crate::controller::app_state::AppState;
use crate::controller::message::Message;
use crate::ui::components::compact;
use crate::ui::components::grid::{self, group_rule, NA, ROW_KEY, ROW_VALUE, STRIP};
use crate::ui::managebtc::btcbump;
use crate::ui::managebtc::btcdashboard::fee_dot;
use crate::ui::managebtc::btctransactions::{self as tx, Fact, Tx};
use crate::ui::managexrp::panes::{
    empty, end_row, group_head, pressable, runs, scroller, SUB, SYM, TOKEN_ROW_PAD_H, TOKEN_ROW_PAD_V, VAL,
};
use crate::utils::fonts::MONO;
use crate::utils::theme::CompactPalette;

pub fn title(state: &AppState) -> &'static str {
    if state.btc_bump_txid.is_some() {
        "bump fee"
    } else if selected(state).is_some() {
        "details"
    } else {
        "transactions"
    }
}

pub fn meta<'a>(state: &AppState, cp: &'static CompactPalette, scale: f32) -> Option<Element<'a, Message>> {
    if state.btc_bump_txid.is_some() {
        return None;
    }
    let t = selected(state)?;
    Some(text(t.state_label()).font(MONO).size(STRIP * scale).color(t.state_color(cp)).boxed())
}

fn selected(state: &AppState) -> Option<Tx> {
    let txid = state.btc_tx_selected.as_deref()?;
    let ours: Vec<String> = crate::wallet::btc_address().into_iter().collect();
    CHANNEL.btc_transactions_rx.borrow().transactions.get(txid).map(|t| Tx::from(t, &ours))
}

pub fn view<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.btc_bump_txid.is_some() {
        return if state.btc_bump_locked.is_some() {
            btcbump::sign(state, cp, scale)
        } else {
            btcbump::rates(state, cp, scale)
        };
    }
    if let Some(t) = selected(state) {
        return details(state, &t, cp, scale);
    }

    let ours: Vec<String> = crate::wallet::btc_address().into_iter().collect();
    let (pending, settled) = tx::split(&ours);
    if pending.is_empty() && settled.is_empty() {
        return empty("No transactions to show", cp, scale);
    }

    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    if !pending.is_empty() {
        let mut band: Column<Element<'_, Message>> = column![group_head("pending", cp, scale)].width(Length::Fill);
        for (i, t) in pending.iter().enumerate() {
            band = band.push(tx_row(t, i + 1 < pending.len(), cp, scale));
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
        list = list.push(tx_row(t, true, cp, scale));
    }
    let end = {
        let txs = CHANNEL.btc_transactions_rx.borrow();
        end_row(&txs.page, txs.held(), HistoryList::BtcTransactions, cp, scale)
    };
    if let Some(end) = end {
        list = list.push(end);
    }
    scroller(list.boxed(), cp, scale)
}

fn tx_row(t: &Tx, rule: bool, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let left = column![
        text(t.direction_word()).size(SYM * scale).color(cp.text).wrapping(Wrapping::None),
        Space::new().height(3.0 * scale),
        text(t.state_label()).font(MONO).size(SUB * scale).color(t.state_color(cp)).wrapping(Wrapping::None),
    ];
    let right = column![
        row![
            text(t.amount_str()).font(MONO).size(VAL * scale).color(cp.text).wrapping(Wrapping::None),
            Space::new().width(5.0 * scale),
            text("BTC").font(MONO).size(SUB * scale).color(cp.muted),
        ]
        .align_y(Alignment::End),
        Space::new().height(3.0 * scale),
        text(tx::stamp_of(t))
            .font(MONO)
            .size(SUB * scale)
            .color(if t.at().is_some() { cp.muted } else { cp.faint })
            .wrapping(Wrapping::None),
    ]
    .align_x(Alignment::End);
    let line = container(row![left, Space::new().width(Length::Fill), right].spacing(12.0 * scale).align_y(Alignment::Center))
        .width(Length::Fill)
        .padding(
            Padding::new(0.0)
                .top(TOKEN_ROW_PAD_V * scale)
                .bottom(TOKEN_ROW_PAD_V * scale)
                .left(TOKEN_ROW_PAD_H * scale)
                .right(TOKEN_ROW_PAD_H * scale),
        );

    let mut col = column![pressable(line.boxed(), Some(Message::BtcTxSelected(t.txid.clone())), cp)].width(Length::Fill);
    if rule {
        col = col.push(
            container(compact::hairline(cp.rule))
                .width(Length::Fill)
                .padding(Padding::new(0.0).left(TOKEN_ROW_PAD_H * scale).right(TOKEN_ROW_PAD_H * scale))
                .boxed(),
        );
    }
    col.boxed()
}

fn details<'a>(state: &'a AppState, t: &Tx, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let (tiers, tip) = {
        let node = CHANNEL.btc_node_rx.borrow();
        (node.tiers, node.tip_height)
    };
    let copied = state.btc_detail_copied.as_deref();
    let (stamp, stamp_ink) = t.at().map_or((NA.to_string(), cp.faint), |s| (tx::full_stamp(s), cp.dim));
    let hero = grid::hero(t.amount_str(), "BTC".to_string(), cp, scale);

    let mut body = column![hero, text(stamp).font(MONO).size(SUB * scale).color(stamp_ink), group_rule(cp, scale)]
        .width(Length::Fill);
    for f in tx::facts(t, tiers, tip, cp) {
        body = body.push(fact_row(f, copied, cp, scale));
    }

    let back = grid::quiet_button("Back", Message::BtcTxBack, cp, scale);
    let buttons = if t.can_bump() {
        let bump = grid::pane_button("Bump fee", true, false, Message::BtcBumpClicked(t.txid.clone()), cp, scale);
        grid::button_pair(back, bump, scale)
    } else {
        back
    };
    column![scroller(body.boxed(), cp, scale), Space::new().height(12.0 * scale), buttons]
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed()
}

fn fact_row(f: Fact, copied: Option<&str>, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let copy = f.copy.map(|full| {
        let done = copied == Some(full.as_str());
        grid::copy_glyph(done, Message::BtcCopyDetail(full), cp, scale)
    });
    let key = text(f.key).size(ROW_KEY * scale).color(cp.dim).wrapping(Wrapping::None);
    let pad = Padding::new(0.0).top(3.0 * scale).bottom(3.0 * scale);
    let mut value: Row<Element<'static, Message>> = row![].spacing(6.0 * scale).align_y(Alignment::Center);
    if let Some(c) = f.dot {
        value = value.push(fee_dot(c, scale));
    }
    value = value.push(runs(f.segs, ROW_VALUE * scale, ROW_KEY * scale));
    if let Some(c) = copy {
        value = value.push(c);
    }
    container(row![key, Space::new().width(Length::Fill), value].align_y(Alignment::Center))
        .width(Length::Fill)
        .padding(pad)
        .boxed()
}
