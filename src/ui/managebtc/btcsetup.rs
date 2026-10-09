//! BTC import/create on the compact one-screen surface — the XRP twin's
//! mirror, differing only in which chain's buffers and messages it passes.
//! Replaces the stepped `btcimport`/`btccreate` directories.

use iced::Element;
use crate::btc_script_type::BtcScriptType;
use crate::controller::app_state::{AppState, ImportMode};
use crate::ui::components::wallet_gate::Mark;
use crate::controller::message::{Message, SecureField};
use crate::ui::components::wallet_setup::{self, SetupFlow, SetupScreenParams};

// The eyebrow's path is the chosen type's [`BtcScriptType::path`]. Its
// native entry is a display copy of the literal in
// `bridge/btc_import_logic.rs` (and its create/auth twins): derivation is
// frozen and those literals hold real funds, so they are not hoisted into a
// shared constant; `path_is_the_derivers` pins the copy to them instead.

/// The picker's cards for `offered`, with `current` lit.
fn type_cards(offered: &[BtcScriptType], current: BtcScriptType) -> Vec<(&'static str, &'static str, bool, Message)> {
    offered
        .iter()
        .map(|t| (t.prefix(), t.name(), *t == current, Message::BtcScriptTypeChosen(*t)))
        .collect()
}

pub fn import(state: &AppState) -> Element<'_, Message> {
    wallet_setup::setup_screen(
        state,
        SetupScreenParams {
            flow: SetupFlow::Import,
            title: "Import wallet",
            mark: Mark::Btc,
            path: state.btc_script_type.path(),
            address_types: type_cards(&BtcScriptType::IMPORT, state.btc_script_type),
            address_type_bip: state.btc_script_type.bip(),
            seed_field: SecureField::BtcImportSeed,
            seed: &state.btc_seed_text,
            seed_reveal: state.btc_seed_reveal,
            copied: false,
            on_generate: None,
            on_copy: None,
            word25: state.btc_word25,
            w25_field: SecureField::BtcBip39,
            w25: &state.btc_bip39_input,
            w25_reveal: state.btc_bip39_reveal,
            on_word25_no: Message::BtcWord25Chosen(false),
            on_word25_yes: Message::BtcWord25Chosen(true),
            mode: state.btc_import_mode,
            enc_field: SecureField::BtcEncryption,
            enc: &state.btc_encryption_input,
            enc_reveal: state.btc_encryption_reveal,
            on_mode_device: Message::BtcImportModeChanged(ImportMode::Standard),
            on_mode_cold: Message::BtcImportModeChanged(ImportMode::Cold),
            on_back: Message::BtcBackClicked,
            on_submit: Message::BtcImportSubmitClicked,
            submit_label: "Import wallet",
        },
    )
}

pub fn create(state: &AppState) -> Element<'_, Message> {
    // Create offers two types; a choice made on the import screen that
    // create does not offer reads as the default here, and submits as it.
    let chosen = create_type(state.btc_script_type);
    wallet_setup::setup_screen(
        state,
        SetupScreenParams {
            flow: SetupFlow::Create,
            title: "Create wallet",
            mark: Mark::Btc,
            path: chosen.path(),
            address_types: type_cards(&BtcScriptType::CREATE, chosen),
            address_type_bip: chosen.bip(),
            seed_field: SecureField::CreateSeed,
            seed: &state.create_mnemonic,
            seed_reveal: state.create_seed_reveal,
            copied: state.btc_copy_feedback,
            on_generate: Some(Message::BtcGenerateMnemonic),
            on_copy: Some(Message::BtcCopyMnemonic),
            word25: state.btc_word25,
            w25_field: SecureField::BtcBip39,
            w25: &state.btc_bip39_input,
            w25_reveal: state.btc_bip39_reveal,
            on_word25_no: Message::BtcWord25Chosen(false),
            on_word25_yes: Message::BtcWord25Chosen(true),
            mode: state.btc_import_mode,
            enc_field: SecureField::BtcEncryption,
            enc: &state.btc_encryption_input,
            enc_reveal: state.btc_encryption_reveal,
            on_mode_device: Message::BtcImportModeChanged(ImportMode::Standard),
            on_mode_cold: Message::BtcImportModeChanged(ImportMode::Cold),
            on_back: Message::BtcBackClicked,
            on_submit: Message::BtcCreateSubmitClicked,
            submit_label: "Create wallet",
        },
    )
}

/// What create derives for a state that holds `chosen`: the choice if create
/// offers it, else the default. The controller applies the same clamp on
/// submit, so what the screen shows is what gets derived.
pub fn create_type(chosen: BtcScriptType) -> BtcScriptType {
    if BtcScriptType::CREATE.contains(&chosen) { chosen } else { BtcScriptType::default() }
}

#[cfg(test)]
mod tests {
    use super::create_type;
    use crate::btc_script_type::BtcScriptType;

    #[test]
    fn create_never_derives_a_type_it_does_not_offer() {
        assert_eq!(create_type(BtcScriptType::Taproot), BtcScriptType::Taproot);
        assert_eq!(create_type(BtcScriptType::Legacy), BtcScriptType::NativeSegwit);
        assert_eq!(create_type(BtcScriptType::NestedSegwit), BtcScriptType::NativeSegwit);
    }
}
