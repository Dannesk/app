use iced::widget::canvas::{self as cnvs, Frame, Geometry, Path, Stroke};
use iced::widget::text::{Ellipsis, LineHeight, Span, Wrapping};
use iced::widget::{button, canvas, column, container, rich_text, row, span, stack, svg, text, Space};
use iced::widget::{Column, Row};
use iced::Widget as _;
use iced::{mouse, Alignment, Border, Color, Element, Length, Padding, Point, Rectangle, Shadow};

use crate::btc_script_type::BtcScriptType;
use crate::controller::app_state::{AppState, ImportMode, SetupChain, SetupTask};
use crate::controller::message::{Message, SecureField, SecureOp};
use crate::secure::SecureString;
use crate::ui::components::compact::{self, minimum_hint};
use crate::ui::components::orbit;
use crate::ui::components::phrase_entry::{self, Entry, Metrics};
use crate::ui::components::tui::Run;
use crate::ui::components::wallet_gate::Mark;
use crate::utils::fonts::{LIGHT, MONO};
use crate::utils::phrase::{self, MASK, SLOTS};
use crate::utils::phrase_grid::{Look, PhraseGrid};
use crate::utils::{entropy, icons, theme};


pub fn mnemonic_checksum_ok(phrase: &str) -> bool {
    dannesk_btc_codec::bip39::Mnemonic::parse(phrase).is_ok()
}

pub(crate) const TARGET_WORDS: usize = 24;


pub(crate) fn held_bits(secret: &SecureString, hold: Option<f64>) -> f64 {
    if secret.char_len() == 0 {
        hold.unwrap_or(0.0)
    } else {
        entropy::bits(secret.as_str().trim())
    }
}


pub const KEY_MIN: usize = 6;

const IMPORT_KEYS: &[(&str, &str)] = &[("tab", "take"), ("1\u{2013}5", "pick"), ("ctrl+v", "paste")];

const FRAME_MAX: f32 = 1040.0;
const FRAME_MARGIN: f32 = 48.0;
const SIDE_W: f32 = 290.0;
const SIDE_PAD_V: f32 = 12.0;
const SIDE_PAD_H: f32 = 16.0;
const MAIN_PAD_V: f32 = 28.0;
const MAIN_PAD_H: f32 = 24.0;
const PHRASE_W: f32 = 560.0;
const TASK_W: f32 = 360.0;
const LIST_BLEED: f32 = 8.0;

const TOOL_W: f32 = 24.0;
const TOOL_H: f32 = 22.0;
const TOOL_ICON: f32 = 13.0;

const ORBIT: f32 = 118.0;
const ORBIT_TOP: f32 = 4.0;
const ORBIT_UNIT: f32 = ORBIT / 200.0;
const PATH_TOP: f32 = 10.0;
const PATH: f32 = 9.5;
const TASKS_TOP: f32 = 14.0;
const TASK_H: f32 = 44.0;
const TASK_GAP: f32 = 2.0;
const COMMIT_TOP: f32 = 14.0;
const COMMIT_H: f32 = 27.0;
const RING: f32 = 16.0;

const ROW_H: f32 = 26.0;
const ZONE_TOP: f32 = 14.0;
const ZONE_H: f32 = 72.0;
const EYEBROW_H: f32 = 19.0;

const DONE_RING: f32 = 40.0;

const CHECK_H: f32 = 34.0;
const CHECK_GAP: f32 = 6.0;
const CHOICES_TOP: f32 = 12.0;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupFlow {
    Import,
    Create,
}


pub fn can_continue_word25(has_word: bool, bip39: &SecureString) -> bool {
    !has_word || bip39.trimmed_char_len() > 0
}

fn word25_done(word25: Option<bool>, bip39: &SecureString) -> bool {
    word25.is_some_and(|yes| can_continue_word25(yes, bip39))
}

fn storage_done(mode: Option<ImportMode>, key: &SecureString) -> bool {
    match mode {
        Some(ImportMode::Cold) => true,
        Some(ImportMode::Standard) => key.trimmed_char_len() >= KEY_MIN,
        None => false,
    }
}

pub fn phrase_done(flow: SetupFlow, seed: &SecureString, checked: bool) -> bool {
    let complete = phrase::count(seed.as_str()) == TARGET_WORDS;
    match flow {
        SetupFlow::Create => complete && checked,
        SetupFlow::Import => complete && mnemonic_checksum_ok(seed.as_str()),
    }
}

pub fn can_finish(
    flow:    SetupFlow,
    seed:    &SecureString,
    checked: bool,
    word25:  Option<bool>,
    w25:     &SecureString,
    mode:    Option<ImportMode>,
    key:     &SecureString,
) -> bool {
    phrase_done(flow, seed, checked) && word25_done(word25, w25) && storage_done(mode, key)
}


pub struct SetupScreenParams<'a> {
    pub flow: SetupFlow,
    pub chain: SetupChain,
    pub title: &'static str,
    pub mark: Mark,
    pub task: SetupTask,
    pub address_types: &'static [BtcScriptType],
    pub address_type: Option<BtcScriptType>,
    pub on_address_type: fn(BtcScriptType) -> Message,
    pub seed_field: SecureField,
    pub seed: &'a SecureString,
    pub seed_reveal: bool,
    pub copied: bool,
    pub on_generate: Option<Message>,
    pub on_copy: Option<Message>,
    pub word25: Option<bool>,
    pub w25_field: SecureField,
    pub w25: &'a SecureString,
    pub w25_reveal: bool,
    pub on_word25: fn(bool) -> Message,
    pub mode: Option<ImportMode>,
    pub enc_field: SecureField,
    pub enc: &'a SecureString,
    pub enc_reveal: bool,
    pub on_mode: fn(ImportMode) -> Message,
    pub on_back: Message,
    pub on_submit: Message,
}

impl SetupScreenParams<'_> {
    fn btc(&self) -> bool {
        !self.address_types.is_empty()
    }

    fn tasks(&self) -> Vec<SetupTask> {
        let mut tasks = vec![SetupTask::Phrase];
        if self.btc() {
            tasks.push(SetupTask::AddressType);
        }
        tasks.push(SetupTask::Word25);
        tasks.push(SetupTask::Storage);
        tasks
    }

    fn open(&self) -> SetupTask {
        if self.tasks().contains(&self.task) { self.task } else { SetupTask::Phrase }
    }

    fn done(&self, state: &AppState, task: SetupTask) -> bool {
        match task {
            SetupTask::Phrase => phrase_done(self.flow, self.seed, state.create_checked),
            SetupTask::AddressType => self.address_type.is_some_and(|t| self.address_types.contains(&t)),
            SetupTask::Word25 => word25_done(self.word25, self.w25),
            SetupTask::Storage => storage_done(self.mode, self.enc),
        }
    }
}


pub fn setup_screen<'a>(state: &'a AppState, prm: SetupScreenParams<'a>) -> Element<'a, Message> {
    let cp = theme::compact(&state.theme);
    let s = state.scale();

    let frame_w = (FRAME_MAX * s).min(state.window_width - FRAME_MARGIN).max((SIDE_W + TASK_W) * s);
    let main_inner = frame_w - SIDE_W * s - 1.0 - 2.0 * MAIN_PAD_H * s;
    let phrase_w = (PHRASE_W * s).min(main_inner);
    let m = phrase_entry::setup(cp, s);
    let cols = PhraseGrid::<Message>::columns(&m.look, phrase_w);

    let tasks = prm.tasks();
    let side_h = side_height(tasks.len()) * s;
    let phrase_h = EYEBROW_H * s + phrase_entry::height(&m, phrase_w);
    let panes_h = side_h.max(phrase_h + 2.0 * MAIN_PAD_V * s);

    let open = prm.open();
    let main: Element<'a, Message> = match open {
        SetupTask::Phrase => match prm.flow {
            SetupFlow::Import => import_phrase(state, &prm, &m, phrase_w, cp, s),
            SetupFlow::Create => create_phrase(state, &prm, &m, phrase_w, cols, cp, s),
        },
        SetupTask::AddressType => address_task(&prm, cp, s),
        SetupTask::Word25 => word25_task(&prm, cp, s),
        SetupTask::Storage => storage_task(state, &prm, cp, s),
    };

    let tools: Option<Element<'a, Message>> = match (open, prm.flow) {
        (SetupTask::Phrase, SetupFlow::Import) => Some(
            tool(eye_icon(prm.seed_reveal), !prm.seed_reveal, Some(reveal_msg(prm.seed_field)), cp, s),
        ),
        (SetupTask::Phrase, SetupFlow::Create)
            if phrase::count(prm.seed.as_str()) == TARGET_WORDS && state.create_check.is_none() =>
        {
            let copy = if prm.copied {
                done_tool(cp, s)
            } else {
                tool(icons::COPY.clone(), false, prm.on_copy.clone(), cp, s)
            };
            Some(
                row![
                    tool(eye_icon(prm.seed_reveal), !prm.seed_reveal, Some(reveal_msg(prm.seed_field)), cp, s),
                    Space::new().width(2.0 * s),
                    copy,
                ]
                .align_y(Alignment::Center)
                .boxed(),
            )
        }
        _ => None,
    };
    let keys: &[(&str, &str)] = match (open, prm.flow) {
        (SetupTask::Phrase, SetupFlow::Import) => IMPORT_KEYS,
        _ => &[],
    };

    let side = side_pane(state, &prm, &tasks, open, cp, s);
    let panes = row![
        container(side)
            .width(Length::Fixed(SIDE_W * s))
            .height(Length::Fixed(panes_h))
            .padding(Padding::new(0.0).top(SIDE_PAD_V * s).bottom(SIDE_PAD_V * s).left(SIDE_PAD_H * s).right(SIDE_PAD_H * s)),
        compact::vrule(cp.rule, panes_h),
        container(main)
            .width(Length::Fill)
            .height(Length::Fixed(panes_h))
            .padding(Padding::new(0.0).top(MAIN_PAD_V * s).bottom(MAIN_PAD_V * s).left(MAIN_PAD_H * s).right(MAIN_PAD_H * s))
            .center_x(Length::Fill)
            .center_y(Length::Fixed(panes_h)),
    ];

    let frame = column![header(prm.title, keys, tools, cp, s), panes].width(Length::Fixed(frame_w));

    stack![
        container(frame)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
        container(compact::back_chevron(prm.on_back.clone(), cp, s))
            .padding(Padding::new(0.0).top(compact::CORNER_TOP * s).left(compact::CORNER_LEFT * s)),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

fn side_height(tasks: usize) -> f32 {
    2.0 * SIDE_PAD_V
        + ORBIT_TOP
        + ORBIT
        + PATH_TOP
        + PATH * 1.3
        + TASKS_TOP
        + tasks as f32 * TASK_H
        + tasks.saturating_sub(1) as f32 * TASK_GAP
        + COMMIT_TOP
        + COMMIT_H
}

fn reveal_msg(field: SecureField) -> Message {
    Message::SecureEdit(field, SecureOp::ToggleReveal)
}

fn eye_icon(revealed: bool) -> svg::Handle {
    if revealed { icons::EYE_CLOSED.clone() } else { icons::EYE_OPEN.clone() }
}


fn header<'a>(
    title: &str,
    keys:  &[(&str, &str)],
    tools: Option<Element<'a, Message>>,
    cp:    &'static theme::CompactPalette,
    s:     f32,
) -> Element<'a, Message> {
    let mut right: Row<Element<'a, Message>> = row![].align_y(Alignment::Center);
    if !keys.is_empty() {
        let mut runs: Vec<Run> = vec![("SHORTCUTS".to_string(), cp.muted)];
        for (key, action) in keys {
            runs.push((format!("   {}", key.to_uppercase()), cp.dim));
            runs.push((format!(" {}", action.to_uppercase()), cp.muted));
        }
        right = right.push(compact::mono_runs(runs, compact::EYEBROW * s));
    }
    if let Some(tools) = tools {
        right = right
            .push(Space::new().width(12.0 * s).boxed())
            .push(compact::vrule(cp.rule, TOOL_H * s))
            .push(Space::new().width(9.0 * s).boxed())
            .push(tools);
    }
    column![
        container(
            row![
                text(title.to_string()).size(compact::TITLE * s).color(cp.text),
                Space::new().width(Length::Fill),
                right,
            ]
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fixed((TOOL_H + 7.0) * s))
        .padding(Padding::new(0.0).left(16.0 * s).right(16.0 * s).bottom(7.0 * s)),
        compact::hairline(cp.rule),
    ]
    .width(Length::Fill)
    .boxed()
}

fn tool<'a>(
    icon:    svg::Handle,
    pressed: bool,
    msg:     Option<Message>,
    cp:      &'static theme::CompactPalette,
    s:       f32,
) -> Element<'a, Message> {
    let live = msg.is_some();
    let glyph = svg(icon)
        .width(Length::Fixed(TOOL_ICON * s))
        .height(Length::Fixed(TOOL_ICON * s))
        .style(move |_, status| svg::Style {
            color: Some(if !live {
                cp.faint
            } else if pressed || matches!(status, svg::Status::Hovered) {
                cp.text
            } else {
                cp.muted
            }),
        });
    button(container(glyph).center(Length::Fill))
        .width(Length::Fixed(TOOL_W * s))
        .height(Length::Fixed(TOOL_H * s))
        .padding(0)
        .on_press_maybe(msg)
        .style(move |_, status| {
            let hot = live && matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if pressed { cp.pill } else if hot { cp.hover } else { Color::TRANSPARENT }.into()),
                border: Border { radius: (5.0 * s).into(), ..Default::default() },
                text_color: cp.text,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}

fn done_tool<'a>(cp: &'static theme::CompactPalette, s: f32) -> Element<'a, Message> {
    container(text("\u{2713}").size(TOOL_ICON * s).color(cp.green))
        .center_x(Length::Fixed(TOOL_W * s))
        .center_y(Length::Fixed(TOOL_H * s))
        .boxed()
}


fn side_pane<'a>(
    state: &'a AppState,
    prm:   &SetupScreenParams<'a>,
    tasks: &[SetupTask],
    open:  SetupTask,
    cp:    &'static theme::CompactPalette,
    s:     f32,
) -> Element<'a, Message> {
    let done = tasks.iter().filter(|t| prm.done(state, **t)).count();

    let mut list: Column<Element<'a, Message>> = column![].spacing(TASK_GAP * s);
    for task in tasks {
        list = list.push(task_row(state, prm, *task, *task == open, cp, s));
    }

    column![
        Space::new().height(ORBIT_TOP * s),
        container(progress_orbit(prm.mark, done, tasks.len(), cp, s)).center_x(Length::Fill),
        Space::new().height(PATH_TOP * s),
        container(path_line(prm, cp, s)).center_x(Length::Fill),
        Space::new().height(TASKS_TOP * s),
        list,
        Space::new().height(Length::Fill),
        compact::cta(prm.title, done == tasks.len(), prm.on_submit.clone(), cp, s),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

fn path_line<'a>(prm: &SetupScreenParams<'a>, cp: &'static theme::CompactPalette, s: f32) -> Element<'a, Message> {
    let runs: Vec<Run> = if !prm.btc() {
        vec![("XRP".to_string(), cp.dim), (format!(" \u{b7} {}", crate::bridge::XRP_PATH), cp.muted)]
    } else {
        match prm.address_type {
            Some(t) => vec![("BTC".to_string(), cp.dim), (format!(" \u{b7} {}", t.path()), cp.muted)],
            None => vec![
                ("BTC".to_string(), cp.dim),
                (" \u{b7} m/".to_string(), cp.muted),
                ("\u{b7}\u{b7}".to_string(), cp.faint),
                ("'/0'/0'/0/0".to_string(), cp.muted),
            ],
        }
    };
    compact::mono_runs(runs, PATH * s)
}

fn task_row<'a>(
    state: &'a AppState,
    prm:   &SetupScreenParams<'a>,
    task:  SetupTask,
    open:  bool,
    cp:    &'static theme::CompactPalette,
    s:     f32,
) -> Element<'a, Message> {
    let done = prm.done(state, task);
    let (title, sub) = task_copy(state, prm, task, cp, s);

    let ring = container(text(if done { "\u{2713}" } else { "" }).size(9.0 * s).color(cp.green))
        .width(Length::Fixed(RING * s))
        .height(Length::Fixed(RING * s))
        .center(Length::Fixed(RING * s))
        .style(move |_| container::Style {
            border: Border { color: if done { cp.green } else { cp.border }, width: 1.5, radius: (RING / 2.0 * s).into() },
            ..Default::default()
        });

    let words = column![
        text(title).size(12.0 * s).line_height(LineHeight::Absolute((15.0 * s).into())).color(cp.text),
        sub,
    ]
    .spacing(2.0 * s)
    .width(Length::Fill);

    button(
        row![ring, words, text("\u{203a}").font(MONO).size(10.0 * s).color(cp.faint)]
            .spacing(11.0 * s)
            .align_y(Alignment::Center)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fixed(TASK_H * s))
    .padding(Padding::new(0.0).left(10.0 * s).right(10.0 * s))
    .on_press(Message::SetupTaskOpened(prm.chain, task))
    .style(move |_, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if open { cp.pill } else if hot { cp.hover } else { Color::TRANSPARENT }.into()),
            border: Border { radius: (7.0 * s).into(), ..Default::default() },
            text_color: cp.text,
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}

fn task_copy<'a>(
    state: &'a AppState,
    prm:   &SetupScreenParams<'a>,
    task:  SetupTask,
    cp:    &'static theme::CompactPalette,
    s:     f32,
) -> (&'static str, Element<'a, Message>) {
    let plain = |words: String, ink: Color| -> Element<'a, Message> {
        text(words)
            .size(10.0 * s)
            .line_height(LineHeight::Absolute((13.0 * s).into()))
            .wrapping(Wrapping::None)
            .ellipsis(Ellipsis::End)
            .color(ink)
            .boxed()
    };
    match task {
        SetupTask::Phrase => {
            let count = phrase::count(prm.seed.as_str());
            let sub = match prm.flow {
                SetupFlow::Create if state.create_checked && count == TARGET_WORDS => plain("24 words \u{b7} checked".into(), cp.dim),
                SetupFlow::Create => plain("Generate and write down 24 words".into(), cp.dim),
                SetupFlow::Import if count == TARGET_WORDS && mnemonic_checksum_ok(prm.seed.as_str()) => {
                    plain("24 words \u{b7} checksum valid".into(), cp.dim)
                }
                SetupFlow::Import if count == TARGET_WORDS => plain("Checksum doesn't match".into(), cp.red),
                SetupFlow::Import => plain("Enter your 24 words".into(), cp.dim),
            };
            ("Recovery phrase", sub)
        }
        SetupTask::AddressType => {
            let sub = match prm.address_type.filter(|t| prm.address_types.contains(t)) {
                Some(t) => {
                    let spans: [Span<'a, ()>; 2] = [
                        span(t.prefix()).font(MONO).size(10.0 * s).color(cp.dim),
                        span(format!(" \u{b7} {}", t.name())).size(10.0 * s).color(cp.dim),
                    ];
                    rich_text(spans)
                        .line_height(LineHeight::Absolute((13.0 * s).into()))
                        .wrapping(Wrapping::None)
                        .boxed()
                }
                None if prm.flow == SetupFlow::Create => {
                    let offered: Vec<&str> = prm.address_types.iter().map(|t| t.prefix()).collect();
                    plain(format!("Choose {}", offered.join(" or ")), cp.dim)
                }
                None => plain("Choose the one your addresses start with".into(), cp.dim),
            };
            ("Address type", sub)
        }
        SetupTask::Word25 => {
            let sub = match prm.word25 {
                None => "Choose yes or no",
                Some(false) => "No",
                Some(true) if prm.w25.trimmed_char_len() > 0 => "Yes",
                Some(true) => "Yes \u{b7} type the 25th word",
            };
            ("25th word", plain(sub.into(), cp.dim))
        }
        SetupTask::Storage => {
            let sub = match prm.mode {
                None => "Choose where the key is kept".to_string(),
                Some(ImportMode::Cold) => "Cold storage".to_string(),
                Some(ImportMode::Standard) if prm.enc.trimmed_char_len() >= KEY_MIN => "On-device \u{b7} AES-256-GCM".to_string(),
                Some(ImportMode::Standard) => format!("On-device \u{b7} {}", minimum_hint(KEY_MIN, "characters")),
            };
            ("Key storage", plain(sub, cp.dim))
        }
    }
}


fn progress_orbit<'a>(mark: Mark, done: usize, total: usize, cp: &'static theme::CompactPalette, s: f32) -> Element<'a, Message> {
    let size = Length::Fixed(ORBIT * s);
    let glyph: Element<'a, Message> = match mark {
        Mark::Btc => text("\u{20bf}")
            .font(LIGHT)
            .size(20.0 * s)
            .line_height(LineHeight::Relative(1.0))
            .color(cp.dim)
            .boxed(),
        Mark::Xrp => svg(icons::XRP.clone())
            .width(Length::Fixed(14.0 * s))
            .height(Length::Fixed(14.0 * s * 424.0 / 512.0))
            .style(move |_, _| svg::Style { color: Some(cp.dim) })
            .boxed(),
    };
    let program = Progress {
        done,
        total,
        track: Color { a: 0.32, ..cp.faint },
        arc: Color { a: 0.85, ..cp.green },
        plate: cp.border_soft,
        unit: ORBIT_UNIT * s,
        scale: s,
    };
    stack![canvas(program).width(size).height(size), container(glyph).center(size)].boxed()
}

struct Progress {
    done:  usize,
    total: usize,
    track: Color,
    arc:   Color,
    plate: Color,
    unit:  f32,
    scale: f32,
}

impl cnvs::Program<Message> for Progress {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<iced::Renderer>> {
        let u = self.unit;
        let mut frame = Frame::new(renderer, bounds.size());
        let c = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let r = 90.0 * u;
        orbit::track(&mut frame, c, r, 2.0 * u, 8.0 * u, self.track);

        let total = self.total.max(1);
        let width = (2.9 * u).max(1.0);
        for i in 0..self.done.min(total) {
            let (start, sweep) = orbit::segment(i, total);
            orbit::arc(&mut frame, c, r, start, sweep, self.arc, width);
        }
        if self.done < total {
            orbit::arc(&mut frame, c, r, orbit::segment(self.done, total).0, orbit::SEG_HEAD, self.arc, width);
        }

        frame.stroke(
            &Path::circle(c, 23.0 * self.scale - 0.5),
            Stroke::default().with_color(self.plate).with_width(1.0),
        );
        vec![frame.into_geometry()]
    }
}


fn eyebrow<'a>(left: &str, right: &str, done: bool, cp: &'static theme::CompactPalette, s: f32) -> Element<'a, Message> {
    let mut r: Row<Element<'a, Message>> = row![
        text(left.to_uppercase()).font(MONO).size(compact::EYEBROW * s).color(cp.muted),
        Space::new().width(Length::Fill),
        text(right.to_uppercase()).font(MONO).size(compact::EYEBROW * s).color(cp.muted),
    ]
    .align_y(Alignment::Center);
    if done {
        r = r
            .push(Space::new().width(7.0 * s).boxed())
            .push(text("\u{2713}").size(compact::EYEBROW * s).color(cp.green).boxed());
    }
    container(r)
        .width(Length::Fill)
        .height(Length::Fixed(EYEBROW_H * s))
        .padding(Padding::new(0.0).bottom(8.0 * s))
        .boxed()
}

fn line_button<'a>(label: &str, msg: Message, cp: &'static theme::CompactPalette, s: f32) -> Element<'a, Message> {
    button(text(label.to_string()).size(compact::SEG * s).color(cp.text))
        .padding(Padding::new(0.0).top(5.0 * s).bottom(5.0 * s).left(12.0 * s).right(12.0 * s))
        .on_press(msg)
        .style(move |_, status| {
            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if hot { cp.hover } else { cp.neutral }.into()),
                border: Border { color: cp.border, width: 1.0, radius: (6.0 * s).into() },
                text_color: cp.text,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}


fn import_phrase<'a>(
    state:    &'a AppState,
    prm:      &SetupScreenParams<'a>,
    m:        &Metrics,
    phrase_w: f32,
    cp:       &'static theme::CompactPalette,
    s:        f32,
) -> Element<'a, Message> {
    let entry = Entry {
        field: prm.seed_field,
        seed: prm.seed,
        revealed: prm.seed_reveal,
        line: &state.phrase_line,
        width: phrase_w,
        eye: false,
        autofocus: true,
    };
    column![
        eyebrow("recovery phrase", "bip-39 \u{b7} 24", phrase_done(prm.flow, prm.seed, false), cp, s),
        phrase_entry::view(entry, m, cp, s),
    ]
    .width(Length::Fixed(phrase_w))
    .boxed()
}


fn create_phrase<'a>(
    state:    &'a AppState,
    prm:      &SetupScreenParams<'a>,
    m:        &Metrics,
    phrase_w: f32,
    cols:     usize,
    cp:       &'static theme::CompactPalette,
    s:        f32,
) -> Element<'a, Message> {
    let words: Vec<&str> = prm.seed.as_str().split_whitespace().collect();
    let generated = words.len() == TARGET_WORDS;

    if let (true, Some(check)) = (generated, state.create_check.as_ref()) {
        return check_view(&words, check, cp, s);
    }

    let checked = state.create_checked && generated;
    if checked && !prm.seed_reveal {
        return checked_view(m, phrase_w, cols, cp, s);
    }
    let left: Element<'a, Message> = if !generated {
        phrase_entry::msg("Nothing generated yet".into(), cp.muted, m.text)
    } else if checked {
        phrase_entry::msg("\u{2713} checked".into(), cp.green, m.text)
    } else if let Some(p) = state.create_mismatch {
        phrase_entry::msg(format!("Word {} didn't match. Check your copy.", p + 1), cp.red, m.text)
    } else {
        phrase_entry::count_text(TARGET_WORDS, None, m.text, cp)
    };
    let right: Element<'a, Message> = match (generated, checked, prm.on_generate.clone()) {
        (false, _, Some(msg)) => line_button("Generate", msg, cp, s),
        (true, false, _) => line_button("I've written them down", Message::SetupCheckStarted, cp, s),
        _ => Space::new().boxed(),
    };

    column![
        eyebrow("recovery phrase", "bip-39 \u{b7} 24", checked, cp, s),
        word_grid(&words, !prm.seed_reveal, cols, phrase_w, &m.look, cp),
        Space::new().height(m.line_top),
        row![left, Space::new().width(Length::Fill), right]
            .align_y(Alignment::Center)
            .height(Length::Fixed(m.line_h)),
    ]
    .width(Length::Fixed(phrase_w))
    .boxed()
}

fn checked_view<'a>(
    m:        &Metrics,
    phrase_w: f32,
    cols:     usize,
    cp:       &'static theme::CompactPalette,
    s:        f32,
) -> Element<'a, Message> {
    let grid_h = PhraseGrid::<Message>::height(&m.look, cols);
    let ring = container(text("\u{2713}").size(16.0 * s).color(cp.green))
        .center_x(Length::Fixed(DONE_RING * s))
        .center_y(Length::Fixed(DONE_RING * s))
        .style(move |_| container::Style {
            border: Border { color: cp.green, width: 1.5, radius: (DONE_RING / 2.0 * s).into() },
            ..Default::default()
        });
    column![
        eyebrow("recovery phrase", "bip-39 \u{b7} 24", true, cp, s),
        container(
            column![
                ring,
                Space::new().height(10.0 * s),
                text("24 words \u{b7} checked").size(11.5 * s).color(cp.dim),
            ]
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fixed(grid_h)),
        Space::new().height(m.line_top + m.line_h),
    ]
    .width(Length::Fixed(phrase_w))
    .boxed()
}

fn word_grid<'a>(
    words:   &[&str],
    masked:  bool,
    cols:    usize,
    block_w: f32,
    look:    &Look,
    cp:      &'static theme::CompactPalette,
) -> Element<'a, Message> {
    let rows = SLOTS / cols;
    let col_w = (block_w - (cols - 1) as f32 * look.col_gap) / cols as f32;
    let mut grid: Row<Element<'a, Message>> = row![].spacing(look.col_gap);
    for c in 0..cols {
        let mut col: Column<Element<'a, Message>> = column![].spacing(look.row_gap).width(Length::Fixed(col_w));
        for r in 0..rows {
            let k = c * rows + r;
            let word: Element<'a, Message> = match words.get(k) {
                Some(w) if !masked => text(w.to_string()).font(MONO).size(look.word).color(cp.text).boxed(),
                _ => text(MASK).font(MONO).size(look.word).color(cp.faint).boxed(),
            };
            col = col.push(column![
                container(
                    row![
                        container(text(format!("{:02}", k + 1)).font(MONO).size(look.index).color(cp.faint))
                            .width(Length::Fixed(look.index_w))
                            .align_x(Alignment::End),
                        Space::new().width(look.gap),
                        word,
                    ]
                    .align_y(Alignment::End),
                )
                .width(Length::Fill)
                .height(Length::Fixed(look.slot_h - 1.0))
                .padding(Padding::new(0.0).top(look.pad_top).left(look.pad_side).right(look.pad_side)),
                compact::hairline(cp.rule),
            ].boxed());
        }
        grid = grid.push(col.boxed());
    }
    grid.boxed()
}

fn check_view<'a>(
    words: &[&str],
    check: &crate::controller::app_state::PhraseCheck,
    cp:    &'static theme::CompactPalette,
    s:     f32,
) -> Element<'a, Message> {
    let a = check.answered.min(check.positions.len());
    let mut slots: Row<Element<'a, Message>> = row![].spacing(CHECK_GAP * s);
    for (j, &p) in check.positions.iter().enumerate() {
        let word: Element<'a, Message> = if j < a {
            text(words.get(p).copied().unwrap_or("").to_string()).font(MONO).size(12.5 * s).color(cp.text).boxed()
        } else {
            text(MASK).font(MONO).size(12.5 * s).color(cp.faint).boxed()
        };
        let current = j == a;
        slots = slots.push(
            container(
                row![
                    text(format!("{:02}", p + 1)).font(MONO).size(9.0 * s).color(cp.faint),
                    Space::new().width(7.0 * s),
                    word,
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fixed(CHECK_H * s))
            .center_x(Length::Fill)
            .center_y(Length::Fixed(CHECK_H * s))
            .style(move |_| container::Style {
                border: Border { color: if current { cp.focus } else { cp.border }, width: 1.0, radius: (6.0 * s).into() },
                ..Default::default()
            })
            .boxed(),
        );
    }

    let options = check.options.get(a).copied().unwrap_or([""; 4]);
    let choice = |i: usize| -> Element<'a, Message> {
        button(
            text(options[i].to_string())
                .font(MONO)
                .size(12.5 * s)
                .color(cp.text)
                .width(Length::Fill)
                .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fixed(CHECK_H * s))
        .padding(Padding::new(0.0).top(9.0 * s))
        .on_press(Message::SetupCheckPicked(i))
        .style(move |_, status| {
            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if hot { cp.hover } else { Color::TRANSPARENT }.into()),
                border: Border { color: cp.border, width: 1.0, radius: (6.0 * s).into() },
                text_color: cp.text,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
    };

    column![
        eyebrow("check your copy", &format!("{a} of 3"), false, cp, s),
        slots,
        Space::new().height(CHOICES_TOP * s),
        column![
            row![choice(0), choice(1)].spacing(CHECK_GAP * s),
            row![choice(2), choice(3)].spacing(CHECK_GAP * s),
        ]
        .spacing(CHECK_GAP * s),
    ]
    .width(Length::Fixed(TASK_W * s))
    .boxed()
}


struct ListRow {
    prefix: Option<&'static str>,
    name:   &'static str,
    detail: Option<&'static str>,
    on:     bool,
    msg:    Message,
}

fn list<'a>(rows: Vec<ListRow>, cp: &'static theme::CompactPalette, s: f32) -> Element<'a, Message> {
    let plain = rows.iter().all(|r| r.prefix.is_none());
    let mut col: Column<Element<'a, Message>> = column![].spacing(2.0 * s).width(Length::Fill);
    for r in rows {
        let ink = if r.on { cp.text } else { cp.dim };
        let mut line: Row<Element<'a, Message>> = row![].spacing(10.0 * s).align_y(Alignment::Center);
        if let Some(prefix) = r.prefix {
            line = line.push(
                container(text(prefix).font(MONO).size(12.0 * s).color(ink)).width(Length::Fixed(34.0 * s)).boxed(),
            );
        }
        line = line
            .push(text(r.name).size(if plain { 11.5 } else { 11.0 } * s).color(ink).boxed())
            .push(Space::new().width(Length::Fill).boxed());
        if let Some(detail) = r.detail {
            line = line.push(text(detail).font(MONO).size(9.5 * s).color(cp.muted).boxed());
        }
        let on = r.on;
        col = col.push(
            button(line.height(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fixed(ROW_H * s))
                .padding(Padding::new(0.0).left(8.0 * s).right(8.0 * s))
                .on_press(r.msg)
                .style(move |_, status| {
                    let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
                    button::Style {
                        background: Some(if on { cp.pill } else if hot { cp.hover } else { Color::TRANSPARENT }.into()),
                        border: Border { radius: (6.0 * s).into(), ..Default::default() },
                        text_color: ink,
                        shadow: Shadow::default(),
                        snap: false,
                    }
                })
                .boxed(),
        );
    }
    col.boxed()
}

fn task_block<'a>(
    head: Element<'a, Message>,
    body: Element<'a, Message>,
    zone: Option<Element<'a, Message>>,
    s:    f32,
) -> Element<'a, Message> {
    let inset = |e: Element<'a, Message>, h: Length| {
        container(e)
            .width(Length::Fill)
            .height(h)
            .padding(Padding::new(0.0).left(LIST_BLEED * s).right(LIST_BLEED * s))
            .boxed()
    };
    let mut col: Column<Element<'a, Message>> =
        column![inset(head, Length::Fit), body].width(Length::Fixed((TASK_W + 2.0 * LIST_BLEED) * s));
    if let Some(zone) = zone {
        col = col
            .push(Space::new().height(ZONE_TOP * s).boxed())
            .push(inset(zone, Length::Fixed(ZONE_H * s)));
    }
    col.boxed()
}

fn address_task<'a>(prm: &SetupScreenParams<'a>, cp: &'static theme::CompactPalette, s: f32) -> Element<'a, Message> {
    let chosen = prm.address_type.filter(|t| prm.address_types.contains(t));
    let rows = prm
        .address_types
        .iter()
        .map(|t| ListRow {
            prefix: Some(t.prefix()),
            name: t.name(),
            detail: Some(short_path(t.path())),
            on: chosen == Some(*t),
            msg: (prm.on_address_type)(*t),
        })
        .collect();
    task_block(
        eyebrow("address type", chosen.map_or("required", |t| t.bip()), chosen.is_some(), cp, s),
        list(rows, cp, s),
        None,
        s,
    )
}

fn short_path(path: &'static str) -> &'static str {
    let end = path.match_indices('/').nth(1).map_or(path.len(), |(i, _)| i);
    &path[..end]
}

fn word25_task<'a>(prm: &SetupScreenParams<'a>, cp: &'static theme::CompactPalette, s: f32) -> Element<'a, Message> {
    let rows = vec![
        ListRow { prefix: None, name: "No", detail: None, on: prm.word25 == Some(false), msg: (prm.on_word25)(false) },
        ListRow { prefix: None, name: "Yes", detail: None, on: prm.word25 == Some(true), msg: (prm.on_word25)(true) },
    ];
    let zone = (prm.word25 == Some(true)).then(|| {
        column![
            compact::boxed_secret(
                prm.w25_field,
                prm.w25,
                prm.w25_reveal,
                super::grid::BIP39_PLACEHOLDER,
                TASK_W,
                Some(prm.on_submit.clone()),
                cp,
                s,
            ),
            Space::new().height(8.0 * s),
            compact::meter_rows(entropy::bits(prm.w25.as_str().trim()), entropy::Kdf::Bip39, cp, s),
        ]
        .boxed()
    });
    task_block(
        eyebrow("25th word", "optional", word25_done(prm.word25, prm.w25), cp, s),
        list(rows, cp, s),
        Some(zone.unwrap_or_else(|| Space::new().boxed())),
        s,
    )
}

fn storage_task<'a>(
    state: &'a AppState,
    prm:   &SetupScreenParams<'a>,
    cp:    &'static theme::CompactPalette,
    s:     f32,
) -> Element<'a, Message> {
    let rows = vec![
        ListRow {
            prefix: None,
            name: "On-device",
            detail: Some("AES-256-GCM \u{b7} Argon2id"),
            on: prm.mode == Some(ImportMode::Standard),
            msg: (prm.on_mode)(ImportMode::Standard),
        },
        ListRow {
            prefix: None,
            name: "Cold storage",
            detail: Some("off-device"),
            on: prm.mode == Some(ImportMode::Cold),
            msg: (prm.on_mode)(ImportMode::Cold),
        },
    ];
    let zone: Element<'a, Message> = match prm.mode {
        Some(ImportMode::Standard) => {
            let hint = minimum_hint(KEY_MIN, "characters");
            column![
                compact::boxed_secret(prm.enc_field, prm.enc, prm.enc_reveal, &hint, TASK_W, Some(prm.on_submit.clone()), cp, s),
                Space::new().height(8.0 * s),
                compact::meter_rows(held_bits(prm.enc, state.entropy_hold), entropy::Kdf::Argon2id, cp, s),
            ]
            .boxed()
        }
        Some(ImportMode::Cold) | None => Space::new().boxed(),
    };
    task_block(
        eyebrow("key storage", "required", storage_done(prm.mode, prm.enc), cp, s),
        list(rows, cp, s),
        Some(zone),
        s,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> SecureString {
        SecureString::new(s.to_string())
    }

    const VALID: &str = "abandon abandon abandon abandon abandon abandon abandon abandon \
                         abandon abandon abandon abandon abandon abandon abandon abandon \
                         abandon abandon abandon abandon abandon abandon abandon art";

    #[test]
    fn a_held_reading_survives_submit_but_not_a_retry() {
        let typed = text("correct horse battery staple");
        let empty = text("");
        let live = entropy::bits(typed.as_str());

        assert_eq!(held_bits(&typed, None), live);
        assert_eq!(held_bits(&empty, None), 0.0);
        assert_eq!(held_bits(&empty, Some(live)), live);
        assert_eq!(held_bits(&typed, Some(1.0)), live);
    }

    #[test]
    fn every_crack_reading_fits_the_task_block() {
        use crate::ui::components::tui::MONO_ADVANCE;
        let budget = ((TASK_W - compact::METER_KEY_W) / (compact::METER * MONO_ADVANCE)).floor() as usize;
        for kdf in [entropy::Kdf::Argon2id, entropy::Kdf::Bip39] {
            for tenths in 0..=4000 {
                let (runs, _) = compact::crack_runs(tenths as f64 / 10.0, kdf, &theme::COMPACT_OBSIDIAN);
                let n = crate::ui::components::tui::n_chars(&runs);
                assert!(n <= budget, "{kdf:?} at {} bits draws {n} columns in a {budget} budget", tenths as f64 / 10.0);
            }
        }
    }

    #[test]
    fn nothing_is_chosen_until_someone_chooses() {
        let key = text(&"k".repeat(KEY_MIN));
        let empty = text("");
        let seed = text(VALID);
        assert!(!can_finish(SetupFlow::Import, &seed, false, None, &empty, Some(ImportMode::Standard), &key));
        assert!(!can_finish(SetupFlow::Import, &seed, false, Some(false), &empty, None, &key));
        assert!(can_finish(SetupFlow::Import, &seed, false, Some(false), &empty, Some(ImportMode::Standard), &key));
    }

    #[test]
    fn each_task_gates_on_its_own_rule() {
        let seed = text(VALID);
        let swapped = text(&VALID.replace("art", "zoo"));
        let empty = text("");
        let word = text("w");
        let key = text(&"k".repeat(KEY_MIN));
        let short = text(&"k".repeat(KEY_MIN - 1));
        use ImportMode::{Cold, Standard};
        assert!(can_finish(SetupFlow::Import, &seed, false, Some(true), &word, Some(Cold), &empty));
        assert!(!can_finish(SetupFlow::Import, &swapped, false, Some(false), &empty, Some(Standard), &key));
        assert!(!can_finish(SetupFlow::Import, &seed, false, Some(true), &empty, Some(Standard), &key));
        assert!(!can_finish(SetupFlow::Import, &seed, false, Some(false), &empty, Some(Standard), &short));
        assert!(can_finish(SetupFlow::Create, &swapped, true, Some(false), &empty, Some(Cold), &empty));
        assert!(!can_finish(SetupFlow::Create, &seed, false, Some(false), &empty, Some(Cold), &empty));
        assert!(!can_finish(SetupFlow::Create, &empty, true, Some(false), &empty, Some(Cold), &empty));
    }

    #[test]
    fn the_word25_gate_refuses_an_empty_yes() {
        assert!(can_continue_word25(false, &text("")));
        assert!(can_continue_word25(false, &text("x")));
        assert!(!can_continue_word25(true, &text("")));
        assert!(can_continue_word25(true, &text("1")));
        assert!(!can_continue_word25(true, &text("      ")));
        assert!(can_continue_word25(true, &text("  1  ")));
        assert!(!word25_done(None, &text("x")));
    }

    #[test]
    fn the_checksum_gate_tells_a_phrase_from_a_typo() {
        assert!(mnemonic_checksum_ok(VALID));
        assert!(mnemonic_checksum_ok(&format!("  {VALID}\n")), "whitespace broke the parse");
        assert!(!mnemonic_checksum_ok(&VALID.replace("art", "zoo")));
        assert!(!mnemonic_checksum_ok(""));
    }

    #[test]
    fn the_checklist_is_the_tallest_part_of_the_frame() {
        let m = phrase_entry::setup(&theme::COMPACT_OBSIDIAN, 1.0);
        let phrase = EYEBROW_H + phrase_entry::height(&m, PHRASE_W) + 2.0 * MAIN_PAD_V;
        let choice = EYEBROW_H + 2.0 * ROW_H + 2.0 + ZONE_TOP + ZONE_H + 2.0 * MAIN_PAD_V;
        for tasks in [3, 4] {
            assert!(side_height(tasks) >= phrase, "{tasks} tasks: the phrase block outgrows the checklist");
            assert!(side_height(tasks) >= choice, "{tasks} tasks: a choice block outgrows the checklist");
        }
    }

    #[test]
    fn the_phrase_block_takes_four_columns_at_full_width() {
        let m = phrase_entry::setup(&theme::COMPACT_OBSIDIAN, 1.0);
        assert_eq!(PhraseGrid::<Message>::columns(&m.look, PHRASE_W), 4);
    }

    #[test]
    fn the_sign_panes_step_down_to_fewer_columns() {
        let m = phrase_entry::pane(&theme::COMPACT_OBSIDIAN, 1.0);
        assert_eq!(PhraseGrid::<Message>::columns(&m.look, 400.0), 4);
        assert_eq!(PhraseGrid::<Message>::columns(&m.look, 300.0), 3);
        assert_eq!(PhraseGrid::<Message>::columns(&m.look, 200.0), 2);
    }

    #[test]
    fn the_short_path_is_the_purpose() {
        assert_eq!(short_path(BtcScriptType::NativeSegwit.path()), "m/84'");
        assert_eq!(short_path(BtcScriptType::Legacy.path()), "m/44'");
    }
}
