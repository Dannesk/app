use iced::widget::Column;
use iced::Widget as _;
use iced::widget::{column, container, responsive, row, text, Space};
use iced::{Alignment, Element, Length, Padding, Size};

use crate::channel::CHANNEL;
use crate::controller::app_state::{AppState, BtcFeeTier, EnableInputMode};
use crate::controller::message::{Message, PlainField, SecureField};
use crate::ui::components::grid::{self, drow, group_rule, HINT, NA, ROW_KEY, ROW_TOTAL};
use crate::ui::components::send_screen::{self as screen, STAGE_PAD};
use crate::ui::components::signing::SignFields;
use crate::ui::components::compact;
use crate::ui::managebtc::btcdashboard::{TAIL, TIERS};
use crate::ui::managebtc::btcsend::{self, Line};
use crate::ui::managebtc::btctransactions::{self as tx, btc};
use crate::ui::managebtc::feeeditor;
use crate::ui::managebtc::node::{rate_text, Verdict};
use crate::utils::fonts::MONO;
use crate::utils::theme::CompactPalette;
use crate::utils::{fiat_amount, fiat_cost, price};
pub use crate::utils::btc_bump::{BtcBumpDraft, BtcBumpQuote, BumpFee};

const NO_QUOTE: &str = "Waiting for this send\u{2019}s details and today\u{2019}s rates";

pub fn draft(state: &AppState) -> Option<BtcBumpDraft> {
    let txid = state.btc_bump_txid.clone()?;
    let custom = state
        .btc_bump_lit
        .is_none()
        .then(|| state.btc_bump_fee.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(BtcBumpDraft { txid, custom })
}

pub fn quote(state: &AppState) -> Option<BtcBumpQuote> {
    let txid = state.btc_bump_txid.as_deref()?;
    CHANNEL.btc_bump_quote_rx.borrow().clone().filter(|q| q.draft.txid == txid)
}

pub fn live_quote(state: &AppState) -> Option<BtcBumpQuote> {
    let draft = draft(state)?;
    quote(state).filter(|q| q.draft == draft)
}

pub fn ready(state: &AppState) -> Option<BumpFee> {
    let q = live_quote(state)?;
    match state.btc_bump_lit.and_then(BtcFeeTier::index) {
        Some(i) => q.tiers[i],
        None => q.custom.filter(|c| c.rate >= q.min_rate),
    }
}

fn reading(state: &AppState, q: &BtcBumpQuote, tiers: Option<[f32; 4]>) -> (Option<u64>, Line) {
    if let Some(i) = state.btc_bump_lit.and_then(BtcFeeTier::index) {
        let fee = q.tiers[i];
        let line = match (fee, tiers) {
            (Some(f), Some(t)) => Line::Tier(tx::wait(f.rate, &t)),
            _ => Line::Custom(None),
        };
        return (fee.map(|f| f.fee), line);
    }
    let positive = state.btc_bump_fee.trim().parse::<f32>().ok().filter(|r| r.is_finite() && *r > 0.0);
    let typed = positive.and(live_quote(state)).and_then(|lq| lq.custom);
    let line = match (positive, typed) {
        (None, _) => Line::Prompt(format!("at least {} sat/vB", rate_text(q.min_rate))),
        (Some(_), Some(c)) if c.rate < q.min_rate => Line::Under(btcsend::below_minimum(q.min_rate)),
        (Some(_), Some(c)) => Line::Custom(tiers.map(|t| tx::wait(c.rate, &t))),
        (Some(_), None) => Line::Custom(None),
    };
    (typed.map(|c| c.fee), line)
}

pub(crate) fn meta(state: &AppState) -> Option<String> {
    let q = quote(state).filter(|_| state.btc_bump_locked.is_none())?;
    Some(format!("now {} sat/vB", rate_text(q.now.rate)))
}

const RATES_MIN_H: f32 = feeeditor::MIN_H + 2.0 * STAGE_PAD + 12.0 + grid::BUTTON_H + 2.0;

pub(crate) fn rates<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    grid::fill_or_scroll_by(RATES_MIN_H, move |size: Size| rates_body(state, size, cp, scale), cp, scale)
}

fn rates_body<'a>(state: &'a AppState, size: Size, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let inner = (size.width / scale).max(1.0);
    let ccy = state.base_currency.code();
    let fx = price::cross("BTC", ccy);
    let tiers = CHANNEL.btc_node_rx.borrow().tiers;
    let ready = ready(state).is_some();
    let middle: Element<'a, Message> = match quote(state) {
        None => container(text(NO_QUOTE).size(ROW_KEY * scale).color(cp.muted))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .boxed(),
        Some(q) => {
            let (cost, line) = reading(state, &q, tiers);
            let ladder = TIERS.map(|t| {
                let i = t.index().unwrap_or(0);
                feeeditor::Tier {
                    name: t.label(),
                    rate: q.tiers[i].map(|f| rate_text(f.rate)).or_else(|| tiers.map(|r| rate_text(r[i]))),
                    dot: tiers.map_or(cp.faint, |r| Verdict::of(&r, i).colour(cp)),
                    lit: state.btc_bump_lit == Some(t),
                    press: q.tiers[i].is_some().then_some(Message::BtcBumpFeeTierSelected(t)),
                }
            });
            let parts = feeeditor::Parts {
                eyebrow: "new fee",
                field: PlainField::BtcBumpFee,
                text: &state.btc_bump_fee,
                focus: state.focus_token(PlainField::BtcBumpFee),
                cost,
                line,
                tiers: ladder,
                on_submit: ready.then_some(Message::BtcBumpReviewClicked),
                price: fx,
                ccy,
            };
            screen::focus_stage(feeeditor::view(parts, inner - 2.0 * STAGE_PAD, cp, scale), PlainField::BtcBumpFee, scale)
        }
    };
    let buttons = grid::button_pair(
        grid::quiet_button("Back", Message::BtcBumpBack, cp, scale),
        grid::primary("Review", ready, Message::BtcBumpReviewClicked, cp, scale),
        scale,
    );
    column![middle, Space::new().height(12.0 * scale), buttons]
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed()
}

pub(crate) fn sign<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let ccy = state.base_currency.code();
        let fx = price::cross("BTC", ccy);
        let (_, _, key_deleted, key_mode) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
        let ours: Vec<String> = crate::wallet::btc_address().into_iter().collect();
        let send = state.btc_bump_txid.as_deref().and_then(|id| {
            CHANNEL.btc_transactions_rx.borrow().transactions.get(id).map(|t| tx::Tx::from(t, &ours))
        });
        let locked = state.btc_bump_locked;
        let with_fiat = |sats: u64, value: String| -> Vec<(String, iced::Color)> {
            let mut runs = vec![(value, cp.text), (" BTC".to_string(), cp.faint)];
            if fx > 0.0 {
                runs.push((format!("  \u{b7}  {} {}", fiat_cost(sats as f64 / 1e8 * fx), ccy), cp.faint));
            }
            runs
        };

        let mut facts: Column<Element<'_, Message>> = column![
            screen::address_block(send.as_ref().and_then(|t| t.to.as_deref()), TAIL, cp, scale),
            Space::new().height(7.0 * scale),
        ]
        .width(Length::Fill);
        let amount = send.as_ref().map(|t| t.sats());
        facts = facts.push(drow(
            "Amount",
            amount.map_or(vec![(NA.to_string(), cp.faint)], |a| vec![(btc(a), cp.text), (" BTC".to_string(), cp.faint)]),
            2.0,
            cp,
            scale,
        ));
        let new_fee = locked.map_or(vec![(NA.to_string(), cp.faint)], |f| with_fiat(f.fee, btc(f.fee)));
        facts = facts.push(
            container(
                row![
                    text("New fee").size(ROW_KEY * scale).color(cp.dim),
                    Space::new().width(5.0 * scale),
                    text(state.btc_bump_fee_tier.label()).font(MONO).size(HINT * scale).color(cp.faint),
                    Space::new().width(Length::Fill),
                    compact::mono_runs(new_fee, grid::ROW_VALUE * scale),
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(Padding::new(0.0).top(2.0 * scale).bottom(3.0 * scale))
            .boxed(),
        );
        facts = facts.push(drow(
            "Fee now",
            locked.map_or(vec![(NA.to_string(), cp.faint)], |f| {
                vec![(btc(f.fee - f.extra), cp.dim), (" BTC".to_string(), cp.faint)]
            }),
            2.0,
            cp,
            scale,
        ));
        let total = match (amount, locked) {
            (Some(a), Some(f)) => {
                let mut runs = vec![(btc(a + f.fee), cp.text), (" BTC".to_string(), cp.faint)];
                if fx > 0.0 {
                    runs.push((format!("  \u{b7}  {} {}", fiat_amount((a + f.fee) as f64 / 1e8 * fx), ccy), cp.faint));
                }
                runs
            }
            _ => vec![(NA.to_string(), cp.faint)],
        };
        facts = facts.push(group_rule(cp, scale)).push(
            row![
                text("Total").size(ROW_KEY * scale).color(cp.dim),
                Space::new().width(Length::Fill),
                compact::mono_runs(total, ROW_TOTAL * scale),
            ]
            .align_y(Alignment::Center)
            .boxed(),
        );

        let fields = SignFields {
            mode: EnableInputMode::detect(key_deleted, key_mode),
            secret: SecureField::BtcBumpPassphrase,
            secret_buf: &state.btc_bump_passphrase,
            secret_reveal: state.btc_bump_passphrase_reveal,
            phrase: SecureField::BtcBumpSeed,
            phrase_buf: &state.btc_bump_seed_text,
            phrase_reveal: state.btc_bump_seed_reveal,
            bip39: SecureField::BtcBumpBip39,
            bip39_buf: &state.btc_bump_bip39,
            bip39_reveal: state.btc_bump_bip39_reveal,
            line: &state.phrase_line,
            on_submit: Message::BtcBumpSubmitClicked,
        };
        let armed = locked.is_some() && send.as_ref().is_some_and(|t| t.can_bump());
        let buttons = grid::button_pair(
            grid::quiet_button("Back", Message::BtcBumpBack, cp, scale),
            grid::primary("Sign & send", armed && fields.can_submit(), Message::BtcBumpSubmitClicked, cp, scale),
            scale,
        );
        let sign = grid::sign_block(
            &fields,
            &state.btc_bump_passphrase,
            &state.btc_bump_seed_text,
            &state.btc_bump_bip39,
            armed,
            buttons,
            inner,
            cp,
            scale,
        );
        grid::scroller(column![facts, sign].width(Length::Fill).boxed(), cp, scale)
    })
    .boxed()
}

