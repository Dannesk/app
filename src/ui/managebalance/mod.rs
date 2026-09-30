pub mod totalbalance;
pub mod settings;

use iced::Element;
use crate::controller::app_state::AppState;
use crate::controller::message::Message;

/// Balance is total-only, with Settings as a routed PAGE beside it rather than
/// a modal floated over it — the compact redesign retired the 640 card, and the
/// page needs the full window to run two columns.
///
/// The dock is composed above us by `dashboard::render_dashboard`, so it stays
/// put and the Balance tab stays lit: Settings is app-level, nothing changes
/// shape and nothing traps you. The back circle (→ `CloseSettings`) is the way out.
pub fn render(state: &AppState) -> Element<'_, Message> {
    if state.settings_open {
        settings::view(state)
    } else {
        totalbalance::view(state)
    }
}
