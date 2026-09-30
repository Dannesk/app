pub mod btcsetup;
pub mod node;
pub mod btctransactions;
pub mod btcsend;
pub mod btcbump;
pub mod btcdashboard;
pub mod panes;

use iced::{Element, Length};
use iced::widget::container;
use crate::controller::message::Message;
use crate::controller::app_state::{AppState, BtcView};
use crate::channel::CHANNEL;
use crate::ui::components::wallet_gate;
use crate::utils::theme;

/// The BTC tab: the import-or-create gate until a wallet exists, then the
/// pane grid (`btcdashboard`). The pre-grid chain — balance screen, receive
/// page, transactions modal, bump stack, key stack — was deleted on
/// 2026-09-10 once every one of its doors had a pane; it is in
/// `_attic/2026-09-10-btc-old-chain/`.
pub fn render_manage_btc(state: &AppState) -> Element<'_, Message> {
    match state.btc_view {
        BtcView::Import => return btcsetup::import(state),
        BtcView::Create => return btcsetup::create(state),
        BtcView::Menu => {}
    }

    let cp = theme::compact(&state.theme);
    let scale = state.scale();
    let (_, address_opt, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();

    if address_opt.is_none() {
        return wallet_gate::view(wallet_gate::Mark::Btc, Message::BtcCreateWalletClicked, Message::BtcImportWalletClicked, cp, scale);
    }

    container(btcdashboard::view(state))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
