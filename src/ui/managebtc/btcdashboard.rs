//! The BTC dashboard as a **pane grid** (design handoff
//! `design_handoff_btc_dashboard`, 2026-09-10) — the XRP grid's twin, built
//! the way that one was: the chrome is [`crate::ui::components::grid`], the
//! model is [`crate::controller::panes`], and this file is what is Bitcoin
//! about the grid — which kind draws what, and the five bodies the other
//! chain does not have: the balance (two numbers), the fee ladder, the send
//! form with its tier chips, and the block train.
//!
//! Seven panes in the default tree — `balance` over `fees` over `chart` on
//! the left, `send` over `wallet` in the middle, `receive` over `blocks` on
//! the right. The wallet pane is in the default on this chain too (user,
//! 2026-09-10); key management is the same process on both.
//!
//! ## Where this file and the handoff part ways (user, 2026-09-10)
//!
//! - **The send logic is ours, to the letter.** The tier chips are the
//!   mock's; what they select, what the line under them says and what the
//!   review totals are the send screen's own: `effective_tier` (the cheapest
//!   tier at the chosen price), `resolved_fee`, the outlook sentences (a
//!   comparison against a measured rate, never a confirmation time), the
//!   live floor gate, and **custom in sats**, not sat/vB — sats are what
//!   people type and what the transactions list shows.
//! - **Signing is the XRP pane's, word for word:** one `sign` label, the
//!   `encryption key` and `optional 25th word` placeholders, `Send
//!   payment` firing the send screen's own Continue and Submit. The
//!   signer is untouched.
//! - **The ladder's colours are derived from the tiers on hand, nothing
//!   fetched:** a tier at the next-block rate is green (it will confirm),
//!   one at the relay floor is red (the eviction edge), between is amber.
//!   A collapsed ladder — every tier the floor — is all green: on a quiet
//!   chain the floor gets in.
//! - **Block cards are tinted by what getting in cost** — the block's
//!   realised feerate through the same bands the fee verdict uses — not by
//!   how full the block came in. Every block has been 3.99 MWu for years;
//!   "fullness" would paint the train red forever. On a quiet chain the
//!   train is green, and when fees rise it turns amber and red block by
//!   block, which is a fact worth a colour.
//! - **The candidate card's count is the walk's**: the best-paying
//!   transactions that fill one block by vsize, read off the same mempool
//!   snapshot as the tiers. Free, and it moves when they move.
//! - No bell, no settings in the top bar (their home is the Balance tab);
//!   the panels menu is the XRP one (checklist, reset, undo).
//! - `hd wallet` was floated for the pane's title; `wallet`, for symmetry.
//!
//! The pre-grid chain — the balance screen, the receive page, the
//! transactions modal, the bump stack, the key stack — was deleted on
//! 2026-09-10 once every door had a pane (`managebtc/panes/`); the node
//! verdicts it owned live in `managebtc/node.rs`.

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
use crate::ui::components::tui::MONO_ADVANCE;
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

// ── Type scale and geometry that are BTC's own ──────────────────────────────

/// The fee ladder: the tier word's column, the rate's, the cost's, the bar.
const TIER_W: f32 = 30.0;
const RATE_W: f32 = 36.0;
const COST_W: f32 = 70.0;
const LADDER_GAP: f32 = 8.0;
const BAR_H: f32 = 6.0;
/// The least a rung's fill can be: `min` sits at the axis's left edge by
/// definition (the floor IS the origin), and a zero-width bar hides the one
/// colour that matters there — the red of the eviction edge.
const BAR_MIN_FILL: f32 = 4.0;
/// The tier chips' gap; the fee line under them (mono).
const CHIP_GAP: f32 = 4.0;
/// The block train: cards share the width, `gap 4`, `padding 5px 4px`,
/// radius 2; height (mono 9) top, age (mono 7.5) bottom.
const CARD_GAP: f32 = 4.0;
const CARD_PAD_V: f32 = 5.0;
const CARD_PAD_H: f32 = 4.0;
const CARD_RADIUS: f32 = 2.0;
const CARD_HEIGHT: f32 = 9.0;
const CARD_AGE: f32 = 7.5;
/// Mined blocks the train shows behind the candidate.
const TRAIN_CARDS: usize = 5;
/// Characters of the address the review underlines — five, not XRP's
/// three: bech32 is long enough that three don't read.
const TAIL: usize = 5;
/// Unit trailing reserve inside the amount boxes (`btc`, the currency).
const UNIT_RESERVE: f32 = 23.0;

// ── The screen ──────────────────────────────────────────────────────────────

pub fn view(state: &AppState) -> Element<'_, Message> {
    let cp = theme::compact(&state.theme);
    let scale = state.scale();
    let host = grid::Host { chain: Chain::Btc, title: "BTC", grid: &state.btc_grid, wrap: Message::BtcGrid, bar: None };
    grid::screen(host, state, pane_content, cp, scale)
}

/// Money — the BTC pane's price.
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
            // A pane wearing a face says so in its strip, as send's do.
            PaneKind::Receive if state.btc_pool_face => (strip("addresses"), None, pool_face(state, cp, scale)),
            PaneKind::Receive => {
                // Whichever pool address the user picked, defaulting to the
                // most recently generated; #0 for a wallet that predates the
                // account xpub.
                let address = state
                    .btc_receive_address
                    .clone()
                    .or_else(|| CHANNEL.bitcoin_wallet_rx.borrow().1.clone())
                    .unwrap_or_else(|| "No Address".to_string());
                // The door to the pool, in the slot XRP's tag line has.
                let link = (
                    format!("addresses \u{b7} {}", state.btc_receive_pool.len().max(1)),
                    Message::BtcTogglePoolFace,
                );
                (
                    strip("receive"),
                    None,
                    grid::receive_pane(address, state.btc_copy_feedback, Message::BtcCopyAddress, link, cp, scale),
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
            // XRP's kinds cannot be loaded into a BTC layout (`from_key` is
            // per chain); if one ever were, it is a well, not a panic.
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

// ── balance ─────────────────────────────────────────────────────────────────

/// The two figures the pane states, in satoshis — mempool.space's own split
/// (user, 2026-09-16: "confirmed really is the raw balance"), applied to
/// every address of the wallet and read straight off the coin set.
///
/// **`confirmed` is the chain's view**: every confirmed coin, counting one
/// our own pending send has consumed — the chain has not seen that spend
/// yet, and the number the user checks us against is the chain's. The set
/// keeps such a coin, marked with its spender, until a block spends it, so
/// nothing here is reconstructed: the figure moves when a block lands and at
/// no other moment (user, 2026-10-06, after a send read +$5, then +$10 on a
/// reopen: the old arithmetic added the consumed coins back on top of a set
/// that still held them).
/// **`unconfirmed` is the mempool's net delta**: coins created for us and not
/// consumed again, minus the confirmed coins our pending sends consume — so
/// `confirmed + unconfirmed` is where the balance lands once the mempool
/// clears, a send reads `−(amount + fee)`, an incoming payment `+amount`, a
/// quiet mempool zero, and a send chained on our own pending change counts
/// the confirmed coin once.
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

/// What the chain says you hold, and what that is worth: the fiat figure
/// of the confirmed balance as the hero, then `confirmed` and
/// `unconfirmed` under it — the explorer's two numbers, so a user who
/// checks us against mempool.space finds the same split (2026-09-16, after
/// a rotated change address sent the user there and back in a panic). The
/// hero never drops by a whole coin the moment a send is signed; the
/// unconfirmed row says what left, signed, and reads zero on a quiet
/// mempool. Under a rule, where the coins sit: `on master` — the one
/// address the wallet pane names and an explorer can be asked about — and
/// `on other addresses`, the rotated receive and change addresses summed,
/// with their count. Two rows, not a list and not a receive/change split:
/// the user's question is why the master reads less than the total, and
/// that is the whole answer (user, same day; the split by chain was
/// bookkeeping nobody asked for, and it lived in the wallet pane, which
/// then no longer fit beside send). Per-transaction detail is the
/// transactions pane's. No reserve, no trustlines: there is nothing else
/// to state. Hiding masks every amount; the hero reads `—` until a rate
/// exists rather than claiming `0.00`.
fn balance_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let hide = state.hide_balance;
    let ccy = state.base_currency.code();
    let rate = price::cross("BTC", ccy);
    let mask = |s: String| if hide { "\u{2022}".repeat(4) } else { s };

    let records = crate::wallet::btc_address_records();
    let master = records.first().map(|r| r.address.clone()).unwrap_or_default();
    let (figures, on_master, on_others, others_n) = {
        let union = CHANNEL.btc_utxos_rx.borrow();
        let on_master: u64 = union.1.iter().filter(|u| u.address == master).map(|u| u.sats).sum();
        let on_others: u64 = union.1.iter().filter(|u| u.address != master).map(|u| u.sats).sum();
        let mut others: Vec<&str> = union.1.iter().filter(|u| u.address != master).map(|u| u.address.as_str()).collect();
        others.sort_unstable();
        others.dedup();
        (balance_figures(&union.1), on_master, on_others, others.len())
    };
    let btc = |sats: u64| format!("{:.8}", sats as f64 / 1e8);

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
    let places = match others_n {
        0 => String::new(),
        1 => "  \u{b7}  1 address".to_string(),
        n => format!("  \u{b7}  {n} addresses"),
    };
    let col = column![
        hero,
        drow(
            "confirmed",
            vec![(mask(btc(figures.confirmed)), cp.text), (" btc".to_string(), cp.faint)],
            9.0,
            cp,
            scale,
        ),
        drow(
            "unconfirmed",
            vec![
                (mask(format!("{sign}{}", btc(figures.unconfirmed.unsigned_abs()))), cp.dim),
                (" btc".to_string(), cp.faint),
            ],
            3.0,
            cp,
            scale,
        ),
        group_rule(cp, scale),
        drow("on master", vec![(mask(btc(on_master)), cp.dim), (" btc".to_string(), cp.faint)], 3.0, cp, scale),
        drow(
            "on other addresses",
            vec![(mask(btc(on_others)), cp.dim), (" btc".to_string(), cp.faint), (places, cp.faint)],
            3.0,
            cp,
            scale,
        ),
    ]
    .width(Length::Fill);

    grid::scroller(col.boxed(), cp, scale)
}

// ── fees ────────────────────────────────────────────────────────────────────

/// The colour of one rung, from the ladder itself: at the next-block rate
/// it will confirm (green); at the relay floor it may never (red) — that is
/// the eviction edge; between, it probably will (amber). Not a fixed
/// mapping: a collapsed ladder is green top to bottom, because on a quiet
/// chain the floor gets in.
fn ladder_colour(tiers: &[f32; 4], i: usize, cp: &'static CompactPalette) -> Color {
    if tiers[i] >= tiers[3] {
        cp.green
    } else if tiers[i] <= tiers[0] {
        cp.red
    } else {
        cp.amber
    }
}

/// Four rows, always exactly four: `min · low · med · high` in sat/vB, a
/// bar that is where the tier sits on the mempool pane's axis, and what
/// each costs the transaction in `send` — the same quote the send pane
/// prices, against the coins this wallet would actually spend. Send owns
/// the choice; this is the readout. `—` and bare tracks until the node has
/// reported.
///
/// The bars were `tier ÷ high` until 2026-09-10 — which pinned `high` full
/// and left the rest barely moving, since the scale was the top rung
/// itself (user: "unclear what the fee bars actually give us"). On the
/// mempool's log axis (the floor at the left, the histogram's top edge at
/// the right) a bar is the rung's place in the queue, the same place the
/// mempool pane draws its cut, and it moves when the market moves.
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

/// One rung: the word (Inter 9.5 `dim`), the bar on a `pill` track, the
/// rate (mono 10 `text`) and the cost (mono 9 `muted`), `padding 2px 0`.
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

/// A track and a fill from the left: a ratio, no axis, no labels. `None`
/// is a bare track; `Some` fills at least `min_fill`, so the colour shows.
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

// ── chart ───────────────────────────────────────────────────────────────────

/// The BTC series in the display currency for the pane's period.
fn chart_series(state: &AppState) -> Vec<(u64, f32)> {
    let history = match state.btc_grid.chart_period {
        ChartPeriod::OneHour => &state.rate_history_short,
        ChartPeriod::OneDay => &state.rate_history_long,
    };
    chart_data("BTC", state.base_currency.code(), history)
}

// ── send ────────────────────────────────────────────────────────────────────

/// The whole transaction in one pane: recipient, the amount duo, the tier
/// chips with the fee line under them, the live review, the credential,
/// `Send payment`. The stack follows its content, as on XRP. The gate
/// lights the button on exactly what the controller re-checks; every number
/// comes from the send screen's own resolvers, so nothing about how a
/// payment is priced, validated, signed or sent lives in this file.
fn send_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let ccy = state.base_currency.code();
        let (_, own_btc, key_deleted, key_mode) = CHANNEL.bitcoin_wallet_rx.borrow().clone();

        // ── The facts, resolved once — the send screen's own ─────────────
        let addr = state.btc_send_recipient.trim();
        let is_self = !addr.is_empty() && own_btc.as_deref().is_some_and(|own| btcsend::same_address(own, addr));
        let addr_ok = !is_self && btcsend::is_valid(addr);
        // Every fault the string itself can have, not just the prefix one:
        // `is_plausible` alone left a right-prefix/impossible-length address
        // (a `bc1` with ninety characters after it) dark and unexplained.
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

        // ── recipient ─────────────────────────────────────────────────────
        // The line under the field speaks only for a fault. The forms the
        // field takes are its placeholder (2026-09-14) — a standing
        // `bech32 · legacy` legend under an empty box was a second line
        // saying what the box already said.
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

        // ── amount: two linked inputs, type in either ─────────────────────
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
        // Over what the wallet can sign against: the button was already dark
        // for it, but a dark button with no words is a form that will not
        // say what is wrong (2026-09-14) — and `10` typed into the btc box
        // for ten dollars is the ordinary way to get here. The figure is the
        // send's own `available`: eligible coins, less nothing, because the
        // fee is the part that made it not fit.
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

        // ── network fee: five chips, and the line that prices the pick ────
        let mut chips: Row<Element<'_, Message>> = row![].spacing(CHIP_GAP * scale).width(Length::Fill);
        for t in BtcFeeTier::ALL {
            chips = chips.push(chip(t.label(), t == tier, Message::BtcSendFeeTierSelected(t), cp, scale));
        }
        let outlook: Option<&'static str> = match fee {
            Some(_) if below_floor => None,
            Some(f) if btcsend::tiers_all_at_floor(state) && f == floor => Some(btcsend::QUIET),
            Some(f) => Some(btcsend::fee_outlook(state, f)),
            None => None,
        };
        let fee_line: Element<'a, Message> = if tier == BtcFeeTier::Custom {
            // A hand-typed fee in SATS — what people type and what the
            // transactions list shows — with the live relay minimum beside
            // it while it is under.
            let note: Element<'a, Message> = match (fee, below_floor) {
                (Some(f), false) => {
                    // The fiat only with a rate: `≈ 0.00` is not true.
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
                    // The fiat only with a rate: `≈ 0.00` is not true.
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
                None => compact::mono_runs(vec![(btcsend::NO_TIERS.to_string(), cp.muted)], HINT * scale),
            }
        };
        let fee_sec = column![
            label("network fee", cp, scale),
            chips,
            Space::new().height(5.0 * scale),
            fee_line,
        ]
        .width(Length::Fill);

        // ── review, live — committed once the submit is in flight ─────────
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
        // The fee row names the tier it prices, as a faint suffix on the
        // key; the value is sats and the fiat twin.
        review = review.push(
            container(
                row![
                    text("network fee").size(ROW_KEY * scale).color(cp.dim),
                    Space::new().width(5.0 * scale),
                    text(tier.label()).font(MONO).size(HINT * scale).color(cp.faint),
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

        // ── sign ──────────────────────────────────────────────────────────
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

/// One tier chip: mono 8.5 upper on a 1px `border_soft`, radius 5, the
/// five sharing the width; `hover` fill and `text` ink when selected, the
/// wash alone on hover.
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

/// The receive card's POOL face — the pane-face idiom (the asset picker on
/// send, the tag on XRP receive): the card swaps in place, never a dropdown
/// and never a modal.
///
/// One row per pool address. Click the address to put it on the QR, the glyph
/// to copy it — that is the whole point of a pool, handing a different address
/// to each payer. `drop` is the secondary action and unsubscribes it by
/// shrinking the live list. No status dots: a dot must be bound to a real
/// liveness channel, and "subscribed" is not one.
///
/// Laid out like the tag face (user, 2026-09-25): the rows as one block
/// centred in the pane, the refusal note under them, then `Cancel` beside
/// `New address` on one line. They had stood one over the other, full width,
/// under a list hugging the top edge, and `back` was the one face that did
/// not say `Cancel` — the asset picker says it for the same immediate pick.
/// The block is centred while it fits and scrolls from the top when it does
/// not; [`pool_min_h`] counts the rows for that. Five rows and the pair come
/// to less than the receive body's own minimum, so wherever the code fits,
/// the list centres.
///
/// A row shows its address whole while the pane holds it, and cut to the
/// columns it has ([`grid::fit_addr`]) when not — it had been the wallet
/// pane's 10…8 at every width (user, 2026-09-25: plenty of space, and no
/// reason). Cutting rather than wrapping keeps every row one line tall, which
/// is what lets the minimum above stay exact.
fn pool_face<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    // A refused generate must NAME what it is waiting on, or the dead control
    // reads as broken.
    use crate::bridge::btc_receive_rotation::{PoolRefusal, MAX_RECEIVE_POOL};
    let note: Option<&'static str> = match state.btc_pool_refusal {
        Some(PoolRefusal::Full) => Some("drop one to make room"),
        // Being paid is the only thing that moves the gap window forward.
        Some(PoolRefusal::OutOfGap) => Some("one of these has to be used first"),
        Some(PoolRefusal::Unavailable) => Some("unavailable"),
        None => None,
    };
    let room = state.btc_receive_pool.len() < MAX_RECEIVE_POOL;

    let body = move |size: Size| -> Element<'a, Message> {
        // The columns an address has on its row: the width less what sits
        // beside it, in `ROW_KEY` mono.
        let cols = ((size.width - POOL_ROW_TAIL * scale) / (ROW_KEY * scale * MONO_ADVANCE)).floor().max(1.0) as usize;
        let mut rows: Column<Element<'_, Message>> = column![].spacing(POOL_ROW_GAP * scale).width(Length::Fill);
        for address in &state.btc_receive_pool {
            let shown = state.btc_receive_address.as_deref() == Some(address.as_str());
            rows = rows.push(
                row![
                    button(
                        text(grid::fit_addr(address, cols))
                            .font(MONO)
                            .size(ROW_KEY * scale)
                            .color(if shown { cp.text } else { cp.dim }),
                    )
                    .on_press(Message::BtcSelectReceiveAddress(address.clone()))
                    .padding(Padding::new(0.0).top(POOL_ROW_PAD * scale).bottom(POOL_ROW_PAD * scale))
                    .style(move |_, status| {
                        let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
                        button::Style {
                            background: Some(if shown || hot { cp.hover } else { Color::TRANSPARENT }.into()),
                            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: (5.0 * scale).into() },
                            text_color: if shown { cp.text } else { cp.dim },
                            shadow: Shadow::default(),
                            snap: false,
                        }
                    }),
                    Space::new().width(Length::Fill),
                    grid::copy_glyph(
                        state.btc_pool_copied.as_deref() == Some(address.as_str()),
                        Message::BtcCopyPoolAddress(address.clone()),
                        cp,
                        scale,
                    ),
                    button(text("drop").size(STRIP * scale).color(cp.muted))
                        .on_press(Message::BtcRemoveReceiveAddress(address.clone()))
                        .padding(Padding::new(0.0).left(8.0 * scale))
                        .style(|_, status| {
                            let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
                            button::Style {
                                background: None,
                                border: Border::default(),
                                text_color: if hot { cp.text } else { cp.muted },
                                shadow: Shadow::default(),
                                snap: false,
                            }
                        }),
                ]
                .spacing(8.0 * scale)
                .align_y(Alignment::Center).boxed(),
            );
        }

        let mut block = column![rows].width(Length::Fill);
        if let Some(note) = note {
            block = block
                .push(Space::new().height(POOL_NOTE_GAP * scale).boxed())
                .push(text(note).size(HINT * scale).color(cp.muted).boxed());
        }
        block = block.push(Space::new().height(POOL_PAIR_GAP * scale).boxed()).push(grid::button_pair(
            grid::quiet_button("Cancel", Message::BtcTogglePoolFace, cp, scale),
            grid::pane_button("New address", room, false, Message::BtcGenerateReceiveAddress, cp, scale),
            scale,
        ));
        container(block).width(Length::Fill).height(Length::Fill).align_y(Alignment::Center).boxed()
    };
    grid::fill_or_scroll_by(pool_min_h(state.btc_receive_pool.len(), note.is_some()), body, cp, scale)
}

/// A pool row's padding above and below its address.
const POOL_ROW_PAD: f32 = 3.0;
/// What a pool row keeps beside its address, unscaled: the copy glyph, `drop`
/// with its lead, and the row's three gaps. The address has the rest.
const POOL_ROW_TAIL: f32 = 64.0;
/// The gap between pool rows.
const POOL_ROW_GAP: f32 = 2.0;
/// Rows to the refusal note.
const POOL_NOTE_GAP: f32 = 8.0;
/// Rows (or the note) to the pair — the tag face's gap.
const POOL_PAIR_GAP: f32 = 12.0;

/// The least the pool face fills before it scrolls: its rows at `ROW_KEY`
/// over iced's line height and their padding, the note while it shows, the
/// gap and the pair. Exact, not padded: under it by a hair the centred block
/// clips a hair, over it by a hair the bar shows for a hair of travel.
fn pool_min_h(rows: usize, note: bool) -> f32 {
    let row = ROW_KEY * 1.3 + 2.0 * POOL_ROW_PAD;
    let rows = rows as f32 * row + rows.saturating_sub(1) as f32 * POOL_ROW_GAP;
    rows + if note { POOL_NOTE_GAP + HINT * 1.3 } else { 0.0 } + POOL_PAIR_GAP + grid::BUTTON_H
}

// ── blocks ──────────────────────────────────────────────────────────────────

/// The train, then the facts: the node's status word, the tip, how long
/// since it, the tip's weight, peers. Everything here is the node frame; a
/// field it cannot measure draws `—`, and a lost node reads `offline`.
/// The least the blocks pane fills before it scrolls: a legible train, the
/// five rows.
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

    // Whole minutes, like the cards (user, 2026-09-10: "minutes matter,
    // seconds don't" — a stopwatch would need a redraw a second for a digit
    // nobody reads). Moves on the frame and rate cadence the pane already has.
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

/// One row of cards: the candidate, then the newest mined blocks, newest
/// first. Cards share the width and take the pane's leftover height — a
/// taller pane makes taller cards, not more of them. Ages are clamped
/// monotonic along the train: header times are not, and a train that
/// read younger as it went back would look broken.
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

/// A mined block: the height top, the age bottom, filled with what getting
/// in cost — the block's realised feerate through the fee verdict's bands,
/// at the handoff's alphas over `window`.
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

/// The candidate: transparent, a dashed edge — it hasn't happened yet.
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

/// Two lines pushed to the card's top and bottom: mono 9 `text` over mono
/// 7.5 `muted` — the smallest type in the app.
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

/// The card's fill: the band's hue at the handoff's alpha — green 12 / 13 %,
/// amber and red 11 / 12 % (light / dark) over `window`.
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

/// An age, on a card or in the `since last block` row: whole minutes, `<1m`
/// under one, hours past sixty.
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

    /// The rule the ladder is coloured by: green at the top rate, red at the
    /// floor, amber between — and every rung green when the ladder has
    /// collapsed onto the floor.
    #[test]
    fn the_ladder_colours_come_from_the_ladder() {
        let p = &theme::COMPACT_OBSIDIAN;
        // The live regime on 2026-09-10.
        let live = [0.1f32, 0.2, 0.3, 0.338];
        assert_eq!(ladder_colour(&live, 0, p), p.red);
        assert_eq!(ladder_colour(&live, 1, p), p.amber);
        assert_eq!(ladder_colour(&live, 2, p), p.amber);
        assert_eq!(ladder_colour(&live, 3, p), p.green);
        // Collapsed: nothing to trade off, the floor gets in.
        let quiet = [0.1f32; 4];
        for i in 0..4 {
            assert_eq!(ladder_colour(&quiet, i, p), p.green, "rung {i}");
        }
        // Partially collapsed: `low` at the floor is the floor's colour,
        // `med` at the top rate is the top's.
        let partial = [0.1f32, 0.1, 0.3, 0.3];
        assert_eq!(ladder_colour(&partial, 1, p), p.red);
        assert_eq!(ladder_colour(&partial, 2, p), p.green);
    }

    /// A card's tint is the fee verdict's own hue — cheap blocks green, dear
    /// ones red — at the handoff's alphas.
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

    /// A quiet mempool: the chain figure alone, unconfirmed exactly zero.
    #[test]
    fn a_quiet_wallet_reads_zero_unconfirmed() {
        let union = vec![coin("a", 106_145, 955_129, OWN, None), coin("b", 93_764_195, 965_706, CHANGE, None)];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 93_870_340, unconfirmed: 0 });
    }

    /// The 2026-10-06 send that found the old arithmetic out: two confirmed
    /// coins consumed, 5,857 to the recipient, 300 fee, 5,568 change. The
    /// chain still counts both coins — the figure does not move — and
    /// unconfirmed reads `−(amount + fee)`: mempool.space's two numbers.
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

    /// A reopen while the send pends brings the same set back from the
    /// whole-wallet fetch, marks and all: a push and a fetch cannot land on
    /// different figures, because there is nothing to reconstruct.
    #[test]
    fn the_figures_are_a_function_of_the_set_alone() {
        let a = vec![coin("x", 10_000, 10, OWN, Some("p")), coin("p", 6_700, 0, CHANGE, None)];
        let b: Vec<BtcUtxo> = a.iter().rev().cloned().collect();
        assert_eq!(balance_figures(&a), balance_figures(&b));
        assert_eq!(balance_figures(&a), BalanceFigures { confirmed: 10_000, unconfirmed: 6_700 - 10_000 });
    }

    /// A foreign payment in the mempool is unconfirmed, positive, and the
    /// chain figure does not move for it either.
    #[test]
    fn a_foreign_mempool_coin_is_unconfirmed_and_positive() {
        let union = vec![coin("a", 1_000, 10, OWN, None), coin("in", 5_000, 0, OWN, None)];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 1_000, unconfirmed: 5_000 });
    }

    /// A send chained on pending change: the first send's change is consumed
    /// again at height 0, so the confirmed coin is counted once and the delta
    /// is both sends' amounts and fees together.
    #[test]
    fn a_send_on_pending_change_counts_the_confirmed_coin_once() {
        // P spent a 10,000 confirmed coin: 6,000 out, 3,000 change, 1,000 fee.
        // Q spent P's change: 1,000 out, 1,500 change, 500 fee.
        let union = vec![
            coin("c", 10_000, 10, OWN, Some("p")),
            coin("p", 3_000, 0, CHANGE, Some("q")),
            coin("q", 1_500, 0, CHANGE, None),
        ];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 10_000, unconfirmed: 1_500 - 10_000 });
    }

    /// The block lands: the consumed coins leave the set, the change is
    /// confirmed, and the figure moves exactly once, to the change.
    #[test]
    fn confirmation_moves_the_figure_once() {
        let union = vec![coin("33cf", 5_568, 970_141, CHANGE, None)];
        assert_eq!(balance_figures(&union), BalanceFigures { confirmed: 5_568, unconfirmed: 0 });
    }
}
