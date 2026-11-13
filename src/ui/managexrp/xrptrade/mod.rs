
pub mod depth;
pub mod picker;

use iced::Color;

use crate::controller::app_state::{AppState, TradeKind};
use crate::utils::tokens;

pub const EYEBROW: f32 = 8.5;
pub const ROW: f32 = 10.5;
pub const NOTE: f32 = 9.5;

const ASK_HUE: Color = Color { r: 224.0 / 255.0, g: 105.0 / 255.0, b: 107.0 / 255.0, a: 1.0 };
const BID_HUE: Color = Color { r: 95.0 / 255.0, g: 192.0 / 255.0, b: 138.0 / 255.0, a: 1.0 };

pub const ASK_WASH: Color = Color { a: 0.13, ..ASK_HUE };
pub const BID_WASH: Color = Color { a: 0.13, ..BID_HUE };

pub fn disp(code: &str) -> &str {
    if code == "XRP" { "XRP" } else { tokens::by_code(code).map(|t| t.display).unwrap_or(code) }
}

pub fn manual(state: &AppState) -> bool {
    state.trade_mode == TradeKind::Limit
}
