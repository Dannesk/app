//! The pane-grid chrome both chain dashboards draw from (2026-09-10, when
//! BTC got its grid): the top bar, the grid and its hairlines, the title
//! strip with its four controls, the `panels +` menu, the empty branch and
//! the well a split leaves, the shared rows and buttons, the inline sign
//! block, and the two pane bodies that are the same on both chains — the
//! chart and the receive code.
//!
//! Moved out of `managexrp/xrpdashboard.rs` verbatim when the BTC dashboard
//! landed; nothing here draws differently from the day the XRP grid was
//! confirmed. What a chain contributes is a [`Host`] — which grid, which
//! word in the bar, which message the grid listens on — and a [`PaneBody`]
//! that says what each kind draws. The model (which panes are open, who
//! inherits on close, persistence) is [`crate::controller::panes`].
//!
//! Framework notes worth knowing before touching this:
//! - The hairline between panes is the grid's `spacing(1)` over a `rule`
//!   background; panes paint `window` and carry no borders of their own.
//! - The strip's **drag handle is the space between the title and the
//!   controls** — iced excludes both from the pick area — so the title stays
//!   Fit-width and the meta rides in the controls row.
//! - Panes clip their bodies. A pane dragged shorter than its content cuts
//!   it off rather than spilling into its neighbour.
//! - Everything here draws from [`CompactPalette`]; the fiat unit is
//!   `state.base_currency`, never a literal.

use iced::widget::Row;
use iced::Widget as _;
use std::sync::LazyLock;

use iced::mouse;
use iced::widget::canvas::{self, fill, gradient as canvas_gradient, Canvas, Fill, Frame, Gradient as CanvasGradient, LineCap, LineDash, LineJoin, Path, Stroke};
use iced::widget::pane_grid::{self, Axis, Controls, Pane, PaneGrid, TitleBar};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text::{Span, Wrapping};
use iced::widget::{button, column, container, mouse_area, opaque, responsive, rich_text, row, scrollable, span, stack, svg, text, Space};
use iced::alignment::{Horizontal, Vertical};
use iced::{Alignment, Border, Color, Element, Length, Padding, Pixels, Point, Rectangle, Shadow, Size, Vector};

use crate::controller::app_state::{AppState, ChartPeriod, EnableInputMode};
use crate::controller::message::{Message, PlainField, SecureField, TagPane};
use crate::controller::panes::{self, Chain, Grid, GridMsg, PaneKind};
use crate::secure::SecureString;
use crate::ui::components::compact;
use crate::ui::components::send_screen::{self as screen};
use crate::ui::components::signing::SignFields;
use crate::ui::components::wallet_setup::{self, PHRASE_PLACEHOLDER, TARGET_WORDS};
use crate::ui::managexrp::tokens as tokens_page;
use crate::utils::fonts::{LIGHT, MONO};
use crate::utils::qr;
use crate::utils::fonts;
use crate::utils::sparkline::{fmt_hover_time, ChartInk};
use crate::utils::theme::CompactPalette;

// ── Type scale ──────────────────────────────────────────────────────────────
// The handoff's sizes, before `scale()`. A 900×560 data screen sits below the
// app's 13/14.5 floor by design.

/// The window title in the top bar (Inter).
pub const TITLE: f32 = 12.5;
/// `panels +` (mono upper).
pub const BAR_BUTTON: f32 = 9.0;
/// Pane titles, field labels, section rules, menu heads (mono upper).
pub const STRIP: f32 = 8.5;
/// The period pill's segments (mono upper).
pub const PILL: f32 = 8.0;
/// The balance hero (Inter 300).
pub const HERO: f32 = 28.0;
/// The hero's unit (mono upper).
pub const HERO_UNIT: f32 = 9.0;
/// Row keys (Inter) and row values (mono).
pub const ROW_KEY: f32 = 9.5;
pub const ROW_VALUE: f32 = 10.0;
/// The total row's value (mono).
pub const ROW_TOTAL: f32 = 11.5;
/// The rate and the fee (mono), and their suffixes.
pub const BIG: f32 = 13.0;
pub const BIG_SUFFIX: f32 = 9.0;
/// A field's value (mono) — the compose boxes, the sign boxes.
pub const FIELD_VALUE: f32 = 11.5;
/// An address in the compose box (mono) — one row for an r-address.
pub const ADDR: f32 = 11.0;
/// The format hint (mono).
pub const HINT: f32 = 8.5;
/// The receive address (mono).
pub const RECEIVE_ADDR: f32 = 10.0;
/// Buttons (Inter).
pub const BUTTON: f32 = 11.0;
/// A pane button's height before scale: its word at iced's default line
/// height over `5` of padding each side. What a face's minimum counts with.
pub const BUTTON_H: f32 = BUTTON * 1.3 + 10.0;
/// The menu: rows (Inter), heads and shortcuts (mono).
const MENU_ITEM: f32 = 12.5;
const MENU_KEY: f32 = 9.0;
/// The empty state (Inter 300).
const EMPTY_PLUS: f32 = 30.0;
const EMPTY_WORD: f32 = 15.0;
/// The chart line.
const CHART_STROKE: f32 = 1.2;

// ── Geometry ────────────────────────────────────────────────────────────────

/// Pane box padding: `10px 12px 11px`; the strip's own bottom is 9.
const PANE_PAD_TOP: f32 = 10.0;
pub const PANE_PAD_H: f32 = 12.0;
const PANE_PAD_BOTTOM: f32 = 11.0;
const STRIP_PAD_BOTTOM: f32 = 9.0;
/// Between the strip's controls, and between the two split glyphs.
const CONTROL_GAP: f32 = 8.0;
/// The drawn split glyphs are 9×9.
const GLYPH: f32 = 9.0;
/// The menu.
const MENU_W: f32 = 254.0;
const MENU_PAD_H: f32 = 13.0;
const MENU_CHECK_W: f32 = 9.0;
/// The amount duo's gap and the `=` between.
pub const AMT_GAP: f32 = 7.0;
pub const EQ_W: f32 = 8.0;
/// The cold-storage word grid inside a signing pane — import's height.
pub const PHRASE_H: f32 = 154.0;
/// The QR's quiet zone, as a share of the tile.
const QR_QUIET: f32 = 0.056;
/// The list scroller's thumb.
const SCROLLER_W: f32 = 3.0;

/// **The QR plate stays white and its modules `#22262C` in every theme** —
/// the handoff's own call, and a code that inverts costs scans.
pub const QR_PLATE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
pub const QR_INK: Color = Color { r: 0.133, g: 0.149, b: 0.173, a: 1.0 };

/// The two drawn marks in the design: a 9×9 box with a 1px bar inset 3px.
/// Stroked black and tinted by the widget, the way the eye icons are.
const SPLIT_V_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="#000" stroke-width="1"><rect x="0.5" y="0.5" width="8" height="8" rx="1"/><line x1="3.5" y1="0.5" x2="3.5" y2="8.5"/></svg>"##;
const SPLIT_H_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="#000" stroke-width="1"><rect x="0.5" y="0.5" width="8" height="8" rx="1"/><line x1="0.5" y1="3.5" x2="8.5" y2="3.5"/></svg>"##;

static SPLIT_V: LazyLock<svg::Handle> = LazyLock::new(|| svg::Handle::from_memory(SPLIT_V_SVG));
static SPLIT_H: LazyLock<svg::Handle> = LazyLock::new(|| svg::Handle::from_memory(SPLIT_H_SVG));

/// The sign boxes say what they are; there is no label over either. The
/// 25th word is `optional` because nothing tracks whether this wallet has
/// one — an import in either mode may have set none.
pub const KEY_PLACEHOLDER: &str = "encryption key";
pub const BIP39_PLACEHOLDER: &str = "optional 25th word";

pub const NA: &str = "\u{2014}";

// ── The host ────────────────────────────────────────────────────────────────

/// What a chain's dashboard brings to the chrome.
#[derive(Clone, Copy)]
pub struct Host<'a> {
    pub chain: Chain,
    /// The top bar's word — `XRP`, `BTC`.
    pub title: &'static str,
    pub grid: &'a Grid,
    /// The message the chain's grid listens on — `Message::Grid` or
    /// `Message::BtcGrid`. A constructor, so the chrome can fire any
    /// [`GridMsg`] at the right grid without knowing which chain it is.
    pub wrap: fn(GridMsg) -> Message,
    /// A control after the chain word and the layer it opens under the bar —
    /// the XRP tab's pair search (2026-09-11). BTC has no pairs: `None`.
    pub bar: Option<BarSlot<'a>>,
}

/// What a chain hangs off its top bar: the control drawn after the title,
/// and the screen-sized layer stacked under the bar while it is open
/// (`None` from the layer means nothing is open).
#[derive(Clone, Copy)]
pub struct BarSlot<'a> {
    pub control: fn(&'a AppState, &'static CompactPalette, f32) -> Element<'a, Message>,
    pub layer: fn(&'a AppState, &'static CompactPalette, f32) -> Option<Element<'a, Message>>,
}

/// What a kind draws, for the chain that owns it: the strip title, its
/// optional meta, and the body — handed back framed by [`pane_frame`].
pub type PaneBody<'a> =
    fn(&'a AppState, Pane, PaneKind, bool, &'static CompactPalette, f32) -> PaneContent<'a>;

/// A pane as the grid takes it: an `Element` title over an `Element` body.
pub type PaneContent<'a> = pane_grid::Content<'a, Message, Element<'a, Message>, Element<'a, Message>>;

/// The whole screen under the dock: the top bar, the grid or its empty
/// branch, and the menu when it is open.
pub fn screen<'a>(
    host: Host<'a>,
    state: &'a AppState,
    body: PaneBody<'a>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let g = host.grid;
    let body_el: Element<'a, Message> = match &g.panes {
        Some(panes) => grid(state, panes, body, host.wrap, cp, scale),
        None => empty_state(host.wrap, cp, scale),
    };

    let base = column![
        top_bar(host, state, cp, scale),
        compact::hairline(cp.rule),
        container(body_el).width(Length::Fill).height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    // The bar control's layer, then the menu over everything: opening the
    // menu folds the search, so the two are never up together.
    //
    // ALWAYS a stack, even with no overlay. The root widget's type is what
    // iced diffs the whole tree by: a column that becomes a stack the moment
    // the results layer appears rebuilt every widget under it — and the pair
    // search field with it, caret back at zero, so the second letter typed
    // landed in front of the first (2026-09-11).
    let bar_layer = host.bar.and_then(|b| (b.layer)(state, cp, scale));
    let mut layers = stack![base].width(Length::Fill).height(Length::Fill);
    if let Some(l) = bar_layer {
        layers = layers.push(l);
    }
    if g.menu_open {
        layers = layers.push(menu_layer(host, cp, scale));
    }
    layers.boxed()
}

// ── Top bar ─────────────────────────────────────────────────────────────────

/// `height 34`, `padding 0 11px`: the chain name left, then the chain's
/// bar control when it has one (`gap 9`, so it reads as scope — `XRP ›
/// XRP / RLUSD`), `panels +` right. Nothing else — the bell and settings
/// belong to the Balance tab (user, 2026-09-10).
fn top_bar<'a>(host: Host<'a>, state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let mut bar = row![text(host.title).size(TITLE * scale).color(cp.text)]
        .align_y(Alignment::Center)
        .height(Length::Fill);
    if let Some(b) = host.bar {
        bar = bar.push(Space::new().width(9.0 * scale).boxed()).push((b.control)(state, cp, scale));
    }
    bar = bar
        .push(Space::new().width(Length::Fill).boxed())
        .push(panels_button(host.grid.menu_open, host.wrap, cp, scale));
    container(bar)
    .width(Length::Fill)
    .height(Length::Fixed(panes::BAR_H * scale))
    .padding(Padding::new(0.0).left(11.0 * scale).right(11.0 * scale))
    .boxed()
}

/// `panels +`: mono 9 upper `muted`; a `hover` wash and `text` ink while
/// hovered or open.
fn panels_button<'a>(open: bool, wrap: fn(GridMsg) -> Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(text("PANELS +").font(MONO).size(BAR_BUTTON * scale))
        .on_press(wrap(GridMsg::MenuToggled))
        .padding(Padding::new(0.0).top(5.0 * scale).bottom(5.0 * scale).left(8.0 * scale).right(8.0 * scale))
        .style(move |_, status| {
            let hot = open || matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if hot { cp.hover } else { Color::TRANSPARENT }.into()),
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (5.0 * scale).into() },
                text_color: if hot { cp.text } else { cp.muted },
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}

// ── The grid ────────────────────────────────────────────────────────────────

fn grid<'a>(
    state: &'a AppState,
    panes: &'a pane_grid::State<PaneKind>,
    body: PaneBody<'a>,
    wrap: fn(GridMsg) -> Message,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let grid = PaneGrid::new(panes, move |pane, kind, maximized| body(state, pane, *kind, maximized, cp, scale))
        .width(Length::Fill)
        .height(Length::Fill)
        .spacing(panes::SPACING)
        .min_size(panes::MIN_H * scale)
        .on_drag(move |event| wrap(GridMsg::Dragged(event)))
        .on_resize(panes::LEEWAY, move |event| wrap(GridMsg::Resized(event)))
        .style(move |_| pane_grid::Style {
            // The drop target: a `hover` wash inside a 1px `focus` outline.
            hovered_region: pane_grid::Highlight {
                background: cp.hover.into(),
                border: Border { color: cp.focus, width: 1.0, radius: 0.0.into() },
            },
            // The hairline itself turns `focus` under the pointer and while
            // dragged — a 1px line over a 1px gap, nothing thicker.
            hovered_split: pane_grid::Line { color: cp.focus, width: 1.0 },
            picked_split: pane_grid::Line { color: cp.focus, width: 1.0 },
        });

    // The `rule` ground shows through the grid's 1px spacing: that is the
    // hairline, and the reason panes need no borders.
    container(grid)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style { background: Some(cp.rule.into()), ..Default::default() })
        .boxed()
}

/// The strip's title word: mono 8.5 upper `muted`.
pub fn strip<'a>(word: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    text(word.to_uppercase()).font(MONO).size(STRIP * scale).color(cp.muted).boxed()
}

/// A pane, framed: the body under the pane padding, clipped, on `window`;
/// the strip over it.
#[allow(clippy::too_many_arguments)]
pub fn pane_frame<'a>(
    pane: Pane,
    kind: PaneKind,
    maximized: bool,
    title: Element<'a, Message>,
    meta: Option<Element<'a, Message>>,
    body: Element<'a, Message>,
    wrap: fn(GridMsg) -> Message,
    cp: &'static CompactPalette,
    scale: f32,
) -> PaneContent<'a> {
    let body = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: PANE_PAD_H * scale,
            bottom: PANE_PAD_BOTTOM * scale,
            left: PANE_PAD_H * scale,
        })
        .clip(true);

    pane_grid::Content::new(body.boxed())
        .title_bar(title_bar(pane, kind, maximized, title, meta, wrap, cp, scale))
        .style(move |_| container::Style { background: Some(cp.window.into()), ..Default::default() })
}

/// The title strip: title left, then the pick area, then the controls —
/// `[meta] ▯ ▭ ⤢ ×`, always visible. At this size a hover-only control is
/// a control nobody finds.
#[allow(clippy::too_many_arguments)]
fn title_bar<'a>(
    pane: Pane,
    kind: PaneKind,
    maximized: bool,
    title: Element<'a, Message>,
    meta: Option<Element<'a, Message>>,
    wrap: fn(GridMsg) -> Message,
    cp: &'static CompactPalette,
    scale: f32,
) -> TitleBar<'a, Message> {
    let mut controls: Row<Element<'_, Message>> = row![].spacing(CONTROL_GAP * scale).align_y(Alignment::Center);
    if let Some(m) = meta {
        controls = controls.push(m);
    }
    if kind != PaneKind::Empty {
        controls = controls
            .push(split_glyph(true, wrap(GridMsg::Split(pane, Axis::Vertical)), cp, scale))
            .push(split_glyph(false, wrap(GridMsg::Split(pane, Axis::Horizontal)), cp, scale))
            // ↗ / ↙ are JetBrains Mono's own. ⤢ ⤡ were not, and a glyph the
            // carried fonts lack is a box until the system fonts are indexed.
            .push(glyph(
                if maximized { "\u{2199}" } else { "\u{2197}" },
                wrap(GridMsg::Maximize(pane)),
                cp,
                scale,
            ));
    }
    controls = controls.push(glyph("\u{d7}", wrap(GridMsg::Close(pane)), cp, scale));

    TitleBar::new(title)
        .controls(Controls::new(controls))
        .always_show_controls()
        .padding(Padding {
            top: PANE_PAD_TOP * scale,
            right: PANE_PAD_H * scale,
            bottom: STRIP_PAD_BOTTOM * scale,
            left: PANE_PAD_H * scale,
        })
}

/// A text control in the strip: mono 10, `muted` lifting to `text`.
fn glyph<'a>(mark: &'static str, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(text(mark).font(MONO).size(10.0 * scale))
        .on_press(msg)
        .padding(Padding::ZERO)
        .style(move |_, status| button::Style {
            background: None,
            border: Border::default(),
            text_color: match status {
                button::Status::Hovered | button::Status::Pressed => cp.text,
                _ => cp.muted,
            },
            shadow: Shadow::default(),
            snap: false,
        })
        .boxed()
}

/// One of the two drawn split marks.
fn split_glyph<'a>(vertical: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let handle = if vertical { SPLIT_V.clone() } else { SPLIT_H.clone() };
    button(
        svg(handle)
            .width(Length::Fixed(GLYPH * scale))
            .height(Length::Fixed(GLYPH * scale))
            .style(move |_, status| svg::Style {
                color: Some(match status {
                    svg::Status::Hovered => cp.text,
                    _ => cp.muted,
                }),
            }),
    )
    .on_press(msg)
    .padding(Padding::ZERO)
    .style(bare)
    .boxed()
}

/// A button that is nothing but its content.
pub fn bare(_: &iced::Theme, _: button::Status) -> button::Style {
    button::Style {
        background: None,
        border: Border::default(),
        text_color: Color::TRANSPARENT,
        shadow: Shadow::default(),
        snap: false,
    }
}

/// The chart pane's `1h` | `1d` pill: 1px `border`, radius 999, mono 8
/// upper, segments `2px 7px`; the active one on `pill` in `text`.
pub fn period_pill<'a>(current: ChartPeriod, wrap: fn(GridMsg) -> Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let seg = |label: &str, period: ChartPeriod| -> Element<'a, Message> {
        let active = current == period;
        button(
            text(label.to_uppercase())
                .font(MONO)
                .size(PILL * scale)
                .color(if active { cp.text } else { cp.muted }),
        )
        .on_press(wrap(GridMsg::Period(period)))
        .padding(Padding::new(0.0).top(2.0 * scale).bottom(2.0 * scale).left(7.0 * scale).right(7.0 * scale))
        .style(move |_, _| button::Style {
            background: Some(if active { cp.pill } else { Color::TRANSPARENT }.into()),
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 999.0.into() },
            text_color: if active { cp.text } else { cp.muted },
            shadow: Shadow::default(),
            snap: false,
        })
        .boxed()
    };
    container(row![seg("1h", ChartPeriod::OneHour), seg("1d", ChartPeriod::OneDay)])
        .style(move |_| container::Style {
            border: Border { color: cp.border, width: 1.0, radius: 999.0.into() },
            ..Default::default()
        })
        .boxed()
}

// ── Shared rows ─────────────────────────────────────────────────────────────

/// A data row: key Inter 9.5 `dim` left, mono 10 runs pinned right,
/// `padding 3px 0` — `top` is the row's top padding (9 for a first row).
pub fn drow<'a>(key: &str, runs: Vec<(String, Color)>, top: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(
        row![
            text(key.to_string()).size(ROW_KEY * scale).color(cp.dim),
            Space::new().width(Length::Fill),
            compact::mono_runs(runs, ROW_VALUE * scale),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(top * scale).bottom(3.0 * scale))
    .boxed()
}

/// The separated group's rule: `margin-top 7`, `padding-top 8`.
pub fn group_rule<'a>(cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    column![
        Space::new().height(7.0 * scale),
        compact::hairline(cp.rule),
        Space::new().height(8.0 * scale),
    ]
    .width(Length::Fill)
    .boxed()
}

/// `1.3851 xrp / usd` · `10 drops quiet`: mono 13 `text`, a mono 9 `muted`
/// suffix, and an optional state word in its severity colour.
pub fn big_line<'a>(value: String, suffix: &str, word: Option<(&str, Color)>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let mut spans: Vec<Span<'static, ()>> = vec![
        span(value).size(BIG * scale).font(MONO).color(cp.text),
        span(format!("  {suffix}")).size(BIG_SUFFIX * scale).font(MONO).color(cp.muted),
    ];
    if let Some((w, c)) = word {
        spans.push(span(format!("  {w}")).size(BIG_SUFFIX * scale).font(MONO).color(c));
    }
    rich_text(spans).wrapping(Wrapping::None).boxed()
}

/// A field label inside a pane: mono 8.5 upper `muted`, `margin-bottom 4`.
pub fn label<'a>(word: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(text(word.to_uppercase()).font(MONO).size(STRIP * scale).color(cp.muted))
        .padding(Padding::new(0.0).bottom(4.0 * scale))
        .boxed()
}

/// A section rule inside a pane — `review`, `sign`: `margin-top 8`,
/// `padding-top 9`, the word, `padding-bottom 5`.
pub fn rule_label<'a>(word: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    column![
        Space::new().height(8.0 * scale),
        compact::hairline(cp.rule),
        Space::new().height(9.0 * scale),
        text(word.to_uppercase()).font(MONO).size(STRIP * scale).color(cp.muted),
        Space::new().height(5.0 * scale),
    ]
    .width(Length::Fill)
    .boxed()
}

/// The pane button: `padding 5px 0`, radius 6, `neutral` fill on a 1px
/// `border`, Inter 11 `text`; the primary variant fills `pill`. Disabled is
/// drawn dead rather than hidden.
pub fn pane_button<'a>(word: &str, enabled: bool, primary: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(
        text(word.to_string())
            .size(BUTTON * scale)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .color(if enabled { cp.text } else { cp.faint }),
    )
    .on_press_maybe(enabled.then_some(msg))
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(5.0 * scale).bottom(5.0 * scale))
    .style(move |_, status| {
        if !enabled {
            return button::Style {
                background: None,
                border: Border { color: cp.border_soft, width: 1.0, radius: (6.0 * scale).into() },
                text_color: cp.faint,
                shadow: Shadow::default(),
                snap: false,
            };
        }
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let fill = if hot { cp.hover } else if primary { cp.pill } else { cp.neutral };
        button::Style {
            background: Some(fill.into()),
            border: Border { color: cp.border, width: 1.0, radius: (6.0 * scale).into() },
            text_color: cp.text,
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}

// ── Buttons ─────────────────────────────────────────────────────────────────

/// The quiet button — `Cancel`, `Keep it`: `dim` ink on a `border_soft`
/// outline, no fill.
pub fn quiet_button<'a>(word: &str, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    outline_button(word, true, cp.dim, cp.hover, msg, cp, scale)
}

/// The destructive button — `Remove wallet`: `red` ink on a `border_soft`
/// outline, the ask tint on hover. Runs on the press; nothing confirms.
pub fn danger_button<'a>(word: &str, enabled: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    outline_button(word, enabled, cp.red, Color { a: 0.11, ..cp.red }, msg, cp, scale)
}

fn outline_button<'a>(
    word: &str,
    enabled: bool,
    ink: Color,
    wash: Color,
    msg: Message,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let ink = if enabled { ink } else { cp.faint };
    button(
        text(word.to_string())
            .size(BUTTON * scale)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .color(ink),
    )
    .on_press_maybe(enabled.then_some(msg))
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(5.0 * scale).bottom(5.0 * scale))
    .style(move |_, status| {
        let hot = enabled && matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if hot { wash } else { Color::TRANSPARENT }.into()),
            border: Border { color: cp.border_soft, width: 1.0, radius: (6.0 * scale).into() },
            text_color: ink,
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}

/// Two buttons on one line, `gap 6`.
pub fn button_pair<'a>(left: Element<'a, Message>, right: Element<'a, Message>, scale: f32) -> Element<'a, Message> {
    row![left, right].spacing(6.0 * scale).width(Length::Fill).boxed()
}

/// The one scroll region a list or form pane has, full-bleed to the pane's
/// edges (the rows carry their own inset). Fills the pane; the user drags
/// the pane for more rows.
pub fn scroller<'a>(content: Element<'a, Message>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    scroll_at(Length::Fill, content, cp, scale)
}

/// The same scroller sized to its CONTENT instead of its parent.
///
/// For a floating panel that should be as tall as the rows it holds — the pair
/// search results — rather than as tall as the space it is dropped into. iced's
/// `scrollable` is Fit by default; [`scroller`] overrides that to Fill
/// because a pane body must fill its pane, and a panel borrowing it inherited
/// the wrong one: the search results stood 380 tall on a single match (user,
/// 2026-09-12: "that height is a bit extreme").
///
/// Cap it with a bounded height (`Length::Fit.max`) on the container around it — under the cap the
/// panel hugs its rows, over it the rows scroll.
pub fn scroller_hug<'a>(content: Element<'a, Message>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    scroll_at(Length::Fit, content, cp, scale)
}

fn scroll_at<'a>(height: Length, content: Element<'a, Message>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    scrollable(content)
        .direction(Direction::Vertical(
            Scrollbar::new()
                .width(SCROLLER_W * scale)
                .scroller_width(SCROLLER_W * scale)
                .margin(0.0),
        ))
        .style(move |theme, status| scrollable::Style {
            container: container::Style::default(),
            vertical_rail: tokens_page::thumb_only(cp.border),
            horizontal_rail: tokens_page::thumb_only(cp.border),
            gap: None,
            ..scrollable::default(theme, status)
        })
        .width(Length::Fill)
        .height(height)
        .boxed()
}

/// A body that fills its cell down to `min` (unscaled px) and scrolls below
/// it: at or above `min` this is exactly the fill layout; below, the same
/// layout held at `min` tall inside the [`scroller`], so the bar appears
/// only once something is actually hidden. Users size their own panes
/// (user, 2026-09-10): a scroller costs nothing on a pane given its full
/// height — iced hides the bar while the content fits — and is there when
/// a short pane needs it. `body` is rebuilt per layout, so it clones what
/// it captures.
pub fn fill_or_scroll<'a>(
    min: f32,
    body: impl Fn() -> Element<'a, Message> + 'a,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    fill_or_scroll_by(min, move |_| body(), cp, scale)
}

/// [`fill_or_scroll`] whose body is handed the cell's size — for rows that
/// cut their content to the width they get, the pool face's addresses. The
/// bar floats over the content when it shows (no `spacing` on the
/// scrollbar), so the width is the same on both branches.
pub fn fill_or_scroll_by<'a>(
    min: f32,
    body: impl Fn(Size) -> Element<'a, Message> + 'a,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    responsive(move |size: Size| {
        if size.height >= min * scale {
            body(size)
        } else {
            scroller(
                container(body(size)).width(Length::Fill).height(Length::Fixed(min * scale)).boxed(),
                cp,
                scale,
            )
        }
    })
    .boxed()
}

/// The least a chart pane fills before it scrolls: the rate line, ~70px of
/// plot, the range row.
const CHART_MIN_H: f32 = 110.0;
/// The receive pane's: a code that still scans, the address, the link.
const RECEIVE_MIN_H: f32 = 190.0;
/// The destination-tag box, send and receive alike. Sized by its placeholder,
/// not its value: the box's width fixes the column count, and `destination
/// tag` is fifteen columns — 118 gave fourteen and clipped to `destination
/// ta`, 144 gave eighteen and a hole after the `g` (user, 2026-09-15). 122
/// is fifteen exactly; a u32's ten digits fit inside that.
pub const TAG_W: f32 = 122.0;

// ── Signing, inline ─────────────────────────────────────────────────────────

/// The 24-word field every dashboard signing surface draws: the grid, then its
/// status line — `N / 24 · words · ✓ checksum valid` on the left, `PASTE ›` on
/// an empty phrase or `CLEAR ›` on a started one at the right.
///
/// **This is the standard box, and it is the restore face's** (user,
/// 2026-09-12): restore had it, the other eight signing surfaces did not, and
/// the answer is that they adopt it rather than that anything about it changes.
/// No label above it — the pane title already says `restore key` / `cancel
/// offer`, and the `sign` rule says the rest; a second title over the box was
/// the same word twice.
///
/// Deliberately NOT shared with import's near-identical block. Import and
/// create are not signing screens; they have their own design and their own
/// type scale, and the two were never meant to be one component.
///
/// The paste link is discoverability, not capability — Ctrl/Cmd+V already
/// works in the field. Nobody types 24 words by hand if they know that, and
/// the people who don't know are exactly the ones who would. There is no copy
/// affordance and never should be: the phrase is masked, and a control that
/// lifted it back to the clipboard is a leak, not a convenience.
pub fn phrase_field<'a>(
    field:     SecureField,
    seed:      &'a SecureString,
    revealed:  bool,
    on_submit: Message,
    cp:        &'static CompactPalette,
    scale:     f32,
) -> Element<'a, Message> {
    let words = seed.as_str().split_whitespace().count();
    let complete = words == TARGET_WORDS;
    let checksum_ok = complete && wallet_setup::mnemonic_checksum_ok(seed.as_str());

    // Faint at zero, amber part-way, then the checksum's own verdict at 24 —
    // a full count with a bad word is a failure, not an achievement.
    let counter_ink = match words {
        0 => cp.faint,
        n if n == TARGET_WORDS => if checksum_ok { cp.green } else { cp.red },
        _ => cp.amber,
    };
    let mut status = vec![
        (format!("{words} / {TARGET_WORDS}"), counter_ink),
        (" \u{b7} words".to_string(), cp.dim),
    ];
    if words > 0 {
        let (verdict, ink) = match (complete, checksum_ok) {
            (false, _) => ("awaiting all 24 words", cp.muted),
            (true, true) => ("\u{2713} checksum valid", cp.green),
            (true, false) => ("\u{2715} invalid checksum", cp.red),
        };
        status.push((format!(" \u{b7} {verdict}"), ink));
    }

    // A paste onto a half-typed phrase replaces it, so the two never both
    // apply — one slot, two states.
    let action = if words == 0 {
        compact::upper_link("Paste", Message::SecurePasteRequested(field), cp, scale)
    } else {
        compact::upper_link("Clear", Message::SecureClearRequested(field), cp, scale)
    };

    column![
        compact::phrase_box(field, seed, revealed, PHRASE_PLACEHOLDER, PHRASE_H, on_submit, cp, scale),
        Space::new().height(5.0 * scale),
        row![
            compact::mono_runs(status, STRIP * scale),
            Space::new().width(Length::Fill),
            action,
        ]
        .align_y(Alignment::Center),
    ]
    .width(Length::Fill)
    .boxed()
}

/// The sign block every signing pane ends in: a rule, the credential the
/// wallet takes, the 25th word, and the buttons the caller passes — the
/// send stack minus the recipient, drawn inside the pane. `armed` is the
/// caller's own gate; the block adds the credential's completeness.
///
/// `sign` is the one label (user, 2026-09-09): the placeholders say which
/// box is which, and a label over the key with none over the 25th word read
/// as a difference that is not there. Same words on both chains.
#[allow(clippy::too_many_arguments)]
pub fn sign_block<'a>(
    fields: &SignFields<'a>,
    secret_buf: &'a SecureString,
    phrase_buf: &'a SecureString,
    bip39_buf: &'a SecureString,
    armed: bool,
    buttons: Element<'a, Message>,
    inner: f32,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let live = armed && fields.can_submit();
    let submit = live.then(|| fields.on_submit.clone());
    let credential: Element<'a, Message> = match fields.mode {
        EnableInputMode::Passphrase => compact::boxed_credential(
            fields.secret,
            secret_buf,
            fields.secret_reveal,
            KEY_PLACEHOLDER,
            inner,
            submit.clone(),
            cp,
            scale,
        ),
        // The restore face's field, status line and `Paste ›` / `Clear ›` and
        // all — one standard box across every signing surface on both chains.
        EnableInputMode::Seed => phrase_field(
            fields.phrase,
            phrase_buf,
            fields.phrase_reveal,
            fields.on_submit.clone(),
            cp,
            scale,
        ),
    };
    // `Clear ›`, and the reason this block has one at all: nothing else in a
    // pane empties these buffers. `clear_send_form` and its twins run from the
    // submit handler only, so before this (2026-09-12) someone who pasted 24
    // words and then thought better of it had no way out but to backspace the
    // phrase character by character — closing the pane, switching chains and
    // resetting the layout all leave `AppState` untouched, because the buffers
    // live there and not in the pane. A mnemonic with no exit is the wrong
    // thing to leave sitting in memory.
    //
    column![
        rule_label("sign", cp, scale),
        credential,
        Space::new().height(8.0 * scale),
        compact::boxed_credential(
            fields.bip39,
            bip39_buf,
            fields.bip39_reveal,
            BIP39_PLACEHOLDER,
            inner,
            submit,
            cp,
            scale,
        ),
        Space::new().height(10.0 * scale),
        buttons,
    ]
    .width(Length::Fill)
    .boxed()
}

/// The primary button of a sign block: `pill` fill, dead until the block is
/// complete.
pub fn primary<'a>(word: &str, live: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    pane_button(word, live, true, msg, cp, scale)
}

/// [`primary`] wearing a caller's colour instead of `pill` — `(fill, border,
/// ink)`, the same triple the ticket's Buy/Sell segments take.
///
/// It exists for one button (2026-09-16): the trade ticket's, which signs a
/// buy or a sell and until now said so only in its verb while the segment
/// two rows above said it in green or red. A CTA that commits a direction
/// should be the direction's colour — a red `Sell XRP` is a different press
/// from a green `Buy XRP`, and the eye should not have to read the word to
/// know which one it is about to make.
///
/// Dead is dead: an incomplete block draws the same faint outline every
/// other pane button does, because a tinted disabled button reads as armed.
/// Hover deepens the caller's fill rather than swapping in `hover`, so the
/// colour never leaves the button once it is live.
#[allow(clippy::too_many_arguments)]
pub fn primary_tinted<'a>(
    word:  &str,
    live:  bool,
    fill:  Color,
    line:  Color,
    ink:   Color,
    msg:   Message,
    cp:    &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    button(
        text(word.to_string())
            .size(BUTTON * scale)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .color(if live { ink } else { cp.faint }),
    )
    .on_press_maybe(live.then_some(msg))
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(5.0 * scale).bottom(5.0 * scale))
    .style(move |_, status| {
        if !live {
            return button::Style {
                background: None,
                border: Border { color: cp.border_soft, width: 1.0, radius: (6.0 * scale).into() },
                text_color: cp.faint,
                shadow: Shadow::default(),
                snap: false,
            };
        }
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = if hot { Color { a: (fill.a * 1.75).min(1.0), ..fill } } else { fill };
        button::Style {
            background: Some(bg.into()),
            border: Border { color: line, width: 1.0, radius: (6.0 * scale).into() },
            text_color: ink,
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}

// ── chart ───────────────────────────────────────────────────────────────────

/// The rate, the chart, and the period's range in one line. `series` is
/// the period's `(stamp, price)` points — the stamps are what the scrub
/// reads; `live` is the current cross (0 when unknown, then the series'
/// last point stands in); `fmt` writes a price the way the chain's pane
/// does — XRP at four decimals, BTC as money.
#[allow(clippy::too_many_arguments)]
pub fn chart_pane<'a>(
    series: Vec<(u64, f32)>,
    live: f32,
    period: ChartPeriod,
    suffix: &str,
    fmt: fn(f32) -> String,
    dark: bool,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let current = if live > 0.0 { live } else { series.last().map_or(0.0, |p| p.1) };
    let up = series.first().map_or(true, |first| current >= first.1);
    let (low, high) = series
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &(_, v)| (lo.min(v), hi.max(v)));

    // The line takes the range's direction; the area under it is NEUTRAL —
    // `dim` fading to nothing — because the direction is already stated
    // twice, and a third tinted wash would stain a quarter of the pane.
    let mut ink = ChartInk::new(cp, up, dark, 0.0, scale);
    ink.fill = cp.dim;
    ink.fill_top_a = if dark { 0.13 } else { 0.16 };

    let range_key = match period {
        ChartPeriod::OneHour => "1h range",
        ChartPeriod::OneDay => "24h range",
    };
    let range = if low.is_finite() && high.is_finite() {
        format!("{} \u{2013} {}", fmt(low), fmt(high))
    } else {
        NA.to_string()
    };
    let rate = if current > 0.0 { fmt(current) } else { NA.to_string() };
    let suffix = suffix.to_string();

    fill_or_scroll(
        CHART_MIN_H,
        move || {
            let chart: Element<'a, Message> = if series.len() >= 2 {
                Canvas::new(PaneChart { series: series.clone(), period, fmt, ink })
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .boxed()
            } else {
                container(text("waiting for data\u{2026}").size(ROW_KEY * scale).color(cp.muted))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .boxed()
            };
            column![
                big_line(rate.clone(), &suffix, None, cp, scale),
                container(chart)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(Padding::new(0.0).top(2.0 * scale)),
                drow(range_key, vec![(range.clone(), cp.dim)], 3.0, cp, scale),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .boxed()
        },
        cp,
        scale,
    )
}

/// The pane chart: three dashed `rule` gridlines, a solid baseline, a
/// neutral wash, the line — and the scrub the full chart screen had
/// (user, 2026-09-10: dropping it with the screen was a regression; the
/// price and the time under the pointer are the point of a chart). The
/// hairline, dot and boxed readout are the old chart's, the box sized to
/// its text so it fits a 275px pane.
struct PaneChart {
    series: Vec<(u64, f32)>,
    period: ChartPeriod,
    fmt: fn(f32) -> String,
    ink: ChartInk,
}

/// Where the pointer is over the chart, kept by the canvas itself.
#[derive(Default, Clone)]
pub struct ChartHover {
    cursor_x: Option<f32>,
}

impl<Message> canvas::Program<Message> for PaneChart {
    type State = ChartHover;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let next = bounds.contains(*position).then(|| position.x - bounds.x);
                if state.cursor_x != next {
                    state.cursor_x = next;
                    return Some(canvas::Action::request_redraw());
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::CursorLeft) if state.cursor_x.is_some() => {
                state.cursor_x = None;
                Some(canvas::Action::request_redraw())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let ink = &self.ink;
        let s = ink.scale;
        let w = bounds.width;
        let h = bounds.height;
        let mut frame = Frame::new(renderer, bounds.size());

        let dash = [2.0 * s, 3.0 * s];
        for f in [0.25, 0.5, 0.75] {
            let y = (h * f).round() + 0.5;
            frame.stroke(
                &Path::line(Point::new(0.0, y), Point::new(w, y)),
                Stroke {
                    line_dash: LineDash { segments: &dash, offset: 0 },
                    ..Stroke::default().with_color(ink.baseline).with_width(1.0)
                },
            );
        }
        let base_y = (h - 1.0).round() + 0.5;
        frame.stroke(
            &Path::line(Point::new(0.0, base_y), Point::new(w, base_y)),
            Stroke::default().with_color(ink.baseline).with_width(1.0),
        );

        let n = self.series.len();
        if n < 2 {
            return vec![frame.into_geometry()];
        }
        let min = self.series.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max = self.series.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        let range = (max - min).max(1e-6);
        let pad = 6.0 * s;
        let plot_h = (h - 1.0 - 2.0 * pad).max(1.0);
        let pt = |i: usize| -> Point {
            let x = i as f32 / (n - 1) as f32 * w;
            let y = pad + (1.0 - (self.series[i].1 - min) / range) * plot_h;
            Point::new(x, y)
        };

        let area = Path::new(|b| {
            b.move_to(pt(0));
            for i in 1..n {
                b.line_to(pt(i));
            }
            b.line_to(Point::new(w, h));
            b.line_to(Point::new(0.0, h));
            b.close();
        });
        let wash = canvas_gradient::Linear::new(Point::new(0.0, 0.0), Point::new(0.0, h))
            .add_stop(0.0, Color { a: ink.fill_top_a, ..ink.fill })
            .add_stop(1.0, Color { a: 0.0, ..ink.fill });
        frame.fill(
            &area,
            Fill { style: fill::Style::Gradient(CanvasGradient::Linear(wash)), rule: fill::Rule::NonZero },
        );

        let line = Path::new(|b| {
            b.move_to(pt(0));
            for i in 1..n {
                b.line_to(pt(i));
            }
        });
        frame.stroke(
            &line,
            Stroke::default()
                .with_color(ink.line)
                .with_width(CHART_STROKE * s)
                .with_line_join(LineJoin::Round)
                .with_line_cap(LineCap::Round),
        );

        // ── The scrub: hairline, dot, and the point's price over its time ──
        if let Some(cx) = state.cursor_x {
            let i = ((cx / w) * (n - 1) as f32).round().clamp(0.0, (n - 1) as f32) as usize;
            let (ts, price) = self.series[i];
            let at = pt(i);
            let x = at.x.round() + 0.5;
            frame.stroke(
                &Path::line(Point::new(x, 0.0), Point::new(x, h)),
                Stroke::default().with_color(Color { a: 0.5, ..ink.hairline }).with_width(1.0),
            );
            frame.fill(&Path::circle(at, 4.0 * s), ink.line);
            frame.fill(&Path::circle(at, 2.4 * s), ink.window);

            // The readout rides the hairline as the old chart's did — a
            // `panel` box on a 1px `border`, centred on the line and kept
            // inside the chart — sized to its text rather than the screen's
            // fixed 118px. Top-left under the rate was tried and read as
            // two prices stacked (user, 2026-09-10).
            let price_word = (self.fmt)(price);
            let time_word = fmt_hover_time(ts, &self.period);
            let price_size = ROW_VALUE * s;
            let time_size = PILL * s;
            let pad_x = 7.0 * s;
            let pad_y = 5.0 * s;
            let advance = 0.6;
            let tw = (price_word.chars().count() as f32 * price_size).max(time_word.chars().count() as f32 * time_size) * advance;
            let box_w = tw + 2.0 * pad_x;
            let box_h = price_size + if time_word.is_empty() { 0.0 } else { time_size + 3.0 * s } + 2.0 * pad_y;
            let box_x = (x - box_w / 2.0).round().clamp(0.0, (w - box_w).max(0.0));
            let box_y = 6.0 * s;
            let tooltip = Path::new(|b| {
                b.rounded_rectangle(Point::new(box_x + 0.5, box_y + 0.5), Size::new(box_w, box_h), iced::border::Radius::from(4.0 * s));
            });
            frame.fill(&tooltip, ink.panel);
            frame.stroke(&tooltip, Stroke::default().with_color(ink.border).with_width(1.0));
            frame.fill_text(canvas::Text {
                content: price_word,
                position: Point::new(box_x + box_w / 2.0, box_y + pad_y),
                color: ink.text,
                size: Pixels(price_size),
                font: fonts::MONO,
                align_x: Horizontal::Center.into(),
                align_y: Vertical::Top.into(),
                ..canvas::Text::default()
            });
            if !time_word.is_empty() {
                frame.fill_text(canvas::Text {
                    content: time_word,
                    position: Point::new(box_x + box_w / 2.0, box_y + pad_y + price_size + 3.0 * s),
                    color: ink.muted,
                    size: Pixels(time_size),
                    font: fonts::MONO,
                    align_x: Horizontal::Center.into(),
                    align_y: Vertical::Top.into(),
                    ..canvas::Text::default()
                });
            }
        }

        vec![frame.into_geometry()]
    }
}

// ── copy glyph ──────────────────────────────────────────────────────────────

/// The copy glyph's box — a hair over the row text it sits beside.
const COPY_GLYPH: f32 = 12.0;

/// The copy control beside an address (2026-09-14): the glyph, `muted` and
/// `text` on hover, or `✓ copied` in its place for the feedback beat. One
/// drawing for the wallet panes and the receive panes on both chains — it
/// replaced the receive pane's full-width `Copy address` button, which was
/// a lot of button for one glyph's worth of action.
pub fn copy_glyph<'a>(copied: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if copied {
        return text("\u{2713} copied").size(ROW_KEY * scale).color(cp.green).boxed();
    }
    let g = COPY_GLYPH * scale;
    button(
        svg(crate::utils::icons::COPY.clone())
            .width(Length::Fixed(g))
            .height(Length::Fixed(g))
            .style(move |_, status| svg::Style {
                color: Some(match status {
                    svg::Status::Hovered => cp.text,
                    _ => cp.muted,
                }),
            }),
    )
    .on_press(msg)
    .padding(Padding::ZERO)
    .style(bare)
    .boxed()
}

/// `bc1qyfxmsj…amh06f67` — 10 and 8, so every address type lands on 19
/// columns: 42 for native SegWit, 62 for Taproot, 34 for the legacy forms.
/// For the wallet panes' address row, where the copy glyph beside it makes
/// the full string unnecessary (2026-09-14). The transaction lists keep
/// their own copies of this rule, per chain.
pub fn elide_addr(a: &str) -> String {
    cut_addr(a, 10, 8)
}

/// [`elide_addr`] only when it must: the address whole while `cols` hold it,
/// otherwise its head cut to the columns left beside the same 8 of tail. For
/// a row that knows its width — the pool face's, which had elided every
/// address at every width (user, 2026-09-25: plenty of space, and no
/// reason). Under the 19 columns the standard cut needs it draws that cut
/// and lets the row clip.
pub fn fit_addr(a: &str, cols: usize) -> String {
    let n = a.chars().count();
    if n <= cols {
        a.to_string()
    } else if cols < 10 + 1 + 8 {
        elide_addr(a)
    } else {
        cut_addr(a, cols - 1 - 8, 8)
    }
}

/// `head…tail`, or the address whole when the cut would not shorten it.
fn cut_addr(a: &str, head: usize, tail: usize) -> String {
    let n = a.chars().count();
    if n <= head + tail + 1 {
        return a.to_string();
    }
    let h: String = a.chars().take(head).collect();
    let t: String = a.chars().skip(n - tail).collect();
    format!("{h}\u{2026}{t}")
}

// ── receive ─────────────────────────────────────────────────────────────────

// ── destination tag ─────────────────────────────────────────────────────────
// XRP only. Two faces on two panes (send, receive), one shape: a readout
// line where the tag shows, and a form the pane swaps to for setting it.
// It was a box sitting open on both panes for a day (2026-09-15) — the user:
// most people never use it, and the ones who do want it to feel locked in.
// A link and a receipt do that; an empty box says "optional, skip".

/// The readout under a recipient or a receive address: `set destination tag ›`
/// when there is none, `tag 12345 ›` when there is — the tag itself is the
/// link (user, 2026-09-15: a `change` beside it was awkward).
pub fn tag_line<'a>(tag: Option<u32>, pane: TagPane, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    pane_link(&tag_word(tag), Message::TagEdit(pane), cp, scale)
}

/// The tag line's words, for a caller that hands them on — the XRP receive
/// pane gives them to [`receive_pane`] as its link.
pub fn tag_word(tag: Option<u32>) -> String {
    match tag {
        None => "set destination tag".to_string(),
        Some(t) => format!("tag {t}"),
    }
}

/// The quiet link a pane draws under its content — the tag line, and BTC's
/// `addresses · 3 ›` under its receive address: the list rows' `WORD ›`
/// voice, at the hint size. BTC's had been a full-width pane button (user,
/// 2026-09-25: the word itself can be the link; a button that wide for one
/// door was width nothing needed).
pub fn pane_link<'a>(word: &str, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(
        text(format!("{} \u{203a}", word.to_uppercase()))
            .font(MONO)
            .size(HINT * scale),
    )
    .padding(Padding::new(2.0 * scale).left(5.0 * scale).right(5.0 * scale))
    .on_press(msg)
    .style(move |_, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if hot { cp.neutral } else { Color::TRANSPARENT }.into()),
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (4.0 * scale).into() },
            text_color: if hot { cp.text } else { cp.dim },
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}

/// The tag form — what a pane's face becomes after `set destination tag ›`:
/// the box, its one possible fault, and `Cancel` / `Set tag`. The strip
/// above already says `destination tag`, so there is no label. `Set tag`
/// with an empty box sets none — that is how a tag is removed.
///
/// The block is centred in the pane, both ways (user, 2026-09-25): a box and
/// two buttons hugging the top edge read as a form that lost its fields.
/// This is not the `space-between` the send pane threw out — the gaps are
/// fixed and only the block moves, the way the code sits in the receive
/// pane. The box is centred over the pair; the pair keeps the full width
/// every pane button has. Under [`tag_min_h`] the same block scrolls from
/// the top, as the receive body does.
pub fn tag_face<'a>(
    pane: TagPane,
    draft: &'a str,
    wrong: bool,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let field = match pane {
        TagPane::Send => PlainField::XrpSendTag,
        TagPane::Receive => PlainField::XrpReceiveTag,
    };
    let body = move || -> Element<'a, Message> {
        let set = Message::TagSet(pane);
        let boxed = screen::boxed_plain(
            field,
            draft,
            "destination tag",
            TAG_W,
            FIELD_VALUE,
            false,
            None,
            0.0,
            None,
            0.0,
            (!wrong).then_some(set.clone()),
            cp,
            scale,
        );
        let mut col = column![container(boxed).width(Length::Fill).align_x(Alignment::Center)].width(Length::Fill);
        if wrong {
            col = col.push(Space::new().height(5.0 * scale).boxed()).push(
                text("a destination tag is a whole number up to 4294967295")
                    .font(MONO)
                    .size(HINT * scale)
                    .color(cp.red)
                    .width(Length::Fill)
                    .align_x(Alignment::Center).boxed(),
            );
        }
        col = col.push(Space::new().height(12.0 * scale).boxed()).push(button_pair(
            quiet_button("Cancel", Message::TagCancelled(pane), cp, scale),
            primary("Set tag", !wrong, set, cp, scale),
            scale,
        ));
        container(col).width(Length::Fill).height(Length::Fill).align_y(Alignment::Center).boxed()
    };
    fill_or_scroll(tag_min_h(wrong), body, cp, scale)
}

/// The least the tag face fills before it scrolls: the box, the `12` gap and
/// the pair — plus the fault line and its gap while it shows.
fn tag_min_h(wrong: bool) -> f32 {
    screen::box_h(FIELD_VALUE) + 12.0 + BUTTON_H + if wrong { 5.0 + HINT * 1.3 } else { 0.0 }
}

/// The code fills the remaining height, square and centred; the address in
/// full under it with the copy glyph beside it; and one quiet link centred
/// under that, `(word, message)` — the door to the pane's other face. XRP's
/// is the tag line (`set destination tag ›` / `tag 12345 ›`), which turns the
/// address above it into a tagged X-address; BTC's is `addresses · 3 ›`,
/// which opens the pool. Nothing else under the address: BTC's caption there
/// was `rotating address` while rotation existed and `receive address` after
/// it went — the strip's word again (user, 2026-09-25). The same anatomy on
/// both chains: code, address and glyph, one link.
pub fn receive_pane<'a>(
    address: String,
    copied: bool,
    on_copy: Message,
    link: (String, Message),
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    fill_or_scroll(
        RECEIVE_MIN_H,
        move || receive_body(address.clone(), copied, on_copy.clone(), link.clone(), cp, scale),
        cp,
        scale,
    )
}

fn receive_body<'a>(
    address: String,
    copied: bool,
    on_copy: Message,
    link: (String, Message),
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let payload = address.clone();

    let code = responsive(move |size: Size| {
        let side = size.width.min(size.height).max(0.0);
        let quiet = (side * QR_QUIET).round();
        let modules = (side - 2.0 * quiet).max(0.0);
        container(
            container(
                svg(svg::Handle::from_memory(qr::svg_bytes(&payload, QR_INK)))
                    .width(Length::Fixed(modules))
                    .height(Length::Fixed(modules)),
            )
            .padding(quiet)
            .style(|_| container::Style {
                background: Some(QR_PLATE.into()),
                border: Border { radius: 1.0.into(), ..Default::default() },
                ..Default::default()
            }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .boxed()
    });

    // The address centred with the glyph at its shoulder: the address wraps
    // when the pane is narrow, so the glyph sits at the top of its span
    // rather than floating mid-block.
    let address_row = row![
        Space::new().width(Length::Fill),
        text(address)
            .font(MONO)
            .size(RECEIVE_ADDR * scale)
            .color(cp.dim)
            .align_x(Alignment::Center)
            .wrapping(Wrapping::Glyph),
        Space::new().width(6.0 * scale),
        copy_glyph(copied, on_copy, cp, scale),
        Space::new().width(Length::Fill),
    ]
    .align_y(Alignment::Start);

    // The link centred under the address. On XRP the address changing shape
    // above it is the receipt for setting a tag.
    let (word, msg) = link;
    column![
        container(code)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::new(0.0).top(2.0 * scale).bottom(9.0 * scale)),
        address_row,
        Space::new().height(6.0 * scale),
        container(pane_link(&word, msg, cp, scale))
            .width(Length::Fill)
            .align_x(Alignment::Center),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

// ── The empty well ──────────────────────────────────────────────────────────

/// The Empty branch — nothing open: one dashed well, a `+`, two words.
/// No reassurance copy and no quick-add row: `panels +` is lit two inches
/// away and holds every panel plus `Reset to default`.
fn empty_state<'a>(wrap: fn(GridMsg) -> Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let words = column![
        plus(EMPTY_PLUS, wrap, cp, scale),
        Space::new().height(9.0 * scale),
        text("No panels").font(LIGHT).size(EMPTY_WORD * scale).color(cp.dim),
    ]
    .align_x(Alignment::Center);
    container(well(words.boxed(), 10.0, cp, scale))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(9.0 * scale)
        .boxed()
}

/// The well a split leaves behind, scoped to that pane: the same `+`, and
/// nothing else. The next panel checked fills it.
pub fn well_pane<'a>(wrap: fn(GridMsg) -> Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    well(plus(EMPTY_PLUS, wrap, cp, scale), 6.0, cp, scale)
}

/// The `+`: Inter 300 `faint`, and a door to the menu.
fn plus<'a>(size: f32, wrap: fn(GridMsg) -> Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(text("+").font(LIGHT).size(size * scale))
        .on_press(wrap(GridMsg::MenuToggled))
        .padding(Padding::ZERO)
        .style(move |_, status| button::Style {
            background: None,
            border: Border::default(),
            text_color: match status {
                button::Status::Hovered | button::Status::Pressed => cp.dim,
                _ => cp.faint,
            },
            shadow: Shadow::default(),
            snap: false,
        })
        .boxed()
}

/// A 1px dashed `border_soft` rounded well with `content` centred in it.
pub fn well<'a>(content: Element<'a, Message>, radius: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let frame = Canvas::new(DashedWell { color: cp.border_soft, radius: radius * scale, dash: 4.0 * scale })
        .width(Length::Fill)
        .height(Length::Fill);
    stack![
        frame,
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

/// iced borders are solid; the dashed edge is a stroked path.
pub struct DashedWell {
    pub color: Color,
    pub radius: f32,
    pub dash: f32,
}

impl<Message> canvas::Program<Message> for DashedWell {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let outline = Path::new(|b| {
            b.rounded_rectangle(
                Point::new(0.5, 0.5),
                Size::new((bounds.width - 1.0).max(0.0), (bounds.height - 1.0).max(0.0)),
                iced::border::Radius::from(self.radius),
            );
        });
        let segments = [self.dash, self.dash * 0.75];
        frame.stroke(
            &outline,
            Stroke {
                line_dash: LineDash { segments: &segments, offset: 0 },
                ..Stroke::default().with_color(self.color).with_width(1.0)
            },
        );
        vec![frame.into_geometry()]
    }
}

// ── panels + ────────────────────────────────────────────────────────────────

/// The dropdown, 4px under the bar with its right edge on the button's.
/// Outside click dismisses; clicks inside are the menu's own.
fn menu_layer<'a>(host: Host<'a>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    mouse_area(
        container(opaque(menu_panel(host, cp, scale)))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::Start)
            .padding(Padding::new(0.0).top((panes::BAR_H + 4.0) * scale + 1.0).right(11.0 * scale)),
    )
    // `on_release`, not `on_press` — see `panes/pair.rs` for the full reason.
    // This area covers the whole grid, and `mouse_area` captures a press but
    // not a release, so `on_press` made the first click anywhere on the
    // dashboard do nothing but close the menu. The scrimless dropdowns both
    // dismiss on release now; `components/modal.rs` keeps `on_press` on
    // purpose, because its scrim visibly dims what is behind it.
    .on_release((host.wrap)(GridMsg::MenuDismissed))
    .boxed()
}

/// PANELS — a checklist of the chain's own kinds: `✓` on a pane and
/// unchecking here are the same action, and checking adds it back. Then
/// reset and undo.
fn menu_panel<'a>(host: Host<'a>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let g = host.grid;
    let wrap = host.wrap;

    let mut col = column![menu_head("panels", cp, scale)].width(Length::Fill);
    for kind in PaneKind::canonical(host.chain).iter().copied() {
        col = col.push(menu_item(
            kind.menu_label(),
            Some(g.has(kind)),
            None,
            true,
            wrap(GridMsg::Toggle(kind)),
            cp,
            scale,
        ));
    }
    // LAYOUT presets (one/two/three columns) PULLED from the menu 2026-09-09
    // (user): they dealt the open panes down equal columns in menu order,
    // ignoring the tree the user had built, so the result never resembled
    // the layout they had. `Preset` and `GridMsg::Preset` stay parked for a
    // future "tidy" that keeps the tree's order.
    col = col
        .push(menu_sep(cp, scale))
        // `Undo close` (ctrl+z) was removed here 2026-09-12 (user). It undid
        // close, preset and reset and nothing else — not add, drag, resize or
        // split — so it read as broken to anyone who tried it after one of
        // those, and it sat greyed out until a `×` had been pressed. `Reset to
        // default` is the way back.
        .push(menu_item("Reset to default", None, Some("ctrl+0"), true, wrap(GridMsg::Reset), cp, scale));

    container(col)
        .width(Length::Fixed(MENU_W * scale))
        .padding(Padding::new(0.0).top(5.0 * scale).bottom(5.0 * scale))
        .style(move |_| container::Style {
            background: Some(cp.panel.into()),
            border: Border { color: cp.border, width: 1.0, radius: (9.0 * scale).into() },
            shadow: Shadow {
                color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.45 },
                offset: Vector::new(0.0, 24.0 * scale),
                blur_radius: 54.0 * scale,
            },
            ..Default::default()
        })
        .boxed()
}

/// A section head: mono 8.5 upper `muted`, `padding 8px 13px 5px`.
fn menu_head<'a>(word: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(text(word.to_uppercase()).font(MONO).size(STRIP * scale).color(cp.muted))
        .padding(Padding::new(0.0).top(8.0 * scale).bottom(5.0 * scale).left(MENU_PAD_H * scale).right(MENU_PAD_H * scale))
        .boxed()
}

/// One row: a 9px check slot, the label (Inter 12.5; `dim` when off,
/// `faint` and dead when the row cannot apply), the shortcut pinned right.
fn menu_item<'a>(
    word: &str,
    checked: Option<bool>,
    shortcut: Option<&str>,
    enabled: bool,
    msg: Message,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let check: Element<'a, Message> = match checked {
        // Inter's check: JetBrains Mono has none, and a fallback glyph changes
        // with the fonts installed.
        Some(true) => text("\u{2713}").size(MENU_KEY * scale).color(cp.dim).boxed(),
        _ => Space::new().boxed(),
    };
    let ink = if !enabled {
        cp.faint
    } else if checked == Some(false) {
        cp.dim
    } else {
        cp.text
    };
    let mut line = row![
        container(check).width(Length::Fixed(MENU_CHECK_W * scale)),
        text(word.to_string()).size(MENU_ITEM * scale).color(ink),
    ]
    .spacing(9.0 * scale)
    .align_y(Alignment::Center);
    if let Some(key) = shortcut {
        line = line
            .push(Space::new().width(Length::Fill).boxed())
            .push(text(key.to_string()).font(MONO).size(MENU_KEY * scale).color(cp.faint).boxed());
    }
    button(line.width(Length::Fill))
        .on_press_maybe(enabled.then_some(msg))
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(5.0 * scale).bottom(5.0 * scale).left(MENU_PAD_H * scale).right(MENU_PAD_H * scale))
        .style(move |_, status| {
            let hot = enabled && matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if hot { cp.hover } else { Color::TRANSPARENT }.into()),
                border: Border::default(),
                text_color: ink,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}

/// A 1px `rule`, `margin 5px 0`.
fn menu_sep<'a>(cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    column![
        Space::new().height(5.0 * scale),
        compact::hairline(cp.rule),
        Space::new().height(5.0 * scale),
    ]
    .width(Length::Fill)
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::{elide_addr, fit_addr};

    const SEGWIT: &str = "bc1qyfxmsjq0pwk4kg3x5e7cqk8rjfhjrm2xamh06f67"; // 44 chars

    #[test]
    fn fit_addr_is_whole_while_the_columns_hold_it() {
        assert_eq!(fit_addr(SEGWIT, SEGWIT.len()), SEGWIT);
        assert_eq!(fit_addr(SEGWIT, 60), SEGWIT);
    }

    #[test]
    fn fit_addr_cuts_to_the_columns_it_has_keeping_the_tail() {
        let cut = fit_addr(SEGWIT, 30);
        assert_eq!(cut.chars().count(), 30, "{cut}");
        assert!(cut.starts_with("bc1qyfxmsjq0pwk4kg3x5"), "{cut}");
        assert!(cut.ends_with("\u{2026}amh06f67"), "{cut}");
    }

    #[test]
    fn fit_addr_falls_back_to_the_standard_cut_under_nineteen_columns() {
        assert_eq!(fit_addr(SEGWIT, 12), elide_addr(SEGWIT));
    }

    #[test]
    fn elide_addr_is_ten_and_eight() {
        assert_eq!(elide_addr(SEGWIT), "bc1qyfxmsj\u{2026}amh06f67");
        assert_eq!(elide_addr("short"), "short");
    }
}
