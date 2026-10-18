

use iced::widget::{Column, Row};
use iced::Widget as _;
use iced::widget::{button, column, container, row, stack, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::channel::{Service, ServiceState, CHANNEL};
use crate::controller::app_state::{AppState, BaseCcy, Theme};
use crate::controller::message::Message;
use crate::ui::components::compact::{self as ck, hairline};
use crate::ui::components::grid;
use crate::utils::fonts::MONO;
use crate::utils::theme::{self, CompactPalette};

const COL_W: f32 = 376.0;
const GUTTER: f32 = 48.0;
const SIDE_PAD: f32 = 50.0;
/// Gap between groups inside a column.
const GROUP_GAP: f32 = 16.0;
/// `padding 8px 0` on every row.
const ROW_PAD_V: f32 = 8.0;

const ROW_NAME: f32 = 12.0;
const ROW_DESC: f32 = 10.5;
const OPTION: f32 = 9.0;
const INERT_VALUE: f32 = 9.5;
const CONFIRM_GAP: f32 = 4.5;


pub fn view(state: &AppState) -> Element<'_, Message> {
    let p = theme::compact(&state.theme);
    let scale = state.scale();

    let block = column![
        header(p, scale),
        row![
            column![appearance(state, p, scale), display_ccy(state, p, scale), security(state, p, scale)]
                .spacing(GROUP_GAP * scale)
                .width(Length::Fixed(COL_W * scale)),
            column![services(p, scale), data(state.prefs_erase_confirm, p, scale)]
                .spacing(GROUP_GAP * scale)
                .width(Length::Fixed(COL_W * scale)),
        ]
        .spacing(GUTTER * scale)
        .padding(Padding::new(0.0).top(16.0 * scale)),
    ]
    .width(Length::Fixed((COL_W * 2.0 + GUTTER) * scale));

 
    let centred = container(block)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding::new(0.0).left(SIDE_PAD * scale).right(SIDE_PAD * scale));

    let back = container(ck::back_chevron(Message::CloseSettings, p, scale))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::new(0.0).top(ck::CORNER_TOP * scale).left(ck::CORNER_LEFT * scale));

    stack![centred, back].width(Length::Fill).height(Length::Fill).boxed()
}

fn header<'a>(p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    column![
        container(text("Settings").size(ck::TITLE * scale).color(p.text))
            .width(Length::Fill)
            .padding(Padding::new(0.0).bottom(8.0 * scale)),
        hairline(p.rule),
    ]
    .width(Length::Fill)
    .boxed()
}


fn group<'a>(
    eyebrow: &'a str,
    count: Option<String>,
    desc: Option<&'a str>,
    body: Element<'a, Message>,
    p: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let mut head = row![text(eyebrow.to_uppercase())
        .font(MONO)
        .size(ck::EYEBROW * scale)
        .color(p.muted)]
    .align_y(Alignment::End);
    if let Some(c) = count {
        head = head.push(Space::new().width(Length::Fill).boxed());
        head = head.push(text(c.to_uppercase()).font(MONO).size(ck::EYEBROW * scale).color(p.muted).boxed());
    }

    let mut c = column![container(head)
        .width(Length::Fill)
        .padding(Padding::new(0.0).bottom(5.0 * scale))]
    .width(Length::Fill);

    if let Some(d) = desc {
        c = c.push(
            container(
                text(d)
                    .size(ROW_DESC * scale)
                    .line_height(iced::widget::text::LineHeight::Relative(1.45))
                    .color(p.dim),
            )
            .width(Length::Fill)
            .padding(Padding::new(0.0).bottom(6.0 * scale)).boxed(),
        );
    }

    c.push(body).boxed()
}

fn srow<'a>(
    name: &'a str,
    desc: &'a str,
    right: Element<'a, Message>,
    p: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    srow_with(name, text(desc).size(ROW_DESC * scale).color(p.dim).wrapping(text::Wrapping::None).boxed(), right, p, scale)
}

fn srow_with<'a>(
    name: &'a str,
    desc: Element<'a, Message>,
    right: Element<'a, Message>,
    p: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    column![
        hairline(p.rule),
        container(
            row![
                column![
                    text(name).size(ROW_NAME * scale).color(p.text),
                    desc,
                ]
                .spacing(2.0 * scale)
                .width(Length::Fill)
                .clip(true),
                right,
            ]
            .spacing(14.0 * scale)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding::new(0.0).top(ROW_PAD_V * scale).bottom(ROW_PAD_V * scale)),
    ]
    .width(Length::Fill)
    .boxed()
}


fn appearance<'a>(state: &'a AppState, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let active = Theme::of(&state.theme);
    let themes: Vec<(&str, bool, Message)> = Theme::SELECTABLE
        .iter()
        .map(|(t, label)| (*label, *t == active, Message::ThemeSet(*t)))
        .collect();

    let hidden = state.hide_balance;
    let mask = vec![
        ("on", hidden, if hidden { Message::Sync } else { Message::ToggleHideBalance }),
        ("off", !hidden, if hidden { Message::ToggleHideBalance } else { Message::Sync }),
    ];

    let body = column![
        srow("Theme", "Interface palette", ck::track(themes, p, scale), p, scale),
        srow("Hide balances", "Mask every value until revealed", ck::track(mask, p, scale), p, scale),
    ]
    .width(Length::Fill);

    group("appearance", None, None, body.boxed(), p, scale)
}


fn display_ccy<'a>(state: &'a AppState, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let selected = state.base_currency;
    let mut chips: Row<Element<'_, Message>> = row![].align_y(Alignment::Center).spacing(6.0 * scale);
    for c in BaseCcy::ALL {
        chips = chips.push(chip(c.code(), c == selected, Message::DisplayCcySet(c), p, scale));
    }
    group(
        "display currency",
        None,
        Some("Quote every balance and rate in this currency"),
        container(chips)
            .padding(Padding::new(0.0).top(1.0 * scale).bottom(3.0 * scale))
            .boxed(),
        p,
        scale,
    )
}

fn security<'a>(state: &'a AppState, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let on = state.gate_enabled;
    let opts = vec![
        ("on", on, if on { Message::Sync } else { Message::GateEnabledToggled(true) }),
        ("off", !on, if on { Message::GateEnabledToggled(false) } else { Message::Sync }),
    ];
    let hint = if on {
        "Ask for your PIN when the app opens"
    } else {
        "The app opens without unlocking"
    };
    group(
        "security",
        None,
        None,
        srow("Launch unlock", hint, ck::track(opts, p, scale), p, scale),
        p,
        scale,
    )
}


const SERVICES: &[(Service, &str, &str)] = &[
    (Service::PriceFeeds, "Price feeds", "Spot market prices and chart data"),
    (Service::XrpNode, "XRPL node", "Order book, market depth and trading engine"),
    (Service::BitcoinNode, "Bitcoin node", "Mempool, index, wallet updates and history"),
    (Service::RelayServer, "Relay server", "XRP live updates and wallet subscriptions"),
];

fn services<'a>(p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let states: Vec<ServiceState> = SERVICES.iter().map(|(s, _, _)| CHANNEL.service_state(*s)).collect();

  
    let degraded = states.iter().filter(|s| **s == ServiceState::Degraded).count();
    let down = states.iter().filter(|s| **s == ServiceState::Down).count();
    let count = match (degraded, down) {
        (0, 0) => "all live".to_string(),
        (d, 0) => format!("{d} degraded"),
        (0, n) => format!("{n} down"),
        (d, n) => format!("{d} degraded · {n} down"),
    };

    let mut body: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for ((_, name, desc), st) in SERVICES.iter().zip(states) {
        body = body.push(srow(name, desc, status(st, p, scale), p, scale));
    }

    group("services", Some(count), None, body.boxed(), p, scale)
}


fn status<'a>(st: ServiceState, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let (dot_c, ink, word) = match st {
        ServiceState::Up => (p.green, p.dim, "up"),
        ServiceState::Degraded => (p.amber, p.amber, "degraded"),
        ServiceState::Down => (p.faint, p.muted, "down"),
    };
    row![
        dot(4.0 * scale, dot_c),
        text(word.to_uppercase()).font(MONO).size(OPTION * scale).color(ink),
    ]
    .spacing(6.0 * scale)
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


fn data<'a>(armed: bool, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let has_wallet = CHANNEL.wallet_balance_rx.borrow().1.is_some()
        || CHANNEL.bitcoin_wallet_rx.borrow().1.is_some();

    let body = if !has_wallet {
        srow(
            "Wipe all data",
            "No wallet data on this device",
            text("none").font(MONO).size(INERT_VALUE * scale).color(p.muted).boxed(),
            p,
            scale,
        )
    } else if armed {
        srow_with("Wipe all data", erase_confirm(p, scale), Space::new().boxed(), p, scale)
    } else {
        srow("Wipe all data", "Wallets, keys and your PIN", erase_link(p, scale), p, scale)
    };

    group("data", None, None, body, p, scale)
}


fn erase_link<'a>(p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let wash = Color { a: 0.10, ..p.red };
    container(
        button(text("ERASE").font(MONO).size(OPTION * scale).color(p.red))
            .on_press(Message::PrefsEraseAsked)
            .padding(Padding::new(2.0 * scale).left(6.0 * scale).right(6.0 * scale))
            .style(move |_, status| {
                let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: Some(if hot { wash } else { Color::TRANSPARENT }.into()),
                    border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (5.0 * scale).into() },
                    text_color: p.red,
                    shadow: Shadow::default(),
                    snap: false,
                }
            }),
    )
    .padding(Padding::new(0.0).right(-6.0 * scale))
    .boxed()
}


fn erase_confirm<'a>(p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    row![
        text("Erase everything?").size(ROW_DESC * scale).color(p.dim),
        grid::line_ok(Some(Message::PrefsEraseConfirmed), p, scale),
        grid::line_no(Message::PrefsEraseDismissed, p, scale),
    ]
    .spacing(CONFIRM_GAP * scale)
    .align_y(Alignment::Center)
    .boxed()
}

fn chip<'a>(
    label: &'a str,
    selected: bool,
    msg: Message,
    p: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let mut inner: Row<Element<'_, Message>> = row![].align_y(Alignment::Center).spacing(4.0 * scale);
    if selected {
        inner = inner.push(text("\u{2713}").size(8.0 * scale).color(p.dim).boxed());
    }
    inner = inner.push(
        text(label.to_uppercase())
            .font(MONO)
            .size(OPTION * scale)
            .color(if selected { p.text } else { p.muted }).boxed(),
    );

    button(inner)
        .on_press(msg)
        .padding(Padding::new(3.0 * scale).left(8.0 * scale).right(8.0 * scale))
        .style(move |_, status| {
            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let (bg, bdr, ink) = if selected {
                (p.pill, p.border, p.text)
            } else if hot {
                (p.hover, p.border_soft, p.dim)
            } else {
                (Color::TRANSPARENT, p.border_soft, p.muted)
            };
            button::Style {
                background: Some(bg.into()),
                border: Border { color: bdr, width: 1.0, radius: (7.0 * scale).into() },
                text_color: ink,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}
