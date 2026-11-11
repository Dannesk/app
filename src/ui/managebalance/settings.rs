use iced::Widget as _;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{button, column, container, row, scrollable, text, Column, Id, Row, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Shadow};

use crate::channel::{Service, ServiceState, CHANNEL};
use crate::controller::app_state::{AppState, BaseCcy, SettingsView, Theme};
use crate::controller::message::Message;
use crate::ui::components::compact::{self as ck, hairline};
use crate::ui::components::grid;
use crate::ui::managexrp::tokens::thumb_only;
use crate::utils::fonts::MONO;
use crate::utils::icons;
use crate::utils::theme::{self, CompactPalette, ThemeVariant};

pub const BODY: Id = Id::new("settings-body");

const CARD_W: f32 = 360.0;
const CARD_H: f32 = 440.0;
const CARD_RADIUS: f32 = 10.0;
const CARD_TOP: f32 = 13.0;
const EDGE: f32 = 1.0;

const HEAD_SIDE: f32 = 16.0;
const HEAD_BOTTOM: f32 = 9.0;
const HEAD_GAP: f32 = 8.0;
const HEAD_ROW: f32 = 20.0;
const TITLE: f32 = 12.5;
const CLOSE: f32 = 10.0;
const BACK: f32 = 20.0;
const BACK_GLYPH: f32 = 11.0;

const BODY_TOP: f32 = 12.0;
const BODY_RIGHT: f32 = 13.0;
const BODY_BOTTOM: f32 = 15.0;
const BODY_LEFT: f32 = 16.0;
const LIST_BLEED: f32 = 6.0;
const SCROLLER_W: f32 = 3.0;
const SCROLLER_INSET: f32 = 4.0;

const GROUP_GAP: f32 = 18.0;
const EYEBROW: f32 = 9.0;
const EYEBROW_BOTTOM: f32 = 2.0;

const ROW_H: f32 = 42.0;
const ROW_H_SUB: f32 = 50.0;
const ROW_GAP: f32 = 12.0;
const NAME: f32 = 13.0;
const SUB: f32 = 11.0;
const NAME_GAP: f32 = 3.0;
const VALUE: f32 = 12.5;
const CODE: f32 = 12.0;
const CHEVRON: f32 = 11.0;
const VALUE_GAP: f32 = 8.0;

const STATUS: f32 = 9.5;
const STATUS_DOT: f32 = 4.0;
const STATUS_GAP: f32 = 6.0;

const BAR_W: f32 = 3.0;
const BAR_GAP: f32 = 2.0;
const BAR_MIN: f32 = 3.0;
const BAR_STEP: f32 = 2.0;
const BAR_RADIUS: f32 = 1.0;

const SWITCH_W: f32 = 32.0;
const SWITCH_H: f32 = 18.0;
const KNOB_OFF: f32 = 9.0;
const KNOB_ON: f32 = 12.0;
const KNOB_OFF_INSET: f32 = 4.0;
const KNOB_ON_INSET: f32 = 2.0;

const WIPE: f32 = 12.0;
const WIPE_PAD_V: f32 = 6.0;
const WIPE_PAD_H: f32 = 14.0;
const WIPE_RADIUS: f32 = 6.0;
const CONFIRM_GAP: f32 = 4.5;

const OPTION_GAP: f32 = 2.0;
const OPTION_PAD: f32 = 8.0;
const OPTION_INNER_GAP: f32 = 10.0;
const OPTION_RADIUS: f32 = 7.0;
const OPTION_NAME: f32 = 12.5;
const OPTION_CODE: f32 = 12.0;
const CODE_SLOT: f32 = 36.0;
const TILE: f32 = 22.0;
const TILE_RADIUS: f32 = 5.0;
const TILE_TEXT: f32 = 10.0;

const SERVICES: &[(Service, &str, &str)] = &[
    (Service::PriceFeeds, "Price feeds", "Spot market prices and chart data"),
    (Service::XrpNode, "XRPL node", "Order book, market depth and trading engine"),
    (Service::BitcoinNode, "Bitcoin node", "Mempool, index, wallet updates and history"),
    (Service::RelayServer, "Relay server", "XRP live updates and wallet subscriptions"),
];


pub fn card(state: &AppState) -> Element<'_, Message> {
    let p = theme::compact(&state.theme);
    let s = state.scale();
    let view = state.settings_view;

    let (title, body, bleed) = match view {
        SettingsView::Main => ("Settings", main_view(state, p, s), 0.0),
        SettingsView::Theme => ("Theme", theme_list(state, p, s), LIST_BLEED),
        SettingsView::Currency => ("Display currency", currency_list(state, p, s), LIST_BLEED),
    };

    let padded = container(body).width(Length::Fill).padding(Padding {
        top: BODY_TOP * s,
        right: (BODY_RIGHT - bleed) * s,
        bottom: BODY_BOTTOM * s,
        left: (BODY_LEFT - bleed) * s,
    });

    let mut scroll = scrollable(padded)
        .id(BODY)
        .direction(Direction::Vertical(
            Scrollbar::new()
                .width(SCROLLER_W * s)
                .scroller_width(SCROLLER_W * s)
                .margin(0.0)
                .padding(SCROLLER_INSET * s),
        ))
        .style(move |theme, status| scrollable::Style {
            container: container::Style::default(),
            vertical_rail: thumb_only(p.border),
            horizontal_rail: thumb_only(p.border),
            gap: None,
            ..scrollable::default(theme, status)
        })
        .width(Length::Fill)
        .height(Length::Fill);
    if view == SettingsView::Main {
        let stored = state.settings_scroll_y;
        scroll = scroll.on_scroll(move |sc| {
            let y = sc.destination().absolute_offset().y;
            (y != stored).then_some(Message::SettingsScrolled(y))
        });
    }

    container(column![header(view, title, p, s), scroll])
        .width(Length::Fixed(CARD_W * s))
        .height(Length::Fixed(CARD_H * s))
        .padding(Padding { top: CARD_TOP * s + EDGE, right: EDGE, bottom: EDGE, left: EDGE })
        .style(move |_| container::Style {
            background: Some(p.panel.into()),
            border: Border { color: p.border, width: EDGE, radius: (CARD_RADIUS * s).into() },
            ..Default::default()
        })
        .boxed()
}

fn header<'a>(view: SettingsView, title: &'a str, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let mut bar: Row<Element<'a, Message>> = row![]
        .spacing(HEAD_GAP * s)
        .height(Length::Fixed(HEAD_ROW * s))
        .align_y(Alignment::Center);
    if view != SettingsView::Main {
        bar = bar.push(ck::round_button(
            icons::CHEVRON_LEFT.clone(),
            Message::SettingsShow(SettingsView::Main),
            BACK,
            BACK_GLYPH,
            p,
            s,
        ));
    }
    bar = bar
        .push(text(title).size(TITLE * s).color(p.text).boxed())
        .push(Space::new().width(Length::Fill).boxed())
        .push(close(p, s));

    column![
        container(bar).width(Length::Fill).padding(Padding::new(0.0).bottom(HEAD_BOTTOM * s)),
        hairline(p.rule),
    ]
    .width(Length::Fill)
    .padding(Padding::new(0.0).left(HEAD_SIDE * s).right(HEAD_SIDE * s))
    .boxed()
}

fn close<'a>(p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    button(
        container(text("\u{2715}").font(MONO).size(CLOSE * s))
            .width(Length::Fixed(HEAD_ROW * s))
            .height(Length::Fixed(HEAD_ROW * s))
            .align_x(Alignment::End)
            .align_y(Alignment::Center),
    )
    .padding(0)
    .on_press(Message::CloseSettings)
    .style(move |_, status| button::Style {
        background: None,
        border: Border::default(),
        text_color: if matches!(status, button::Status::Hovered | button::Status::Pressed) { p.text } else { p.muted },
        shadow: Shadow::default(),
        snap: false,
    })
    .boxed()
}


fn main_view<'a>(state: &'a AppState, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    column![
        appearance(state, p, s),
        security(state, p, s),
        services(p, s),
        data(state.prefs_erase_confirm, p, s),
    ]
    .spacing(GROUP_GAP * s)
    .width(Length::Fill)
    .boxed()
}

fn appearance<'a>(state: &'a AppState, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let theme_name = theme_label(Theme::of(&state.theme));
    let code = state.base_currency.code();
    group(
        eyebrow("appearance", None, p, s),
        vec![
            press_row(
                line("Theme", None, value(text(theme_name).size(VALUE * s).color(p.dim).boxed(), p, s), p, s),
                Message::SettingsShow(SettingsView::Theme),
                p,
            ),
            press_row(
                line("Hide balances", None, switch(state.hide_balance, p, s), p, s),
                Message::ToggleHideBalance,
                p,
            ),
            press_row(
                line("Display currency", None, value(text(code).font(MONO).size(CODE * s).color(p.dim).boxed(), p, s), p, s),
                Message::SettingsShow(SettingsView::Currency),
                p,
            ),
        ],
    )
}

fn security<'a>(state: &'a AppState, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let on = state.gate_enabled;
    let hint = if on { "PIN activated" } else { "PIN deactivated" };
    group(
        eyebrow("security", None, p, s),
        vec![press_row(
            line("Six-digit PIN", Some(subtitle(hint, p, s)), switch(on, p, s), p, s),
            Message::GateEnabledToggled(!on),
            p,
        )],
    )
}

fn services<'a>(p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let states: Vec<ServiceState> = SERVICES.iter().map(|(sv, _, _)| CHANNEL.service_state(*sv)).collect();
    let bars = signal_bars(&states, p, s);
    let rows = SERVICES
        .iter()
        .zip(states)
        .map(|((_, name, desc), st)| ruled(line(name, Some(subtitle(desc, p, s)), status(st, p, s), p, s), p))
        .collect();

    group(eyebrow("services", Some(bars), p, s), rows)
}

fn signal_bars<'a>(states: &[ServiceState], p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let live = states.iter().filter(|st| **st != ServiceState::Down).count();
    let (lit, colour) = if states.iter().all(|st| *st == ServiceState::Up) {
        (states.len(), p.green)
    } else if live == 0 {
        (1, p.red)
    } else {
        (live, p.amber)
    };

    let mut bars: Row<Element<'a, Message>> = row![].spacing(BAR_GAP * s).align_y(Alignment::End);
    for i in 0..states.len() {
        let ink = if i < lit { colour } else { p.border };
        bars = bars.push(
            container(Space::new())
                .width(Length::Fixed(BAR_W * s))
                .height(Length::Fixed((BAR_MIN + BAR_STEP * i as f32) * s))
                .style(move |_| container::Style {
                    background: Some(ink.into()),
                    border: Border { radius: (BAR_RADIUS * s).into(), ..Default::default() },
                    ..Default::default()
                })
                .boxed(),
        );
    }
    bars.boxed()
}

fn data<'a>(armed: bool, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let has_wallet = CHANNEL.wallet_balance_rx.borrow().1.is_some()
        || CHANNEL.bitcoin_wallet_rx.borrow().1.is_some();

    let content = if !has_wallet {
        line("Wipe all data", Some(subtitle("No wallet data on this device", p, s)), wipe_button(false, p, s), p, s)
    } else if armed {
        line("Wipe all data", Some(wipe_confirm(p, s)), Space::new().boxed(), p, s)
    } else {
        line("Wipe all data", Some(subtitle("Wallets, keys and your PIN", p, s)), wipe_button(true, p, s), p, s)
    };

    group(eyebrow("data", None, p, s), vec![ruled(content, p)])
}


fn theme_list<'a>(state: &'a AppState, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let active = Theme::of(&state.theme);
    let mut list: Column<Element<'a, Message>> = column![].spacing(OPTION_GAP * s).width(Length::Fill);
    for (t, label) in Theme::SELECTABLE.iter().copied() {
        list = list.push(option(tile(CompactPalette::of(variant(t)), p, s), label, t == active, Message::ThemeSet(t), p, s));
    }
    list.boxed()
}

fn currency_list<'a>(state: &'a AppState, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let selected = state.base_currency;
    let mut list: Column<Element<'a, Message>> = column![].spacing(OPTION_GAP * s).width(Length::Fill);
    for c in BaseCcy::ALL {
        let code = container(text(c.code()).font(MONO).size(OPTION_CODE * s).color(p.text))
            .width(Length::Fixed(CODE_SLOT * s))
            .boxed();
        list = list.push(option(code, currency_name(c), c == selected, Message::DisplayCcySet(c), p, s));
    }
    list.boxed()
}

fn option<'a>(
    lead: Element<'a, Message>,
    name: &'a str,
    selected: bool,
    msg: Message,
    p: &'static CompactPalette,
    s: f32,
) -> Element<'a, Message> {
    button(
        row![lead, text(name).size(OPTION_NAME * s).color(p.text)]
            .spacing(OPTION_INNER_GAP * s)
            .align_y(Alignment::Center),
    )
    .on_press(msg)
    .padding(OPTION_PAD * s)
    .width(Length::Fill)
    .style(move |_, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let fill = if selected {
            Some(p.pill)
        } else if hot {
            Some(p.hover)
        } else {
            None
        };
        button::Style {
            background: fill.map(Background::Color),
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (OPTION_RADIUS * s).into() },
            text_color: p.text,
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}

fn tile<'a>(of: &'static CompactPalette, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    container(text("Aa").size(TILE_TEXT * s).color(of.text))
        .width(Length::Fixed(TILE * s))
        .height(Length::Fixed(TILE * s))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(of.window.into()),
            border: Border { color: p.border, width: EDGE, radius: (TILE_RADIUS * s).into() },
            ..Default::default()
        })
        .boxed()
}


fn group<'a>(head: Element<'a, Message>, rows: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let list: Column<Element<'a, Message>> = column![].width(Length::Fill);
    list.push(head).extend(rows).boxed()
}

fn eyebrow<'a>(
    label: &'a str,
    right: Option<Element<'a, Message>>,
    p: &'static CompactPalette,
    s: f32,
) -> Element<'a, Message> {
    let mut head: Row<Element<'a, Message>> =
        row![text(label.to_uppercase()).font(MONO).size(EYEBROW * s).color(p.muted)].align_y(Alignment::Center);
    if let Some(r) = right {
        head = head.push(Space::new().width(Length::Fill).boxed()).push(r);
    }
    container(head)
        .width(Length::Fill)
        .padding(Padding::new(0.0).bottom(EYEBROW_BOTTOM * s))
        .boxed()
}

fn line<'a>(
    name: &'a str,
    sub: Option<Element<'a, Message>>,
    right: Element<'a, Message>,
    p: &'static CompactPalette,
    s: f32,
) -> Element<'a, Message> {
    let h = if sub.is_some() { ROW_H_SUB } else { ROW_H };
    let mut names: Column<Element<'a, Message>> = column![text(name)
        .size(NAME * s)
        .color(p.text)
        .wrapping(Wrapping::None)
        .ellipsis(Ellipsis::End)]
    .spacing(NAME_GAP * s)
    .width(Length::Fill);
    if let Some(sub) = sub {
        names = names.push(sub);
    }

    container(row![names, right].spacing(ROW_GAP * s).align_y(Alignment::Center))
        .width(Length::Fill)
        .height(Length::Fixed(h * s - EDGE))
        .align_y(Alignment::Center)
        .boxed()
}

fn subtitle<'a>(words: &'a str, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    text(words)
        .size(SUB * s)
        .color(p.dim)
        .wrapping(Wrapping::None)
        .ellipsis(Ellipsis::End)
        .boxed()
}

fn ruled<'a>(content: Element<'a, Message>, p: &'static CompactPalette) -> Element<'a, Message> {
    column![content, hairline(p.rule)].width(Length::Fill).boxed()
}

fn press_row<'a>(content: Element<'a, Message>, msg: Message, p: &'static CompactPalette) -> Element<'a, Message> {
    let pressable = button(content)
        .on_press(msg)
        .padding(0)
        .width(Length::Fill)
        .style(move |_, status| {
            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: hot.then_some(Background::Color(p.hover)),
                border: Border::default(),
                text_color: p.text,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed();
    ruled(pressable, p)
}

fn value<'a>(label: Element<'a, Message>, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    row![label, text("\u{203A}").font(MONO).size(CHEVRON * s).color(p.faint)]
        .spacing(VALUE_GAP * s)
        .align_y(Alignment::Center)
        .boxed()
}

fn switch<'a>(on: bool, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let (track, edge, ink, d, inset) = if on {
        (p.mint, p.mint_line, p.mint_ink, KNOB_ON, KNOB_ON_INSET)
    } else {
        (p.pill, p.border, p.muted, KNOB_OFF, KNOB_OFF_INSET)
    };
    let pad = EDGE + inset * s;

    let knob = container(Space::new().width(d * s).height(d * s)).style(move |_| container::Style {
        background: Some(ink.into()),
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (d * s / 2.0).into() },
        ..Default::default()
    });

    container(knob)
        .width(Length::Fixed(SWITCH_W * s))
        .height(Length::Fixed(SWITCH_H * s))
        .align_x(if on { Alignment::End } else { Alignment::Start })
        .align_y(Alignment::Center)
        .padding(if on { Padding::new(0.0).right(pad) } else { Padding::new(0.0).left(pad) })
        .style(move |_| container::Style {
            background: Some(track.into()),
            border: Border { color: edge, width: EDGE, radius: (SWITCH_H * s / 2.0).into() },
            ..Default::default()
        })
        .boxed()
}

fn status<'a>(st: ServiceState, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let (dot_c, ink, word) = match st {
        ServiceState::Up => (p.green, p.dim, "up"),
        ServiceState::Degraded => (p.amber, p.amber, "degraded"),
        ServiceState::Down => (p.faint, p.muted, "down"),
    };
    row![
        dot(STATUS_DOT * s, dot_c),
        text(word.to_uppercase()).font(MONO).size(STATUS * s).color(ink),
    ]
    .spacing(STATUS_GAP * s)
    .align_y(Alignment::Center)
    .boxed()
}

fn dot<'a>(size: f32, color: Color) -> Element<'a, Message> {
    container(Space::new().width(size).height(size))
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (size / 2.0).into() },
            ..Default::default()
        })
        .boxed()
}

fn wipe_button<'a>(enabled: bool, p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    let wash = Color { a: 0.10, ..p.red };
    button(text("Wipe").size(WIPE * s).line_height(1.0))
        .on_press_maybe(enabled.then_some(Message::PrefsEraseAsked))
        .padding(
            Padding::new(WIPE_PAD_V * s + EDGE)
                .left(WIPE_PAD_H * s + EDGE)
                .right(WIPE_PAD_H * s + EDGE),
        )
        .style(move |_, status| {
            let (fill, edge, ink) = match status {
                button::Status::Disabled => (Color::TRANSPARENT, p.border_soft, p.faint),
                button::Status::Hovered | button::Status::Pressed => (wash, p.border, p.red),
                _ => (Color::TRANSPARENT, p.border, p.red),
            };
            button::Style {
                background: Some(fill.into()),
                border: Border { color: edge, width: EDGE, radius: (WIPE_RADIUS * s).into() },
                text_color: ink,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}

fn wipe_confirm<'a>(p: &'static CompactPalette, s: f32) -> Element<'a, Message> {
    row![
        text("Wipe everything?").size(SUB * s).color(p.dim),
        grid::line_ok(Some(Message::PrefsEraseConfirmed), p, s),
        grid::line_no(Message::PrefsEraseDismissed, p, s),
    ]
    .spacing(CONFIRM_GAP * s)
    .align_y(Alignment::Center)
    .boxed()
}


fn theme_label(t: Theme) -> &'static str {
    Theme::SELECTABLE.iter().find(|(v, _)| *v == t).map_or("Paper", |(_, label)| *label)
}

fn variant(t: Theme) -> ThemeVariant {
    match t {
        Theme::Paper => ThemeVariant::Paper,
        Theme::Chalk => ThemeVariant::Chalk,
        Theme::Graphite => ThemeVariant::Graphite,
        Theme::Obsidian => ThemeVariant::Obsidian,
    }
}

fn currency_name(c: BaseCcy) -> &'static str {
    match c {
        BaseCcy::Usd => "US dollar",
        BaseCcy::Eur => "Euro",
        BaseCcy::Sgd => "Singapore dollar",
        BaseCcy::Aud => "Australian dollar",
    }
}
