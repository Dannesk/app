//! Settings — one page, two columns, no scroll region.
//!
//! Replaces the floating 640 stack (22px title, 16/13.5 rows, `26px 0 34px` row
//! padding) that ran roughly three screens deep inside a card inside a window.
//! Nothing about the content was too long; the type was too big. At compact
//! density the same content is ~300px and fits above the fold, so this is a
//! routed page in the send/import grammar: back circle top-left, the dock still
//! under it, and the tab you arrived from still lit.
//!
//! **Left is what you set** (appearance, currency, security), **right is what
//! you read** (the service reports). Data sits at the foot of the right column
//! so the one destructive control is nowhere near the theme switch.
//!
//! Palette is [`CompactPalette`], never `AppPalette` — see
//! [`crate::ui::components::compact`] on why the two vocabularies must not mix.
//! One trap, pinned here because the handoff README and its own mock disagree:
//! the README's prose calls the selected segment/chip fill `neutral`, but the
//! mock CSS paints it `var(--pill)`, and the mock wins. In light our `neutral`
//! is #FBFBFA — the window itself — so a selected segment would have been
//! invisible. It is `pill` in all three themes.

use iced::widget::{button, column, container, row, stack, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::channel::{Service, ServiceState, CHANNEL};
use crate::controller::app_state::{AppState, BaseCcy, Theme};
use crate::controller::message::Message;
use crate::ui::components::compact::{self as ck, hairline};
use crate::utils::fonts::MONO;
use crate::utils::theme::{self, CompactPalette};

// ── Geometry ────────────────────────────────────────────────────────────────
// The 800 block is two 376 columns with a 48 gutter. One 560 column at this
// density is a thin ribbon in a 900-wide window; the split uses the shell and
// carries the set/read meaning.

const COL_W: f32 = 376.0;
const GUTTER: f32 = 48.0;
const SIDE_PAD: f32 = 50.0;
/// Gap between groups inside a column.
const GROUP_GAP: f32 = 16.0;
/// `padding 8px 0` on every row.
const ROW_PAD_V: f32 = 8.0;

// ── Type ────────────────────────────────────────────────────────────────────
// Inter for names and prose, Mono for every option label, status and value.

const ROW_NAME: f32 = 12.0;
const ROW_DESC: f32 = 10.5;
const OPTION: f32 = 9.0;
const INERT_VALUE: f32 = 9.5;

/// The whole page. `managebalance::render` returns this in place of
/// totalbalance while `settings_open` — there is no card and no scrim, so the
/// dock composed by `dashboard::render_dashboard` sits under it unchanged.
pub fn view(state: &AppState) -> Element<'_, Message> {
    let p = theme::compact(&state.theme);
    let scale = state.scale();

    let block = column![
        header(p, scale),
        row![
            column![appearance(state, p, scale), display_ccy(state, p, scale), security(state, p, scale)]
                .spacing(GROUP_GAP * scale)
                .width(Length::Fixed(COL_W * scale)),
            column![services(p, scale), data(p, scale)]
                .spacing(GROUP_GAP * scale)
                .width(Length::Fixed(COL_W * scale)),
        ]
        .spacing(GUTTER * scale)
        .padding(Padding::new(0.0).top(16.0 * scale)),
    ]
    .width(Length::Fixed((COL_W * 2.0 + GUTTER) * scale));

    // Centred in the space the dock leaves, with the back circle floated over
    // the top left rather than pushed into the flow — in the flow it would
    // shove the block off centre by its own height. It sits exactly where
    // Balance's Preferences circle was, so the way out is under the pointer
    // that came in.
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

    stack![centred, back].width(Length::Fill).height(Length::Fill).into()
}

/// `Settings` on a `rule` hairline and **nothing else on the line**. No version
/// and no footer: someone opening Settings came to read their settings, a build
/// number belongs to the landing page, and an update is a notification. The
/// right-hand slot Tokens uses for a count stays empty.
fn header<'a>(p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    column![
        container(text("Settings").size(ck::TITLE * scale).color(p.text))
            .width(Length::Fill)
            .padding(Padding::new(0.0).bottom(8.0 * scale)),
        hairline(p.rule),
    ]
    .width(Length::Fill)
    .into()
}

// ── Groups ──────────────────────────────────────────────────────────────────

/// A group: mono eyebrow (with an optional right-slot count where the group can
/// answer its own question), an optional Inter description, then its rows.
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
        head = head.push(Space::new().width(Length::Fill));
        head = head.push(text(c.to_uppercase()).font(MONO).size(ck::EYEBROW * scale).color(p.muted));
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
            .padding(Padding::new(0.0).bottom(6.0 * scale)),
        );
    }

    c.push(body).into()
}

/// One row: 1px `rule` on TOP, `8px 0`, name over description on the left, the
/// control or the value pinned right.
///
/// The description takes `Fill` and clips so a long line ellipsises inside the
/// row instead of pushing the control out of it.
fn srow<'a>(
    name: &'a str,
    desc: &'a str,
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
                    text(desc).size(ROW_DESC * scale).color(p.dim).wrapping(text::Wrapping::None),
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
    .into()
}

// ── Left column ─────────────────────────────────────────────────────────────

/// Theme is an N-option segment driven by [`Theme::SELECTABLE`], not a flip —
/// graphite becomes selectable by adding one entry there. Hide balances lands
/// here because Balance v3 moved the mask out of the hero and into preferences,
/// and this is preferences.
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

    group("appearance", None, None, body.into(), p, scale)
}

/// The ONLY place a currency is chosen — no screen carries tabs of its own
/// (2026-08-29); every total and every `XRP/…` rate simply reads in this base.
fn display_ccy<'a>(state: &'a AppState, p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let selected = state.base_currency;
    let mut chips = row![].align_y(Alignment::Center).spacing(6.0 * scale);
    for c in BaseCcy::ALL {
        chips = chips.push(chip(c.code(), c == selected, Message::DisplayCcySet(c), p, scale));
    }
    group(
        "display currency",
        None,
        Some("Quote every balance and rate in this currency"),
        container(chips)
            .padding(Padding::new(0.0).top(1.0 * scale).bottom(3.0 * scale))
            .into(),
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

// ── Right column: services ──────────────────────────────────────────────────

/// Four rows, not the eight components behind them.
///
/// **Each row names a thing, and its description names that thing's own job.**
/// The earlier pass named rows by capability instead, and capabilities overlap:
/// two nodes and a delivery path all ended up claiming "balances", which is
/// both redundant and false. Neither node provides this wallet a balance —
/// `btc:node` is only "bitcoind answers" (probed by indexd, our sole window
/// onto the node), and `btc:indexd` is the index at tip; since 2026-09-14 the
/// Bitcoin relay lives inside indexd, so its delivery has no separate part to
/// fail and folds into the node row. On the XRP side the relay translates and
/// Redis delivers, which is what the relay row reports. So the nodes describe
/// the chain, and the one delivery row is XRP's.
///
/// The collapsing is still by consequence. The three price feeds are ONE
/// statement — whether prices are current — and a per-provider list would be
/// both longer and less true: binance dying strands AUD and SGD even though
/// kraken and gemini are up, so "2 of 3 sources live" would be a false
/// reassurance in the likeliest outage. Each node absorbs the process sitting
/// over it (the book stream broadcasts the XRP node, `indexd` owns bitcoind);
/// neither can meaningfully fail while its node is healthy.
///
/// See [`crate::channel::Channel::service_state`] for the three-state model and
/// why bitcoin has no degraded state while XRP does.
const SERVICES: &[(Service, &str, &str)] = &[
    (Service::PriceFeeds, "Price feeds", "Spot market prices and chart data"),
    (Service::XrpNode, "XRPL node", "Order book, market depth and trading engine"),
    (Service::BitcoinNode, "Bitcoin node", "Mempool, index, wallet updates and history"),
    (Service::RelayServer, "Relay server", "XRP live updates and wallet subscriptions"),
];

fn services<'a>(p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let states: Vec<ServiceState> = SERVICES.iter().map(|(s, _, _)| CHANNEL.service_state(*s)).collect();

    // The group answers its own question before you read it. Not a tally: with
    // three states "3 up · 1 down" is arithmetic, and what you want to know is
    // whether anything needs you.
    let degraded = states.iter().filter(|s| **s == ServiceState::Degraded).count();
    let down = states.iter().filter(|s| **s == ServiceState::Down).count();
    let count = match (degraded, down) {
        (0, 0) => "all live".to_string(),
        (d, 0) => format!("{d} degraded"),
        (0, n) => format!("{n} down"),
        (d, n) => format!("{d} degraded · {n} down"),
    };

    let mut body = column![].width(Length::Fill);
    for ((_, name, desc), st) in SERVICES.iter().zip(states) {
        body = body.push(srow(name, desc, status(st, p, scale), p, scale));
    }

    group("services", Some(count), None, body.into(), p, scale)
}

/// 4px dot + mono word. Read-only — no control and no hover, these rows are
/// reports.
///
/// Down is `faint`/`muted`, not red: red on this screen is the erase act, not a
/// topic, and a down service is already self-evident from the screen that
/// depends on it. Degraded is the state you would otherwise MISS, so it is the
/// one that carries colour into the label as well as the dot.
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
    .into()
}

fn dot<'a>(size: f32, color: Color) -> Element<'a, Message> {
    container(Space::new().width(size).height(size))
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (size / 2.0).into() },
            ..Default::default()
        })
        .into()
}

// ── Right column: data ──────────────────────────────────────────────────────

/// Wipe all data — the trigger, not a door.
///
/// No confirmation stack, no typed field, no routed page: it is an emergency
/// button, and a stack is the opposite of one. Granular removal already lives in
/// Key management, so this row has exactly one job, and nothing it does reaches
/// the chain. With nothing to wipe the row goes INERT — a `muted` value in the
/// right slot, no link and so no hover — which is also what confirms the wipe
/// afterwards, since the row re-reads its own state.
///
/// The row itself is never red. Only the link is: the red is the act, not the
/// topic.
fn data<'a>(p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let has_wallet = CHANNEL.wallet_balance_rx.borrow().1.is_some()
        || CHANNEL.bitcoin_wallet_rx.borrow().1.is_some();

    let (desc, right): (&str, Element<'a, Message>) = if has_wallet {
        ("Wallets, keys and your PIN", erase_link(p, scale))
    } else {
        (
            "No wallet data on this device",
            text("none").font(MONO).size(INERT_VALUE * scale).color(p.muted).into(),
        )
    };

    group("data", None, None, srow("Wipe all data", desc, right, p, scale), p, scale)
}

/// The same link treatment Tokens gives `enable`, in `red`: mono 9 upper, a
/// `2px 6px` pad with matching negative margin so the hover wash reads as a
/// chip without moving the label off the row's right edge.
fn erase_link<'a>(p: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let wash = Color { a: 0.10, ..p.red };
    container(
        button(text("ERASE").font(MONO).size(OPTION * scale).color(p.red))
            .on_press(Message::PrefsEraseConfirmed)
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
    .into()
}

// ── Controls ────────────────────────────────────────────────────────────────

/// One display-currency chip. Selected = `pill` fill, `border` border, `text`
/// ink and a leading check; unselected takes a `hover` wash and `dim` ink.
fn chip<'a>(
    label: &'a str,
    selected: bool,
    msg: Message,
    p: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let mut inner = row![].align_y(Alignment::Center).spacing(4.0 * scale);
    if selected {
        inner = inner.push(text("\u{2713}").font(MONO).size(8.0 * scale).color(p.dim));
    }
    inner = inner.push(
        text(label.to_uppercase())
            .font(MONO)
            .size(OPTION * scale)
            .color(if selected { p.text } else { p.muted }),
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
        .into()
}
