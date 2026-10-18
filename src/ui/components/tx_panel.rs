

use iced::Color;

use crate::controller::message::Message;

#[derive(Debug, Clone)]
pub struct Seg {
    pub s: String,
    pub color: Color,
    pub mono: bool,
}

impl Seg {
    pub fn mono(s: impl Into<String>, color: Color) -> Self {
        Self { s: s.into(), color, mono: true }
    }

    pub fn word(s: impl Into<String>, color: Color) -> Self {
        Self { s: s.into(), color, mono: false }
    }
}

pub type Line = (&'static str, Vec<Seg>);

pub struct IdLine {
    pub key: &'static str,
    pub value: String,
    pub copy: Message,
    pub copied: bool,
}

pub enum Tail {
  
    Link { label: &'static str, color: Color, msg: Message },
    Prose { s: &'static str, color: Color },
}

pub struct Detail {
    pub id: IdLine,
    pub lines: Vec<Line>,
    pub tail: Option<Tail>,
}
