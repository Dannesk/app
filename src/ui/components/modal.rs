

use iced::Widget as _;
use iced::widget::{center, container, mouse_area, opaque, stack};
use iced::{Color, Element};
use crate::controller::message::Message;

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
