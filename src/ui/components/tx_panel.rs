

use iced::Color;

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

pub struct Fact {
    pub key: &'static str,
    pub dot: Option<Color>,
    pub segs: Vec<Seg>,
    pub copy: Option<String>,
}
