//! The **ticket** — the order, top to bottom: pair pill · Market / Limit ·
//! Buy / Sell · the fields · the stats · the verb.
//!
//! ## The ticket speaks the trader's language; the engine speaks pay/receive
//!
//! The XRPL engine only knows *TakerGets* and *TakerPays*, and every function
//! in the controller works in that orientation. A trader **sells XRP** or
//! **buys XRP**, at a price in **RLUSD per XRP**, for an **amount** of XRP and
//! a **total** in RLUSD. So this pane is a translation layer: `Sell` pays the
//! base and receives the quote, `Buy` the reverse; the price shown and typed
//! is always quote-per-base. [`trade_market_pair`] and [`trade_market_price`]
//! are the whole of that translation.
//!
//! ## Two contracts, not two modes
//!
//! **Market**: we own the bound. The stats headline the walk's estimate; the
//! signed number is the walked floor moved by the cushion, struck at the
//! press (spec §1, §3, §4). Always IOC — the TIF row is absent, not greyed.
//! Offered on every XRP-leg pair ([`trade_market_offered`]); on a cross pair
//! the segment is dead and the `Market` row says `limit only` (§3.4).
//!
//! **Limit**: the user owns the price. Signed exactly as typed, TIF a real
//! choice, blocked only for being invalid, never mispriced (§2).
//!
//! ## Warn, never decide (2026-09-16)
//!
//! Nothing on this ticket enables or disables itself on the book's mood.
//! The Market segment used to die when the windowed classification read
//! `illiquid`, and come back when it didn't — a control that flickers on a
//! statistic, deciding for the user what the walk measures exactly for
//! their size. Now the walk always runs, and what it finds is PRINTED:
//! the `Book` row wears the pair pane's ink (green only for liquid AND
//! stable), and under Market the `Market` row states the contract's
//! standing for this order — `measuring…`, `waiting for the book…`,
//! `no offers`, `off market · wait a ledger`, or the walk itself as
//! `fills 100% · −0.4%`, amber where it should give pause
//! ([`trade_market_verdict`]). The button goes dark only where there is no
//! number to sign: no book yet, no offers, or a gap on an unstable book.
//! Under Limit the row is absent — the price is the user's and a Market
//! fault is not theirs — except on a cross pair, where `limit only` is the
//! one explanation the dead segment has.
//!
//! ## One amount field; its unit is the exact side
//!
//! `Amount` in the base asset or `Total` in the quote — the chip inside the
//! box flips between them, and the label follows. The unit IS the anchor:
//! the side you type in is the exact one, the other is derived on every
//! `Sync` ([`crate::controller::xrp::refresh_trade_twin`]) and both are
//! printed in the `Swap` row, pay for receive. Flipping promotes the derived number to the typed
//! one in place. (Until 2026-09-16 these were two live fields and the
//! anchor was whichever one you typed in last.) The engine's `tfSell` falls
//! out of which side is exact — the stats say which flags are signed.
//!
//! **Nothing here reports an error.** The verb lights on exactly what the
//! controller re-checks when it is pressed; everything past the stack is the
//! activity log's.

use iced::widget::{Column, Row};
use iced::Widget as _;
use iced::widget::{button, column, row, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Shadow};

use crate::controller::app_state::{AppState, TradeAnchor, TradeContract, TradeSide};
use crate::controller::message::{Message, PlainField};
use crate::controller::xrp::{
    trade_available, trade_book_quote, trade_decimals, trade_effective_limit, trade_expected_price, trade_fits,
    trade_market_offered, trade_market_pair, trade_market_price, trade_market_verdict, trade_terms, trade_tif,
    xrp_fee, MarketVerdict,
};
use crate::channel::CHANNEL;
use crate::ui::components::compact;
use crate::ui::components::grid::{HINT, NA};
use crate::ui::components::send_screen;
use crate::ui::managexrp::panes;
use crate::ui::managexrp::xrpsend::{NO_XRP_FOR_FEE, OVER_AVAILABLE};
use crate::utils::fonts::MONO;
use crate::utils::format_token_amount;
use crate::utils::orderbook::fmt_price;
use crate::utils::theme::CompactPalette;

use super::{disp, BUY_BORDER, BUY_FILL, FIELD, LABEL, ROW, SELL_BORDER, SELL_FILL, UNIT};

/// Time in force, in the order the segments draw them.
pub const TIF_CODES: [&str; 3] = ["GTC", "IOC", "FOK"];
/// The same three, spelled out — the industry names, never the ledger's
/// `tf…` flag names.
pub const TIF_NAMES: [&str; 3] = ["Good til Cancelled", "Immediate or Cancel", "Fill or Kill"];

const SEG: f32 = 11.5;

/// Which time-in-force this order will actually be signed with: 0 GTC, 1 IOC,
/// 2 FOK. Reads the DERIVED tif, so Market always reads IOC no matter what
/// the row was last set to under Limit.
pub fn time_in_force(state: &AppState) -> usize {
    let tif = trade_tif(state);
    if tif.iter().any(|f| f == "tfImmediateOrCancel") {
        1
    } else if tif.iter().any(|f| f == "tfFillOrKill") {
        2
    } else {
        0
    }
}

/// The side's verb, for the segment and the CTA.
pub fn side_word(side: TradeSide) -> &'static str {
    match side {
        TradeSide::Sell => "Sell",
        TradeSide::Buy => "Buy",
    }
}

/// The side with its object — `Buy XRP`, `Sell RLUSD` — the words on the
/// segment AND on the sign button, so "buy *what*?" is answered at the
/// click (user, 2026-09-16: a wallet holding XRP read `Buy` on XRP/RLUSD as
/// "buy my first token" and was buying XRP). The bare verb before a pair is
/// held.
pub fn side_label(side: TradeSide, base: &str) -> String {
    if base.is_empty() {
        side_word(side).to_string()
    } else {
        format!("{} {}", side_word(side), disp(base))
    }
}

/// A field's empty-state placeholder: to the drop for XRP, to the cent for a
/// stablecoin.
pub(crate) fn placeholder(dec: usize) -> &'static str {
    if dec == 6 { "0.000000" } else { "0.00" }
}

/// The venue mix the walk says this order crosses — disclosure, never a fee
/// split (spec §7). Before an amount is typed, what the pair offers.
pub(crate) fn routed_via(state: &AppState) -> &'static str {
    match trade_book_quote(state) {
        Some(q) if q.filled > 0.0 => match (q.clob_levels > 0, q.from_pool > 0.0) {
            (true, true) => "Book + pool",
            (false, true) => "Pool",
            _ => "Book",
        },
        _ => {
            let (base, quote) = trade_market_pair(state);
            let has_pool = crate::utils::orderbook::oriented(base, quote).is_some_and(|(b, _)| b.amm.is_some());
            if has_pool { "Book + pool" } else { "Book" }
        }
    }
}

/// Whether the order as typed can be signed: priced, non-zero on both sides,
/// and affordable — the same facts `TradeContinueClicked` re-checks.
pub(crate) fn valid(state: &AppState) -> bool {
    let limit = trade_effective_limit(state);
    let terms = trade_terms(state, limit);
    let quoting = limit > 0.0 && terms.pay > 0.0 && terms.receive > 0.0;
    let fits = terms.pay <= 0.0 || trade_fits(state, terms.pay);
    quoting && fits
}

/// The ticket under its pair: Market / Limit · Buy / Sell, the fields in the
/// ticket's orientation, and the stats — the review, live. **One
/// implementation for the trade screen and the pane** (2026-09-09): the pane
/// grid's ticket draws this verbatim at its own width, so the two contracts,
/// the field linking and the review rows cannot diverge. `on_submit` is what
/// Enter in a field does — the screen's Continue, the pane's sign.
pub(crate) fn form<'a>(
    state:     &'a AppState,
    inner:     f32,
    on_submit: Option<Message>,
    cp:        &'static CompactPalette,
    scale:     f32,
) -> Element<'a, Message> {
    // ── Orientation ────────────────────────────────────────────────────────
    let selling = state.trade_side == TradeSide::Sell;
    let (base, quote) = trade_market_pair(state);

    // Limit is the contract the ticket opens on (2026-09-16) — the answer
    // that cannot go wrong, since it prices at a typed number and the worst
    // it does is not fill. Market is LIVE on every XRP-leg pair and DEAD
    // only where the contract does not exist (a cross pair, a restricted
    // issuer) — never on the book's mood, and never swapped for Limit on
    // its say-so (user, 2026-09-15: the row flipped four times in a
    // session, rewriting the ticket each time). Whether the walk can price
    // this order is the button's and the `Market` row's to say.
    let no_pair = base.is_empty() || quote.is_empty();
    let offered = trade_market_offered(state);
    let manual = super::manual(state);
    let market = state.trade_contract == Some(TradeContract::Market);
    let limit = trade_effective_limit(state);
    let terms = trade_terms(state, limit);
    let quoting = limit > 0.0 && terms.pay > 0.0 && terms.receive > 0.0;
    let tif = time_in_force(state);

    // ── Market / Limit · Buy / Sell ────────────────────────────────────────
    // Neutral actives on the contract row; the only colour in the ticket is
    // the direct Buy/Sell row's active segment.
    let mode_items = vec![
        Segment {
            label: "Market".to_string(),
            active: market,
            msg: (offered || no_pair).then_some(Message::TradeMarketSelected),
            colour: None,
        },
        Segment { label: "Limit".to_string(), active: manual, msg: Some(Message::TradeLimitSelected), colour: None },
    ];
    let mode_seg = segmented(mode_items, cp, scale);
    let side_seg = segmented(
        vec![
            Segment {
                label: side_label(TradeSide::Buy, base),
                active: !selling,
                msg: Some(Message::TradeSideSet(TradeSide::Buy)),
                colour: Some((BUY_FILL, BUY_BORDER, cp.green)),
            },
            Segment {
                label: side_label(TradeSide::Sell, base),
                active: selling,
                msg: Some(Message::TradeSideSet(TradeSide::Sell)),
                colour: Some((SELL_FILL, SELL_BORDER, cp.red)),
            },
        ],
        cp,
        scale,
    );

    // ── The field — in the ticket's orientation ────────────────────────────
    // Underneath, `trade_amount` is always the pay side and `trade_receive`
    // the receive side, and the anchor says which one was typed. The unit
    // chip is that anchor in the ticket's words: the base asset under Sell
    // is the pay side, under Buy the receive side.
    let unit_is_base = matches!(
        (selling, state.trade_anchor),
        (true, TradeAnchor::Pay) | (false, TradeAnchor::Receive)
    );
    let (typed_field, typed_buf, twin_buf) = match state.trade_anchor {
        TradeAnchor::Pay => (PlainField::TradeAmount, state.trade_amount.as_str(), state.trade_receive.as_str()),
        TradeAnchor::Receive => (PlainField::TradeReceive, state.trade_receive.as_str(), state.trade_amount.as_str()),
    };
    let unit_asset = if unit_is_base { base } else { quote };
    let field_label = if unit_is_base { "Amount" } else { "Total" };
    // `max ›` fills the PAY side — the side a balance is spent from, and
    // the one a user reads off a two-place balance pane and types short
    // (2026-09-15: 2.74 of 2.744127260319475, and the line could not be
    // closed). The handler anchors there, so the field flips to that unit
    // by itself if it was showing the other.
    let can_max = trade_available(state).is_some_and(|a| a > 0.0);
    let max = can_max.then_some(Message::TradeMaxClicked);
    let chip_w = (disp(base).chars().count() + disp(quote).chars().count() + 3) as f32 * UNIT * 0.6 + 6.0;
    let chip = unit_chip(disp(base), disp(quote), unit_is_base, cp, scale);

    let mut fields: Column<Element<'_, Message>> = column![].width(Length::Fill);
    if manual {
        // The placeholder hints the market's mid — THE SAME NUMBER, from the
        // same source and through the same formatter, as the pair picker's
        // row (`liquidity::market`, bookd's windowed median). It used to
        // hint the touch on the taking side, and on a 5% spread that read
        // 0.87 against the picker's 0.85 (user, 2026-09-16: "users will
        // notice that discrepancy") — and flipped whenever the side did. The
        // fiat index is deliberately never offered as a price.
        let ph = crate::utils::liquidity::market(base, quote)
            .map(|m| m.mid)
            .filter(|&x| x > 0.0)
            .map(fmt_price)
            .unwrap_or_else(|| "0.0000".to_string());
        // The unit is the quote asset alone. `EURØP per RLUSD` was tried
        // for an hour on 2026-09-16 and dropped (user: "it doesn't help") —
        // the direction is the swap line's job, under the stats.
        let unit_w = disp(quote).chars().count() as f32 * UNIT * 0.6 + 6.0;
        fields = fields.push(field(
            "Limit price",
            PlainField::TradeLimit,
            &state.trade_limit_price,
            &ph,
            send_screen::unit(disp(quote), cp, scale),
            unit_w,
            inner,
            on_submit.clone(),
            None,
            cp,
            scale,
        ));
    }
    fields = fields.push(field(
        field_label,
        typed_field,
        typed_buf,
        placeholder(trade_decimals(unit_asset)),
        chip,
        chip_w,
        inner,
        on_submit.clone(),
        max,
        cp,
        scale,
    ));
    // The balance is named the moment the order exceeds it, and nowhere
    // else — under the field, in the send panes' own words (user,
    // 2026-09-16: "only show amount if they tried to type more than they
    // have"). The pay side is what a balance is spent from, whichever unit
    // the field is in: under Buy it is the derived Total. The same
    // `trade_fits` the button reads, so the note and the dark button agree.
    if terms.pay > 0.0 && !trade_fits(state, terms.pay) {
        let pay_asset = state.trade_pay_asset.as_str();
        let note: Vec<(String, Color)> = if pay_asset != "XRP" && terms.pay <= CHANNEL.token(pay_asset).0 {
            // The token fits; the XRP for the fee does not — say that instead.
            vec![(NO_XRP_FOR_FEE.to_string(), cp.red)]
        } else {
            let avail = trade_available(state)
                .map_or_else(|| NA.to_string(), |a| format_token_amount(a, trade_decimals(pay_asset)));
            vec![
                (OVER_AVAILABLE.to_string(), cp.red),
                (format!("  \u{b7}  {} {} available", avail, disp(pay_asset)), cp.muted),
            ]
        };
        fields = fields.push(Space::new().height(5.0 * scale).boxed()).push(compact::mono_runs(note, HINT * scale));
    }
    if manual {
        // TIF is a genuine choice under Limit (§2) and not one under Market
        // (§1, always IOC): hidden there rather than greyed — a greyed row of
        // three invites "why can't I pick" when it was never a question.
        let tif_seg = segmented_at(
            TIF_CODES
                .iter()
                .enumerate()
                .map(|(i, code)| Segment {
                    label: code.to_string(),
                    active: tif == i,
                    msg: Some(Message::TradeOrderOptionSet(i as u8)),
                    colour: None,
                })
                .collect(),
            TIF_SHRINK,
            cp,
            scale,
        );
        fields = fields.push(
            column![
                Space::new().height(9.0 * scale),
                text("Time in force").size(LABEL * scale).color(cp.dim),
                Space::new().height(3.0 * scale),
                tif_seg,
            ]
            .width(Length::Fill).boxed(),
        );
    }

    // ── Stats — the review, live ───────────────────────────────────────────
    // The review confirms the ORDER: the price, the side of it that was
    // derived, and the swap in one sentence. No `Available` row (user,
    // 2026-09-16: on a two-token pair it did not say whose balance it was,
    // and the balance is not part of the order) — a balance the order
    // exceeds is named under the field instead, the moment it is exceeded.
    let fee = format!("{} XRP", format_token_amount(xrp_fee(), 6));
    // `Book` wears the pair pane's ink — one bit, `liquidity::healthy`, so
    // the two never disagree on which markets are green. `Market` is the
    // contract's standing for THIS order: every reason the button is dark
    // and every number that should give pause, amber where it should.
    let (book, book_ink) = match crate::utils::liquidity::healthy(base, quote) {
        Some(true) => (crate::utils::liquidity::label(base, quote), cp.green),
        Some(false) => (crate::utils::liquidity::label(base, quote), cp.amber),
        None => (crate::utils::liquidity::label(base, quote), cp.faint),
    };
    // Under Limit the Market contract is nobody's business — the price is
    // the user's, and a Market-contract fault printed on a Limit ticket read
    // as a Limit fault (user, 2026-09-16: "on limit order it says no frame
    // so even limit is blocked"). So under Limit the row exists only where
    // it states something about THIS ticket that nothing else does: a cross
    // pair, whose dead Market segment would otherwise go unexplained. That
    // is a fact of the pair, so the row never comes and goes on a frame.
    let verdict = trade_market_verdict(state);
    let market_row = (!manual || verdict == MarketVerdict::LimitOnly).then(|| {
        let (words, tone) = verdict_words(verdict);
        let ink = match tone {
            Tone::Plain => cp.text,
            Tone::Warn => cp.amber,
            Tone::Faint => cp.faint,
        };
        ("Market", words, ink)
    });
    // The headline is the walk's own estimate under Market — the top of
    // book before an amount is typed, the walk's average after — never a
    // ceiling; the bound is the `Market` row's number. Under Limit it is the
    // typed price restated. EITHER WAY IT GOES THROUGH `trade_market_price`:
    // `limit` is the engine's receive-per-pay, and under Buy that is the
    // inverse of what was typed — a Buy at 0.8734 printed `1.1450` for an
    // hour on 2026-09-16. With no current frame there is no price, and none
    // is invented.
    let price = if !manual {
        trade_expected_price(state, limit)
            .filter(|_| quoting)
            .or_else(|| trade_book_quote(state).map(|q| trade_market_price(state, q.vwap)).filter(|p| *p > 0.0))
            .map(|p| format!("{} {}", fmt_price(p), disp(quote)))
            .unwrap_or_else(|| "\u{2014}".to_string())
    } else if limit > 0.0 {
        format!("{} {}", fmt_price(trade_market_price(state, limit)), disp(quote))
    } else {
        "\u{2014}".to_string()
    };
    // `Swap` — `0.87 EURØP for 1 RLUSD`: the whole order in one row (user,
    // 2026-09-16, off the screenshot; it replaced a `Total` row plus a
    // prose line under it, which "looked bad"). Always the engine's pay →
    // receive, which is what "X for Y" means on either side and does not
    // depend on which field was typed; the numbers are the field's and the
    // twin's strings verbatim, so the row can never disagree with the
    // field. Under Market the derived side is the walk's estimate and wears
    // `≈`. `—` until both numbers exist — the row itself never comes and
    // goes.
    let (pay_str, recv_str) = match state.trade_anchor {
        TradeAnchor::Pay => (typed_buf.trim(), twin_buf.trim()),
        TradeAnchor::Receive => (twin_buf.trim(), typed_buf.trim()),
    };
    let swap = if pay_str.is_empty() || recv_str.is_empty() {
        "\u{2014}".to_string()
    } else {
        let pay_derived = state.trade_anchor == TradeAnchor::Receive;
        let approx = |derived: bool| if derived && !manual { "\u{2248} " } else { "" };
        format!(
            "{}{} {} for {}{} {}",
            approx(pay_derived),
            pay_str,
            disp(&state.trade_pay_asset),
            approx(!pay_derived),
            recv_str,
            disp(&state.trade_receive_asset),
        )
    };
    let mut stat_list: Column<Element<'_, Message>> = column![].spacing(3.0 * scale).width(Length::Fill);
    stat_list = stat_list.push(stat_row("Price", price, cp.text, cp, scale));
    stat_list = stat_list.push(stat_row("Swap", swap, cp.text, cp, scale));
    if !manual {
        stat_list = stat_list.push(stat_row("Routed via", routed_via(state).to_string(), cp.text, cp, scale));
    }
    stat_list = stat_list.push(stat_row("Fee", fee, cp.text, cp, scale));
    stat_list = stat_list.push(stat_row("Book", book, book_ink, cp, scale));
    if let Some((k, v, ink)) = market_row {
        stat_list = stat_list.push(stat_row(k, v, ink, cp, scale));
    }
    if manual {
        // The order in the trader's words, never the ledger's: the code the
        // segments use and its industry-standard name. The anchor (`tfSell`)
        // is not restated here — it is the field's unit.
        let order = format!("{} \u{b7} {}", TIF_CODES[tif], TIF_NAMES[tif]);
        stat_list = stat_list.push(stat_row("Order", order, cp.text, cp, scale));
    }
    let stats = column![
        Space::new().height(10.0 * scale),
        compact::hairline(cp.rule),
        Space::new().height(8.0 * scale),
        stat_list,
    ]
    .width(Length::Fill);

    column![mode_seg, Space::new().height(4.0 * scale), side_seg, fields, stats]
        .width(Length::Fill)
        .boxed()
}

/// A labelled boxed field: Inter 10.5 `dim` over the compact box, the unit
/// riding inside it on the right — a plain word on the price field, the
/// flipping chip on the amount field — `unit_w` being the width it needs.
/// `max` puts a `max ›` link at the label row's far end.
#[allow(clippy::too_many_arguments)]
fn field<'a>(
    label:       &'static str,
    field:       PlainField,
    value:       &'a str,
    placeholder: &str,
    unit:        Element<'a, Message>,
    unit_w:      f32,
    inner:       f32,
    on_submit:   Option<Message>,
    max:         Option<Message>,
    cp:          &'static CompactPalette,
    scale:       f32,
) -> Element<'a, Message> {
    let mut head = row![text(label).size(LABEL * scale).color(cp.dim)].align_y(Alignment::Center).width(Length::Fill);
    if let Some(msg) = max {
        head = head.push(Space::new().width(Length::Fill).boxed()).push(panes::link("max", cp.dim, msg, cp, scale));
    }
    column![
        Space::new().height(9.0 * scale),
        head,
        Space::new().height(3.0 * scale),
        send_screen::boxed_plain(
            field,
            value,
            placeholder,
            inner,
            FIELD,
            false,
            None,
            0.0,
            Some(unit),
            unit_w,
            on_submit,
            cp,
            scale,
        ),
    ]
    .width(Length::Fill)
    .boxed()
}

/// The amount field's unit chip: both assets, mono 9 upper, the one the
/// field is in lit `text`, the other `faint` and pressable — `XRP · RLUSD`.
/// Pressing the faint one flips the field to that unit
/// (`TradeUnitToggled`). Same size and padding either way, so a flip never
/// reflows the box around it.
fn unit_chip<'a>(base: &str, quote: &str, base_active: bool, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let word = |u: &str, active: bool| -> Element<'a, Message> {
        let ink = if active { cp.text } else { cp.faint };
        button(text(u.to_uppercase()).font(MONO).size(UNIT * scale).color(ink))
            .on_press_maybe((!active).then_some(Message::TradeUnitToggled))
            .padding(Padding::ZERO)
            .style(move |_, status| {
                let hot = !active && matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: None,
                    border: Border::default(),
                    text_color: if hot { cp.dim } else { ink },
                    shadow: Shadow::default(),
                    snap: false,
                }
            })
            .boxed()
    };
    row![
        word(base, base_active),
        text(" \u{b7} ").font(MONO).size(UNIT * scale).color(cp.faint),
        word(quote, !base_active),
    ]
    .align_y(Alignment::Center)
    .boxed()
}

/// One review row: Inter 10.5 `dim` key, mono 10 value in the caller's ink,
/// pinned right.
fn stat_row<'a>(key: &'static str, value: String, ink: Color, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    row![
        text(key).size(LABEL * scale).color(cp.dim),
        Space::new().width(Length::Fill),
        text(value).font(MONO).size(ROW * scale).color(ink),
    ]
    .align_y(Alignment::Center)
    .boxed()
}

/// How a stats value is inked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Plain,
    Warn,
    Faint,
}

/// The `Market` row's words for a verdict, and their tone. Numbers are the
/// walk's own: coverage of the typed size, and where the bound sits against
/// the windowed mid, signed and in the trader's orientation — a sell's deep
/// walk reads `−6.2%`, a buy's `+6.2%`. Amber wherever the ledger would do
/// something other than fill the whole order near the mid.
///
/// **Words a trader can read, never the engine's** (user, 2026-09-16: "how
/// is the user going to understand what no frame means … they will think
/// the ticket is somehow broken"). The three states with a dark button each
/// say what is happening and, where there is one, what to do: the book is
/// still on its way (`waiting for the book…` — and it IS on its way, bookd
/// re-reads every book every ledger); there is nothing on the side this
/// order takes (`no offers`); or this ledger's book is a gap the last
/// twenty do not recognise (`off market · wait a ledger`).
fn verdict_words(v: MarketVerdict) -> (String, Tone) {
    use crate::controller::xrp::BOUND_GUARD_PCT;
    match v {
        MarketVerdict::LimitOnly => ("limit only".to_string(), Tone::Warn),
        MarketVerdict::Measuring => ("measuring\u{2026}".to_string(), Tone::Faint),
        MarketVerdict::NoFrame => ("waiting for the book\u{2026}".to_string(), Tone::Faint),
        MarketVerdict::Idle => ("\u{2014}".to_string(), Tone::Faint),
        MarketVerdict::NoDepth => ("no offers".to_string(), Tone::Warn),
        MarketVerdict::Walked { held: true, .. } => ("off market \u{b7} wait a ledger".to_string(), Tone::Warn),
        MarketVerdict::Walked { coverage_pct, bound_pct, held: false } => {
            let short = coverage_pct < 100.0 - 1e-6;
            let far = bound_pct.abs() > BOUND_GUARD_PCT;
            let sign = if bound_pct < 0.0 { "\u{2212}" } else { "+" };
            let words = format!("fills {:.0}% \u{b7} {}{:.1}%", coverage_pct, sign, bound_pct.abs());
            (words, if short || far { Tone::Warn } else { Tone::Plain })
        }
    }
}

/// One segment of a segmented control.
pub(crate) struct Segment {
    pub(crate) label:  String,
    pub(crate) active: bool,
    pub(crate) msg:    Option<Message>,
    /// `(fill, border, text)` for the ACTIVE state when the row is coloured —
    /// the Buy/Sell row only. `None` = neutral: `neutral` fill, `border`, `text`.
    pub(crate) colour: Option<(Color, Color, Color)>,
}

/// The time-in-force row at 82% of the contract rows (user, 2026-09-11:
/// at full size it pulled the eye off the fields).
const TIF_SHRINK: f32 = 0.82;

/// An N-up segmented control: equal widths, gap 4, 1px `border`, radius 6,
/// Inter 11.5. Inactive = transparent + `muted`, `dim` on hover.
pub(crate) fn segmented<'a>(items: Vec<Segment>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    segmented_at(items, 1.0, cp, scale)
}

/// [`segmented`] at a fraction of its size — text and vertical padding both,
/// so the row shrinks as one thing.
fn segmented_at<'a>(items: Vec<Segment>, shrink: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let scale = scale * shrink;
    let mut r: Row<Element<'_, Message>> = row![].spacing(4.0 * scale).width(Length::Fill);
    for s in items {
        let Segment { label, active, msg, colour } = s;
        // A dead segment (no message) keeps its place and reads `faint`:
        // the Market contract withheld on a book too thin to walk.
        let live = msg.is_some();
        let (fill, line, ink) = match (active, colour) {
            (true, Some(c)) => c,
            // Lit but dead: the user's Market over a book that cannot be
            // walked right now — the pick stands, the ink says wait.
            (true, None) if !live => (cp.neutral, cp.border, cp.faint),
            (true, None) => (cp.neutral, cp.border, cp.text),
            (false, _) if !live => (Color::TRANSPARENT, cp.border, cp.faint),
            (false, _) => (Color::TRANSPARENT, cp.border, cp.muted),
        };
        r = r.push(
            button(
                text(label)
                    .size(SEG * scale)
                    .width(Length::Fill)
                    .align_x(Alignment::Center)
                    .color(ink),
            )
            .on_press_maybe(msg)
            .width(Length::Fill)
            .padding(Padding::new(0.0).top(4.0 * scale).bottom(4.0 * scale))
            .style(move |_, status| {
                let hot = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: Some(fill.into()),
                    border: Border { color: line, width: 1.0, radius: (6.0 * scale).into() },
                    text_color: if live && !active && hot { cp.dim } else { ink },
                    shadow: Shadow::default(),
                    snap: false,
                }
            }).boxed(),
        );
    }
    r.boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `Market` row: plain only for a full fill near the mid; amber for
    /// a short fill, a far bound, a hole, no depth, or no contract; faint
    /// where there is nothing to judge yet. The sign is the trader's — a
    /// sell's deep walk reads minus.
    #[test]
    fn the_market_row_is_amber_wherever_the_ledger_would_not_just_fill() {
        let walked = |c: f64, b: f64, held: bool| MarketVerdict::Walked { coverage_pct: c, bound_pct: b, held };
        assert_eq!(verdict_words(walked(100.0, -0.4, false)), ("fills 100% \u{b7} \u{2212}0.4%".to_string(), Tone::Plain));
        assert_eq!(verdict_words(walked(100.0, 0.4, false)), ("fills 100% \u{b7} +0.4%".to_string(), Tone::Plain));
        assert_eq!(verdict_words(walked(3.0, -0.2, false)).1, Tone::Warn);
        assert_eq!(verdict_words(walked(100.0, -6.2, false)).1, Tone::Warn);
        assert_eq!(verdict_words(walked(100.0, 7.0, false)).0, "fills 100% \u{b7} +7.0%");
        assert_eq!(verdict_words(walked(100.0, -93.0, true)), ("off market \u{b7} wait a ledger".to_string(), Tone::Warn));
        assert_eq!(verdict_words(MarketVerdict::LimitOnly), ("limit only".to_string(), Tone::Warn));
        assert_eq!(verdict_words(MarketVerdict::NoDepth), ("no offers".to_string(), Tone::Warn));
        assert_eq!(verdict_words(MarketVerdict::Measuring).1, Tone::Faint);
        assert_eq!(verdict_words(MarketVerdict::NoFrame), ("waiting for the book\u{2026}".to_string(), Tone::Faint));
        assert_eq!(verdict_words(MarketVerdict::Idle), ("\u{2014}".to_string(), Tone::Faint));
    }

    /// The side decides the verb, and the market's base is its object.
    #[test]
    fn the_verb_follows_the_side() {
        assert_eq!(side_word(TradeSide::Sell), "Sell");
        assert_eq!(side_word(TradeSide::Buy), "Buy");
        assert_eq!(side_label(TradeSide::Buy, "XRP"), "Buy XRP");
        assert_eq!(side_label(TradeSide::Sell, "RLUSD"), "Sell RLUSD");
        assert_eq!(side_label(TradeSide::Sell, ""), "Sell");
    }
}
