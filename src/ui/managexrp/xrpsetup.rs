//! XRP import/create on the compact one-screen surface — the Bitcoin twin's
//! mirror, differing only in which chain's buffers and messages it passes.
//! Replaces the stepped `xrpimport`/`xrpcreate` directories: no step state,
//! no per-step routing — one [`wallet_setup::setup_screen`] call per flow.

use iced::Element;
use crate::controller::app_state::{AppState, ImportMode};
use crate::ui::components::wallet_gate::Mark;
use crate::controller::message::{Message, SecureField};
use crate::ui::components::wallet_setup::{self, SetupFlow, SetupScreenParams};

/// The path the derivers walk — the core's display copy, `bridge::XRP_PATH`.
/// Derivation is frozen and those literals hold real funds, so they are not
/// hoisted into a shared constant; a test in the core pins its copy to them.
const PATH: &str = crate::bridge::XRP_PATH;

pub fn import(state: &AppState) -> Element<'_, Message> {
    wallet_setup::setup_screen(
        state,
        SetupScreenParams {
            flow: SetupFlow::Import,
            title: "Import wallet",
            mark: Mark::Xrp,
            path: PATH,
            address_types: Vec::new(),
            address_type_bip: "",
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
            on_word25_no: Message::Word25Chosen(false),
            on_word25_yes: Message::Word25Chosen(true),
            mode: state.xrp_import_mode,
            enc_field: SecureField::XrpEncryption,
            enc: &state.xrp_encryption_input,
            enc_reveal: state.xrp_encryption_reveal,
            on_mode_device: Message::ImportModeChanged(ImportMode::Standard),
            on_mode_cold: Message::ImportModeChanged(ImportMode::Cold),
            on_back: Message::BackClicked,
            on_submit: Message::ImportSubmitClicked,
            submit_label: "Import wallet",
        },
    )
}

pub fn create(state: &AppState) -> Element<'_, Message> {
    wallet_setup::setup_screen(
        state,
        SetupScreenParams {
            flow: SetupFlow::Create,
            title: "Create wallet",
            mark: Mark::Xrp,
            path: PATH,
            address_types: Vec::new(),
            address_type_bip: "",
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
            on_word25_no: Message::Word25Chosen(false),
            on_word25_yes: Message::Word25Chosen(true),
            mode: state.xrp_import_mode,
            enc_field: SecureField::XrpEncryption,
            enc: &state.xrp_encryption_input,
            enc_reveal: state.xrp_encryption_reveal,
            on_mode_device: Message::ImportModeChanged(ImportMode::Standard),
            on_mode_cold: Message::ImportModeChanged(ImportMode::Cold),
            on_back: Message::BackClicked,
            on_submit: Message::CreateSubmitClicked,
            submit_label: "Create wallet",
        },
    )
}
