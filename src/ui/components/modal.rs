//! A generic centered modal: a scrim dims the screen and floats `card` over the
//! current `base`. Click the scrim — anywhere outside the card — to dismiss;
//! clicks on the card itself are swallowed (`opaque`) so they never reach the
//! backdrop. Desktop affordance: on a phone each of these would be its own
//! screen, but on a wide window data that's smaller than a page reads better as
//! an overlay. Built on iced's `stack` + `mouse_area(opaque(..))` idiom — the
//! same layering enterpin's "Forgot PIN?" dialog already uses, centralized here
//! so every modal behaves identically.

use iced::Widget as _;
use iced::widget::{center, container, mouse_area, opaque, stack};
use iced::{Color, Element};
use crate::controller::message::Message;

/// Float `card` over `base`, dimmed by `scrim`. `on_dismiss` fires on a
/// backdrop click. The `card` keeps its own size/style; this only centers it.
pub fn view<'a>(
    base: Element<'a, Message>,
    card: Element<'a, Message>,
    on_dismiss: Message,
    scrim: Color,
) -> Element<'a, Message> {
    let backdrop = mouse_area(
        center(opaque(card)).style(move |_| container::Style {
            background: Some(scrim.into()),
            ..Default::default()
        }),
    )
    .on_press(on_dismiss);

    stack![base, backdrop].boxed()
}
