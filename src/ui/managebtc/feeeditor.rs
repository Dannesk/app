use iced::Widget as _;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text, Row, Space};
use iced::{Alignment, Color, Element, Length, Padding};

use crate::controller::message::{Message, PlainField};
use crate::ui::components::compact;
use crate::ui::components::grid::{NA, ROW_KEY};
use crate::ui::components::send_screen as screen;
use crate::ui::managebtc::btcdashboard::{fee_dot, sats_cost};
use crate::ui::managebtc::btcsend::Line;
use crate::ui::managexrp::xrpdashboard;
use crate::utils::fonts::MONO;
use crate::utils::theme::CompactPalette;

const LADDER_W: f32 = 300.0;
const COST_GAP: f32 = 7.0;
const LINE_GAP: f32 = 5.0;
const LADDER_GAP: f32 = 22.0;
const LADDER_PAD: f32 = 9.0;
const TIER_PAD_V: f32 = 4.0;
const TIER_GAP: f32 = 3.0;
const TIER_RATE: f32 = 9.0;
const FIGURE_H: f32 = 24.0 * 1.4;

pub(crate) const MIN_H: f32 = screen::UNIT * 1.3
    + 8.0
    + FIGURE_H
    + COST_GAP
    + ROW_KEY * 1.3
    + LINE_GAP
    + ROW_KEY * 1.3
    + LADDER_GAP
    + 1.0
    + LADDER_PAD
    + 2.0 * TIER_PAD_V
    + ROW_KEY * 1.3
    + TIER_GAP
    + TIER_RATE * 1.3;

pub(crate) struct Tier {
    pub name: &'static str,
    pub rate: Option<String>,
    pub dot: Color,
    pub lit: bool,
    pub press: Option<Message>,
}

pub(crate) struct Parts<'a> {
    pub eyebrow: &'static str,
    pub field: PlainField,
    pub text: &'a str,
    pub focus: Option<u32>,
    pub cost: Option<u64>,
    pub line: Line,
    pub tiers: [Tier; 4],
    pub on_submit: Option<Message>,
    pub price: f64,
    pub ccy: &'static str,
}

pub(crate) fn view<'a>(p: Parts<'a>, room: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let figure = xrpdashboard::figure_field(p.field, p.text, "0.00", "sat/vB", room, false, p.on_submit, p.focus, cp, scale);
    let cost: Element<'a, Message> = match p.cost {
        Some(sats) => compact::mono_runs(sats_cost(Some(sats), p.price, p.ccy, cp), ROW_KEY * scale),
        None => Space::new().boxed(),
    };
    column![
        screen::unit(p.eyebrow, cp, scale),
        Space::new().height(8.0 * scale),
        figure,
        Space::new().height(COST_GAP * scale),
        container(cost).height(Length::Fixed(ROW_KEY * 1.3 * scale)).align_y(Alignment::Center),
        Space::new().height(LINE_GAP * scale),
        line(p.line, cp, scale),
        Space::new().height(LADDER_GAP * scale),
        ladder(p.tiers, cp, scale),
    ]
    .align_x(Alignment::Center)
    .width(Length::Fill)
    .boxed()
}

fn line<'a>(line: Line, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let (dot, words, ink) = match line {
        Line::Prompt(p) => (None, p, cp.dim),
        Line::Tier(v) => (Some(v.colour(cp)), v.word().to_string(), cp.dim),
        Line::Custom(Some(v)) => (Some(v.colour(cp)), format!("custom \u{b7} {}", v.word()), cp.dim),
        Line::Custom(None) => (None, "custom".to_string(), cp.dim),
        Line::Under(w) => (Some(cp.red), w, cp.red),
    };
    let mut r: Row<Element<'a, Message>> = row![].spacing(5.0 * scale).align_y(Alignment::Center);
    if let Some(d) = dot {
        r = r.push(fee_dot(d, scale));
    }
    container(r.push(text(words).size(ROW_KEY * scale).color(ink).wrapping(Wrapping::None).boxed()))
        .height(Length::Fixed(ROW_KEY * 1.3 * scale))
        .align_y(Alignment::Center)
        .boxed()
}

fn ladder<'a>(tiers: [Tier; 4], cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let mut cells: Row<Element<'a, Message>> = row![].width(Length::Fill);
    for t in tiers {
        let live = t.press.is_some();
        let (name_ink, rate_ink) = match (t.lit, live) {
            (true, _) => (cp.text, cp.dim),
            (false, true) => (cp.muted, cp.faint),
            (false, false) => (cp.faint, cp.faint),
        };
        let mut name: Row<Element<'a, Message>> = row![].spacing(5.0 * scale).align_y(Alignment::Center);
        if t.lit {
            name = name.push(fee_dot(t.dot, scale));
        }
        name = name.push(text(t.name).size(ROW_KEY * scale).color(name_ink).wrapping(Wrapping::None).boxed());
        let cell = column![
            name,
            Space::new().height(TIER_GAP * scale),
            text(t.rate.unwrap_or_else(|| NA.to_string())).font(MONO).size(TIER_RATE * scale).color(rate_ink),
        ]
        .align_x(Alignment::Center);
        cells = cells.push(
            button(container(cell).width(Length::Fill).align_x(Alignment::Center))
                .on_press_maybe(t.press)
                .width(Length::FillPortion(1))
                .padding(Padding::new(TIER_PAD_V * scale).left(0.0).right(0.0))
                .style(move |_, status| screen::row_style(live, status, screen::SMALL_RADIUS * scale, cp))
                .boxed(),
        );
    }
    container(column![compact::hairline(cp.rule), Space::new().height(LADDER_PAD * scale), cells].width(Length::Fill))
        .width(Length::Fill.max(LADDER_W * scale))
        .boxed()
}
