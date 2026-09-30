//! The `wallet` pane — key management only, and its `restore key` face.
//! The XRP pane's twin on this chain's state, this chain's files and this
//! chain's messages; key management is the same process on both.
//!
//! The address row is keyed `master wallet` (2026-09-15; `hd wallet` for a
//! day before that — the landing and the docs say master): this chain rotates its
//! receive address, so the receive pane beside this one shows an address
//! that is NOT the one here, and the row has to say what kind of wallet
//! makes that so. The address carries a copy glyph for the same reason —
//! some people send to the master on purpose, and the receive pane only
//! copies the rotating one.
//!
//! Two states, two labels: **On-device** (`Remove key` / `Remove wallet`)
//! and **Cold storage** (`Restore key` / `Remove wallet`), the state word as
//! the pane's big line — the same two words import and create offer under
//! `key storage`, so the pane says what was chosen in the words it was
//! chosen in. (`Standard` was the name from when a TPM tier existed beside
//! it.) Rows: seed · cipher · sign with · wallet — the three facts that
//! decide how you sign, and the wallet they belong to, in full. Cold
//! storage has no cipher row: nothing is encrypted.
//!
//! **Neither destructive action confirms** (user, 2026-09-09: both are
//! harmless — re-import if you jump the gun). `Remove key` runs and the pane
//! comes back as Cold storage with `Restore key` where the button was; the
//! state change is the receipt. `Remove wallet` purges the local data and
//! drops the tab on the import-or-create gate.
//!
//! **Restore key replaces the pane.** One form: the recovery phrase in the
//! import grid with its status line underneath, the 25th word (recalled, so
//! no meter), the new encryption key with import's entropy meter, and a
//! pinned `Cancel` / `Restore key` row. The bridge derives #0 and verifies
//! it against the stored wallet before anything touches the disk, so a
//! wrong phrase or a forgotten 25th word comes back as an error line and
//! nothing was written — which is what lets the 25th word be a plain
//! optional field here rather than the import's explicit question.

use iced::widget::text::Wrapping;
use iced::widget::{column, container, responsive, row, text, Space};
use iced::{Alignment, Element, Length, Padding, Size};

use crate::channel::CHANNEL;
use crate::controller::app_state::AppState;
use crate::controller::message::{Message, SecureField};
use crate::ui::components::compact;
use crate::ui::components::grid::{
    self, button_pair, danger_button, drow, group_rule, pane_button, quiet_button, scroller, BIG, BIP39_PLACEHOLDER,
    KEY_PLACEHOLDER, ROW_KEY, ROW_VALUE,
};
use crate::ui::components::wallet_setup::{self, KEY_MIN, TARGET_WORDS};
use crate::utils::entropy;
use crate::utils::fonts::MONO;
use crate::utils::theme::CompactPalette;

const STANDARD: &str = "On-device";
const COLD: &str = "Cold storage";
const STANDARD_SEED: &str = "encrypted on this device";
const STANDARD_CIPHER: &str = "AES-256 \u{b7} Argon2id";
const STANDARD_SIGN: &str = "encryption key";
const COLD_SEED: &str = "not on this device";
const COLD_SIGN: &str = "24 word mnemonic";

/// The pane's title: `restore key` while the form is up.
pub fn title(state: &AppState) -> &'static str {
    if state.btc_key_mgmt_restoring { "restore key" } else { "wallet" }
}

pub fn view<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    if state.btc_key_mgmt_restoring {
        return restore(state, cp, scale);
    }
    let (_, address, key_deleted, _) = CHANNEL.bitcoin_wallet_rx.borrow().clone();
    let address = address.unwrap_or_default();
    let cold = key_deleted;

    let mut rows = column![
        text(if cold { COLD } else { STANDARD }).font(MONO).size(BIG * scale).color(cp.text),
        group_rule(cp, scale),
        drow("seed", vec![((if cold { COLD_SEED } else { STANDARD_SEED }).to_string(), cp.dim)], 0.0, cp, scale),
    ]
    .width(Length::Fill);
    if !cold {
        rows = rows.push(drow("cipher", vec![(STANDARD_CIPHER.to_string(), cp.dim)], 3.0, cp, scale));
    }
    rows = rows
        .push(drow("sign with", vec![((if cold { COLD_SIGN } else { STANDARD_SIGN }).to_string(), cp.dim)], 3.0, cp, scale))
        .push(
            // The master address in full — a key pane that will not say
            // which wallet it holds the key for is asking you to take its
            // word for it. It wraps rather than truncates when the pane is
            // narrow. Elided since 2026-09-14 (10 … 8 — a taproot address is
            // 62 characters): the glyph after it copies the whole thing, and
            // `copied` takes the glyph's place for the beat.
            container(
                row![
                    text("master wallet").size(ROW_KEY * scale).color(cp.dim),
                    Space::new().width(Length::Fill),
                    text(grid::elide_addr(&address)).font(MONO).size(ROW_VALUE * scale).color(cp.dim).wrapping(Wrapping::Glyph),
                    grid::copy_glyph(state.btc_master_copy_feedback, Message::BtcCopyMasterAddress, cp, scale),
                ]
                .spacing(10.0 * scale)
                .align_y(Alignment::Start),
            )
            .width(Length::Fill)
            .padding(Padding::new(0.0).top(3.0 * scale).bottom(3.0 * scale)),
        );

    // Cold → Standard routes into the restore face; Standard → Cold is the
    // one-click seed delete. Both buttons draw dead through the async gap.
    let mode: Element<'a, Message> = if cold {
        pane_button("Restore key", true, false, Message::BtcKeyMgmtRestoreToggled, cp, scale)
    } else {
        pane_button("Remove key", !state.btc_key_purge_requested, false, Message::BtcDeleteKey, cp, scale)
    };
    let remove = danger_button("Remove wallet", !state.btc_remove_wallet_requested, Message::BtcRemoveWallet, cp, scale);

    scroller(
        column![
            rows,
            Space::new().height(12.0 * scale),
            button_pair(mode, remove, scale),
        ]
        .width(Length::Fill)
        .into(),
        cp,
        scale,
    )
}

/// The restore form. The body scrolls under a pinned button row, so the pane
/// never lies about its size and the buttons never scroll out of reach.
fn restore<'a>(state: &'a AppState, cp: &'static CompactPalette, scale: f32) -> Element<'a, Message> {
    responsive(move |size: Size| {
        let inner = (size.width / scale).max(1.0);
        let seed = &state.btc_reimport_seed_text;
        let words = seed.as_str().split_whitespace().count();
        let complete = words == TARGET_WORDS;
        let checksum_ok = complete && wallet_setup::mnemonic_checksum_ok(seed.as_str());
        let key_ok = state.btc_reimport_passphrase.trimmed_char_len() >= KEY_MIN;
        let armed = checksum_ok && key_ok && !state.btc_reimport_spinning;
        let submit = armed.then_some(Message::BtcReimportConfirm);

        // ── recovery phrase ───────────────────────────────────────────────
        // `grid::phrase_field` — the box, its `N / 24 · words · verdict` line
        // and `Paste ›` / `Clear ›`. This face is where that pattern started;
        // every signing surface on both chains draws it now (2026-09-12).
        //
        // NO label over it (user, same day): the pane strip already says
        // `restore key`, so a `RECOVERY PHRASE` head above the box was the
        // title twice. The placeholder says which box this is.
        let phrase = grid::phrase_field(
            SecureField::BtcReimportSeed,
            seed,
            state.btc_reimport_seed_reveal,
            Message::BtcReimportConfirm,
            cp,
            scale,
        );

        // ── 25th word — recalled, not chosen: no meter ────────────────────
        let word25 = compact::boxed_credential(
            SecureField::BtcReimportBip39,
            &state.btc_reimport_bip39,
            state.btc_reimport_bip39_reveal,
            BIP39_PLACEHOLDER,
            inner,
            submit.clone(),
            cp,
            scale,
        );

        // ── encryption key — chosen fresh, so import's meter ──────────────
        // Was eight bullet glyphs and a `ENCRYPTION KEY` label — the one
        // placeholder in the app that did not name its own field, and the
        // only signing key box wearing a head (user, 2026-09-12).
        let mut key = column![compact::boxed_credential(
            SecureField::BtcReimportPassphrase,
            &state.btc_reimport_passphrase,
            state.btc_reimport_passphrase_reveal,
            KEY_PLACEHOLDER,
            inner,
            submit,
            cp,
            scale,
        )]
        .width(Length::Fill);
        let bits = wallet_setup::held_bits(&state.btc_reimport_passphrase, state.entropy_hold);
        if bits > 0.0 {
            key = key
                .push(Space::new().height(6.0 * scale))
                .push(compact::meter_rows(bits, entropy::Kdf::Argon2id, cp, scale));
        }
        if let Some(e) = &state.btc_reimport_error {
            key = key
                .push(Space::new().height(6.0 * scale))
                .push(text(e.clone()).size(compact::NOTE * scale).color(cp.red));
        }

        let body = column![
            phrase,
            Space::new().height(8.0 * scale),
            word25,
            Space::new().height(8.0 * scale),
            key,
            Space::new().height(4.0 * scale),
        ]
        .width(Length::Fill);

        let buttons = button_pair(
            quiet_button("Cancel", Message::BtcKeyMgmtRestoreToggled, cp, scale),
            pane_button("Restore key", armed, true, Message::BtcReimportConfirm, cp, scale),
            scale,
        );

        column![
            scroller(body.into(), cp, scale),
            Space::new().height(8.0 * scale),
            buttons,
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    })
    .into()
}
