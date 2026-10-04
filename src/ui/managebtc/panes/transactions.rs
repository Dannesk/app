//! The `transactions` pane — one list, every transaction this wallet has
//! seen, and the `modify fee` face it becomes when a pending row's bump is
//! armed.
//!
//! **One list, no tabs, no group head** (user, 2026-09-10). Pending rows
//! first, then settled, each half newest first — the status column already
//! says which is which (`incoming` / `outgoing` against `success` /
//! `rejected` / `replaced`), so nothing else labels the split. The rows are
//! the transactions modal's own ([`crate::ui::managebtc::btctransactions`]):
//! the same resolution from the channel, the same detail lines, the same
//! tail — `modify fee ›` on a pending outgoing row, `copy txid ›` on a
//! settled one. Only the frame changed: the list is the XRP pane's list row
//! narrowed to the pane, with the txid in mono where XRP puts its type word.
//!
//! **The bump face replaces the list**, the way the XRP orders pane becomes
//! `cancel offer`: the existing lead (the transaction, what it pays now, the
//! tier row and its readout) over the inline sign block, `Keep it` /
//! `Modify fee`. The controller paths are the bump stack's, untouched — the
//! fee is committed at the press from the same planner the lead drew from,
//! and the form is torn down on dispatch.

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

// ── The row's numbers: the XRP list row's, with a mono txid column ──────────

/// The caret (mono) and its width.
const CARET: f32 = 8.0;
const CARET_W: f32 = 6.0;
/// The txid column — `a3f9…c21d`, nine mono characters at the cell size.
const TXID_W: f32 = 54.0;
/// Amount · status · time (mono).
pub(crate) const CELL: f32 = 9.5;
/// `incoming` / `outgoing` are eight characters; XRP's words are shorter.
const STATUS_W: f32 = 48.0;
pub(crate) const TIME_W: f32 = 66.0;
const ROW_GAP: f32 = 5.0;
const ROW_PAD_V: f32 = 6.0;
const ROW_PAD_H: f32 = 11.0;

/// The pane's title: `modify fee` while the bump is armed.
pub fn title(state: &AppState) -> &'static str {
    if state.btc_bump_txid.is_some() { "modify fee" } else { "transactions" }
}

pub fn view<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.btc_bump_txid.is_some() {
        return bump(state, cp, scale);
    }

    let (_, address_opt, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
    let own = address_opt.as_deref().unwrap_or("");
    let (pending, settled) = tx::split(own);
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
            .then(|| Detail { lines: tx::detail_lines(t, &ctx), tail: tx::tail(t, cp) });
        list = list.push(tx_row(t, tx::time_label(t, now), detail, cp, scale));
    }
    // `load 20 more ›`, where the rows run out. `held` is counted off the
    // records, not the rows drawn: a replaced original folds into its
    // replacement here, and the relay counts records.
    let end = {
        let txs = CHANNEL.btc_transactions_rx.borrow();
        end_row(&txs.page, txs.held(), HistoryList::BtcTransactions, cp, scale)
    };
    if let Some(end) = end {
        list = list.push(end);
    }
    scroller(list.boxed(), cp, scale)
}

/// One row: `▸` · txid (mono) · signed amount · state · time — the XRP list
/// row's shape with the txid where the type word goes. An expanded row drops
/// its rule and hangs its detail, which scrolls with the rest.
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
        cell(text(t.short_txid()).font(MONO).size(CELL * scale).color(cp.text).wrapping(Wrapping::None).boxed(), TXID_W, Alignment::Start),
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

/// The `modify fee` face: the bump lead, then the sign block on the bump
/// flow's own buffers. `Keep it` disarms; `Modify fee` commits the lead's
/// plan at the press.
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
