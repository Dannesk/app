use iced::widget::Column;
use iced::widget::Row;
use iced::Widget as _;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::channel::{CHANNEL, HistoryList};
use crate::controller::app_state::AppState;
use crate::controller::message::Message;
use crate::ui::components::{compact, grid};
use crate::ui::managebtc::btcbump;
use crate::ui::managebtc::btcdashboard::fee_dot;
use crate::ui::managebtc::btctransactions::{self as tx, Fact, Tx};
use crate::ui::managexrp::panes::{
    empty, end_row, group_head, inset_rule, link, runs, scroller, DETAIL_INDENT, DETAIL_ROW_PAD_V, KEY, KEY_W,
    TOKEN_ROW_PAD_H, VALUE,
};
use crate::utils::fonts::MONO;
use crate::utils::theme::CompactPalette;

const CARET: f32 = 8.0;
const CARET_W: f32 = 6.0;
const TYPE: f32 = 10.0;
const TYPE_W: f32 = 42.0;
pub(crate) const CELL: f32 = 9.5;
const STATUS_W: f32 = 48.0;
pub(crate) const TIME_W: f32 = 66.0;
const ROW_GAP: f32 = 5.0;
const ROW_PAD_V: f32 = 6.0;
const ROW_PAD_H: f32 = 11.0;

pub fn title(state: &AppState) -> &'static str {
    if state.btc_bump_txid.is_some() { "bump fee" } else { "transactions" }
}

pub fn view<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.btc_bump_txid.is_some() {
        return if state.btc_bump_locked.is_some() {
            btcbump::sign(state, cp, scale)
        } else {
            btcbump::rates(state, cp, scale)
        };
    }

    let ours: Vec<String> = crate::wallet::btc_address().into_iter().collect();
    let (pending, settled) = tx::split(&ours);
    if pending.is_empty() && settled.is_empty() {
        return empty("No transactions to show", cp, scale);
    }

    let (tiers, tip) = {
        let node = CHANNEL.btc_node_rx.borrow();
        (node.tiers, node.tip_height)
    };
    let selected = state.btc_tx_selected.as_deref();
    let copied = state.btc_detail_copied.as_deref();
    let row_of = |t: &Tx, rule: bool| -> Element<'static, Message> {
        let detail = (selected == Some(t.txid.as_str())).then(|| {
            let bump = t.can_bump().then(|| Message::BtcBumpClicked(t.txid.clone()));
            detail(tx::facts(t, tiers, tip, cp), bump, copied, rule, cp, scale)
        });
        tx_row(t, detail, rule, cp, scale)
    };

    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    if !pending.is_empty() {
        let mut band: Column<Element<'_, Message>> = column![group_head("pending", cp, scale)].width(Length::Fill);
        for (i, t) in pending.iter().enumerate() {
            band = band.push(row_of(t, i + 1 < pending.len()));
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
        list = list.push(row_of(t, true));
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

fn tx_row(t: &Tx, detail: Option<Element<'static, Message>>, rule: bool, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let open = detail.is_some();
    let quiet = if open { cp.dim } else { cp.faint };
    let time_ink = if open { cp.dim } else { cp.muted };

    let cell = |el: Element<'static, Message>, w: f32, align: Alignment| -> Element<'static, Message> {
        container(el).width(Length::Fixed(w * scale)).align_x(align).boxed()
    };

    let line = row![
        cell(text(if open { "\u{25be}" } else { "\u{25b8}" }).font(MONO).size(CARET * scale).color(quiet).boxed(), CARET_W, Alignment::Start),
        cell(text(t.direction_word()).size(TYPE * scale).color(cp.text).wrapping(Wrapping::None).boxed(), TYPE_W, Alignment::Start),
        container(text(t.amount_str()).font(MONO).size(CELL * scale).color(cp.text).wrapping(Wrapping::None))
            .width(Length::Fill)
            .align_x(Alignment::End),
        cell(text(t.state_label()).font(MONO).size(CELL * scale).color(t.state_color(cp)).wrapping(Wrapping::None).boxed(), STATUS_W, Alignment::Start),
        cell(text(tx::stamp_of(t)).font(MONO).size(CELL * scale).color(time_ink).wrapping(Wrapping::None).boxed(), TIME_W, Alignment::End),
    ]
    .spacing(ROW_GAP * scale)
    .align_y(Alignment::Center);

    let hit = button(line)
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(ROW_PAD_V * scale).bottom(ROW_PAD_V * scale).left(ROW_PAD_H * scale).right(ROW_PAD_H * scale))
        .style(move |_, status| button::Style {
            background: Some(match status {
                button::Status::Hovered | button::Status::Pressed => cp.hover,
                _ if open => cp.hover,
                _ => Color::TRANSPARENT,
            }
            .into()),
            border: Border::default(),
            text_color: cp.text,
            shadow: Shadow::default(),
            snap: false,
        })
        .on_press(Message::BtcTxSelected(t.txid.clone()));

    let mut col = column![hit].width(Length::Fill);
    if let Some(d) = detail {
        col = col.push(d);
    } else if rule {
        col = col.push(inset_rule(cp, scale));
    }
    col.boxed()
}

fn detail(
    facts: Vec<Fact>,
    bump: Option<Message>,
    copied: Option<&str>,
    rule: bool,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'static, Message> {
    let mut col: Column<Element<'static, Message>> = column![].width(Length::Fill);
    for f in facts {
        let mut value: Row<Element<'static, Message>> = row![].spacing(5.0 * scale).align_y(Alignment::Center);
        if let Some(c) = f.dot {
            value = value.push(fee_dot(c, scale));
        }
        value = value.push(runs(f.segs, VALUE * scale, KEY * scale));
        if let Some(full) = f.copy {
            let done = copied == Some(full.as_str());
            value = value.push(Space::new().width(5.0 * scale).boxed()).push(grid::copy_glyph(done, Message::BtcCopyDetail(full), cp, scale));
        }
        col = col.push(
            row![
                container(text(f.key).size(KEY * scale).color(cp.dim).wrapping(Wrapping::None)).width(Length::Fixed(KEY_W * scale)),
                container(value).width(Length::Fill),
            ]
            .spacing(10.0 * scale)
            .align_y(Alignment::Center)
            .padding(Padding::new(0.0).top(DETAIL_ROW_PAD_V * scale).bottom(DETAIL_ROW_PAD_V * scale).left(DETAIL_INDENT * scale))
            .boxed(),
        );
    }
    if let Some(msg) = bump {
        col = col.push(container(link("bump fee", cp.dim, msg, cp, scale)).padding(Padding::new(0.0).top(7.0 * scale).left(DETAIL_INDENT * scale)).boxed());
    }
    let mut block = column![
        container(col)
            .width(Length::Fill)
            .padding(Padding::new(0.0).bottom(9.0 * scale).left(TOKEN_ROW_PAD_H * scale).right(TOKEN_ROW_PAD_H * scale)),
    ]
    .width(Length::Fill);
    if rule {
        block = block.push(
            container(compact::hairline(cp.rule))
                .width(Length::Fill)
                .padding(Padding::new(0.0).left(TOKEN_ROW_PAD_H * scale).right(TOKEN_ROW_PAD_H * scale))
                .boxed(),
        );
    }
    block.boxed()
}
