pub mod xrpsetup;
pub mod xrpsend;
pub mod xrptrade;
pub mod xrptransactions;
pub mod tokens;
pub mod xrpdashboard;
pub mod panes;

use iced::{Element, Length};
use iced::widget::container;
use crate::controller::message::Message;
use crate::controller::app_state::{AppState, XrpView};
use crate::channel::CHANNEL;
use crate::ui::components::wallet_gate;
use crate::utils::theme;

pub fn render_manage_xrp(state: &AppState) -> Element<'_, Message> {
    // Sub-views: early return, no tabs
    match state.xrp_view {
        XrpView::Import       => return xrpsetup::import(state),
        XrpView::Create       => return xrpsetup::create(state),
        XrpView::Menu => {}
    }

    let cp = theme::compact(&state.theme);
    let scale = state.scale();
    let (_, address_opt, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();

    if address_opt.is_none() {
        return wallet_gate::view(wallet_gate::Mark::Xrp, Message::CreateWalletClicked, Message::ImportWalletClicked, cp, scale);
    }

    // THE PANE GRID (2026-09-09): the XRP tab is the pane grid. The old
    // balance screen and its overlays were deleted 2026-09-09 once every one
    // of them had a pane or a face.
    container(xrpdashboard::view(state))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
