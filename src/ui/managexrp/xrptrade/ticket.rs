
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

pub const TIF_CODES: [&str; 3] = ["GTC", "IOC", "FOK"];
pub const TIF_NAMES: [&str; 3] = ["Good til Cancelled", "Immediate or Cancel", "Fill or Kill"];

const SEG: f32 = 11.5;

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

pub fn side_word(side: TradeSide) -> &'static str {
    match side {
        TradeSide::Sell => "Sell",
        TradeSide::Buy => "Buy",
    }
}

pub fn side_label(side: TradeSide, base: &str) -> String {
    if base.is_empty() {
        side_word(side).to_string()
    } else {
        format!("{} {}", side_word(side), disp(base))
    }
}

pub(crate) fn placeholder(dec: usize) -> &'static str {
    if dec == 6 { "0.000000" } else { "0.00" }
}

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

pub(crate) fn valid(state: &AppState) -> bool {
    let limit = trade_effective_limit(state);
    let terms = trade_terms(state, limit);
    let quoting = limit > 0.0 && terms.pay > 0.0 && terms.receive > 0.0;
    let fits = terms.pay <= 0.0 || trade_fits(state, terms.pay);
    quoting && fits
}

pub(crate) fn form<'a>(
    state:     &'a AppState,
    inner:     f32,
    on_submit: Option<Message>,
    cp:        &'static CompactPalette,
    scale:     f32,
) -> Element<'a, Message> {
    let selling = state.trade_side == TradeSide::Sell;
    let (base, quote) = trade_market_pair(state);

    let no_pair = base.is_empty() || quote.is_empty();
    let offered = trade_market_offered(state);
    let manual = super::manual(state);
    let market = state.trade_contract == Some(TradeContract::Market);
    let limit = trade_effective_limit(state);
    let terms = trade_terms(state, limit);
    let quoting = limit > 0.0 && terms.pay > 0.0 && terms.receive > 0.0;
    let tif = time_in_force(state);

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
    let can_max = trade_available(state).is_some_and(|a| a > 0.0);
    let max = can_max.then_some(Message::TradeMaxClicked);
    let chip_w = (disp(base).chars().count() + disp(quote).chars().count() + 3) as f32 * UNIT * 0.6 + 6.0;
    let chip = unit_chip(disp(base), disp(quote), unit_is_base, cp, scale);

    let mut fields: Column<Element<'_, Message>> = column![].width(Length::Fill);
    if manual {
        let ph = crate::utils::liquidity::market(base, quote)
            .map(|m| m.mid)
            .filter(|&x| x > 0.0)
            .map(fmt_price)
            .unwrap_or_else(|| "0.0000".to_string());
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
    if terms.pay > 0.0 && !trade_fits(state, terms.pay) {
        let pay_asset = state.trade_pay_asset.as_str();
        let note: Vec<(String, Color)> = if pay_asset != "XRP" && terms.pay <= CHANNEL.token(pay_asset).0 {
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

    let fee = format!("{} XRP", format_token_amount(xrp_fee(), 6));
    let (book, book_ink) = match crate::utils::liquidity::healthy(base, quote) {
        Some(true) => (crate::utils::liquidity::label(base, quote), cp.green),
        Some(false) => (crate::utils::liquidity::label(base, quote), cp.amber),
        None => (crate::utils::liquidity::label(base, quote), cp.faint),
    };
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

fn stat_row<'a>(key: &'static str, value: String, ink: Color, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    row![
        text(key).size(LABEL * scale).color(cp.dim),
        Space::new().width(Length::Fill),
        text(value).font(MONO).size(ROW * scale).color(ink),
    ]
    .align_y(Alignment::Center)
    .boxed()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Plain,
    Warn,
    Faint,
}

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

pub(crate) struct Segment {
    pub(crate) label:  String,
    pub(crate) active: bool,
    pub(crate) msg:    Option<Message>,
    pub(crate) colour: Option<(Color, Color, Color)>,
}

const TIF_SHRINK: f32 = 0.82;

pub(crate) fn segmented<'a>(items: Vec<Segment>, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    segmented_at(items, 1.0, cp, scale)
}

fn segmented_at<'a>(items: Vec<Segment>, shrink: f32, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let scale = scale * shrink;
    let mut r: Row<Element<'_, Message>> = row![].spacing(4.0 * scale).width(Length::Fill);
    for s in items {
        let Segment { label, active, msg, colour } = s;
        let live = msg.is_some();
        let (fill, line, ink) = match (active, colour) {
            (true, Some(c)) => c,
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

    #[test]
    fn the_verb_follows_the_side() {
        assert_eq!(side_word(TradeSide::Sell), "Sell");
        assert_eq!(side_word(TradeSide::Buy), "Buy");
        assert_eq!(side_label(TradeSide::Buy, "XRP"), "Buy XRP");
        assert_eq!(side_label(TradeSide::Sell, "RLUSD"), "Sell RLUSD");
        assert_eq!(side_label(TradeSide::Sell, ""), "Sell");
    }
}
