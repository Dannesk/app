use iced::Element;
use crate::controller::app_state::{AppState, SetupChain};
use crate::ui::components::wallet_gate::Mark;
use crate::controller::message::{Message, SecureField};
use crate::ui::components::wallet_setup::{self, SetupFlow, SetupScreenParams};

pub fn import(state: &AppState) -> Element<'_, Message> {
    wallet_setup::setup_screen(
        state,
        SetupScreenParams {
            flow: SetupFlow::Import,
            chain: SetupChain::Xrp,
            title: "Import wallet",
            mark: Mark::Xrp,
            task: state.xrp_setup_task,
            address_types: &[],
            address_type: None,
            on_address_type: Message::BtcScriptTypeChosen,
            seed_field: SecureField::XrpImportSeed,
            seed: &state.xrp_seed_text,
            seed_reveal: state.xrp_seed_reveal,
            copied: false,
            on_generate: None,
            on_copy: None,
            word25: state.xrp_word25,
            w25_field: SecureField::XrpBip39,
            w25: &state.xrp_bip39_input,
            w25_reveal: state.xrp_bip39_reveal,
            on_word25: Message::Word25Chosen,
            mode: state.xrp_import_mode,
            enc_field: SecureField::XrpEncryption,
            enc: &state.xrp_encryption_input,
            enc_reveal: state.xrp_encryption_reveal,
            on_mode: Message::ImportModeChanged,
            on_back: Message::BackClicked,
            on_submit: Message::ImportSubmitClicked,
        },
    )
}

pub fn create(state: &AppState) -> Element<'_, Message> {
    wallet_setup::setup_screen(
        state,
        SetupScreenParams {
            flow: SetupFlow::Create,
            chain: SetupChain::Xrp,
            title: "Create wallet",
            mark: Mark::Xrp,
            task: state.xrp_setup_task,
            address_types: &[],
            address_type: None,
            on_address_type: Message::BtcScriptTypeChosen,
            seed_field: SecureField::CreateSeed,
            seed: &state.create_mnemonic,
            seed_reveal: state.create_seed_reveal,
            copied: state.xrp_copy_feedback,
            on_generate: Some(Message::GenerateMnemonic),
            on_copy: Some(Message::CopyMnemonic),
            word25: state.xrp_word25,
            w25_field: SecureField::XrpBip39,
            w25: &state.xrp_bip39_input,
            w25_reveal: state.xrp_bip39_reveal,
            on_word25: Message::Word25Chosen,
            mode: state.xrp_import_mode,
            enc_field: SecureField::XrpEncryption,
            enc: &state.xrp_encryption_input,
            enc_reveal: state.xrp_encryption_reveal,
            on_mode: Message::ImportModeChanged,
            on_back: Message::BackClicked,
            on_submit: Message::CreateSubmitClicked,
        },
    )
}
