use iced::Color;

const fn cp(hex: u32) -> Color {
    Color {
        r: ((hex >> 16) & 0xFF) as f32 / 255.0,
        g: ((hex >> 8) & 0xFF) as f32 / 255.0,
        b: (hex & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

const fn cpa(hex: u32, a: f32) -> Color {
    Color { a, ..cp(hex) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeVariant {
    Paper,
    Chalk,
    Obsidian,
    Graphite,
}

#[derive(Debug, Clone, Copy)]
pub struct CompactPalette {
    pub window: Color,
    pub panel: Color,
    pub field: Color,
    pub neutral: Color,
    pub pill: Color,
    pub hover: Color,
    pub scrim: Color,
    pub text: Color,
    pub dim: Color,
    pub muted: Color,
    pub faint: Color,
    pub border: Color,
    pub border_soft: Color,
    pub rule: Color,
    pub focus: Color,
    pub green: Color,
    pub amber: Color,
    pub red: Color,
    pub mint: Color,
    pub mint_line: Color,
    pub mint_ink: Color,
    pub mint_glyph: Color,
}

pub const COMPACT_PAPER: CompactPalette = CompactPalette {
    window: cp(0xFFFFFF),
    panel: cp(0xF6F8FA),
    field: cp(0xFFFFFF),
    neutral: cp(0xF6F8FA),
    pill: cp(0xE9EDF1),
    hover: cp(0xF1F3F6),
    scrim: cpa(0x1C2128, 0.34),
    text: cp(0x1C2128),
    dim: cp(0x59636E),
    muted: cp(0x6E7781),
    faint: cp(0xA2AAB3),
    border: cp(0xD3D9DF),
    border_soft: cp(0xDFE4E9),
    rule: cp(0xE7EBEF),
    focus: cp(0x6E7781),
    green: cp(0x1F8A4C),
    amber: cp(0x9A6A14),
    red: cp(0xC93A3A),
    mint: cp(0xEAF8ED),
    mint_line: cp(0xD3E7D8),
    mint_ink: cp(0x1C2128),
    mint_glyph: cp(0x59636E),
};

pub const COMPACT_CHALK: CompactPalette = CompactPalette {
    window: cp(0xFBFBFA),
    panel: cp(0xFFFFFF),
    field: cp(0xFBFBFA),
    neutral: cp(0xFBFBFA),
    pill: cp(0xECEEF0),
    hover: cp(0xF2F3F4),
    scrim: cpa(0x181C22, 0.34),
    text: cp(0x22262C),
    dim: cp(0x8A8F98),
    muted: cp(0x9AA0AA),
    faint: cp(0xB5BAC2),
    border: cp(0xDCDEE1),
    border_soft: cp(0xE3E4E6),
    rule: cp(0xECECEE),
    focus: cp(0x8A8F98),
    green: cp(0x3E8E63),
    amber: cp(0xA97A2F),
    red: cp(0xBF4F4A),
    mint: cp(0xEAF8ED),
    mint_line: cp(0xD3E7D8),
    mint_ink: cp(0x22262C),
    mint_glyph: cp(0x8A8F98),
};

pub const COMPACT_OBSIDIAN: CompactPalette = CompactPalette {
    window: cp(0x1B1E24),
    panel: cp(0x23272E),
    field: cp(0x1B1E24),
    neutral: cp(0x242932),
    pill: cp(0x2E343D),
    hover: cpa(0xFFFFFF, 0.045),
    scrim: cpa(0x000000, 0.55),
    text: cp(0xE7E9EC),
    dim: cp(0xA2A8B2),
    muted: cp(0x828993),
    faint: cp(0x5A6169),
    border: cpa(0xFFFFFF, 0.11),
    border_soft: cpa(0xFFFFFF, 0.09),
    rule: cpa(0xFFFFFF, 0.07),
    focus: cp(0x7C838D),
    green: cp(0x5FC08A),
    amber: cp(0xD9A35E),
    red: cp(0xE0696B),
    mint: cp(0xEAF8ED),
    mint_line: cp(0xD3E7D8),
    mint_ink: cp(0x1B1E24),
    mint_glyph: cp(0x1B1E24),
};

pub const COMPACT_GRAPHITE: CompactPalette = CompactPalette {
    window: cp(0x1E2227),
    panel: cp(0x272C33),
    field: cp(0x1B1F25),
    neutral: cp(0x30353D),
    pill: cp(0x343A43),
    hover: cpa(0xFFFFFF, 0.05),
    scrim: cpa(0x000000, 0.55),
    text: cp(0xF2F4F7),
    dim: cp(0xAEB4BD),
    muted: cp(0x8C939C),
    faint: cp(0x656C76),
    border: cpa(0xFFFFFF, 0.13),
    border_soft: cpa(0xFFFFFF, 0.11),
    rule: cpa(0xFFFFFF, 0.09),
    focus: cp(0x868D96),
    green: cp(0x5FC08A),
    amber: cp(0xD9A35E),
    red: cp(0xE0696B),
    mint: cp(0xEAF8ED),
    mint_line: cp(0xD3E7D8),
    mint_ink: cp(0x1E2227),
    mint_glyph: cp(0x1E2227),
};

impl CompactPalette {
    pub fn of(variant: ThemeVariant) -> &'static Self {
        match variant {
            ThemeVariant::Paper => &COMPACT_PAPER,
            ThemeVariant::Chalk => &COMPACT_CHALK,
            ThemeVariant::Obsidian => &COMPACT_OBSIDIAN,
            ThemeVariant::Graphite => &COMPACT_GRAPHITE,
        }
    }
}

pub fn compact(theme: &iced::Theme) -> &'static CompactPalette {
    if theme.to_string().contains("Graphite") {
        &COMPACT_GRAPHITE
    } else if is_dark(theme) {
        &COMPACT_OBSIDIAN
    } else if theme.to_string().contains("Chalk") {
        &COMPACT_CHALK
    } else {
        &COMPACT_PAPER
    }
}

pub fn bloom_alpha(theme: &iced::Theme) -> f32 {
    if theme.to_string().contains("Graphite") {
        0.26
    } else if is_dark(theme) {
        0.28
    } else {
        0.30
    }
}

pub fn is_dark(theme: &iced::Theme) -> bool {
    matches!(theme, iced::Theme::Dark)
        || theme.to_string().contains("Obsidian")
        || theme.to_string().contains("Graphite")
}

pub fn paper_theme() -> iced::Theme {
    custom_theme("CoolPaper", &COMPACT_PAPER)
}

pub fn obsidian_theme() -> iced::Theme {
    custom_theme("CoolObsidian", &COMPACT_OBSIDIAN)
}

pub fn graphite_theme() -> iced::Theme {
    custom_theme("CoolGraphite", &COMPACT_GRAPHITE)
}

pub fn chalk_theme() -> iced::Theme {
    custom_theme("CoolChalk", &COMPACT_CHALK)
}

fn custom_theme(name: &str, p: &CompactPalette) -> iced::Theme {
    iced::Theme::custom(
        name.to_string(),
        iced::theme::palette::Seed {
            background: p.window,
            text:       p.text,
            primary:    p.focus,
            success:    p.green,
            warning:    p.amber,
            danger:     p.red,
        },
    )
}