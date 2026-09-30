//! The no-wallet gate (no wallet v8, 2026-09-10): the create / import chooser
//! shown on a chain's tab while that chain has no wallet yet.
//!
//! One figure, centred, with two words under it — `Create wallet` │
//! `Import wallet`. Nothing else on the stage: no title, no chain name, no
//! descriptions, no chevrons, no derivation paths. Chain identity is carried
//! by the mark in the figure and the lit dock tab.
//!
//! ## The figure — the flat orbit, on both chains
//!
//! A dashed `r 170` ring with one `green` arc running it, one lap per 21 s,
//! and the chain's mark on a 142px plate at the centre: `₿` for Bitcoin, the
//! XRP mark tinted `dim` for XRP. The handoff specifies the orbit for BTC and a
//! rotating transfer globe for XRP; the orbit went to both (user, 2026-09-10),
//! with the globe as the fallback if the tinted XRP mark reads poorly here.
//! The globe lives on the landing page (`landing/src/components/Transfers.astro`).
//!
//! [`figure`] draws the orbit on its own, without the stage or the words, so
//! the import/create panes can stand a small one in the space below the
//! phrase box. It is the same canvas at a smaller `scale` — not a second
//! drawing to keep in step.
//!
//! ## How it animates — no subscription, no message
//!
//! The canvas drives itself. Every `RedrawRequested` it receives, it asks for
//! the next one [`FRAME`] later (`canvas::Action::request_redraw_at`). That
//! costs a redraw of what is on screen and nothing else: no `Message`, no
//! `update`, no `view` rebuild — unlike the gate's 16ms `Message::Sync` pump,
//! which re-runs the whole app every frame.
//!
//! It also switches itself off. The loop only exists while the canvas is in
//! the tree: switch tabs, or load a wallet, and nothing asks for a frame any
//! more. There is no subscription to gate and none to forget.
//!
//! The arc's position is read off [`bloom::clock_secs`], the process clock the
//! bloom and the activity spinner already share, so the canvas keeps no state
//! and a tree rebuild can never make the arc jump.
//!
//! ## The guard
//!
//! Nothing on this screen derives anything: both words are reversible
//! navigation. The derivation path and phrase length live on create and
//! import, beside the phrase they apply to, from the constant the deriver
//! uses — never here (settled 2026-09-06).

use iced::time::Duration;
use iced::widget::canvas::{self as cnvs, path::Arc, Frame, Geometry, LineCap, LineDash, Path, Stroke};
use iced::widget::text::LineHeight;
use iced::widget::{button, canvas, column, container, row, stack, svg, text, Space};
use iced::{mouse, window, Border, Color, Element, Length, Padding, Point, Radians, Rectangle, Shadow};

use crate::controller::message::Message;
use crate::ui::components::compact::vrule;
use crate::utils::bloom;
use crate::utils::fonts::LIGHT;
use crate::utils::icons;
use crate::utils::theme::CompactPalette;

// ── The figure ──────────────────────────────────────────────────────────────

/// The figure's bounding box, at the gate's own size. Everything below is a
/// fraction of it and scales with it, so [`figure`] draws the same shape at
/// any size — the setup screens' pane orbit is this one at `140 / FIG`.
pub const FIG: f32 = 376.0;
/// The ring both the track and the lit arc run on.
const TRACK_R: f32 = 170.0;
/// The track's dash: 2 on, 6 off. The gap is stretched a hair so a whole
/// number of dashes closes the ring — otherwise the seam shows at 3 o'clock.
const DASH: f32 = 2.0;
const DASH_PITCH: f32 = 8.0;
/// The lit segment's arc length (≈ 19.5° of the ring) and weight.
const LIT: f32 = 58.0;
const LIT_W: f32 = 1.7;
/// One lap. Deliberately slower than notice.
const LAP_SECS: f32 = 21.0;
/// The plate the mark sits on: a 1px `border_soft` ring, no fill.
const PLATE: f32 = 142.0;
/// `₿` in Inter 300.
const GLYPH: f32 = 62.0;
/// The XRP mark's width; its height follows the file's 512 × 424 box. Smaller
/// than `₿` on purpose — a filled mark carries more ink than a 300 glyph.
const XRP_MARK_W: f32 = 44.0;
const XRP_MARK_H: f32 = XRP_MARK_W * 424.0 / 512.0;

/// The frame interval. Everything here moves slowly — the arc head at ~50 px/s
/// — so 30 fps reads as smooth and costs half of 60.
const FRAME: Duration = Duration::from_millis(33);

// ── The words ───────────────────────────────────────────────────────────────

/// Inter 400, the app default.
const WORDS: f32 = 15.0;
const WORD_LINE: f32 = 20.0;
/// The hit target is 40 tall though the ink is 20 — padded, per the handoff.
const HIT_H: f32 = 40.0;
/// Ink either side of the hairline. It is the buttons' own horizontal padding,
/// so the hit area reaches the rule and the row stays symmetric.
const WORD_GAP: f32 = 22.0;
const DIVIDER_H: f32 = 15.0;
/// Figure box to the words' line box.
const WORDS_TOP: f32 = 34.0;

/// Which mark the plate carries.
#[derive(Debug, Clone, Copy)]
pub enum Mark {
    Btc,
    Xrp,
}

/// The gate for one chain. `create_msg` / `import_msg` fire on press so the
/// calling menu keeps ownership of its own navigation. The dock is composed
/// above this by `dashboard`, so the column centres in what it leaves.
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
    .into()
}

/// The orbit alone — the dashed ring, the lit arc, the plate and the chain's
/// mark, in a `FIG * scale` box. No words, no stage: the caller places it.
///
/// `scale` is the UI scale times whatever fraction of [`FIG`] the surface
/// wants, and the whole figure follows it — dash pitch, plate, mark. The gate
/// passes the UI scale; the import/create panes pass a fraction of it to sit a
/// small one in the slack below the phrase box.
///
/// Self-driving, so a caller adds no subscription and gates nothing: the
/// canvas asks for its own next frame while it is in the tree and stops when
/// it leaves. Two of these on screen at once would simply each ask.
pub fn figure(mark: Mark, p: &'static CompactPalette, scale: f32) -> Element<'static, Message> {
    let fig = Length::Fixed(FIG * scale);

    let glyph: Element<'static, Message> = match mark {
        Mark::Btc => text("\u{20bf}")
            .font(LIGHT)
            .size(GLYPH * scale)
            .line_height(LineHeight::Relative(1.0))
            .color(p.dim)
            .into(),
        Mark::Xrp => svg(icons::XRP.clone())
            .width(Length::Fixed(XRP_MARK_W * scale))
            .height(Length::Fixed(XRP_MARK_H * scale))
            .style(move |_, _| svg::Style { color: Some(p.dim) })
            .into(),
    };

    let orbit = Orbit {
        track: Color { a: 0.32, ..p.faint },
        lit:   Color { a: 0.85, ..p.green },
        plate: p.border_soft,
        scale,
    };

    // The stack hands every event to both layers, so the canvas underneath
    // still gets its `RedrawRequested` with the mark on top.
    stack![
        canvas(orbit).width(fig).height(fig),
        container(glyph).center(fig),
    ]
    .into()
}

/// A word: `dim`, brightening to `text` on hover. No fill, no underline.
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
    .into()
}

// ── The orbit ───────────────────────────────────────────────────────────────

/// The ring, the lit arc and the plate. The mark is a widget stacked on top,
/// so the SVG tint and the glyph go through the same paths as everywhere else.
struct Orbit {
    track: Color,
    lit:   Color,
    plate: Color,
    scale: f32,
}

impl cnvs::Program<Message> for Orbit {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &cnvs::Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<cnvs::Action<Message>> {
        match event {
            cnvs::Event::Window(window::Event::RedrawRequested(now)) => {
                Some(cnvs::Action::request_redraw_at(*now + FRAME))
            }
            _ => None,
        }
    }

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

        // The track — a whole number of dashes round the ring.
        let circumference = std::f32::consts::TAU * r;
        let pitch = circumference / (circumference / (DASH_PITCH * s)).round();
        let dash = [DASH * s, pitch - DASH * s];
        frame.stroke(
            &Path::circle(c, r),
            Stroke {
                line_dash: LineDash { segments: &dash, offset: 0 },
                line_cap: LineCap::Round,
                ..Stroke::default().with_color(self.track).with_width(1.0)
            },
        );

        // The lit arc — clockwise from 3 o'clock, as the mock's dash offset
        // runs. `LineDash::offset` is a segment index in iced, not a distance,
        // so the arc is drawn as an arc rather than as a sliding dash.
        let head = (bloom::clock_secs() % LAP_SECS) / LAP_SECS * std::f32::consts::TAU;
        let sweep = LIT / TRACK_R;
        frame.stroke(
            &Path::new(|b| {
                b.arc(Arc {
                    center: c,
                    radius: r,
                    start_angle: Radians(head),
                    end_angle: Radians(head + sweep),
                })
            }),
            Stroke {
                line_cap: LineCap::Round,
                // Floored at the track's own hairline. The arc has to read as
                // heavier than the dots it runs over, and at the panes' 0.37
                // it would otherwise come out at 0.6 — thinner than the track,
                // which inverts the figure. No-op at the gate, where `s` is
                // the UI scale and this is already 1.7 or more.
                ..Stroke::default().with_color(self.lit).with_width((LIT_W * s).max(1.0))
            },
        );

        // The plate. Half a pixel in, so the 1px ring sits inside the 142 box
        // the way a CSS border does.
        frame.stroke(
            &Path::circle(c, PLATE / 2.0 * s - 0.5),
            Stroke::default().with_color(self.plate).with_width(1.0),
        );

        vec![frame.into_geometry()]
    }
}
