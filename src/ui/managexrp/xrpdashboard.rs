//! The XRP dashboard as a **pane grid** — phase 1 of the pane system
//! (design handoff `design_handoff_xrp_dashboard_panes`, 2026-09-09).
//!
//! Five panes in a binary split tree — `balance` over `chart` on the left,
//! `send` down the middle, `receive` over `network` on the right — every one
//! of them closable, splittable, draggable and resizable, and a `panels +`
//! menu that can re-establish any of it. The model (which panes are open,
//! who inherits what on close, presets, undo) is
//! [`crate::controller::panes`]; the chrome — top bar, grid, strips, menu,
//! the empty well, the shared rows, the chart and receive bodies — is
//! [`crate::ui::components::grid`], shared with the BTC dashboard since
//! 2026-09-10 and moved there verbatim. This file is what is XRP about the
//! grid: the balance, send and network panes, and which kind draws what.
//!
//! **The old dashboard is gone** (deleted 2026-09-09, once every one of its
//! screens had a pane or a pane face and the user had run them): the balance
//! screen, the receive page, the transactions modal, the cancel stack, the
//! key stack faces, the tokens page and the trade screen. What survives of
//! those files is the logic the panes draw: the transaction row model, the
//! ticket's form, the pair listing, the depth chart, the token row columns.
//!
//! What this pass deliberately leaves out (user, 2026-09-09: "hyperfocused
//! on one thing"): the bell and settings in the top bar, chrome hiding, the
//! Balance tab's removal. Getting the default layout's behaviour right is
//! the 80%. The eight remaining panes landed the same afternoon as phase 2 —
//! see `managexrp::panes`.

use iced::Widget as _;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame};
use iced::widget::pane_grid::Pane;
use iced::widget::{column, container, responsive, row, text, Space};
use iced::{Alignment, Color, Element, Length, Padding, Point, Rectangle, Size};

use crate::channel::{XrpNodeStats as NodeFrame, CHANNEL};
use crate::controller::app_state::{AppState, ChartPeriod, EnableInputMode, XrpTokenTab};
use crate::controller::message::{Message, PlainField, SecureField, TagPane};
use crate::controller::panes::{Chain, GridMsg, LedgerBar, PaneKind};
use crate::controller::xrp::{self, send_fits, xrp_fee, xrp_reserve};
use crate::ui::components::grid::{self, big_line, HERO, HERO_UNIT, ROW_KEY, ROW_TOTAL, ADDR, HINT, AMT_GAP, EQ_W};
use crate::ui::components::send_screen::{self as screen};
use crate::ui::components::signing::SignFields;
use crate::utils::{fiat_amount, money};
use crate::ui::components::compact;
use crate::ui::managexrp::panes as panes_ui;
use crate::ui::managexrp::xrpsend;
use crate::utils::fonts::{LIGHT, MONO};
use crate::utils::sparkline::{change_pct, chart_data};
use crate::utils::theme::{self, CompactPalette};
use crate::utils::{add_commas, format_token_amount, price, tokens};

// The chrome's rows, labels, buttons and words, re-exported so the pane
// files under `managexrp::panes` keep importing them from here.
pub(crate) use crate::ui::components::grid::{
    drow, group_rule, label, pane_button, rule_label, BIP39_PLACEHOLDER, FIELD_VALUE, NA, ROW_VALUE, STRIP,
};

// ── Type scale and geometry that are XRP's own ──────────────────────────────

/// The tape's bar pitch — 24 bars at the default width, 48 at double.
const TAPE_PITCH: f32 = 11.5;
const TAPE_GAP: f32 = 2.0;
/// Characters of the address the review underlines.
const TAIL: usize = 3;

// ── The screen ──────────────────────────────────────────────────────────────

pub fn view(state: &AppState) -> Element<'_, Message> {
    let cp = theme::compact(&state.theme);
    let scale = state.scale();
    let g = &state.xrp_grid;
    let host = grid::Host {
        chain: Chain::Xrp,
        title: "XRP",
        grid: g,
        wrap: Message::Grid,
        bar: Some(grid::BarSlot { control: panes_ui::pair::control, layer: panes_ui::pair::layer }),
    };
    // No modal: the send pane's asset picker and both tag forms are FACES of
    // their panes, like `restore key` and the cancel stack (2026-09-15). The
    // top bar's pair picker is the one overlay, and it belongs to no pane.
    grid::screen(host, state, pane_content, cp, scale)
}

/// Four decimals — the XRP pane's price.
fn four_dp(v: f32) -> String {
    format!("{v:.4}")
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
    // The three market panes print the top bar's pair after their title —
    // plain type, no edge, no hit target: a readout, not a control. Before a
    // pair is held the title stands alone.
    let pair_strip = |word: &str| -> Element<'a, Message> {
        match panes_ui::pair::label(state) {
            Some(pair) => row![
                strip(word),
                Space::new().width(panes_ui::pair::PANE_GAP * scale),
                text(pair).font(MONO).size(panes_ui::pair::PANE_LINE * scale).color(cp.text),
            ]
            .align_y(Alignment::Center)
            .boxed(),
            None => strip(word),
        }
    };

    let (title, meta, body): (Element<'a, Message>, Option<Element<'a, Message>>, Element<'a, Message>) =
        match kind {
            PaneKind::Balance => (strip("balance"), None, balance_pane(state, cp, scale)),
            PaneKind::Chart => {
                let series = chart_series(state);
                let prices: Vec<f32> = series.iter().map(|p| p.1).collect();
                let ccy = state.base_currency.code();
                // The change wears the range's direction; no period suffix —
                // the pill says it.
                let (word, colour) = match change_pct(&prices) {
                    Some(p) => (format!("{p:+.2}%"), if p >= 0.0 { cp.green } else { cp.red }),
                    None => (NA.to_string(), cp.faint),
                };
                (
                    row![
                        strip("chart"),
                        Space::new().width(9.0 * scale),
                        grid::period_pill(state.xrp_grid.chart_period, Message::Grid, cp, scale),
                    ]
                    .align_y(Alignment::Center)
                    .boxed(),
                    Some(text(word).font(MONO).size(STRIP * scale).color(colour).boxed()),
                    grid::chart_pane(
                        series,
                        price::cross("XRP", ccy) as f32,
                        state.xrp_grid.chart_period,
                        &format!("xrp / {}", ccy.to_lowercase()),
                        four_dp,
                        theme::is_dark(&state.theme),
                        cp,
                        scale,
                    ),
                )
            }
            PaneKind::Send => (strip(send_title(state)), None, send_pane(state, cp, scale)),
            PaneKind::Receive if state.receive_tag_editing => (
                strip("destination tag"),
                None,
                grid::tag_face(TagPane::Receive, &state.receive_tag_draft, xrp::tag_wrong(&state.receive_tag_draft), cp, scale),
            ),
            PaneKind::Receive => {
                // The classic address, or — with a tag set — the same
                // account as a tagged X-address. `xrp::receive_address` is
                // what the copy glyph puts on the clipboard, so the QR, the
                // line and the copy are one string.
                let address = CHANNEL
                    .wallet_balance_rx
                    .borrow()
                    .1
                    .as_deref()
                    .map(|a| xrp::receive_address(state, a))
                    .unwrap_or_else(|| "No Address".to_string());
                let link = (grid::tag_word(xrp::tag_of(&state.receive_tag)), Message::TagEdit(TagPane::Receive));
                (
                    strip("receive"),
                    None,
                    grid::receive_pane(address, state.xrp_copy_feedback, Message::CopyAddress, link, cp, scale),
                )
            }
            PaneKind::Network => (strip("network"), None, network_pane(state, cp, scale)),
            // Phase 2 — `managexrp::panes`. A pane that has become a form
            // (restore key, cancel offer, enable trustline) says so in its
            // title; the depth pane's meta is its state.
            PaneKind::Wallet => (strip(panes_ui::wallet::title(state)), None, panes_ui::wallet::view(state, cp, scale)),
            PaneKind::Ticket => (pair_strip("ticket"), None, panes_ui::ticket::view(state, cp, scale)),
            PaneKind::Book => (pair_strip("order book"), None, panes_ui::market::book(state, cp, scale)),
            PaneKind::Depth => (
                pair_strip("depth"),
                Some(text(panes_ui::market::depth_meta(state)).font(MONO).size(STRIP * scale).color(cp.faint).boxed()),
                panes_ui::market::depth(state, cp, scale),
            ),
            PaneKind::Transactions => (strip("transactions"), None, panes_ui::lists::transactions(state, cp, scale)),
            PaneKind::Orders => (strip(panes_ui::lists::orders_title(state)), None, panes_ui::lists::orders(state, cp, scale)),
            PaneKind::Tokens => (strip(panes_ui::tokens::held_title(state)), None, panes_ui::tokens::held(state, cp, scale)),
            PaneKind::AvailableTokens => (
                strip(panes_ui::tokens::available_title(state)),
                None,
                panes_ui::tokens::available(state, cp, scale),
            ),
            // BTC's kinds cannot be loaded into an XRP layout (`from_key` is
            // per chain); if one ever were, it is a well, not a panic.
            PaneKind::Fees | PaneKind::Blocks | PaneKind::Mempool | PaneKind::Intervals | PaneKind::Empty => {
                (Space::new().boxed(), None, grid::well_pane(Message::Grid, cp, scale))
            }
        };

    grid::pane_frame(pane, kind, maximized, title, meta, body, Message::Grid, cp, scale)
}

// ── balance ─────────────────────────────────────────────────────────────────

/// Everything the account holds: the fiat total of XRP plus issued tokens as
/// the hero, one raw row per holding under it, then `available` and
/// `reserved` — in XRP, under a rule, because they qualify the XRP line, not
/// the total. Balance is the chain you are on: no scope toggle.
fn balance_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let (xrp_amount, _, _, _) = CHANNEL.wallet_balance_rx.borrow().clone();
    let hide = state.hide_balance;
    let ccy = state.base_currency.code();
    let mask = |s: String| if hide { "\u{2022}".repeat(4) } else { s };

    let mut total = xrp_amount * price::cross("XRP", ccy);
    let mut rows: Vec<Element<'a, Message>> = vec![drow(
        "xrp",
        vec![(mask(format!("{xrp_amount:.6}")), cp.text)],
        9.0,
        cp,
        scale,
    )];
    // Every ENABLED token has a row, at zero too (user, 2026-09-15): the
    // line is the fact the pane reports, and a line at zero is the one the
    // holder is about to fund or close. The send picker is the surface that
    // hides an empty one — nothing can be sent from it.
    for t in tokens::TOKENS {
        let (bal, has, _) = CHANNEL.token(t.code);
        if !has {
            continue;
        }
        total += bal * price::cross(t.rate_key, ccy);
        rows.push(drow(
            &t.display.to_lowercase(),
            // Six places, not two: a tail under a cent is still a balance,
            // and `0.00` on a line that cannot be closed is a lie.
            vec![(mask(format_token_amount(bal, 6)), cp.text)],
            3.0,
            cp,
            scale,
        ));
    }

    let hero_value = if hide { "\u{2022}".repeat(6) } else { money(total) };
    let hero = row![
        text(hero_value).font(LIGHT).size(HERO * scale).color(cp.text),
        Space::new().width(8.0 * scale),
        container(text(ccy.to_uppercase()).font(MONO).size(HERO_UNIT * scale).color(cp.muted))
            .padding(Padding::new(0.0).bottom(5.0 * scale)),
    ]
    .align_y(Alignment::End);

    let mut col = column![hero, column(rows).width(Length::Fill), group_rule(cp, scale)].width(Length::Fill);

    // Chain-exact or nothing: the reserve the ledger states, or `—` until it
    // has. An account that does not exist yet has no reserve to state.
    if CHANNEL.xrp_exists() {
        match xrp_reserve() {
            Some(r) => {
                col = col
                    .push(drow(
                        "available",
                        vec![(mask(format!("{:.6}", r.available)), cp.text), (" xrp".to_string(), cp.faint)],
                        3.0,
                        cp,
                        scale,
                    ))
                    .push(drow(
                        "reserved",
                        vec![(format!("{:.6}", r.total), cp.dim), (" xrp".to_string(), cp.faint)],
                        3.0,
                        cp,
                        scale,
                    ));
            }
            None => {
                col = col
                    .push(drow("available", vec![(NA.to_string(), cp.muted)], 3.0, cp, scale))
                    .push(drow("reserved", vec![(NA.to_string(), cp.muted)], 3.0, cp, scale));
            }
        }
    } else {
        col = col.push(drow("account", vec![("inactive".to_string(), cp.dim)], 3.0, cp, scale));
    }

    grid::scroller(col.boxed(), cp, scale)
}

// ── chart ───────────────────────────────────────────────────────────────────

/// The XRP series in the display currency for the pane's period.
fn chart_series(state: &AppState) -> Vec<(u64, f32)> {
    let history = match state.xrp_grid.chart_period {
        ChartPeriod::OneHour => &state.rate_history_short,
        ChartPeriod::OneDay => &state.rate_history_long,
    };
    chart_data("XRP", state.base_currency.code(), history)
}

// ── send ────────────────────────────────────────────────────────────────────

/// The whole transaction in one pane: recipient, the amount duo, the live
/// review, the credential, `Send payment`. **The stack follows its
/// content**: fixed gaps, the button right after the last field, and the
/// pane's spare height below it. The mock's `space-between` — first element
/// on the top edge, button on the bottom edge, the leftover shared by the
/// gaps — opened three holes at any height above the default, and the user
/// called it (2026-09-09); it is the same rule the compose screens settled
/// on. No signing modal: the stack has no meaning without a transaction in
/// front of it, and the review is right here.
///
/// The gate lights the button on exactly what the controller re-checks; the
/// inputs are send v3's own fields and buffers, so nothing about how a
/// payment is validated, signed or sent lives in this file.
/// The send pane's title follows its face.
fn send_title(state: &AppState) -> &'static str {
    if state.send_asset_picker_open {
        "asset"
    } else if state.send_tag_editing {
        "destination tag"
    } else {
        "send"
    }
}

fn send_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    // The two faces the pane can wear instead of the form. Both swap back
    // by themselves: a pick, `Set tag`, or `Cancel`.
    if state.send_asset_picker_open {
        return xrpsend::asset_face(&state.xrp_token_tab, state.base_currency.code(), cp, scale);
    }
    if state.send_tag_editing {
        return grid::tag_face(TagPane::Send, &state.send_tag_draft, xrp::tag_wrong(&state.send_tag_draft), cp, scale);
    }
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let is_xrp = matches!(state.xrp_token_tab, XrpTokenTab::Xrp);
        let display = xrpsend::asset_display(&state.xrp_token_tab);
        let unit = display.to_lowercase();
        let ccy = state.base_currency.code();
        let (_, own_xrp, key_deleted, key_mode) = CHANNEL.wallet_balance_rx.borrow().clone();

        // ── The facts, resolved once ──────────────────────────────────────
        let addr = state.send_recipient.trim();
        let resolved = crate::utils::xaddress::resolve(addr);
        let from_x = resolved.as_ref().is_some_and(|r| r.from_xaddress);
        let baked_tag = resolved.as_ref().and_then(|r| if r.from_xaddress { r.tag } else { None });
        let is_self = resolved
            .as_ref()
            .is_some_and(|r| own_xrp.as_deref() == Some(r.classic.as_str()));
        let set_tag = xrp::tag_of(&state.send_destination_tag);
        let addr_ok = !is_self && resolved.is_some();
        let amount = state.send_amount.trim().parse::<f64>().ok().filter(|v| *v > 0.0);
        let insufficient = amount.is_some_and(|a| !send_fits(state, a));
        let valid = addr_ok && amount.is_some() && !insufficient;
        let fee_drops = CHANNEL.xrp_node_rx.borrow().open_ledger_fee;
        let fee = xrp_fee();
        let rate = price::cross("XRP", ccy);
        let has_choice = xrpsend::holdings().len() > 1;

        // ── recipient ─────────────────────────────────────────────────────
        // Self first, then whatever is wrong with the string itself. Before
        // this (2026-09-12) only the self case spoke and every other fault
        // left the static hint standing under a dark button — so editing a
        // character of a pasted address, which is precisely what the base58
        // checksum exists to catch, disabled `Send payment` and said
        // nothing about why. `recipient_fault` decides the line; `addr_ok`
        // below still decides the gate, off `resolve` as it always did.
        let fault = xrpsend::recipient_fault(addr);
        // The line under the field speaks only for a fault; the two forms
        // are the placeholder (2026-09-14, with the BTC twin).
        let hint: Option<Element<'a, Message>> = if is_self {
            Some(compact::mono_runs(vec![(xrpsend::SELF_ERROR.to_string(), cp.red)], HINT * scale))
        } else {
            fault.map(|note| compact::mono_runs(vec![(note.to_string(), cp.red)], HINT * scale))
        };
        let mut recipient = column![
            label("recipient", cp, scale),
            screen::boxed_plain(
                PlainField::XrpSendRecipient,
                &state.send_recipient,
                "r\u{2026}  \u{b7}  X\u{2026}",
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
        recipient = recipient.push(tag_row(from_x, baked_tag, set_tag, cp, scale));

        // ── amount: two linked inputs, type in either ─────────────────────
        let chip: Element<'a, Message> = if has_choice {
            screen::asset_chip(display, state.send_asset_picker_open, Message::SendAssetPickerToggled, cp, scale)
        } else {
            screen::unit(display, cp, scale)
        };
        // Two boxes for every asset (2026-09-15; a token used to take the
        // whole row): the second is the amount in the BASE currency, and
        // that comparison is wanted whatever is being sent.
        let box_w = ((inner - 2.0 * AMT_GAP - EQ_W) / 2.0).max(60.0);
        let crypto = screen::boxed_plain(
            PlainField::XrpSendAmount,
            &state.send_amount,
            if is_xrp { "0.000000" } else { "0.00" },
            box_w,
            FIELD_VALUE,
            false,
            None,
            0.0,
            Some(chip),
            screen::chip_reserve(display),
            None,
            cp,
            scale,
        );
        let amount_row = row![
            crypto,
            Space::new().width(AMT_GAP * scale),
            text("=").font(MONO).size(ROW_VALUE * scale).color(cp.faint),
            Space::new().width(AMT_GAP * scale),
            screen::boxed_plain(
                PlainField::XrpSendFiat,
                &state.send_fiat_amount,
                "0.00",
                box_w,
                FIELD_VALUE,
                false,
                None,
                0.0,
                Some(screen::unit(ccy, cp, scale)),
                23.0,
                None,
                cp,
                scale,
            ),
        ]
        .align_y(Alignment::Center);
        // Over what can be sent: the button was already dark for it, but a
        // dark button with no words is a form that will not say what is
        // wrong (2026-09-14). XRP: the reserve-adjusted available, which the
        // fee also comes out of. A token: its own balance, or — when the
        // token fits and the XRP for the fee does not — say that instead.
        // `max ›` at the label row's end: the whole balance, to the last
        // digit the ledger holds — a two-place balance pane invites typing
        // two places, and a token line with a tail cannot be closed.
        let can_max = match state.xrp_token_tab {
            XrpTokenTab::Xrp => crate::controller::xrp::xrp_available().is_some_and(|a| a > xrp_fee()),
            XrpTokenTab::Token(code) => CHANNEL.token(code).0 > 0.0,
        };
        let mut head = row![label("amount", cp, scale)].align_y(Alignment::Center).width(Length::Fill);
        if can_max {
            head = head
                .push(Space::new().width(Length::Fill).boxed())
                .push(panes_ui::link("max", cp.dim, Message::SendMaxClicked, cp, scale));
        }
        let mut amount_sec = column![head, amount_row].width(Length::Fill);
        if insufficient {
            let note: Vec<(String, Color)> = match state.xrp_token_tab {
                XrpTokenTab::Xrp => vec![
                    (xrpsend::OVER_AVAILABLE.to_string(), cp.red),
                    (
                        format!(
                            "  \u{b7}  {} xrp available",
                            crate::controller::xrp::xrp_available().map_or(NA.to_string(), |a| format_token_amount(a, 6))
                        ),
                        cp.muted,
                    ),
                ],
                XrpTokenTab::Token(code) => {
                    let bal = CHANNEL.token(code).0;
                    if amount.is_some_and(|a| a <= bal) {
                        vec![(xrpsend::NO_XRP_FOR_FEE.to_string(), cp.red)]
                    } else {
                        vec![
                            (xrpsend::OVER_AVAILABLE.to_string(), cp.red),
                            (format!("  \u{b7}  {} {} available", format_token_amount(bal, 6), unit), cp.muted),
                        ]
                    }
                }
            };
            amount_sec = amount_sec.push(Space::new().height(5.0 * scale).boxed()).push(compact::mono_runs(note, HINT * scale));
        }

        // ── review, live ──────────────────────────────────────────────────
        let mut review = column![
            rule_label("review", cp, scale),
            screen::address_block((!addr.is_empty()).then_some(addr), TAIL, cp, scale),
            Space::new().height(7.0 * scale),
        ]
        .width(Length::Fill);
        review = review.push(drow(
            "amount",
            match amount {
                Some(a) => vec![(format_token_amount(a, 6), cp.text), (format!(" {unit}"), cp.faint)],
                None => vec![(NA.to_string(), cp.faint)],
            },
            2.0,
            cp,
            scale,
        ));
        review = review.push(drow(
            "network fee",
            vec![(fee_drops.map_or(NA.to_string(), |d| format!("{d} drops")), cp.dim)],
            2.0,
            cp,
            scale,
        ));
        // Only when the units agree: a token send moves the token and pays
        // the fee in XRP, so there is nothing to total.
        if is_xrp {
            let total = match amount {
                Some(a) => {
                    let mut runs = vec![(format_token_amount(a + fee, 6), cp.text), (" xrp".to_string(), cp.faint)];
                    if rate > 0.0 {
                        runs.push((format!("  \u{b7}  {} {}", fiat_amount((a + fee) * rate), ccy.to_lowercase()), cp.faint));
                    }
                    runs
                }
                None => vec![(NA.to_string(), cp.faint)],
            };
            review = review.push(group_rule(cp, scale)).push(
                row![
                    text("total").size(ROW_KEY * scale).color(cp.dim),
                    Space::new().width(Length::Fill),
                    compact::mono_runs(total, ROW_TOTAL * scale),
                ]
                .align_y(Alignment::Center).boxed(),
            );
        }

        // ── sign ──────────────────────────────────────────────────────────
        let sign_msg = Message::Grid(GridMsg::SendSign);
        let fields = SignFields {
            mode: EnableInputMode::detect(key_deleted, key_mode),
            secret: SecureField::SendPassphrase,
            secret_buf: &state.send_passphrase,
            secret_reveal: state.send_passphrase_reveal,
            phrase: SecureField::SendSeed,
            phrase_buf: &state.send_seed_text,
            phrase_reveal: state.send_seed_reveal,
            bip39: SecureField::SendBip39,
            bip39_buf: &state.send_bip39,
            bip39_reveal: state.send_bip39_reveal,
            on_submit: sign_msg.clone(),
        };
        let armed = valid && fields.can_submit();
        // `grid::sign_block`, like every other signing pane on both chains.
        // This pane used to hand-roll the identical stack — same label, same
        // gaps, same boxes — which meant it silently missed the `Clear ›` the
        // shared block gained (2026-09-12) and would have missed every later
        // change to it too. There is nothing XRP-specific in a sign stack.
        let sign = grid::sign_block(
            &fields,
            &state.send_passphrase,
            &state.send_seed_text,
            &state.send_bip39,
            valid,
            grid::primary("Send payment", armed, sign_msg, cp, scale),
            inner,
            cp,
            scale,
        );

        // The field gap between recipient and amount; review and sign carry
        // their own rules. Nothing is pinned to the foot.
        grid::scroller(
            column![
                recipient,
                Space::new().height(8.0 * scale),
                amount_sec,
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

/// The destination tag, right under the recipient — a readout, and the door
/// to the form. Three states: an X-address carries its own tag, so the line
/// says what it decoded to and offers nothing; a set tag reads `tag 12345 ·
/// change ›`; none reads `set destination tag ›`. It was an open box for a
/// day (2026-09-15) and an `Advanced ⌄` curtain over a full-width one before
/// that — the user wanted the few who use it to feel it lock in, and the
/// many who don't to see one quiet link.
fn tag_row<'a>(
    from_x: bool,
    baked_tag: Option<u32>,
    set_tag: Option<u32>,
    cp: &'static CompactPalette,
    scale: f32,
) -> Element<'a, Message> {
    let line: Element<'a, Message> = if from_x {
        let val = baked_tag.map_or_else(|| "none".to_string(), |t| t.to_string());
        compact::mono_runs(
            vec![(format!("tag {val}"), cp.dim), ("  \u{b7}  from the x-address".to_string(), cp.muted)],
            HINT * scale,
        )
    } else {
        grid::tag_line(set_tag, TagPane::Send, cp, scale)
    };
    column![Space::new().height(6.0 * scale), line].width(Length::Fill).boxed()
}

// ── network ─────────────────────────────────────────────────────────────────

// Moved from `xrpbalance.rs` when the old balance screen was deleted
// (2026-09-09): the fee verdict and the status word are the network pane's.
/// Fee verdict bands on the **escalation ratio** `open_ledger_fee ÷ fee_base`:
/// `(upper bound, exclusive; word)`.
///
/// The XRPL fee market is a step function, not a curve: the open-ledger fee
/// sits at exactly the base fee (ratio 1) for the overwhelming majority of
/// ledgers and then escalates multiplicatively when a ledger fills. So
/// `quiet` is the identity, `busy` is "escalation has begun" and `congested`
/// is an order of magnitude — the three words the handoff asks for, placed
/// where the mechanism actually has its edges.
const FEE_BANDS: [(f64, &str); 3] = [
    (1.0 + f64::EPSILON, "quiet"),
    (10.0,               "busy"),
    (f64::INFINITY,      "congested"),
];

/// The severity ramp for the fee verdict — the same green / amber / red the
/// BTC adapter uses, so one fee vocabulary covers both chains.
fn band_color(band: usize, p: &'static CompactPalette) -> Color {
    match band {
        0 => p.green,
        1 => p.amber,
        _ => p.red,
    }
}

// ── Node data ────────────────────────────────────────────────────────────────

impl NodeFrame {
    /// Nothing measured at all: the relay dropped the frame because it lost
    /// the node.
    fn is_empty(&self) -> bool {
        self.ledger_index.is_none() && self.peers.is_none() && self.state.is_none()
    }

    /// The fee verdict: one word and its colour, or `None` when unmeasured.
    pub(crate) fn verdict(&self, p: &'static CompactPalette) -> Option<(&'static str, Color)> {
        let (open, base) = (self.open_ledger_fee?, self.fee_base?);
        if base == 0 {
            return None;
        }
        let ratio = open as f64 / base as f64;
        let band = FEE_BANDS.iter().position(|(hi, _)| ratio < *hi).unwrap_or(FEE_BANDS.len() - 1);
        Some((FEE_BANDS[band].1, band_color(band, p)))
    }

    /// What the status dot says — the only claim this screen makes about the
    /// node. A not-synced node reports rippled's own state word (`syncing`,
    /// `connected`, `tracking`…) rather than a paraphrase.
    pub(crate) fn status(&self, p: &'static CompactPalette) -> (String, Color) {
        if self.is_empty() {
            return ("offline".to_string(), p.red);
        }
        match (self.synced, self.state.as_deref()) {
            (Some(true), _) => ("synced".to_string(), p.green),
            (Some(false), Some(s)) => (s.to_string(), p.amber),
            _ => ("unknown".to_string(), p.muted),
        }
    }
}


/// The open-ledger fee with its verdict, the ledger tape, and the node's
/// facts. Everything here is the relay's node frame; a field it cannot
/// measure draws `—`, and a lost node reads `offline`.
/// The least the network pane fills before it scrolls: the fee line, a
/// legible tape, the three rows.
const NETWORK_MIN_H: f32 = 120.0;

fn network_pane<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    grid::fill_or_scroll(NETWORK_MIN_H, move || network_body(state, cp, scale), cp, scale)
}

fn network_body<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    let node = CHANNEL.xrp_node_rx.borrow().clone();
    let fee = node.open_ledger_fee.map_or(NA.to_string(), |d| d.to_string());
    let verdict = node.verdict(cp);
    let (word, dot) = node.status(cp);

    let tape = Canvas::new(Tape {
        bars: state.ledger_tape.iter().copied().collect(),
        faint: cp.faint,
        amber: cp.amber,
        dim: cp.dim,
        scale,
    })
    .width(Length::Fill)
    .height(Length::Fill);

    column![
        big_line(fee, "drops", verdict, cp, scale),
        container(tape)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::new(0.0).top(6.0 * scale).bottom(9.0 * scale)),
        drow("status", vec![(format!("\u{25cf} {word}"), dot)], 3.0, cp, scale),
        drow(
            "ledger",
            vec![(node.ledger_index.map_or(NA.to_string(), |h| add_commas(h as i64)), cp.dim)],
            3.0,
            cp,
            scale,
        ),
        drow("peers", vec![(node.peers.map_or(NA.to_string(), |n| n.to_string()), cp.dim)], 3.0, cp, scale),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .boxed()
}

/// The ledger tape: one bottom-aligned bar per validated ledger, newest at
/// the right edge, as many as the width holds. Height follows the ledger's
/// transaction count against the window's busiest; `faint`, a busy ledger
/// `amber`, the current one `dim`.
struct Tape {
    bars: Vec<LedgerBar>,
    faint: Color,
    amber: Color,
    dim: Color,
    scale: f32,
}

impl<Message> canvas::Program<Message> for Tape {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let w = bounds.width;
        let h = bounds.height;
        let s = self.scale;
        let mut frame = Frame::new(renderer, bounds.size());
        if self.bars.is_empty() || w <= 0.0 || h <= 0.0 {
            return vec![frame.into_geometry()];
        }
        let gap = TAPE_GAP * s;
        let slots = ((w + gap) / (TAPE_PITCH * s)).floor().max(1.0) as usize;
        let bar_w = ((w - gap * (slots as f32 - 1.0)) / slots as f32).max(1.0);
        let start = self.bars.len().saturating_sub(slots);
        let shown = &self.bars[start..];
        let n = shown.len();
        let busiest = shown.iter().map(|b| b.txns).max().unwrap_or(1).max(1) as f32;
        let x0 = w - (n as f32 * bar_w + (n as f32 - 1.0).max(0.0) * gap);
        for (i, bar) in shown.iter().enumerate() {
            let frac = 0.2 + 0.8 * (bar.txns as f32 / busiest);
            let bh = (h * frac).max(1.0);
            let x = x0 + i as f32 * (bar_w + gap);
            let colour = if i + 1 == n {
                self.dim
            } else if bar.escalated {
                self.amber
            } else {
                self.faint
            };
            frame.fill_rectangle(Point::new(x, h - bh), Size::new(bar_w, bh), colour);
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(open: u64, base: u64) -> NodeFrame {
        NodeFrame { open_ledger_fee: Some(open), fee_base: Some(base), ..Default::default() }
    }

    /// The verdict is the whole fee statement, so its edges are what keep it
    /// honest: identity is quiet, any escalation is busy, 10× is congested.
    #[test]
    fn the_verdict_word_matches_its_band() {
        let p = &theme::COMPACT_OBSIDIAN;
        for (open, want) in [(10, "quiet"), (11, "busy"), (99, "busy"), (100, "congested"), (5000, "congested")] {
            assert_eq!(at(open, 10).verdict(p).unwrap().0, want, "wrong word at {open}");
        }
    }

    #[test]
    fn no_fee_means_no_verdict() {
        assert!(NodeFrame::default().verdict(&theme::COMPACT_OBSIDIAN).is_none());
        assert!(at(10, 0).verdict(&theme::COMPACT_OBSIDIAN).is_none());
    }

    /// An empty frame is what the relay publishes when it loses xrpld. It must
    /// read as `offline`, never as a green dot over a row of dashes.
    #[test]
    fn an_empty_frame_reads_as_offline() {
        let p = &theme::COMPACT_OBSIDIAN;
        let (word, color) = NodeFrame::default().status(p);
        assert_eq!(word, "offline");
        assert_eq!(color, p.red);
    }

    /// A node that is up but not serving the validated ledger reports
    /// rippled's own word for where it is, in amber.
    #[test]
    fn a_not_synced_node_reports_its_own_state_word() {
        let p = &theme::COMPACT_OBSIDIAN;
        let f = NodeFrame {
            ledger_index: Some(1), peers: Some(3), synced: Some(false), state: Some("syncing".into()),
            ..Default::default()
        };
        assert_eq!(f.status(p), ("syncing".to_string(), p.amber));
        let full = NodeFrame { synced: Some(true), state: Some("full".into()), peers: Some(21), ..Default::default() };
        assert_eq!(full.status(p), ("synced".to_string(), p.green));
    }

    /// Both chains' fee verdicts speak one vocabulary: green while it is
    /// ordinary, amber once it escalates, red at an order of magnitude.
    #[test]
    fn the_fee_ramp_only_climbs() {
        let p = &theme::COMPACT_OBSIDIAN;
        assert_eq!(at(10, 10).verdict(p).unwrap().1, p.green);
        assert_eq!(at(50, 10).verdict(p).unwrap().1, p.amber);
        assert_eq!(at(500, 10).verdict(p).unwrap().1, p.red);
    }
}
