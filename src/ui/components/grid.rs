

use iced::widget::{Column, Row};
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

use crate::controller::app_state::{AppState, ChartPeriod, EnableInputMode, ReceiveFlash};
use crate::controller::message::{Message, PlainField, SecureField, TagPane};
use crate::controller::panes::{self, Chain, Grid, GridMsg, PaneKind};
use crate::secure::SecureString;
use crate::ui::components::compact;
use crate::ui::components::tui::MONO_ADVANCE;
use crate::ui::components::signing::SignFields;
use crate::controller::app_state::PhraseLine;
use crate::ui::components::phrase_entry::{self, Entry};
use crate::ui::managexrp::tokens as tokens_page;
use crate::utils::fonts::{LIGHT, MONO};
use crate::utils::plain_field::plain_field_grid_ink_requested;
use crate::utils::qr;
use crate::utils::fonts;
use crate::utils::sparkline::{fmt_hover_time, ChartInk};
use crate::utils::theme::CompactPalette;


pub const BAR_BUTTON: f32 = 9.0;
pub const STRIP: f32 = 8.5;
pub const PILL: f32 = 8.0;
pub const HERO: f32 = 20.0;
pub const HERO_UNIT: f32 = 9.0;
pub const ROW_KEY: f32 = 9.5;
pub const ROW_VALUE: f32 = 10.0;
pub const ROW_TOTAL: f32 = 11.5;
pub const BIG: f32 = 13.0;
pub const BIG_SUFFIX: f32 = 9.0;
pub const FIELD_VALUE: f32 = 11.5;
pub const ADDR: f32 = 11.0;
pub const HINT: f32 = 8.5;
pub const RECEIVE_ADDR: f32 = 10.0;
const RECEIVE_ADDR_MARGIN: f32 = 24.0;
pub const BUTTON: f32 = 11.0;
pub const BUTTON_H: f32 = BUTTON * 1.3 + 10.0;
const MENU_ITEM: f32 = 12.5;
const MENU_KEY: f32 = 9.0;
const EMPTY_PLUS: f32 = 30.0;
const EMPTY_WORD: f32 = 15.0;
const CHART_STROKE: f32 = 1.2;


const PANE_PAD_TOP: f32 = 10.0;
pub const PANE_PAD_H: f32 = 12.0;
const PANE_PAD_BOTTOM: f32 = 11.0;
const STRIP_PAD_BOTTOM: f32 = 9.0;
const CONTROL_GAP: f32 = 8.0;
const GLYPH: f32 = 9.0;
const GLYPH_TEXT: f32 = 10.0;
const CLOSE_GLYPH: f32 = 12.0;
const GLYPH_LINE: f32 = 13.0;

const MENU_W: f32 = 254.0;
const MENU_PAD_H: f32 = 13.0;
const MENU_CHECK_W: f32 = 9.0;
pub const AMT_GAP: f32 = 7.0;
pub const EQ_W: f32 = 8.0;
const QR_QUIET: f32 = 0.056;
const SCROLLER_W: f32 = 3.0;

pub const QR_PLATE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
pub const QR_INK: Color = Color { r: 0.133, g: 0.149, b: 0.173, a: 1.0 };

const SPLIT_V_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="#000" stroke-width="1"><rect x="0.5" y="0.5" width="8" height="8" rx="1"/><line x1="3.5" y1="0.5" x2="3.5" y2="8.5"/></svg>"##;
const SPLIT_H_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="9" height="9" viewBox="0 0 9 9" fill="none" stroke="#000" stroke-width="1"><rect x="0.5" y="0.5" width="8" height="8" rx="1"/><line x1="0.5" y1="3.5" x2="8.5" y2="3.5"/></svg>"##;

static SPLIT_V: LazyLock<svg::Handle> = LazyLock::new(|| svg::Handle::from_memory(SPLIT_V_SVG));
static SPLIT_H: LazyLock<svg::Handle> = LazyLock::new(|| svg::Handle::from_memory(SPLIT_H_SVG));


pub const KEY_PLACEHOLDER: &str = "encryption key";
pub const BIP39_PLACEHOLDER: &str = "optional 25th word";

pub const NA: &str = "\u{2014}";


#[derive(Clone, Copy)]
pub struct Host<'a> {
    pub chain: Chain,

    pub grid: &'a Grid,

    pub wrap: fn(GridMsg) -> Message,
}


pub type PaneBody<'a> =
    fn(&'a AppState, Pane, PaneKind, bool, &'static CompactPalette, f32) -> PaneContent<'a>;

pub type PaneContent<'a> = pane_grid::Content<'a, Message, Element<'a, Message>, Element<'a, Message>>;


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
        top_bar(host, cp, scale),
        compact::hairline(cp.rule),
        container(body_el).width(Length::Fill).height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill);
    let mut layers = stack![base].width(Length::Fill).height(Length::Fill);
    if g.menu_open {
        layers = layers.push(menu_layer(host, cp, scale));
    }
    layers.boxed()
}


fn top_bar<'a>(host: Host<'a>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let bar = row![compact::corner_button(crate::utils::icons::SLIDERS.clone(), Message::OpenSettings, cp, scale)]
        .align_y(Alignment::Center)
        .height(Length::Fill)
        .push(Space::new().width(Length::Fill).boxed())
        .push(panels_button(host.grid.menu_open, host.wrap, cp, scale));
    container(bar)
    .width(Length::Fill)
    .height(Length::Fixed(panes::BAR_H * scale))
    .padding(Padding::new(0.0).left(11.0 * scale).right(11.0 * scale))
    .boxed()
}

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
            hovered_region: pane_grid::Highlight {
                background: cp.hover.into(),
                border: Border { color: cp.focus, width: 1.0, radius: 0.0.into() },
            },
       
            hovered_split: pane_grid::Line { color: cp.focus, width: 1.0 },
            picked_split: pane_grid::Line { color: cp.focus, width: 1.0 },
        });


    container(grid)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style { background: Some(cp.rule.into()), ..Default::default() })
        .boxed()
}

pub fn strip<'a>(word: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    text(word.to_uppercase()).font(MONO).size(STRIP * scale).color(cp.muted).boxed()
}

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
          
            .push(glyph(
                if maximized { "\u{2199}" } else { "\u{2197}" },
                GLYPH_TEXT,
                wrap(GridMsg::Maximize(pane)),
                cp,
                scale,
            ));
    }
    controls = controls.push(glyph("\u{2715}", CLOSE_GLYPH, wrap(GridMsg::Close(pane)), cp, scale));

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

fn glyph<'a>(mark: &'static str, size: f32, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(
        text(mark)
            .font(MONO)
            .size(size * scale)
            .line_height(iced::widget::text::LineHeight::Absolute(Pixels(GLYPH_LINE * scale))),
    )
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

pub fn bare(_: &iced::Theme, _: button::Status) -> button::Style {
    button::Style {
        background: None,
        border: Border::default(),
        text_color: Color::TRANSPARENT,
        shadow: Shadow::default(),
        snap: false,
    }
}


pub fn period_links<'a>(current: ChartPeriod, wrap: fn(GridMsg) -> Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let link = |label: &str, period: ChartPeriod| -> Element<'a, Message> {
        let active = current == period;
        let word = label.to_uppercase();
        let under = container(Space::new())
            .width(Length::Fixed(word.chars().count() as f32 * PILL * scale * MONO_ADVANCE))
            .height(Length::Fixed(1.0))
            .style(move |_| container::Style { background: active.then_some(cp.text.into()), ..Default::default() });
        button(column![text(word).font(MONO).size(PILL * scale), under].spacing(2.0 * scale))
            .on_press(wrap(GridMsg::Period(period)))
            .padding(Padding::new(0.0).top(2.0 * scale).left(4.0 * scale).right(4.0 * scale))
            .style(move |_, status| button::Style {
                background: None,
                border: Border::default(),
                text_color: if active {
                    cp.text
                } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    cp.dim
                } else {
                    cp.muted
                },
                shadow: Shadow::default(),
                snap: false,
            })
            .boxed()
    };
    row![link("1h", ChartPeriod::OneHour), link("1d", ChartPeriod::OneDay)].spacing(4.0 * scale).boxed()
}


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

pub fn group_rule<'a>(cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    column![
        Space::new().height(7.0 * scale),
        compact::hairline(cp.rule),
        Space::new().height(8.0 * scale),
    ]
    .width(Length::Fill)
    .boxed()
}


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

pub fn label<'a>(word: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(text(word.to_uppercase()).font(MONO).size(STRIP * scale).color(cp.muted))
        .padding(Padding::new(0.0).bottom(4.0 * scale))
        .boxed()
}


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

pub fn pane_button<'a>(word: &str, enabled: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    marked_button(word, enabled, msg, cp.accent, cp, scale)
}

pub fn marked_button<'a>(word: &str, enabled: bool, msg: Message, mark: Color, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let face = container(text(word.to_string()).size(BUTTON * scale).width(Length::Fill).align_x(Alignment::Center))
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(5.0 * scale).bottom(5.0 * scale));
    button(crate::ui::components::compact::cornered(face.boxed(), mark, 6.0 * scale, scale))
        .on_press_maybe(enabled.then_some(msg))
        .width(Length::Fill)
        .padding(Padding::ZERO)
        .style(move |_, status| crate::ui::components::compact::button_style(cp, 6.0 * scale, enabled, status))
        .boxed()
}


pub fn quiet_button<'a>(word: &str, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    outline_button(word, true, cp.dim, cp.hover, msg, cp, scale)
}

pub fn outline<'a>(word: &str, enabled: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    outline_button(word, enabled, cp.text, cp.hover, msg, cp, scale)
}

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

pub fn button_pair<'a>(left: Element<'a, Message>, right: Element<'a, Message>, scale: f32) -> Element<'a, Message> {
    row![left, right].spacing(6.0 * scale).width(Length::Fill).boxed()
}


pub fn scroller<'a>(content: Element<'a, Message>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    scroll_at(Length::Fill, content, cp, scale)
}


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


pub fn fill_or_scroll<'a>(
    min: f32,
    body: impl Fn() -> Element<'a, Message> + 'a,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    fill_or_scroll_by(min, move |_| body(), cp, scale)
}

pub fn fill_or_scroll_by<'a>(
    min: f32,
    body: impl Fn(Size) -> Element<'a, Message> + 'a,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    fill_or_scroll_fit(move |_| min, body, cp, scale)
}

pub fn fill_or_scroll_fit<'a>(
    min: impl Fn(Size) -> f32 + 'a,
    body: impl Fn(Size) -> Element<'a, Message> + 'a,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let floor = min(size);
        if size.height >= floor * scale {
            body(size)
        } else {
            scroller(
                container(body(size)).width(Length::Fill).height(Length::Fixed(floor * scale)).boxed(),
                cp,
                scale,
            )
        }
    })
    .boxed()
}

const CHART_MIN_H: f32 = 110.0;
const RECEIVE_CODE_MIN: f32 = 150.0;
const RECEIVE_SLOT: f32 = 2.0 * COPY_GLYPH + RECEIVE_GAP;
const RECEIVE_GAP: f32 = 6.0;
const RECEIVE_FIELD_COLS: usize = 18;



pub fn phrase_field<'a>(
    field:    SecureField,
    seed:     &'a SecureString,
    revealed: bool,
    line:     &'a PhraseLine,
    inner:    f32,
    cp:       &'static CompactPalette,
    scale:    f32,
) -> Element<'a, Message> {
    let entry = Entry { field, seed, revealed, line, width: inner * scale, eye: true, autofocus: false };
    phrase_entry::view(entry, &phrase_entry::pane(cp, scale), cp, scale)
}


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
    column![
        rule_label("sign", cp, scale),
        sign_fields(fields, secret_buf, phrase_buf, bip39_buf, armed, inner, cp, scale),
        Space::new().height(10.0 * scale),
        buttons,
    ]
    .width(Length::Fill)
    .boxed()
}

#[allow(clippy::too_many_arguments)]
pub fn sign_fields<'a>(
    fields: &SignFields<'a>,
    secret_buf: &'a SecureString,
    phrase_buf: &'a SecureString,
    bip39_buf: &'a SecureString,
    armed: bool,
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
    
        EnableInputMode::Seed => phrase_field(
            fields.phrase,
            phrase_buf,
            fields.phrase_reveal,
            fields.line,
            inner,
            cp,
            scale,
        ),
    };

    column![
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
    ]
    .width(Length::Fill)
    .boxed()
}

pub fn primary<'a>(word: &str, live: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    pane_button(word, live, msg, cp, scale)
}


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
                container(text("Waiting for data\u{2026}").size(ROW_KEY * scale).color(cp.muted))
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


struct PaneChart {
    series: Vec<(u64, f32)>,
    period: ChartPeriod,
    fmt: fn(f32) -> String,
    ink: ChartInk,
}

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


const COPY_GLYPH: f32 = 12.0;


pub fn hero<'a>(figure: String, unit: String, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    hero_ink(figure, unit, cp.text, cp, scale)
}

pub fn hero_ink<'a>(figure: String, unit: String, ink: Color, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let figure = text(figure).font(LIGHT).size(HERO * scale).color(ink);
    if unit.is_empty() {
        return figure.boxed();
    }
    row![
        figure,
        Space::new().width(7.0 * scale),
        container(text(unit).font(MONO).size(HERO_UNIT * scale).color(cp.muted))
            .padding(Padding::new(0.0).bottom((HERO * 0.17).round() * scale)),
    ]
    .align_y(Alignment::End)
    .boxed()
}

pub fn swap_glyph<'a>(text_size: f32, color: Color) -> Element<'a, Message> {
    let s = text_size * 14.0 / 13.0;
    svg(crate::utils::icons::SWAP.clone())
        .width(Length::Fixed(s))
        .height(Length::Fixed(s))
        .style(move |_, _| svg::Style { color: Some(color) })
        .boxed()
}

pub fn copy_glyph<'a>(copied: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if copied {
        return done_mark(cp, scale);
    }
    glyph_button(crate::utils::icons::COPY.clone(), msg, cp, scale)
}

fn done_mark<'a>(cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let g = COPY_GLYPH * scale;
    container(text("\u{2713}").size(ROW_KEY * scale).color(cp.green))
        .width(Length::Fixed(g))
        .height(Length::Fixed(g))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .boxed()
}


pub fn elide_addr(a: &str) -> String {
    cut_addr(a, 10, 8)
}


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

fn cut_addr(a: &str, head: usize, tail: usize) -> String {
    let n = a.chars().count();
    if n <= head + tail + 1 {
        return a.to_string();
    }
    let h: String = a.chars().take(head).collect();
    let t: String = a.chars().skip(n - tail).collect();
    format!("{h}\u{2026}{t}")
}


pub fn tag_line<'a>(tag: Option<u32>, pane: TagPane, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    pane_link(&tag_word(tag), Message::TagEdit(pane), cp, scale)
}


pub fn tag_word(tag: Option<u32>) -> String {
    match tag {
        None => "Set destination tag".to_string(),
        Some(t) => format!("Tag {t}"),
    }
}


pub fn pane_link<'a>(word: &str, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(
        text(format!("{} \u{203a}", word))
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


pub const RECEIVE_LINE_H: f32 = 21.0;
const LINE_BTN: f32 = 10.0;
pub const TAG_FIELD_H: f32 = 20.0;
const TAG_FIELD_PAD_H: f32 = 4.0;

#[allow(clippy::too_many_arguments)]
pub fn receive_pane<'a>(
    rows: impl Fn(f32) -> f32 + 'a,
    body: impl Fn(f32) -> Element<'a, Message> + 'a,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    fill_or_scroll_fit(move |size: Size| RECEIVE_CODE_MIN + rows(size.width), move |size: Size| body(size.width), cp, scale)
}

fn even_lines(s: &str, per_line: usize) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= per_line {
        return vec![s.to_string()];
    }
    let lines = chars.len().div_ceil(per_line.max(1));
    chars.chunks(chars.len().div_ceil(lines)).map(|c| c.iter().collect()).collect()
}

pub fn receive_code<'a>(payload: String, scale: f32) -> Element<'a, Message> {
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

    container(code)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::new(0.0).top(2.0 * scale).bottom(9.0 * scale))
        .boxed()
}

fn receive_lines(address: &str, width: f32, scale: f32) -> Vec<String> {
    let room = width - (2.0 * (RECEIVE_SLOT + RECEIVE_GAP) + RECEIVE_ADDR_MARGIN) * scale;
    let per_line = (room / (RECEIVE_ADDR * scale * MONO_ADVANCE)).floor().max(1.0) as usize;
    even_lines(address, per_line)
}

pub fn receive_rows_h(address: &str, width: f32, lines_under: usize, scale: f32) -> f32 {
    let lines = receive_lines(address, width, scale).len() as f32;
    let view = (lines * RECEIVE_ADDR * 1.3).max(COPY_GLYPH) + RECEIVE_GAP + lines_under as f32 * RECEIVE_LINE_H;
    view.max(TAG_FIELD_H + RECEIVE_GAP + RECEIVE_LINE_H)
}

pub fn receive_hang<'a>(content: Element<'a, Message>, tools: Element<'a, Message>, scale: f32) -> Element<'a, Message> {
    let slot = RECEIVE_SLOT * scale;
    let gap = RECEIVE_GAP * scale;
    row![
        Space::new().width(Length::Fill),
        Space::new().width(Length::Fixed(slot + gap)),
        content,
        Space::new().width(Length::Fixed(gap)),
        container(tools).width(Length::Fixed(slot)),
        Space::new().width(Length::Fill),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .boxed()
}

pub fn receive_tools<'a>(
    flash: Option<ReceiveFlash>,
    on_copy: Message,
    on_download: Option<Message>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    if flash == Some(ReceiveFlash::SaveFailed) {
        return text("Couldn't save").size(ROW_KEY * scale).color(cp.red).wrapping(Wrapping::None).boxed();
    }
    let copied = matches!(flash, Some(ReceiveFlash::Copied | ReceiveFlash::TagCopied));
    let mut tools = row![copy_glyph(copied, on_copy, cp, scale)].align_y(Alignment::Center);
    if let Some(msg) = on_download {
        let save = if flash == Some(ReceiveFlash::Saved) {
            done_mark(cp, scale)
        } else {
            glyph_button(crate::utils::icons::DOWNLOAD.clone(), msg, cp, scale)
        };
        tools = tools.push(Space::new().width(Length::Fixed(RECEIVE_GAP * scale)).boxed()).push(save);
    }
    tools.boxed()
}

pub fn receive_links<'a>(links: Vec<(&'static str, Message)>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let mut words = row![].align_y(Alignment::Center);
    for (i, (word, msg)) in links.into_iter().enumerate() {
        if i > 0 {
            words = words.push(rule_v(cp, scale));
        }
        words = words.push(receive_word(format!("{word} \u{203a}"), Some(msg), cp.dim, cp, scale));
    }
    container(words)
        .width(Length::Fill)
        .height(Length::Fixed(RECEIVE_LINE_H * scale))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .boxed()
}

#[allow(clippy::too_many_arguments)]
pub fn receive_editor<'a>(
    left: Element<'a, Message>,
    field: Element<'a, Message>,
    right: Element<'a, Message>,
    set: Option<(&'static str, Option<Message>)>,
    remove: Option<Message>,
    cancel: Message,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let gap = 12.0 * scale;
    let fields = row![
        container(left).width(Length::Fill).align_x(Alignment::End),
        Space::new().width(Length::Fixed(gap)),
        field,
        Space::new().width(Length::Fixed(gap)),
        container(right).width(Length::Fill).align_x(Alignment::Start),
    ]
    .align_y(Alignment::Center)
    .height(Length::Fixed(TAG_FIELD_H * scale));
    let cancel = receive_word("Cancel".to_string(), Some(cancel), cp.dim, cp, scale);
    let actions = match set {
        Some((word, msg)) => {
            let ink = if msg.is_some() { cp.text } else { cp.faint };
            let mut middle = row![rule_v(cp, scale)].align_y(Alignment::Center);
            if let Some(remove) = remove {
                middle = middle
                    .push(receive_word("Remove".to_string(), Some(remove), cp.dim, cp, scale))
                    .push(rule_v(cp, scale));
            }
            row![
                container(receive_word(word.to_string(), msg, ink, cp, scale)).width(Length::Fill).align_x(Alignment::End),
                middle,
                container(cancel).width(Length::Fill).align_x(Alignment::Start),
            ]
            .align_y(Alignment::Center)
            .boxed()
        }
        None => cancel,
    };
    column![
        fields,
        Space::new().height(Length::Fixed(RECEIVE_GAP * scale)),
        container(actions)
            .width(Length::Fill)
            .height(Length::Fixed(RECEIVE_LINE_H * scale))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    ]
    .width(Length::Fill)
    .boxed()
}

pub fn receive_field<'a>(
    field: PlainField,
    value: &'a str,
    placeholder: &str,
    red: bool,
    focus: Option<u32>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let s = RECEIVE_ADDR * scale;
    let advance = s * MONO_ADVANCE;
    let shown = if value.is_empty() { placeholder } else { value };
    let cols = shown.chars().count() + 1;
    let w = RECEIVE_FIELD_COLS as f32 * advance + 2.0 * TAG_FIELD_PAD_H * scale;
    let input = plain_field_grid_ink_requested(
        field,
        value,
        cols,
        placeholder,
        None,
        advance,
        s * 1.6,
        s,
        MONO,
        if red { cp.red } else { cp.text },
        cp.text,
        cp.faint,
        fonts::SANS,
        focus,
    );
    column![
        container(container(input).width(Length::Fixed(cols as f32 * advance)))
            .width(Length::Fixed(w))
            .height(Length::Fixed(TAG_FIELD_H * scale - 1.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
        container(Space::new())
            .width(Length::Fixed(w))
            .height(Length::Fixed(1.0))
            .style(move |_| container::Style { background: Some(cp.focus.into()), ..Default::default() }),
    ]
    .boxed()
}

pub fn receive_chip<'a>(code: &str, open: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let g = 8.0 * scale;
    let turn = if open { -std::f32::consts::FRAC_PI_2 } else { 0.0 };
    let chevron = svg(crate::utils::icons::CHEVRON_RIGHT.clone())
        .width(Length::Fixed(g))
        .height(Length::Fixed(g))
        .rotation(iced::Radians(turn))
        .style(move |_, _| svg::Style { color: Some(if open { cp.green } else { cp.faint }) });
    button(
        row![text(code.to_string()).font(MONO).size(STRIP * scale).color(cp.dim), Space::new().width(Length::Fixed(4.0 * scale)), chevron]
            .align_y(Alignment::Center),
    )
    .on_press(msg)
    .padding(Padding::new(2.0 * scale).left(6.0 * scale).right(4.0 * scale))
    .style(bare)
    .boxed()
}

pub fn receive_request<'a>(
    figure: String,
    unit: String,
    issuer: Option<(String, String)>,
    msg: Message,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let mut lines = column![hero(figure, unit, cp, scale)].align_x(Alignment::Center);
    if let Some((name, short)) = issuer {
        lines = lines
            .push(Space::new().height(Length::Fixed(3.0 * scale)).boxed())
            .push(compact::mono_runs(vec![(name, cp.dim), (format!("  \u{b7}  {short}"), cp.muted)], HINT * scale));
    }
    container(button(lines).on_press(msg).padding(Padding::new(2.0 * scale)).style(bare))
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .padding(Padding::new(0.0).top(2.0 * scale))
        .boxed()
}

pub fn receive_fiat<'a>(draft: &str, rate: f64, ccy: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let (word, ink) = match draft.trim().parse::<f64>() {
        _ if rate <= 0.0 => (NA.to_string(), cp.faint),
        Ok(v) => (format!("\u{2248} {} {ccy}", crate::utils::money(v * rate)), cp.muted),
        Err(_) => (format!("\u{2248} 0.00 {ccy}"), cp.faint),
    };
    text(word).font(MONO).size(HINT * scale).color(ink).wrapping(Wrapping::None).boxed()
}

fn receive_word<'a>(word: String, msg: Option<Message>, ink: Color, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let live = msg.is_some();
    button(text(word).font(fonts::SANS).size(ROW_KEY * scale).wrapping(Wrapping::None))
        .on_press_maybe(msg)
        .padding(Padding::new(2.0 * scale).left(8.0 * scale).right(8.0 * scale))
        .style(move |_, status| {
            let hot = live && matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: None,
                border: Border::default(),
                text_color: if hot { cp.text } else { ink },
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}

fn rule_v<'a>(cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(Space::new())
        .width(Length::Fixed(1.0))
        .height(Length::Fixed(9.0 * scale))
        .style(move |_| container::Style { background: Some(cp.border.into()), ..Default::default() })
        .boxed()
}

fn glyph_button<'a>(icon: svg::Handle, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let g = COPY_GLYPH * scale;
    button(
        svg(icon)
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

pub fn receive_address<'a>(
    address: &str,
    tail: usize,
    width: f32,
    tools: Element<'a, Message>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let tail_from = address.chars().count().saturating_sub(tail);
    let mut shown: Column<Element<'a, Message>> = column![].align_x(Alignment::Center);
    let mut at = 0;
    for line in receive_lines(address, width, scale) {
        let len = line.chars().count();
        let cut = tail_from.saturating_sub(at).min(len);
        let mut spans: Vec<Span<'static, ()>> =
            vec![span(line.chars().take(cut).collect::<String>()).size(RECEIVE_ADDR * scale).font(MONO).color(cp.dim)];
        if cut < len {
            spans.push(
                span(line.chars().skip(cut).collect::<String>())
                    .size(RECEIVE_ADDR * scale)
                    .font(MONO)
                    .color(cp.dim)
                    .underline(true),
            );
        }
        shown = shown.push(rich_text(spans).wrapping(Wrapping::None).boxed());
        at += len;
    }
    receive_hang(shown.boxed(), tools, scale)
}


pub fn line_button<'a>(
    word: &'static str,
    size: f32,
    ink: Color,
    hot_ink: Color,
    msg: Option<Message>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    line_glyph(text(word).font(MONO).size(size * scale), ink, hot_ink, msg, cp, scale)
}

fn line_glyph<'a>(
    glyph: iced::widget::Text<'a>,
    ink: Color,
    hot_ink: Color,
    msg: Option<Message>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    button(glyph)
        .on_press_maybe(msg)
        .padding(Padding::new(1.0 * scale).left(4.0 * scale).right(4.0 * scale))
        .style(move |_, status| {
            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if hot { cp.hover } else { Color::TRANSPARENT }.into()),
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (4.0 * scale).into() },
                text_color: if hot { hot_ink } else { ink },
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}

pub fn line_ok<'a>(msg: Option<Message>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let ink = if msg.is_some() { cp.green } else { cp.faint };
    line_glyph(text("\u{2713}").size(LINE_BTN * scale), ink, ink, msg, cp, scale)
}

pub fn line_no<'a>(msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    line_button("\u{2715}", LINE_BTN, cp.muted, cp.text, Some(msg), cp, scale)
}

pub fn line_label<'a>(word: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    text(word.to_uppercase()).font(MONO).size(STRIP * scale).color(cp.muted).boxed()
}

pub fn line_value<'a>(word: String, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(text(word).font(MONO).size(RECEIVE_ADDR * scale))
        .on_press(msg)
        .padding(Padding::new(1.0 * scale).left(TAG_FIELD_PAD_H * scale).right(TAG_FIELD_PAD_H * scale))
        .style(move |_, _| button::Style {
            background: None,
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (4.0 * scale).into() },
            text_color: cp.text,
            shadow: Shadow::default(),
            snap: false,
        })
        .boxed()
}


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


pub fn well_pane<'a>(wrap: fn(GridMsg) -> Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    well(plus(EMPTY_PLUS, wrap, cp, scale), 6.0, cp, scale)
}

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


fn menu_layer<'a>(host: Host<'a>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    mouse_area(
        container(opaque(menu_panel(host, cp, scale)))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::Start)
            .padding(Padding::new(0.0).top((panes::BAR_H + 4.0) * scale + 1.0).right(11.0 * scale)),
    )
  
    .on_release((host.wrap)(GridMsg::MenuDismissed))
    .boxed()
}


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
 
    col = col
        .push(menu_sep(cp, scale))
     
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

fn menu_head<'a>(word: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(text(word.to_uppercase()).font(MONO).size(STRIP * scale).color(cp.muted))
        .padding(Padding::new(0.0).top(8.0 * scale).bottom(5.0 * scale).left(MENU_PAD_H * scale).right(MENU_PAD_H * scale))
        .boxed()
}


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
    use super::{elide_addr, even_lines, fit_addr};

    const SEGWIT: &str = "bc1qyfxmsjq0pwk4kg3x5e7cqk8rjfhjrm2xamh06f67";

    #[test]
    fn a_receive_address_that_overflows_splits_into_even_lines() {
        assert_eq!(even_lines(SEGWIT, 60), [SEGWIT]);
        let two = even_lines(SEGWIT, 30);
        assert_eq!(two.iter().map(|l| l.chars().count()).collect::<Vec<_>>(), [22, 22]);
        assert_eq!(two.concat(), SEGWIT);
        let taproot = "bc1pmzfrwwndsqmk5yh69yjr5lfgfg4ev8c0tsc06e2ahcm7cq0v6nrsgvdvgd";
        assert_eq!(even_lines(taproot, 40).iter().map(|l| l.chars().count()).collect::<Vec<_>>(), [31, 31]);
    }

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
