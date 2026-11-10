use iced::widget::Column;
use iced::Widget as _;
use iced::widget::{column, container, responsive, row, text, Space};
use iced::{Alignment, Element, Length, Padding, Size};

use crate::channel::CHANNEL;
use crate::controller::app_state::{AppState, BtcFeeTier, EnableInputMode};
use crate::controller::message::{Message, PlainField, SecureField};
use crate::ui::components::grid::{self, drow, group_rule, FIELD_VALUE, HINT, NA, ROW_KEY, ROW_TOTAL};
use crate::ui::components::send_screen as screen;
use crate::ui::components::signing::SignFields;
use crate::ui::components::compact;
use crate::ui::managebtc::btcdashboard::{choice_button, fee_choice, sats_cost, Caption, Gutter, RateCell, TAIL};
use crate::ui::managebtc::btcsend;
use crate::ui::managebtc::btctransactions::{self as tx, btc};
use crate::ui::managebtc::node::{rate_text, Verdict};
use crate::utils::fonts::MONO;
use crate::utils::theme::CompactPalette;
use crate::utils::{add_commas, fiat_amount, fiat_cost, price};
pub use crate::utils::btc_bump::{BtcBumpDraft, BtcBumpQuote, BumpFee};

const NO_QUOTE: &str = "waiting for this send\u{2019}s details and today\u{2019}s rates";

pub fn draft(state: &AppState) -> Option<BtcBumpDraft> {
    let txid = state.btc_bump_txid.clone()?;
    let custom = state
        .btc_bump_custom_open
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

pub fn custom_ready(state: &AppState) -> Option<BumpFee> {
    if !state.btc_bump_custom_open {
        return None;
    }
    let q = live_quote(state)?;
    q.custom.filter(|c| c.rate >= q.min_rate)
}

fn custom_caption(q: &BtcBumpQuote, custom: Option<BumpFee>, tiers: Option<[f32; 4]>) -> Caption {
    let least = || Caption::Note(format!("at least {} sat/vB", rate_text(q.min_rate)));
    match custom {
        Some(c) if c.rate < q.min_rate => Caption::Warning(format!("below the {} minimum", rate_text(q.min_rate))),
        Some(c) => tiers.map_or_else(least, |t| Caption::Verdict(tx::wait(c.rate, &t))),
        None => least(),
    }
}

pub(crate) fn rates<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let ccy = state.base_currency.code();
    let fx = price::cross("BTC", ccy);
    let tiers = CHANNEL.btc_node_rx.borrow().tiers;
    let open = state.btc_bump_custom_open;
    let mut col: Column<Element<'_, Message>> = column![].width(Length::Fill);

    let ready = custom_ready(state).is_some();
    match quote(state) {
        None => {
            col = col.push(text(NO_QUOTE).size(ROW_KEY * scale).color(cp.muted).boxed());
        }
        Some(q) => {
            let now = fee_choice(
                "now",
                None,
                tiers.map(|t| Caption::Verdict(tx::wait(q.now.rate, &t))),
                RateCell::Text(rate_text(q.now.rate), q.now.vsize),
                sats_cost(Some(q.now.fee), fx, ccy, cp),
                Gutter::Check(true),
                cp,
                scale,
            );
            col = col.push(choice_button(now, !open, None, cp, scale));
            for (i, t) in [BtcFeeTier::Minimum, BtcFeeTier::Low, BtcFeeTier::Med, BtcFeeTier::High].into_iter().enumerate() {
                let Some(b) = q.tiers[i] else { continue };
                let body = fee_choice(
                    t.label(),
                    None,
                    tiers.map(|ts| Caption::Verdict(Verdict::of(&ts, i))),
                    RateCell::Text(rate_text(b.rate), b.vsize),
                    sats_cost(Some(b.fee), fx, ccy, cp),
                    Gutter::Check(false),
                    cp,
                    scale,
                );
                col = col.push(compact::hairline(cp.rule));
                col = col.push(choice_button(body, false, Some(Message::BtcBumpFeeTierSelected(t)), cp, scale));
            }
            col = col.push(compact::hairline(cp.rule));
            let custom_row = if open {
                let typed = live_quote(state).and_then(|lq| lq.custom);
                let field = screen::boxed_plain(
                    PlainField::BtcBumpFee,
                    &state.btc_bump_fee,
                    "0.00",
                    btcsend::FEE_BOX_W,
                    FIELD_VALUE,
                    false,
                    None,
                    0.0,
                    None,
                    0.0,
                    ready.then_some(Message::BtcBumpReviewClicked),
                    cp,
                    scale,
                );
                let body = fee_choice(
                    "custom",
                    None,
                    Some(custom_caption(&q, typed, tiers)),
                    RateCell::Field(field),
                    sats_cost(typed.map(|c| c.fee), fx, ccy, cp),
                    Gutter::Check(false),
                    cp,
                    scale,
                );
                choice_button(body, true, None, cp, scale)
            } else {
                let body = fee_choice(
                    "custom",
                    None,
                    Some(custom_caption(&q, None, tiers)),
                    RateCell::Empty,
                    Vec::new(),
                    Gutter::Chevron,
                    cp,
                    scale,
                );
                choice_button(body, false, Some(Message::BtcBumpFeeTierSelected(BtcFeeTier::Custom)), cp, scale)
            };
            col = col.push(custom_row);
        }
    }

    let buttons = if open {
        grid::button_pair(
            grid::quiet_button("Back", Message::BtcBumpBack, cp, scale),
            grid::primary("Review", ready, Message::BtcBumpReviewClicked, cp, scale),
            scale,
        )
    } else {
        grid::quiet_button("Back", Message::BtcBumpBack, cp, scale)
    };
    column![grid::scroller(col.boxed(), cp, scale), Space::new().height(12.0 * scale), buttons]
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed()
}

pub(crate) fn remark(fee: &BumpFee) -> String {
    let extra = add_commas(fee.extra as i64);
    if fee.adds_coin {
        format!("It replaces the pending send: same amount and recipient, and one more of your coins pays the extra {extra} sat.")
    } else {
        format!("It replaces the pending send: same amount and recipient, and the extra {extra} sat comes out of what returns to your address.")
    }
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
            let mut runs = vec![(value, cp.text), (" btc".to_string(), cp.faint)];
            if fx > 0.0 {
                runs.push((format!("  \u{b7}  {} {}", fiat_cost(sats as f64 / 1e8 * fx), ccy.to_lowercase()), cp.faint));
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
            "amount",
            amount.map_or(vec![(NA.to_string(), cp.faint)], |a| vec![(btc(a), cp.text), (" btc".to_string(), cp.faint)]),
            2.0,
            cp,
            scale,
        ));
        let new_fee = locked.map_or(vec![(NA.to_string(), cp.faint)], |f| with_fiat(f.fee, btc(f.fee)));
        facts = facts.push(
            container(
                row![
                    text("new fee").size(ROW_KEY * scale).color(cp.dim),
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
            "fee now",
            locked.map_or(vec![(NA.to_string(), cp.faint)], |f| {
                vec![(btc(f.fee - f.extra), cp.dim), (" btc".to_string(), cp.faint)]
            }),
            2.0,
            cp,
            scale,
        ));
        let total = match (amount, locked) {
            (Some(a), Some(f)) => {
                let mut runs = vec![(btc(a + f.fee), cp.text), (" btc".to_string(), cp.faint)];
                if fx > 0.0 {
                    runs.push((format!("  \u{b7}  {} {}", fiat_amount((a + f.fee) as f64 / 1e8 * fx), ccy.to_lowercase()), cp.faint));
                }
                runs
            }
            _ => vec![(NA.to_string(), cp.faint)],
        };
        facts = facts.push(group_rule(cp, scale)).push(
            row![
                text("total").size(ROW_KEY * scale).color(cp.dim),
                Space::new().width(Length::Fill),
                compact::mono_runs(total, ROW_TOTAL * scale),
            ]
            .align_y(Alignment::Center)
            .boxed(),
        );
        if let Some(f) = &locked {
            facts = facts.push(
                container(text(remark(f)).size(ROW_KEY * scale).color(cp.dim))
                    .width(Length::Fill)
                    .padding(Padding::new(0.0).top(10.0 * scale))
                    .boxed(),
            );
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn fee(extra: u64, adds_coin: bool) -> BumpFee {
        BumpFee { rate: 0.52, fee: 48 + extra, vsize: 141, extra, adds_coin }
    }

    #[test]
    fn the_remark_says_where_the_extra_comes_from_without_saying_change() {
        let kept = remark(&fee(26, false));
        assert!(kept.contains("extra 26 sat comes out of what returns to your address"), "{kept}");
        let coin = remark(&fee(1_200, true));
        assert!(coin.contains("one more of your coins pays the extra 1,200 sat"), "{coin}");
        assert!(!kept.contains("change") && !coin.contains("change"));
    }
}
