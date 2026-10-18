use iced::Widget as _;
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Border, Element, Length, Padding, Shadow};
use crate::controller::message::Message;
use crate::controller::app_state::{AppState, Tab};
use crate::ui::components::compact;
use crate::ui::{activity_log, managexrp, managebtc, managebalance};
use crate::utils::theme::{self, CompactPalette};

pub fn render_dashboard(state: &AppState) -> Element<'_, Message> {
  
    if !state.gate_unlocked {
        return crate::ui::enterpin::render_gate(state);
    }

    if let Some(ref log) = state.activity_log {
        return activity_log::render_activity_log(log, state.activity_tick, &state.theme, state.scale());
    }


    let content: Element<'_, Message> = match state.selected_tab {
        Tab::Balance => managebalance::render(state),
        Tab::Xrp    => managexrp::render_manage_xrp(state),
        Tab::Btc    => managebtc::render_manage_btc(state),
    };

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
    ].boxed()
}



const DOCK_LABEL: f32 = 11.0;
const DOCK_PAD_V: f32 = 8.0;
const DOCK_BAR_H: f32 = 2.0;
const DOCK_BAR_INSET: u16 = 41;
const DOCK_BAR_SPAN: u16 = 18;


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
        .boxed()
    } else {
        Space::new().height(Length::Fixed(bar_h)).boxed()
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
        btn.boxed()
    } else {
        btn.on_press(Message::TabChanged(target)).boxed()
    }
}
