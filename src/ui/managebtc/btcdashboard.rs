

use iced::widget::{Column, Row};
use iced::Widget as _;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame};
use iced::widget::pane_grid::Pane;
use iced::widget::{button, column, container, responsive, row, stack, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Point, Rectangle, Shadow, Size};

use crate::channel::{BtcNodeStats as NodeFrame, CHANNEL};
use crate::controller::app_state::{AppState, BtcFeeTier, ChartPeriod, EnableInputMode};
use crate::controller::message::{Message, PlainField, SecureField};
use crate::controller::panes::{Chain, GridMsg, PaneKind};
use crate::ui::components::grid::{
    self, drow, group_rule, label, rule_label, DashedWell, ADDR, AMT_GAP, EQ_W, FIELD_VALUE, HERO, HERO_UNIT, HINT, NA,
    ROW_KEY, ROW_TOTAL, STRIP,
};
use crate::ui::components::send_screen::{self as screen};
use crate::ui::components::signing::SignFields;
use crate::utils::{fiat_amount, money};
use crate::ui::components::compact;
use crate::ui::managebtc::node::{band_color, fee_band, tip_age, trim_rate, NodeFrameView};
use crate::ui::managebtc::btcsend;
use crate::ui::managebtc::panes as panes_ui;
use crate::utils::fonts::{LIGHT, MONO};
use crate::utils::sparkline::{change_pct, chart_data};
use crate::utils::theme::{self, CompactPalette};
use crate::utils::{add_commas, format_token_amount, format_usd, price};


const TIER_W: f32 = 30.0;
const RATE_W: f32 = 36.0;
const COST_W: f32 = 70.0;
const LADDER_GAP: f32 = 8.0;
const BAR_H: f32 = 6.0;

const BAR_MIN_FILL: f32 = 4.0;
const CHIP_GAP: f32 = 4.0;

const CARD_GAP: f32 = 4.0;
const CARD_PAD_V: f32 = 5.0;
const CARD_PAD_H: f32 = 4.0;
const CARD_RADIUS: f32 = 2.0;
const CARD_HEIGHT: f32 = 9.0;
const CARD_AGE: f32 = 7.5;
const TRAIN_CARDS: usize = 5;

const TAIL: usize = 5;
const UNIT_RESERVE: f32 = 23.0;


pub fn view(state: &AppState) -> Element<'_, Message> {
    let cp = theme::compact(&state.theme);
    let scale = state.scale();
    let host = grid::Host { chain: Chain::Btc, title: "BTC", grid: &state.btc_grid, wrap: Message::BtcGrid, bar: None };
    grid::screen(host, state, pane_content, cp, scale)
}

fn as_money(v: f32) -> String {
    money(v as f64)
}

fn pane_content<'a>(
    state: &'a AppState,
    pane: Pane,
    kind: PaneKind,
    maximized: bool,
    cp: &'static CompactPalette,
    scale: f32,
) -> grid::PaneContent<'a> {
    let strip = |word: &str| grid::strip(word, cp, scale);

    let (title, meta, body): (Element<'a, Message>, Option<Element<'a, Message>>, Element<'a, Message>) =
        match kind {
            PaneKind::Balance => (strip("balance"), None, balance_pane(state, cp, scale)),
            PaneKind::Fees => (
                strip("fees"),
                Some(text("sat/vB").font(MONO).size(STRIP * scale).color(cp.faint).boxed()),
                fees_pane(state, cp, scale),
            ),
            PaneKind::Chart => {
                let series = chart_series(state);
                let prices: Vec<f32> = series.iter().map(|p| p.1).collect();
                let ccy = state.base_currency.code();
                let (word, colour) = match change_pct(&prices) {
                    Some(p) => (format!("{p:+.2}%"), if p >= 0.0 { cp.green } else { cp.red }),
                    None => (NA.to_string(), cp.faint),
                };
                (
                    row![
                        strip("chart"),
                        Space::new().width(9.0 * scale),
                        grid::period_pill(state.btc_grid.chart_period, Message::BtcGrid, cp, scale),
                    ]
                    .align_y(Alignment::Center)
                    .boxed(),
                    Some(text(word).font(MONO).size(STRIP * scale).color(colour).boxed()),
                    grid::chart_pane(
                        series,
                        price::cross("BTC", ccy) as f32,
                        state.btc_grid.chart_period,
                        &format!("btc / {}", ccy.to_lowercase()),
                        as_money,
                        theme::is_dark(&state.theme),
                        cp,
                        scale,
                    ),
                )
            }
            PaneKind::Send => (strip("send"), None, send_pane(state, cp, scale)),
            PaneKind::Wallet => (strip(panes_ui::wallet::title(state)), None, panes_ui::wallet::view(state, cp, scale)),
            PaneKind::Receive => {
                let address = CHANNEL.bitcoin_wallet_rx.borrow().1.clone().unwrap_or_else(|| "No Address".to_string());
                let line_address = address.clone();
                (
                    strip("receive"),
                    None,
                    grid::receive_pane(
                        address.clone(),
                        address,
                        ADDR_TAIL,
                        state.btc_copy_feedback,
                        Message::BtcCopyAddress,
                        None,
                        move || receive_line(&line_address, cp, scale),
                        cp,
                        scale,
                    ),
                )
            }
            PaneKind::Blocks => (strip("blocks"), None, blocks_pane(state, cp, scale)),
            PaneKind::Transactions => (
                strip(panes_ui::transactions::title(state)),
                None,
                panes_ui::transactions::view(state, cp, scale),
            ),
            PaneKind::Mempool => (strip("mempool"), None, panes_ui::mempool::view(state, cp, scale)),
            PaneKind::Intervals => (strip("block intervals"), None, panes_ui::intervals::view(state, cp, scale)),
           
            PaneKind::Network
            | PaneKind::Ticket
            | PaneKind::Book
            | PaneKind::Depth
            | PaneKind::Orders
            | PaneKind::Tokens
            | PaneKind::AvailableTokens
            | PaneKind::Empty => (Space::new().boxed(), None, grid::well_pane(Message::BtcGrid, cp, scale)),
        };

    grid::pane_frame(pane, kind, maximized, title, meta, body, Message::BtcGrid, cp, scale)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BalanceFigures {
    confirmed: u64,
    unconfirmed: i64,
}

fn balance_figures(union: &[crate::channel::BtcUtxo]) -> BalanceFigures {
    let confirmed: u64 = union.iter().filter(|u| u.height > 0).map(|u| u.sats).sum();
    let created: u64 = union.iter().filter(|u| u.height == 0 && u.spent_by.is_none()).map(|u| u.sats).sum();
    let consumed: u64 = union.iter().filter(|u| u.height > 0 && u.spent_by.is_some()).map(|u| u.sats).sum();
    BalanceFigures { confirmed, unconfirmed: created as i64 - consumed as i64 }
}


fn figure_row<'a>(
    key: &'static str,
    runs: Vec<(String, Color)>,
    unit: &'static str,
    top: f32,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    container(
        row![
            text(key).size(ROW_KEY * scale).color(cp.dim),
            Space::new().width(Length::Fill),
            compact::mono_runs(runs, grid::ROW_VALUE * scale),
            unit_toggle(unit, cp, scale),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(top * scale).bottom(3.0 * scale))
    .boxed()
}

fn unit_toggle<'a>(word: &'static str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(text(word).font(MONO).size(grid::ROW_VALUE * scale))
        .on_press(Message::BtcBalanceUnitToggled)
        .padding(Padding::new(1.0 * scale).left(3.0 * scale).right(3.0 * scale))
        .style(move |_, status| {
            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: Some(if hot { cp.hover } else { Color::TRANSPARENT }.into()),
                border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (4.0 * scale).into() },
                text_color: if hot { cp.text } else { cp.faint },
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .boxed()
}

fn balance_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let hide = state.hide_balance;
    let ccy = state.base_currency.code();
    let rate = price::cross("BTC", ccy);
    let mask = |s: String| if hide { "\u{2022}".repeat(4) } else { s };

  
    let figures = balance_figures(&CHANNEL.btc_utxos_rx.borrow().1);
    let sats_mode = state.btc_balance_sats;
    let unit = if sats_mode { "sats" } else { "btc" };
    let amount = |sats: u64| if sats_mode { add_commas(sats as i64) } else { format!("{:.8}", sats as f64 / 1e8) };

    let hero_value = if hide {
        "\u{2022}".repeat(6)
    } else if rate > 0.0 {
        money(figures.confirmed as f64 / 1e8 * rate)
    } else {
        NA.to_string()
    };
    let hero = row![
        text(hero_value).font(LIGHT).size(HERO * scale).color(cp.text),
        Space::new().width(8.0 * scale),
        container(text(ccy.to_uppercase()).font(MONO).size(HERO_UNIT * scale).color(cp.muted))
            .padding(Padding::new(0.0).bottom(5.0 * scale)),
    ]
    .align_y(Alignment::End);

    let sign = match figures.unconfirmed {
        n if n < 0 => "\u{2212}",
        n if n > 0 => "+",
        _ => "",
    };
    let col = column![
        hero,
        figure_row("confirmed", vec![(mask(amount(figures.confirmed)), cp.text)], unit, 9.0, cp, scale),
        figure_row(
            "unconfirmed",
            vec![(mask(format!("{sign}{}", amount(figures.unconfirmed.unsigned_abs()))), cp.dim)],
            unit,
            3.0,
            cp,
            scale,
        ),
    ]
    .width(Length::Fill);

    grid::scroller(col.boxed(), cp, scale)
}


fn ladder_colour(tiers: &[f32; 4], i: usize, cp: &'static CompactPalette) -> Color {
    if tiers[i] >= tiers[3] {
        cp.green
    } else if tiers[i] <= tiers[0] {
        cp.red
    } else {
        cp.amber
    }
}

fn fees_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let node = NodeFrame::current();
    let tiers = node.tiers;
    let quotes = btcsend::tier_quotes(state);
    let ccy = state.base_currency.code();
    let rate = price::cross("BTC", ccy);

    let mut col: Column<Element<'_, Message>> = column![].width(Length::Fill);
    for (i, name) in ["min", "low", "med", "high"].into_iter().enumerate() {
        let (ratio, colour, rate_text, cost) = match tiers {
            Some(t) => {
                let cost = quotes.map(|q| q[i]).map(|sats| {
                    if rate > 0.0 {
                        format!("{} {}", format_usd(sats as f64 / 1e8 * rate), ccy.to_lowercase())
                    } else {
                        format!("{sats} sats")
                    }
                });
                let ratio = node.bands.and_then(|b| panes_ui::mempool::axis_position(&b, t[i]));
                (ratio, ladder_colour(&t, i, cp), trim_rate(t[i]), cost)
            }
            None => (None, cp.faint, NA.to_string(), None),
        };
        col = col.push(ladder_row(name, ratio, colour, rate_text, cost, cp, scale));
    }
    grid::scroller(col.boxed(), cp, scale)
}

fn ladder_row<'a>(
    name: &str,
    ratio: Option<f32>,
    colour: Color,
    rate: String,
    cost: Option<String>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let bar = Canvas::new(FeeBar { ratio, fill: colour, track: cp.pill, min_fill: BAR_MIN_FILL * scale })
        .width(Length::Fill)
        .height(Length::Fixed(BAR_H * scale));
    container(
        row![
            container(text(name.to_string()).size(ROW_KEY * scale).color(cp.dim)).width(Length::Fixed(TIER_W * scale)),
            bar,
            container(text(rate).font(MONO).size(grid::ROW_VALUE * scale).color(cp.text))
                .width(Length::Fixed(RATE_W * scale))
                .align_x(Alignment::End),
            container(
                text(cost.unwrap_or_else(|| NA.to_string()))
                    .font(MONO)
                    .size(9.0 * scale)
                    .color(cp.muted),
            )
            .width(Length::Fixed(COST_W * scale))
            .align_x(Alignment::End),
        ]
        .spacing(LADDER_GAP * scale)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(2.0 * scale).bottom(2.0 * scale))
    .boxed()
}


struct FeeBar {
    ratio: Option<f32>,
    fill: Color,
    track: Color,
    min_fill: f32,
}

impl<Message> canvas::Program<Message> for FeeBar {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;
        frame.fill_rectangle(Point::ORIGIN, Size::new(w, h), self.track);
        if let Some(r) = self.ratio {
            let fw = (w * r.clamp(0.0, 1.0)).round().max(self.min_fill.min(w));
            frame.fill_rectangle(Point::ORIGIN, Size::new(fw, h), self.fill);
        }
        vec![frame.into_geometry()]
    }
}


fn chart_series(state: &AppState) -> Vec<(u64, f32)> {
    let history = match state.btc_grid.chart_period {
        ChartPeriod::OneHour => &state.rate_history_short,
        ChartPeriod::OneDay => &state.rate_history_long,
    };
    chart_data("BTC", state.base_currency.code(), history)
}


fn send_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let ccy = state.base_currency.code();
        let (_, own_btc, key_deleted, key_mode) = CHANNEL.bitcoin_wallet_rx.borrow().clone();

        let addr = state.btc_send_recipient.trim();
        let is_self = !addr.is_empty() && own_btc.as_deref().is_some_and(|own| btcsend::same_address(own, addr));
        let addr_ok = !is_self && btcsend::is_valid(addr);
     
        let fault = btcsend::recipient_fault(addr);
        let rate = price::cross("BTC", ccy);
        let tier = btcsend::effective_tier(state);
        let fee = btcsend::resolved_fee(state);
        let floor = btcsend::floor_sats(state);
        let below_floor = fee.is_some_and(|f| f < floor);
        let fee_colour = btcsend::selected_colour(state, fee, cp);
        let amount = state.btc_send_amount.trim().parse::<f64>().ok().filter(|v| *v > 0.0);
        let insufficient = match (amount, fee) {
            (Some(a), Some(f)) => !btcsend::fits(a, f),
            _ => false,
        };
        let valid = addr_ok && amount.is_some() && fee.is_some_and(|f| f >= floor) && !insufficient;
        let fiat_of = |sats: u64| format!("{} {}", format_usd(sats as f64 / 1e8 * rate), ccy.to_lowercase());

      
        let hint: Option<Element<'a, Message>> = if is_self {
            Some(compact::mono_runs(vec![(btcsend::SELF_ERROR.to_string(), cp.red)], HINT * scale))
        } else {
            fault.map(|note| compact::mono_runs(vec![(note.to_string(), cp.red)], HINT * scale))
        };
        let mut recipient = column![
            label("recipient", cp, scale),
            screen::boxed_plain(
                PlainField::BtcSendRecipient,
                &state.btc_send_recipient,
                btcsend::PLACEHOLDER,
                inner,
                ADDR,
                true,
                None,
                0.0,
                None,
                0.0,
                None,
                cp,
                scale,
            ),
        ]
        .width(Length::Fill);
        if let Some(hint) = hint {
            recipient = recipient.push(Space::new().height(5.0 * scale).boxed()).push(hint);
        }

        let box_w = ((inner - 2.0 * AMT_GAP - EQ_W) / 2.0).max(60.0);
        let amount_row = row![
            screen::boxed_plain(
                PlainField::BtcSendAmount,
                &state.btc_send_amount,
                "0.00000000",
                box_w,
                FIELD_VALUE,
                false,
                None,
                0.0,
                Some(screen::unit("btc", cp, scale)),
                UNIT_RESERVE,
                None,
                cp,
                scale,
            ),
            Space::new().width(AMT_GAP * scale),
            text("=").font(MONO).size(grid::ROW_VALUE * scale).color(cp.faint),
            Space::new().width(AMT_GAP * scale),
            screen::boxed_plain(
                PlainField::BtcSendFiat,
                &state.btc_send_fiat_amount,
                "0.00",
                box_w,
                FIELD_VALUE,
                false,
                None,
                0.0,
                Some(screen::unit(ccy, cp, scale)),
                UNIT_RESERVE,
                None,
                cp,
                scale,
            ),
        ]
        .align_y(Alignment::Center);
   
        let mut amount_sec = column![label("amount", cp, scale), amount_row].width(Length::Fill);
        if insufficient {
            amount_sec = amount_sec.push(Space::new().height(5.0 * scale).boxed()).push(compact::mono_runs(
                vec![
                    (btcsend::OVER_AVAILABLE.to_string(), cp.red),
                    (format!("  \u{b7}  {} btc available", format_token_amount(btcsend::available_btc(), 8)), cp.muted),
                ],
                HINT * scale,
            ));
        }

        let mut chips: Row<Element<'_, Message>> = row![].spacing(CHIP_GAP * scale).width(Length::Fill);
        for t in BtcFeeTier::ALL {
            chips = chips.push(chip(t.label(), Some(t) == tier, Message::BtcSendFeeTierSelected(t), cp, scale));
        }
        let outlook: Option<&'static str> = match fee {
            Some(_) if below_floor => None,
            Some(f) if btcsend::tiers_all_at_floor(state) && f == floor => Some(btcsend::QUIET),
            Some(f) => Some(btcsend::fee_outlook(state, f)),
            None => None,
        };
        let fee_line: Element<'a, Message> = if tier == Some(BtcFeeTier::Custom) {
         
            let note: Element<'a, Message> = match (fee, below_floor) {
                (Some(f), false) => {
                    let mut runs = Vec::new();
                    if rate > 0.0 {
                        runs.push((format!("\u{2248} {}", fiat_of(f)), cp.muted));
                    }
                    if let Some(o) = outlook {
                        if !runs.is_empty() {
                            runs.push(("  \u{b7}  ".to_string(), cp.faint));
                        }
                        runs.push((o.to_string(), cp.muted));
                    }
                    compact::mono_runs(runs, HINT * scale)
                }
                (Some(_), true) => compact::mono_runs(vec![(format!("minimum {floor}"), cp.red)], HINT * scale),
                (None, _) => compact::mono_runs(vec![(format!("minimum {floor}"), cp.muted)], HINT * scale),
            };
            row![
                screen::boxed_plain(
                    PlainField::BtcSendFee,
                    &state.btc_send_fee,
                    "0",
                    btcsend::FEE_BOX_W,
                    FIELD_VALUE,
                    false,
                    None,
                    0.0,
                    Some(screen::unit("sats", cp, scale)),
                    29.0,
                    None,
                    cp,
                    scale,
                ),
                Space::new().width(8.0 * scale),
                note,
            ]
            .align_y(Alignment::Center)
            .boxed()
        } else {
            match fee {
                Some(f) => {
                    let mut runs = vec![(format!("{f} sats"), fee_colour)];
                    if rate > 0.0 {
                        runs.push(("  \u{b7}  ".to_string(), cp.faint));
                        runs.push((format!("\u{2248} {}", fiat_of(f)), cp.muted));
                    }
                    if let Some(o) = outlook {
                        runs.push(("  \u{b7}  ".to_string(), cp.faint));
                        runs.push((o.to_string(), cp.muted));
                    }
                    compact::mono_runs(runs, HINT * scale)
                }
                None => {
                    let note = if btcsend::tier_quotes(state).is_some() { btcsend::PICK_FEE } else { btcsend::NO_TIERS };
                    compact::mono_runs(vec![(note.to_string(), cp.muted)], HINT * scale)
                }
            }
        };
        let fee_sec = column![
            label("network fee", cp, scale),
            chips,
            Space::new().height(5.0 * scale),
            fee_line,
        ]
        .width(Length::Fill);

        let signing = state.btc_send_step >= 2;
        let display_fee = if signing { Some(btcsend::committed_fee(state)).filter(|f| *f > 0) } else { fee };
        let mut review = column![
            rule_label("review", cp, scale),
            screen::address_block((!addr.is_empty()).then_some(addr), TAIL, cp, scale),
            Space::new().height(7.0 * scale),
        ]
        .width(Length::Fill);
        review = review.push(drow(
            "amount",
            match amount {
                Some(a) => vec![(format_token_amount(a, 8), cp.text), (" btc".to_string(), cp.faint)],
                None => vec![(NA.to_string(), cp.faint)],
            },
            2.0,
            cp,
            scale,
        ));
     
        review = review.push(
            container(
                row![
                    text("network fee").size(ROW_KEY * scale).color(cp.dim),
                    Space::new().width(5.0 * scale),
                    text(tier.map_or("", BtcFeeTier::label)).font(MONO).size(HINT * scale).color(cp.faint),
                    Space::new().width(Length::Fill),
                    compact::mono_runs(
                        match display_fee {
                            Some(f) => {
                                let mut runs = vec![(format!("{f} sats"), cp.dim)];
                                if rate > 0.0 {
                                    runs.push((format!("  \u{b7}  {}", fiat_of(f)), cp.faint));
                                }
                                runs
                            }
                            None => vec![(NA.to_string(), cp.faint)],
                        },
                        grid::ROW_VALUE * scale,
                    ),
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(Padding::new(0.0).top(2.0 * scale).bottom(3.0 * scale)).boxed(),
        );
        let total = match (amount, display_fee) {
            (Some(a), Some(f)) => {
                let sum = a + f as f64 / 1e8;
                let mut runs = vec![(format_token_amount(sum, 8), cp.text), (" btc".to_string(), cp.faint)];
                if rate > 0.0 {
                    runs.push((format!("  \u{b7}  {} {}", fiat_amount(sum * rate), ccy.to_lowercase()), cp.faint));
                }
                runs
            }
            _ => vec![(NA.to_string(), cp.faint)],
        };
        review = review.push(group_rule(cp, scale)).push(
            row![
                text("total").size(ROW_KEY * scale).color(cp.dim),
                Space::new().width(Length::Fill),
                compact::mono_runs(total, ROW_TOTAL * scale),
            ]
            .align_y(Alignment::Center).boxed(),
        );

        let sign_msg = Message::BtcGrid(GridMsg::SendSign);
        let fields = SignFields {
            mode: EnableInputMode::detect(key_deleted, key_mode),
            secret: SecureField::BtcSendPassphrase,
            secret_buf: &state.btc_send_passphrase,
            secret_reveal: state.btc_send_passphrase_reveal,
            phrase: SecureField::BtcSendSeed,
            phrase_buf: &state.btc_send_seed_text,
            phrase_reveal: state.btc_send_seed_reveal,
            bip39: SecureField::BtcSendBip39,
            bip39_buf: &state.btc_send_bip39,
            bip39_reveal: state.btc_send_bip39_reveal,
            on_submit: sign_msg.clone(),
        };
        let live = valid && fields.can_submit();
        let sign = grid::sign_block(
            &fields,
            &state.btc_send_passphrase,
            &state.btc_send_seed_text,
            &state.btc_send_bip39,
            valid,
            grid::primary("Send payment", live, sign_msg, cp, scale),
            inner,
            cp,
            scale,
        );

        grid::scroller(
            column![
                recipient,
                Space::new().height(8.0 * scale),
                amount_sec,
                Space::new().height(8.0 * scale),
                fee_sec,
                review,
                sign,
            ]
            .width(Length::Fill)
            .boxed(),
            cp,
            scale,
        )
    })
    .boxed()
}


fn chip<'a>(word: &str, selected: bool, msg: Message, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    button(
        text(word.to_uppercase())
            .font(MONO)
            .size(STRIP * scale)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .color(if selected { cp.text } else { cp.muted }),
    )
    .on_press(msg)
    .width(Length::Fill)
    .padding(Padding::new(0.0).top(3.0 * scale).bottom(3.0 * scale))
    .style(move |_, status| {
        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(if selected || hot { cp.hover } else { Color::TRANSPARENT }.into()),
            border: Border { color: cp.border_soft, width: 1.0, radius: (5.0 * scale).into() },
            text_color: if selected { cp.text } else { cp.muted },
            shadow: Shadow::default(),
            snap: false,
        }
    })
    .boxed()
}



const ADDR_TAIL: usize = 5;

fn receive_line<'a>(address: &str, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    match crate::btc_script_type::BtcScriptType::of_address(address) {
        Some(kind) => text(format!("{} \u{b7} {}", kind.name(), kind.bip()))
            .font(MONO)
            .size(HINT * scale)
            .color(cp.muted)
            .boxed(),
        None => Space::new().boxed(),
    }
}


const BLOCKS_MIN_H: f32 = 170.0;

fn blocks_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    grid::fill_or_scroll(BLOCKS_MIN_H, move || blocks_body(state, cp, scale), cp, scale)
}

fn blocks_body<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let node = NodeFrame::current();
    let now = now_secs();
    let dark = theme::is_dark(&state.theme);
    let (word, dot) = node.status(cp);
    let newest = node.blocks[0];

    let since = match tip_age(node.tip_at) {
        Some(age) => vec![(age_word(age), cp.text)],
        None => vec![(NA.to_string(), cp.muted)],
    };
    let weight = match newest {
        Some(b) => vec![
            (format!("{:.2} MWu", b.weight as f64 / 1e6), cp.dim),
            (format!("  {}%", (b.weight as f64 / 40_000.0).floor() as u64), cp.faint),
        ],
        None => vec![(NA.to_string(), cp.muted)],
    };

    column![
        container(block_train(&node, now, dark, cp, scale))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::new(0.0).top(2.0 * scale).bottom(10.0 * scale)),
        drow("status", vec![(format!("\u{25cf} {word}"), dot)], 3.0, cp, scale),
        drow(
            "tip",
            vec![(node.tip_height.map_or(NA.to_string(), |h| add_commas(h as i64)), cp.text)],
            3.0,
            cp,
            scale,
        ),
        drow("since last block", since, 3.0, cp, scale),
        drow("weight", weight, 3.0, cp, scale),
        drow("peers", vec![(node.peers.map_or(NA.to_string(), |n| n.to_string()), cp.dim)], 3.0, cp, scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}


fn block_train<'a>(node: &NodeFrame, now: u64, dark: bool, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let count = node.next_block_txs.map_or(NA.to_string(), |n| format!("{} tx", add_commas(n as i64)));
    let mut cards = row![candidate_card("next", count, cp, scale)]
        .spacing(CARD_GAP * scale)
        .width(Length::Fill)
        .height(Length::Fill);
    let mut floor_age = 0u64;
    for b in node.blocks.iter().flatten().take(TRAIN_CARDS) {
        let age = now.saturating_sub(b.at).max(floor_age);
        floor_age = age;
        cards = cards.push(block_card(b.height.to_string(), age_word(age), block_tint(b.feerate, dark, cp), cp, scale));
    }
    cards.boxed()
}


fn block_card<'a>(height: String, age: String, tint: Color, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    container(card_lines(height, age, cp, scale))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::new(0.0).top(CARD_PAD_V * scale).bottom(CARD_PAD_V * scale).left(CARD_PAD_H * scale).right(CARD_PAD_H * scale))
        .style(move |_| container::Style {
            background: Some(tint.into()),
            border: Border { color: cp.border_soft, width: 1.0, radius: (CARD_RADIUS * scale).into() },
            ..Default::default()
        })
        .boxed()
}

fn candidate_card<'a>(word: &str, count: String, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let edge = Canvas::new(DashedWell { color: cp.border_soft, radius: CARD_RADIUS * scale, dash: 3.0 * scale })
        .width(Length::Fill)
        .height(Length::Fill);
    stack![
        edge,
        container(card_lines(word.to_string(), count, cp, scale))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::new(0.0).top(CARD_PAD_V * scale).bottom(CARD_PAD_V * scale).left(CARD_PAD_H * scale).right(CARD_PAD_H * scale)),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}


fn card_lines<'a>(top: String, bottom: String, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    column![
        text(top).font(MONO).size(CARD_HEIGHT * scale).color(cp.text),
        Space::new().height(Length::Fill),
        text(bottom).font(MONO).size(CARD_AGE * scale).color(cp.muted),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

fn block_tint(feerate: f32, dark: bool, cp: &'static CompactPalette) -> Color {
    let band = fee_band(feerate);
    let a = match (band, dark) {
        (0 | 1, false) => 0.12,
        (0 | 1, true) => 0.13,
        (_, false) => 0.11,
        (_, true) => 0.12,
    };
    Color { a, ..band_color(band, cp) }
}


fn age_word(secs: u64) -> String {
    match secs {
        s if s < 60 => "<1m".to_string(),
        s if s < 3600 => format!("{}m", s / 60),
        s => format!("{}h {}m", s / 3600, (s % 3600) / 60),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

   
    #[test]
    fn the_ladder_colours_come_from_the_ladder() {
        let p = &theme::COMPACT_OBSIDIAN;
        let live = [0.1f32, 0.2, 0.3, 0.338];
        assert_eq!(ladder_colour(&live, 0, p), p.red);
        assert_eq!(ladder_colour(&live, 1, p), p.amber);
        assert_eq!(ladder_colour(&live, 2, p), p.amber);
        assert_eq!(ladder_colour(&live, 3, p), p.green);
        let quiet = [0.1f32; 4];
        for i in 0..4 {
            assert_eq!(ladder_colour(&quiet, i, p), p.green, "rung {i}");
        }
     
        let partial = [0.1f32, 0.1, 0.3, 0.3];
        assert_eq!(ladder_colour(&partial, 1, p), p.red);
        assert_eq!(ladder_colour(&partial, 2, p), p.green);
    }

    #[test]
    fn a_block_is_tinted_by_what_getting_in_cost() {
        let p = &theme::COMPACT_OBSIDIAN;
        let cheap = block_tint(0.95, true, p);
        assert_eq!(Color { a: 1.0, ..cheap }, p.green);
        assert!((cheap.a - 0.13).abs() < 1e-6);
        let busy = block_tint(30.0, false, p);
        assert_eq!(Color { a: 1.0, ..busy }, p.amber);
        assert!((busy.a - 0.11).abs() < 1e-6);
        let dear = block_tint(200.0, true, p);
        assert_eq!(Color { a: 1.0, ..dear }, p.red);
        assert!((dear.a - 0.12).abs() < 1e-6);
    }

    #[test]
    fn ages_read_as_the_cards_show_them() {
        assert_eq!(age_word(12), "<1m");
        assert_eq!(age_word(6 * 60 + 12), "6m");
        assert_eq!(age_word(59 * 60 + 59), "59m");
        assert_eq!(age_word(3 * 3600 + 5 * 60), "3h 5m");
    }
}

#[cfg(test)]
mod balance_figure_tests {
    use super::{balance_figures, BalanceFigures};
    use crate::channel::BtcUtxo;

    const OWN: &str = "bc1qown";
    const CHANGE: &str = "bc1qchange";

    fn coin(txid: &str, sats: u64, height: u64, address: &str, spent_by: Option<&str>) -> BtcUtxo {
        BtcUtxo {
            txid: txid.to_string(),
            vout: 0,
            sats,
            address: address.to_string(),
            height,
            spent_by: spent_by.map(String::from),
        }
    }

    #[test]
    fn a_quiet_wallet_reads_zero_unconfirmed() {
        let union = vec![coin("a", 106_145, 955_129, OWN, None), coin("b", 93_764_195, 965_706, CHANGE, None)];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 93_870_340, unconfirmed: 0 });
    }

 
    #[test]
    fn a_pending_send_keeps_the_chain_figure_and_states_the_delta() {
        let union = vec![
            coin("5154", 5_802, 969_974, OWN, Some("33cf")),
            coin("b4d4", 5_923, 965_711, CHANGE, Some("33cf")),
            coin("33cf", 5_568, 0, CHANGE, None),
        ];
        let f = balance_figures(&union);
        assert_eq!(f, BalanceFigures { confirmed: 11_725, unconfirmed: -(5_857 + 300) });
        assert_eq!(f.confirmed as i64 + f.unconfirmed, 5_568);
    }

   
    #[test]
    fn the_figures_are_a_function_of_the_set_alone() {
        let a = vec![coin("x", 10_000, 10, OWN, Some("p")), coin("p", 6_700, 0, CHANGE, None)];
        let b: Vec<BtcUtxo> = a.iter().rev().cloned().collect();
        assert_eq!(balance_figures(&a), balance_figures(&b));
        assert_eq!(balance_figures(&a), BalanceFigures { confirmed: 10_000, unconfirmed: 6_700 - 10_000 });
    }

    #[test]
    fn a_foreign_mempool_coin_is_unconfirmed_and_positive() {
        let union = vec![coin("a", 1_000, 10, OWN, None), coin("in", 5_000, 0, OWN, None)];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 1_000, unconfirmed: 5_000 });
    }

    #[test]
    fn a_send_on_pending_change_counts_the_confirmed_coin_once() {
      
        let union = vec![
            coin("c", 10_000, 10, OWN, Some("p")),
            coin("p", 3_000, 0, CHANGE, Some("q")),
            coin("q", 1_500, 0, CHANGE, None),
        ];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 10_000, unconfirmed: 1_500 - 10_000 });
    }

    #[test]
    fn confirmation_moves_the_figure_once() {
        let union = vec![coin("33cf", 5_568, 970_141, CHANGE, None)];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 5_568, unconfirmed: 0 });
    }
}
