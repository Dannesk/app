use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Border, Element, Length, Padding, Shadow};
use crate::controller::message::Message;
use crate::controller::app_state::{AppState, Tab};
use crate::ui::components::compact;
use crate::ui::{activity_log, managexrp, managebtc, managebalance};
use crate::utils::theme::{self, CompactPalette};

pub fn render_dashboard(state: &AppState) -> Element<'_, Message> {
    // Startup view-gate: nothing renders until the pattern is drawn (created on
    // first run, entered thereafter). Pure theatre over the real credentials.
    if !state.gate_unlocked {
        return crate::ui::enterpin::render_gate(state);
    }

    if let Some(ref log) = state.activity_log {
        return activity_log::render_activity_log(log, state.activity_tick, &state.theme, state.scale());
    }

    // Each tab owns its own modal stack. Balance swaps in the Settings PAGE
    // (its trigger lives on totalbalance) rather than floating a card over it —
    // the compact redesign retired the 640 stack. The Rates lookup modal that
    // used to float here over every tab is gone (2026-08-29): the fiat feed
    // still prices every balance, but a list of a handful of tickers earned no
    // screen of its own; the trade screen's index chart is the only chart.
    // The asset chart (`chart ›` on a balance screen) takes the tab's content
    // slot and keeps the dock — chart v3 draws it under the stats — so a dock
    // tap leaves it the way it leaves any screen.
    let content: Element<'_, Message> = match state.selected_tab {
        Tab::Balance => managebalance::render(state),
        Tab::Xrp    => managexrp::render_manage_xrp(state),
        Tab::Btc    => managebtc::render_manage_btc(state),
    };

    // The dock (dock v3, 2026-09-03): a `rule` hairline and three words. It is
    // drawn here, outside every screen's centred block, so it is identical on
    // every screen and never scrolls. Settings is app-level and routes inside
    // the Balance tab, so the tab you arrived from stays lit: nothing changes
    // shape and nothing traps you.
    let scale = state.scale();
    let p = theme::compact(&state.theme);
    column![
        container(content).width(Length::Fill).height(Length::Fill),
        compact::hairline(p.rule),
        row![
            dock_tab(Tab::Balance, &state.selected_tab, p, scale),
            dock_tab(Tab::Xrp,     &state.selected_tab, p, scale),
            dock_tab(Tab::Btc,     &state.selected_tab, p, scale),
        ]
        .width(Length::Fill),
    ]
    .into()
}

// ── Dock ────────────────────────────────────────────────────────────────────
//
// Three equal tabs, label only. The glyphs are gone — they were three text
// characters and only one of them was any good: the X was a multiplication
// sign standing in for the XRP mark, and in a wallet a maths operator reads as
// *close, delete, wrong*. One good glyph out of three is not a set. If marks
// ever come back they come back as a drawn set of three, sized 10–12 beside the
// label — an icon commission, not a character lookup. Until then the dock
// needs no assets at all.
//
// The entire active state is a 2px `dim` bar on the tab's top edge across its
// middle 18% and the label stepping from `dim` to `text`. The bar is the same
// ink as the tabs you are NOT on, so it marks position without outranking the
// screen above it; in the dark themes it is the only opaque light value on the
// dock's top edge, which is what makes it read as lit rather than as a thicker
// rule. No hover wash, no press fill, no badges, no disabled tab.

/// The label: Inter 11.
const DOCK_LABEL: f32 = 11.0;
/// `padding 8px 0` — the bar sits inside the top 8, not on top of it.
const DOCK_PAD_V: f32 = 8.0;
const DOCK_BAR_H: f32 = 2.0;
/// `left 41%` / `right 41%` — the bar is a percentage of the tab, so it holds
/// at any window width.
const DOCK_BAR_INSET: u16 = 41;
const DOCK_BAR_SPAN: u16 = 18;

/// The dock's word for a tab — and, for the two chains, the ticker the no-wallet
/// gate stacks in its gutter (`components::wallet_gate`). One constant, so the
/// gutter and the lit tab can never disagree.
pub fn tab_label(tab: &Tab) -> &'static str {
    match tab {
        Tab::Balance => "Balance",
        Tab::Xrp     => "XRP",
        Tab::Btc     => "BTC",
    }
}

fn dock_tab(
    target:  Tab,
    current: &Tab,
    p:       &'static CompactPalette,
    scale:   f32,
) -> Element<'static, Message> {
    let is_active = &target == current;
    let label = tab_label(&target);
    let ink = if is_active { p.text } else { p.dim };

    // The bar occupies its 2px in layout (iced has no absolute positioning), so
    // the top padding is the bar plus the rest of the 8.
    let bar_h = DOCK_BAR_H * scale;
    let bar: Element<'static, Message> = if is_active {
        row![
            Space::new().width(Length::FillPortion(DOCK_BAR_INSET)),
            container(Space::new())
                .width(Length::FillPortion(DOCK_BAR_SPAN))
                .height(Length::Fixed(bar_h))
                .style(move |_| container::Style { background: Some(p.dim.into()), ..Default::default() }),
            Space::new().width(Length::FillPortion(DOCK_BAR_INSET)),
        ]
        .width(Length::Fill)
        .into()
    } else {
        Space::new().height(Length::Fixed(bar_h)).into()
    };

    let cell = column![
        bar,
        Space::new().height(Length::Fixed(DOCK_PAD_V * scale - bar_h)),
        container(text(label).size(DOCK_LABEL * scale).color(ink))
            .width(Length::Fill)
            .align_x(Alignment::Center),
        Space::new().height(Length::Fixed(DOCK_PAD_V * scale)),
    ]
    .width(Length::Fill);

    let btn = button(cell)
        .width(Length::Fill)
        .padding(Padding::ZERO)
        .style(move |_, _| button::Style {
            background: None,
            border: Border::default(),
            text_color: ink,
            shadow: Shadow::default(),
            snap: false,
        });

    if is_active {
        btn.into()
    } else {
        btn.on_press(Message::TabChanged(target)).into()
    }
}
