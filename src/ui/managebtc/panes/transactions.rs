

use iced::widget::Column;
use iced::Widget as _;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, responsive, row, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow, Size};

use crate::channel::{CHANNEL, HistoryList};
use crate::controller::app_state::{AppState, EnableInputMode};
use crate::controller::message::{Message, SecureField};
use crate::ui::components::signing::SignFields;
use crate::ui::components::tx_panel::{Detail, Seg};
use crate::ui::managebtc::btcbump;
use crate::ui::managebtc::btctransactions::{self as tx, Ctx, Tx};
use crate::ui::managexrp::panes::{button_pair, detail_block, empty, end_row, inset_rule, primary, quiet_button, runs, scroller, sign_block};
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
    if state.btc_bump_txid.is_some() { "modify fee" } else { "transactions" }
}

pub fn view<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.btc_bump_txid.is_some() {
        return bump(state, cp, scale);
    }

    
    let ours: Vec<String> = crate::wallet::btc_address().into_iter().collect();
    let (pending, settled) = tx::split(&ours);
    if pending.is_empty() && settled.is_empty() {
        return empty("No transactions to show", cp, scale);
    }

    let ccy = state.base_currency.code();
    let ctx = Ctx { btc_rate: crate::utils::price::cross("BTC", ccy), ccy, tip: CHANNEL.btc_node_rx.borrow().tip_height, p: cp };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let selected = state.btc_tx_selected.as_deref();

    let mut list: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for t in pending.iter().chain(settled.iter()) {
        let detail = (selected == Some(t.txid.as_str()))
            .then(|| Detail {
                id: tx::id_line(t, state.btc_txid_copied.as_deref()),
                lines: tx::detail_lines(t, &ctx),
                tail: tx::tail(t, cp),
            });
        list = list.push(tx_row(t, tx::time_label(t, now), detail, cp, scale));
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


fn tx_row(t: &Tx, time: String, detail: Option<Detail>, cp: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let open = detail.is_some();
    let quiet = if open { cp.dim } else { cp.faint };
    let time_ink = if open { cp.dim } else { cp.muted };
    let (status, status_ink) = (t.state_label(), t.state_color(cp));

    let cell = |el: Element<'static, Message>, w: f32, align: Alignment| -> Element<'static, Message> {
        container(el).width(Length::Fixed(w * scale)).align_x(align).boxed()
    };

    let line = row![
        cell(text(if open { "\u{25be}" } else { "\u{25b8}" }).font(MONO).size(CARET * scale).color(quiet).boxed(), CARET_W, Alignment::Start),
        cell(text(t.direction_word()).size(TYPE * scale).color(cp.text).wrapping(Wrapping::None).boxed(), TYPE_W, Alignment::Start),
        container(runs(vec![Seg::mono(t.amount_str(), cp.text)], CELL * scale, CELL * scale))
            .width(Length::Fill)
            .align_x(Alignment::End),
        cell(text(status).font(MONO).size(CELL * scale).color(status_ink).wrapping(Wrapping::None).boxed(), STATUS_W, Alignment::Start),
        cell(text(time).font(MONO).size(CELL * scale).color(time_ink).wrapping(Wrapping::None).boxed(), TIME_W, Alignment::End),
    ]
    .spacing(ROW_GAP * scale)
    .align_y(Alignment::Center);

    let hit = button(line)
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(ROW_PAD_V * scale).bottom(ROW_PAD_V * scale).left(ROW_PAD_H * scale).right(ROW_PAD_H * scale))
        .style(move |_, status| button::Style {
            background: Some(match status {
                button::Status::Hovered | button::Status::Pressed => cp.hover,
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
    col = match detail {
        Some(d) => col.push(detail_block(d, cp, scale)),
        None => col.push(inset_rule(cp, scale)),
    };
    col.boxed()
}


fn bump<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let (lead, armed) = btcbump::lead(state, cp, scale);
        let mode = {
            let w = CHANNEL.bitcoin_wallet_rx.borrow();
            EnableInputMode::detect(w.2, w.3)
        };
        let fields = SignFields {
            mode,
            secret: SecureField::BtcBumpPassphrase,
            secret_buf: &state.btc_bump_passphrase,
            secret_reveal: state.btc_bump_passphrase_reveal,
            phrase: SecureField::BtcBumpSeed,
            phrase_buf: &state.btc_bump_seed_text,
            phrase_reveal: state.btc_bump_seed_reveal,
            bip39: SecureField::BtcBumpBip39,
            bip39_buf: &state.btc_bump_bip39,
            bip39_reveal: state.btc_bump_bip39_reveal,
            on_submit: Message::BtcBumpSubmitClicked,
        };
        let live = armed && fields.can_submit();
        let buttons = button_pair(
            quiet_button("Keep it", Message::BtcBumpDismissed, cp, scale),
            primary("Modify fee", live, Message::BtcBumpSubmitClicked, cp, scale),
            scale,
        );
        let sign = sign_block(
            &fields,
            &state.btc_bump_passphrase,
            &state.btc_bump_seed_text,
            &state.btc_bump_bip39,
            armed,
            buttons,
            inner,
            cp,
            scale,
        );
        let body = column![lead, sign, Space::new().height(2.0 * scale)].width(Length::Fill);
        scroller(body.boxed(), cp, scale)
    })
    .boxed()
}
