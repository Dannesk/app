

use iced::Widget as _;
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
            .padding(Padding::new(0.0).top(3.0 * scale).bottom(3.0 * scale)).boxed(),
        );

 
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
        .boxed(),
        cp,
        scale,
    )
}


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

       
        let phrase = grid::phrase_field(
            SecureField::BtcReimportSeed,
            seed,
            state.btc_reimport_seed_reveal,
            Message::BtcReimportConfirm,
            cp,
            scale,
        );

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
                .push(Space::new().height(6.0 * scale).boxed())
                .push(compact::meter_rows(bits, entropy::Kdf::Argon2id, cp, scale));
        }
        if let Some(e) = &state.btc_reimport_error {
            key = key
                .push(Space::new().height(6.0 * scale).boxed())
                .push(text(e.clone()).size(compact::NOTE * scale).color(cp.red).boxed());
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
            scroller(body.boxed(), cp, scale),
            Space::new().height(8.0 * scale),
            buttons,
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .boxed()
    })
    .boxed()
}
