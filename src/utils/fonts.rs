use iced::{Font, font::{Family, Weight}};

pub const MONO: Font = Font {
    family: Family::Name("JetBrains Mono"),
    weight: Weight::Normal,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};

pub const LIGHT: Font = Font {
    family: Family::Name("Inter"),
    weight: Weight::Light,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};
