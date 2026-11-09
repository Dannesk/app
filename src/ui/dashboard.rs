use iced::Widget as _;
use iced::widget::canvas::{self, Canvas, Frame, Path, Stroke};
use iced::widget::{button, column, container, row, svg, text, Space};
use iced::{mouse, Alignment, Border, Color, Element, Length, Padding, Rectangle, Shadow};
use crate::channel::CHANNEL;
use crate::controller::message::Message;
use crate::controller::app_state::{AppState, Tab};
use crate::ui::components::{compact, orbit};
use crate::ui::{activity_log, managexrp, managebtc, managebalance};
use crate::utils::icons;
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
const DOCK_ICON: f32 = 17.0;
const DOCK_MARK: f32 = 14.0;
const DOCK_ICON_GAP: f32 = 6.0;
const DOCK_RING_R: f32 = 7.3 / 22.0;
const DOCK_RING_W: f32 = 1.8 / 22.0;
const DOCK_RING_ARC: f32 = 108.0;
const DOCK_LINE: f32 = 1.375;
const DOCK_ROW: f32 = if DOCK_ICON > DOCK_LABEL * DOCK_LINE { DOCK_ICON } else { DOCK_LABEL * DOCK_LINE };
pub const DOCK_H: f32 = 2.0 * DOCK_PAD_V + DOCK_ROW;


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
        container(
            row![
                dock_icon(&target, is_active, ink, p, scale),
                Space::new().width(Length::Fixed(DOCK_ICON_GAP * scale)),
                text(label).size(DOCK_LABEL * scale).line_height(DOCK_LINE).color(ink),
            ]
            .align_y(Alignment::Center),
        )
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

fn dock_icon(
    tab:    &Tab,
    active: bool,
    ink:    Color,
    p:      &'static CompactPalette,
    scale:  f32,
) -> Element<'static, Message> {
    let icon = Length::Fixed(DOCK_ICON * scale);
    let mark = Length::Fixed(DOCK_MARK * scale);
    let tinted = |handle: svg::Handle, held: bool| -> Element<'static, Message> {
        let color = if active || held { ink } else { p.faint };
        svg(handle)
            .width(mark)
            .height(mark)
            .style(move |_, _| svg::Style { color: Some(color) })
            .boxed()
    };

    let glyph = match tab {
        Tab::Balance => Canvas::new(DockRing { track: ink, arc: p.mint })
            .width(icon)
            .height(icon)
            .boxed(),
        Tab::Xrp => tinted(icons::XRP.clone(), CHANNEL.wallet_balance_rx.borrow().1.is_some()),
        Tab::Btc if active => svg(icons::BTC.clone()).width(mark).height(mark).boxed(),
        Tab::Btc => tinted(icons::BTC_MONO.clone(), CHANNEL.bitcoin_wallet_rx.borrow().1.is_some()),
    };
    container(glyph).center(icon).boxed()
}

struct DockRing {
    track: Color,
    arc:   Color,
}

impl<M> canvas::Program<M> for DockRing {
    type State = ();

    fn draw(
        &self,
        _state:   &(),
        renderer: &iced::Renderer,
        _theme:   &iced::Theme,
        bounds:   Rectangle,
        _cursor:  mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let c = frame.center();
        let d = bounds.width.min(bounds.height);
        let r = DOCK_RING_R * d;
        let w = DOCK_RING_W * d;
        frame.stroke(&Path::circle(c, r), Stroke::default().with_color(self.track).with_width(w));
        orbit::arc(&mut frame, c, r, -std::f32::consts::FRAC_PI_2, DOCK_RING_ARC.to_radians(), self.arc, w);
        vec![frame.into_geometry()]
    }
}
