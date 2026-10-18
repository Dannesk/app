

use iced::Element;
use crate::btc_script_type::BtcScriptType;
use crate::controller::app_state::{AppState, ImportMode};
use crate::ui::components::wallet_gate::Mark;
use crate::controller::message::{Message, SecureField};
use crate::ui::components::wallet_setup::{self, SetupFlow, SetupScreenParams};


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
