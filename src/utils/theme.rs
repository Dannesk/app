use iced::Color;

// ── The palette ───────────────────────────────────────────────────────────────
//
// The compact design language (2026-08-31 → 2026-09-05): cardless panes on
// hairline rules, mono eyebrows, 8.5–12.5px type, neutral CTAs. One token
// vocabulary for the whole app, from the designer's handoff `theme.rs` —
// values are the mock's, verbatim, and every handoff since has been
// script-diffed against this file and found identical.
//
// **There is no brand blue.** No blue token exists here and none may be
// added. Selection = `neutral`/`pill` fill + `text` ink; the dock indicator is
// a 2px `dim` bar; links are `dim → text`; emphasis comes from the ink ramp.
// The only colour is the severity ramp — `green`/`amber`/`red` — used where
// health, severity or direction is literally the subject. Any blue is a bug.
//
// **Do not change a value on a handoff's say-so.** When a new mock contradicts
// this file for the same token, that is a designer's slip until verified —
// stop and ask, then fix the mock, not the file.
//
// The old `AppPalette` (brand blue, per-asset purples and oranges, the
// hand-rounded input colours) was deleted 2026-09-05 once its last caller —
// the pre-v3 chart — went; it is in `_attic/2026-09-05-update-screen/`.
// Three variants: Light, Obsidian (the dark default) and Graphite.

/// One-hex colour for the compact consts. `const fn`, so the palettes can stay
/// `const` like the two above.
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

/// Which compact variant a theme resolves to. `Light`/`Obsidian` map onto the
/// existing Light/Dark setting; `Graphite` is a real third variant (raised
/// window, recessed `field`, higher text ramp, stronger hairlines), parked
/// until the settings picker grows a third option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeVariant {
    Light,
    Obsidian,
    Graphite,
}

/// The compact spec's tokens. Alpha entries carry their composited-opaque
/// equivalent in comments (over the window; the sign stack's `panel` variants
/// are in the handoff README when send v3 needs them).
#[derive(Debug, Clone, Copy)]
pub struct CompactPalette {
    // surfaces
    pub window: Color,
    pub panel: Color,   // raised overlay fill — the send v3 sign stack (unused on import)
    pub field: Color,   // input fill (= window except graphite, where it recesses)
    pub neutral: Color, // active segmented + neutral button fill
    pub pill: Color,    // entropy-bar track
    pub hover: Color,   // region wash
    pub scrim: Color,   // dim behind the sign stack (unused on import)
    // ink
    pub text: Color,
    pub dim: Color,
    pub muted: Color,
    pub faint: Color,
    // lines
    pub border: Color,      // fields, segmented, window edge
    pub border_soft: Color, // back chevron, disabled button edge
    pub rule: Color,        // pane divider, section dividers, dock top edge
    pub focus: Color,       // focused/filled field border — GRAY, not blue
    // severity (solid, no alpha)
    pub green: Color,
    pub amber: Color,
    pub red: Color,
}

/// Light — warm near-white window (#FBFBFA), explicit cool-gray hairlines
/// (black alphas went muddy over the warm white). Panel is the one pure white.
pub const COMPACT_LIGHT: CompactPalette = CompactPalette {
    window: cp(0xFBFBFA),
    panel: cp(0xFFFFFF),
    field: cp(0xFBFBFA),
    neutral: cp(0xFBFBFA),
    pill: cp(0xECEEF0),
    hover: cp(0xF2F3F4),
    scrim: cpa(0x181C22, 0.34), // opaque over window #AEAFB1
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
};

/// Obsidian — the branded dark default. Window/panel unchanged from shipped;
/// secondary ink a step lighter, hairlines a step stronger (`.11` not `.09`).
pub const COMPACT_OBSIDIAN: CompactPalette = CompactPalette {
    window: cp(0x1B1E24),
    panel: cp(0x23272E),
    field: cp(0x1B1E24),
    neutral: cp(0x242932),
    pill: cp(0x2E343D),
    hover: cpa(0xFFFFFF, 0.045), // opaque #25282E
    scrim: cpa(0x000000, 0.55),  // opaque over window #0C0E10
    text: cp(0xE7E9EC),
    dim: cp(0xA2A8B2),
    muted: cp(0x828993),
    faint: cp(0x5A6169),
    border: cpa(0xFFFFFF, 0.11),      // opaque #34373C
    border_soft: cpa(0xFFFFFF, 0.09), // opaque #303238
    rule: cpa(0xFFFFFF, 0.07),        // opaque #2B2E33
    focus: cp(0x7C838D),
    green: cp(0x5FC08A),
    amber: cp(0xD9A35E),
    red: cp(0xE0696B),
};

/// Graphite — cooler, one step lighter than obsidian; `field` sits slightly
/// BELOW the window so inputs read recessed. A real third variant, not
/// obsidian with a tweak.
pub const COMPACT_GRAPHITE: CompactPalette = CompactPalette {
    window: cp(0x1E2227),
    panel: cp(0x272C33),
    field: cp(0x1B1F25),
    neutral: cp(0x30353D),
    pill: cp(0x343A43),
    hover: cpa(0xFFFFFF, 0.05), // opaque #292D32
    scrim: cpa(0x000000, 0.55), // opaque over window #0E0F12
    text: cp(0xF2F4F7),
    dim: cp(0xAEB4BD),
    muted: cp(0x8C939C),
    faint: cp(0x656C76),
    border: cpa(0xFFFFFF, 0.13),      // opaque #3B3F43
    border_soft: cpa(0xFFFFFF, 0.11), // opaque #373A3F
    rule: cpa(0xFFFFFF, 0.09),        // opaque #32363A
    focus: cp(0x868D96),
    green: cp(0x5FC08A),
    amber: cp(0xD9A35E),
    red: cp(0xE0696B),
};

impl CompactPalette {
    pub fn of(variant: ThemeVariant) -> &'static Self {
        match variant {
            ThemeVariant::Light => &COMPACT_LIGHT,
            ThemeVariant::Obsidian => &COMPACT_OBSIDIAN,
            ThemeVariant::Graphite => &COMPACT_GRAPHITE,
        }
    }
}

/// The compact palette for the active theme. Dark resolves to Obsidian; a
/// future "CoolGraphite" `iced::Theme` resolves by name once the picker
/// offers it.
pub fn compact(theme: &iced::Theme) -> &'static CompactPalette {
    if theme.to_string().contains("Graphite") {
        &COMPACT_GRAPHITE
    } else if is_dark(theme) {
        &COMPACT_OBSIDIAN
    } else {
        &COMPACT_LIGHT
    }
}

/// The gate bloom's stroke alpha for the active theme.
///
/// Not a palette entry, because it is not a colour: it is how much of `faint →
/// text` survives at a 0.9px stroke on that theme's ground. Light needs the
/// most — dark ink on a near-white window loses weight in a way pale ink on a
/// dark one does not.
pub fn bloom_alpha(theme: &iced::Theme) -> f32 {
    if theme.to_string().contains("Graphite") {
        0.26
    } else if is_dark(theme) {
        0.28
    } else {
        0.30
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Graphite counts as dark: the two dark variants share their alpha rows
/// (chart fills, the gate bloom) and differ only in tokens, which
/// [`compact`] resolves first.
pub fn is_dark(theme: &iced::Theme) -> bool {
    matches!(theme, iced::Theme::Dark)
        || theme.to_string().contains("Dark")
        || theme.to_string().contains("Graphite")
}

pub fn dark_theme() -> iced::Theme {
    custom_theme("CoolDark", &COMPACT_OBSIDIAN)
}

/// Graphite — the cool third variant. Named "CoolGraphite" because
/// [`compact`] and [`bloom_alpha`] resolve it by that substring, the same way
/// the other two resolve by `is_dark`.
pub fn graphite_theme() -> iced::Theme {
    custom_theme("CoolGraphite", &COMPACT_GRAPHITE)
}

pub fn light_theme() -> iced::Theme {
    custom_theme("CoolLight", &COMPACT_LIGHT)
}

/// The `iced::Theme` a compact palette resolves to. Only iced's own default
/// widget styles read this — every screen styles itself from
/// [`CompactPalette`] — so `primary` is the gray `focus` token: a stock iced
/// widget that ever slipped through would draw gray, never blue.
fn custom_theme(name: &str, p: &CompactPalette) -> iced::Theme {
    iced::Theme::custom(
        name.to_string(),
        iced::theme::Palette {
            background: p.window,
            text:       p.text,
            primary:    p.focus,
            success:    p.green,
            warning:    p.amber,
            danger:     p.red,
        },
    )
}