
pub mod depth;
pub mod picker;
pub mod ticket;

use iced::Color;

use crate::controller::app_state::AppState;
use crate::utils::tokens;

pub const EYEBROW: f32 = 8.5;
pub const ROW: f32 = 10.5;
pub const MID: f32 = 11.0;
pub const FIELD: f32 = 12.5;
pub const LABEL: f32 = 10.5;
pub const PILL: f32 = 10.0;
pub const UNIT: f32 = 9.0;
pub const NOTE: f32 = 9.5;

const ASK_HUE: Color = Color { r: 224.0 / 255.0, g: 105.0 / 255.0, b: 107.0 / 255.0, a: 1.0 };
const BID_HUE: Color = Color { r: 95.0 / 255.0, g: 192.0 / 255.0, b: 138.0 / 255.0, a: 1.0 };

pub const ASK_WASH: Color = Color { a: 0.13, ..ASK_HUE };
pub const BID_WASH: Color = Color { a: 0.13, ..BID_HUE };
pub const BUY_FILL: Color = Color { a: 0.12, ..BID_HUE };
pub const BUY_BORDER: Color = Color { a: 0.42, ..BID_HUE };
pub const SELL_FILL: Color = Color { a: 0.12, ..ASK_HUE };
pub const SELL_BORDER: Color = Color { a: 0.42, ..ASK_HUE };

pub fn disp(code: &str) -> &str {
    if code == "XRP" { "XRP" } else { tokens::by_code(code).map(|t| t.display).unwrap_or(code) }
}

pub fn manual(state: &AppState) -> bool {
    crate::controller::xrp::trade_is_limit(state)
}
