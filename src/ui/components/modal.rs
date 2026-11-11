use iced::Widget as _;
use iced::widget::{center, container, opaque};
use iced::{Color, Element};
use crate::controller::message::Message;

pub fn layer<'a>(card: Element<'a, Message>, scrim: Color) -> Element<'a, Message> {
    opaque(center(card).style(move |_| container::Style {
        background: Some(scrim.into()),
        ..Default::default()
    }))
    .boxed()
}
