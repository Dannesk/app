//! Modify fee — the **lead** of the transactions pane's `modify fee` face
//! ([`crate::ui::managebtc::panes::transactions`]): the transaction, what
//! it pays now, and the tier row for what it will pay instead. The pane
//! puts the inline sign block under it, on the bump flow's own buffers.
//!
//! The precedent is XRP's cancel face: `modify fee ›` on a pending outgoing
//! row arms `btc_bump_txid`. The transaction's body is already in its history
//! record (indexd's mempool frame, carried by the relay) and the two policy
//! numbers are on the node frame, so the picker is live at once — nothing is
//! fetched, ever. A row from before records carried the body says so in
//! place of a picker. Nothing here is a backend error — those are the
//! activity log's, once a broadcast exists to fail.
//!
//! **One planner, two readers.** Every number on this face comes from
//! [`bitcoin_payment::plan_replacement`], and the controller commits the
//! plan's fee at the press ([`fee_to_sign`]) — the signer re-plans from that
//! fee and lands on the same shape by construction. A tier is offered exactly
//! when the plan at its rate satisfies BIP125; the rest read inert, and the
//! custom box says what the least acceptable fee is.

use iced::widget::Row;
use iced::Widget as _;
use iced::widget::{column, row, text, Space};
use iced::{Alignment, Color, Element, Length};

use crate::channel::{BtcRbfInfo, CHANNEL};
use crate::controller::app_state::{AppState, BtcFeeTier};
use crate::controller::message::{Message, PlainField};
use crate::ui::components::compact;
use crate::ui::components::send_screen::{self as screen, READOUT};
use crate::ui::managebtc::btcsend::{tier_cell, FEE_BOX_W, TIER_GAP};
use crate::ui::managexrp::xrptrade::{NOTE, ROW};
use crate::utils::fonts::MONO;
use crate::utils::format_usd;
use crate::utils::theme::CompactPalette;
use crate::ws::commands::bitcoin_payment::{self, ReplacementPlan, Utxo};

/// A row recorded before records carried the transaction body, or a node
/// frame that has not reported yet. Nothing to price against, so nothing to
/// sign — and nothing to ask for, either: the body rides the record.
const NO_BODY: &str = "no details on record for this transaction";
const NO_TIERS: &str = "no fee rates from the node yet \u{2014} set one yourself";

// ── What the face prices against ───────────────────────────────────────────

/// The armed transaction's body, from its own history record and the node
/// frame. See [`bitcoin_payment::rbf_info_for`].
fn quote(state: &AppState) -> Option<BtcRbfInfo> {
    bitcoin_payment::rbf_info_for(state.btc_bump_txid.as_deref()?)
}

/// Everything the planner needs besides a fee.
struct Pricing {
    info: BtcRbfInfo,
    ours: Vec<String>,
    spare: Option<Utxo>,
}

fn pricing(state: &AppState) -> Option<Pricing> {
    let (_, wallet, _, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
    let ours: Vec<String> = crate::wallet::btc_address_records().into_iter().map(|r| r.address).collect();
    let info = quote(state)?;
    let spare = wallet.as_deref().and_then(|w| bitcoin_payment::spare_coin(w, &info));
    Some(Pricing { info, ours, spare })
}

/// The four named tiers, each planned at its rate — `Err` is a tier BIP125
/// will not take. `None` when the node has reported no tiers.
fn tier_plans(p: &Pricing) -> Option<[Result<ReplacementPlan, String>; 4]> {
    let tiers = CHANNEL.btc_node_rx.borrow().tiers?;
    Some(tiers.map(|rate| bitcoin_payment::quote_replacement(&p.info, &p.ours, p.spare.as_ref(), rate)))
}

fn effective_tier(state: &AppState, plans: &Option<[Result<ReplacementPlan, String>; 4]>) -> BtcFeeTier {
    match state.btc_bump_fee_tier {
        BtcFeeTier::Custom => BtcFeeTier::Custom,
        named if plans.is_some() => named,
        _ => BtcFeeTier::Custom,
    }
}

/// The plan for the current selection: the tier's, or the custom fee's.
fn resolved_plan(
    state: &AppState,
    p: &Pricing,
    plans: &Option<[Result<ReplacementPlan, String>; 4]>,
) -> Result<ReplacementPlan, String> {
    match effective_tier(state, plans) {
        BtcFeeTier::Custom => {
            let fee: u64 = state
                .btc_bump_fee
                .trim()
                .parse()
                .map_err(|_| String::new())?;
            bitcoin_payment::plan_replacement(&p.info, &p.ours, p.spare.as_ref(), fee)
        }
        named => {
            let i = named.index().ok_or_else(String::new)?;
            plans.as_ref().ok_or_else(|| NO_TIERS.to_string())?[i].clone()
        }
    }
}

/// The fee the controller commits at the press: the current plan's, or
/// nothing — in which case nothing is dispatched. Read from the same planner
/// the face drew from, at the same instant.
pub fn fee_to_sign(state: &AppState) -> Option<u64> {
    let p = pricing(state)?;
    let plans = tier_plans(&p);
    resolved_plan(state, &p, &plans).ok().map(|plan| plan.fee)
}

// ── The lead ────────────────────────────────────────────────────────────────

fn short(txid: &str) -> String {
    let n = txid.chars().count();
    if n <= 8 {
        return txid.to_string();
    }
    let head: String = txid.chars().take(4).collect();
    let tail: String = txid.chars().skip(n - 4).collect();
    format!("{head}\u{2026}{tail}")
}

/// Line one names the transaction; line two what it pays now; then the tier
/// row and its readout. Returns the element and whether the CTA is armed.
pub(crate) fn lead<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> (Element<'a, Message>, bool) {
    let txid = state.btc_bump_txid.as_deref().unwrap_or("");
    let title = text(format!("Modify fee \u{b7} {}", short(txid)))
        .font(MONO)
        .size(ROW * scale)
        .color(cp.text);
    let mut col = column![title].width(Length::Fill);

    let Some(p) = pricing(state) else {
        col = col.push(Space::new().height(4.0 * scale).boxed()).push(note(NO_BODY, cp.muted, scale));
        return (col.boxed(), false);
    };

    let paying = format!(
        "paying {} sats \u{b7} {:.1} sat/vB",
        p.info.fee_sats,
        p.info.fee_sats as f64 / p.info.vsize.max(1) as f64
    );
    col = col.push(Space::new().height(4.0 * scale).boxed()).push(note(&paying, cp.dim, scale));

    let plans = tier_plans(&p);
    let tier = effective_tier(state, &plans);
    let plan = resolved_plan(state, &p, &plans);

    // The selected tier's colour: green at or above the going rate, amber
    // below it, dim when there is no number to rate — the send screen's rule.
    let going = plans
        .as_ref()
        .and_then(|ps| ps[2].as_ref().ok().map(|x| x.rate_sat_vb()));
    let colour: Color = match (&plan, going) {
        (Ok(x), Some(g)) if x.rate_sat_vb() >= g => cp.green,
        (Ok(_), _) => cp.amber,
        _ => cp.dim,
    };

    let mut tiers_row: Row<Element<'_, Message>> = row![].align_y(Alignment::Center);
    for (i, t) in BtcFeeTier::ALL.into_iter().enumerate() {
        if i > 0 {
            tiers_row = tiers_row.push(Space::new().width(TIER_GAP * scale).boxed());
        }
        let offered = match t.index() {
            None => true,
            Some(idx) => plans.as_ref().is_some_and(|ps| ps[idx].is_ok()),
        };
        tiers_row = tiers_row.push(tier_cell(
            t, t == tier, offered, colour, Message::BtcBumpFeeTierSelected, cp, scale,
        ));
    }

    let ccy = state.base_currency.code();
    let sym = state.base_currency.symbol();
    let rate = crate::utils::price::cross("BTC", ccy);
    let readout = |x: &ReplacementPlan, lead_with_fee: bool| -> Element<'a, Message> {
        let mut runs: Vec<(String, Color)> = Vec::new();
        if lead_with_fee {
            runs.push((format!("{} sats", x.fee), colour));
            runs.push((" \u{b7} ".to_string(), cp.faint));
        } else {
            runs.push(("\u{b7} ".to_string(), cp.faint));
        }
        if rate > 0.0 {
            runs.push((format!("\u{2248} {sym}{}", format_usd(x.fee as f64 / 1e8 * rate)), cp.muted));
            runs.push((" \u{b7} ".to_string(), cp.faint));
        }
        runs.push((format!("{:.1} sat/vB", x.rate_sat_vb()), cp.muted));
        if x.added_input {
            runs.push((" \u{b7} adds a coin".to_string(), cp.muted));
        }
        screen::mono_line(runs, READOUT, scale)
    };

    let output: Element<'a, Message> = if tier == BtcFeeTier::Custom {
        let least = bitcoin_payment::min_replacement_fee(&p.info, p.info.vsize);
        let side: Element<'a, Message> = match (&plan, state.btc_bump_fee.trim().is_empty()) {
            (Ok(x), _) => readout(x, false),
            (Err(_), true) => screen::mono_line(vec![(format!("at least {least}"), cp.muted)], READOUT, scale),
            (Err(e), false) => screen::mono_line(vec![(e.clone(), cp.red)], READOUT, scale),
        };
        row![
            screen::boxed_plain(
                PlainField::BtcBumpFee,
                &state.btc_bump_fee,
                "0",
                FEE_BOX_W,
                compact::FIELD,
                false,
                None,
                0.0,
                Some(screen::unit("sats", cp, scale)),
                29.0,
                Some(Message::BtcBumpSubmitClicked),
                cp,
                scale,
            ),
            Space::new().width(8.0 * scale),
            side,
        ]
        .align_y(Alignment::Center)
        .boxed()
    } else {
        match &plan {
            Ok(x) => readout(x, true),
            Err(_) if plans.is_none() => note(NO_TIERS, cp.muted, scale),
            Err(e) => note(e, cp.muted, scale),
        }
    };

    col = col
        .push(Space::new().height(10.0 * scale).boxed())
        .push(compact::eyebrow("new fee", tier.label(), cp, scale))
        .push(tiers_row.boxed())
        .push(Space::new().height(3.0 * scale).boxed())
        .push(output);

    (col.boxed(), plan.is_ok())
}

fn note<'a>(s: &str, color: Color, scale: f32) -> Element<'a, Message> {
    text(s.to_string()).font(MONO).size(NOTE * scale).color(color).boxed()
}
