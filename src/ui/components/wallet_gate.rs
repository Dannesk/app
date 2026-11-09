

use iced::Widget as _;
use iced::widget::canvas::{self as cnvs, Frame, Geometry, Path, Stroke};
use iced::widget::text::LineHeight;
use iced::widget::{button, canvas, column, container, row, stack, svg, text, Space};
use iced::{mouse, Border, Color, Element, Length, Padding, Point, Rectangle, Shadow};

use crate::channel::Socket;
use crate::controller::message::Message;
use crate::ui::components::compact::vrule;
use crate::ui::components::orbit::{self, Chain};
use crate::utils::fonts::LIGHT;
use crate::utils::icons;
use crate::utils::theme::CompactPalette;

pub const FIG: f32 = 376.0;
const TRACK_R: f32 = 170.0;

const DASH: f32 = 2.0;
const DASH_PITCH: f32 = 8.0;
const LIT: f32 = 58.0;
const LIT_W: f32 = 1.7;
const PLATE: f32 = 142.0;
const GLYPH: f32 = 62.0;
const XRP_MARK_W: f32 = 44.0;
const XRP_MARK_H: f32 = XRP_MARK_W * 424.0 / 512.0;


const WORDS: f32 = 15.0;
const WORD_LINE: f32 = 20.0;
const HIT_H: f32 = 40.0;

const WORD_GAP: f32 = 22.0;
const DIVIDER_H: f32 = 15.0;
const WORDS_TOP: f32 = 34.0;

#[derive(Debug, Clone, Copy)]
pub enum Mark {
    Btc,
    Xrp,
}


pub fn view(
    mark:       Mark,
    create_msg: Message,
    import_msg: Message,
    p:          &'static CompactPalette,
    scale:      f32,
) -> Element<'static, Message> {
    let words = row![
        word("Create wallet", create_msg, p, scale),
        vrule(p.rule, DIVIDER_H * scale),
        word("Import wallet", import_msg, p, scale),
    ]
    .align_y(iced::Alignment::Center);

    let pad_v = (HIT_H - WORD_LINE) / 2.0;
    container(
        column![
            figure(mark, p, scale),
            Space::new().height((WORDS_TOP - pad_v) * scale),
            words,
        ]
        .align_x(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .boxed()
}

pub fn figure(mark: Mark, p: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let fig = Length::Fixed(FIG * scale);

    let glyph: Element<'static, Message> = match mark {
        Mark::Btc => text("\u{20bf}")
            .font(LIGHT)
            .size(GLYPH * scale)
            .line_height(LineHeight::Relative(1.0))
            .color(p.dim)
            .boxed(),
        Mark::Xrp => svg(icons::XRP.clone())
            .width(Length::Fixed(XRP_MARK_W * scale))
            .height(Length::Fixed(XRP_MARK_H * scale))
            .style(move |_, _| svg::Style { color: Some(p.dim) })
            .boxed(),
    };

    let chain = match mark {
        Mark::Btc => Chain::Btc,
        Mark::Xrp => Chain::Xrp,
    };
    let node = orbit::node(chain, orbit::socket() == Socket::Live);
    let orbit = Orbit {
        track: Color { a: 0.32, ..p.faint },
        lit:   Color { a: 0.85, ..node.color(p) },
        plate: p.border_soft,
        scale,
    };


    stack![
        canvas(orbit).width(fig).height(fig),
        container(glyph).center(fig),
    ].boxed()
}

fn word(
    label: &'static str,
    msg:   Message,
    p:     &'static CompactPalette,
    scale: f32,
) -> Element<'static, Message> {
    let pad_v = (HIT_H - WORD_LINE) / 2.0 * scale;
    let pad_h = WORD_GAP * scale;
    button(
        text(label)
            .size(WORDS * scale)
            .line_height(LineHeight::Absolute((WORD_LINE * scale).into())),
    )
    .padding(Padding::new(0.0).top(pad_v).bottom(pad_v).left(pad_h).right(pad_h))
    .on_press(msg)
    .style(move |_, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: None,
            text_color: if hot { p.text } else { p.dim },
            border: Border::default(),
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}


struct Orbit {
    track: Color,
    lit:   Color,
    plate: Color,
    scale: f32,
}

impl cnvs::Program<Message> for Orbit {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<iced::Renderer>> {
        let s = self.scale;
        let mut frame = Frame::new(renderer, bounds.size());
        let c = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let r = TRACK_R * s;
        orbit::track(&mut frame, c, r, DASH * s, DASH_PITCH * s, self.track);
        orbit::arc(
            &mut frame,
            c,
            r,
            -std::f32::consts::FRAC_PI_2,
            LIT / TRACK_R,
            self.lit,
            (LIT_W * s).max(1.0),
        );

        frame.stroke(
            &Path::circle(c, PLATE / 2.0 * s - 0.5),
            Stroke::default().with_color(self.plate).with_width(1.0),
        );

        vec![frame.into_geometry()]
    }
}
