pub mod totalbalance;
pub mod settings;

use iced::Element;
use crate::controller::app_state::AppState;
use crate::controller::message::Message;

pub fn render(state: &AppState) -> Element<'_, Message> {
    if state.settings_open {
        settings::view(state)
    } else {
        totalbalance::view(state)
    }
}
