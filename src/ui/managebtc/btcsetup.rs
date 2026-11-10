use iced::Element;
use crate::btc_script_type::BtcScriptType;
use crate::controller::app_state::{AppState, SetupChain};
use crate::ui::components::wallet_gate::Mark;
use crate::controller::message::{Message, SecureField};
use crate::ui::components::wallet_setup::{self, SetupFlow, SetupScreenParams};


pub fn import(state: &AppState) -> Element<'_, Message> {
    wallet_setup::setup_screen(
        state,
        SetupScreenParams {
            flow: SetupFlow::Import,
            chain: SetupChain::Btc,
            title: "Import wallet",
            mark: Mark::Btc,
            task: state.btc_setup_task,
            address_types: &BtcScriptType::IMPORT,
            address_type: state.btc_script_type,
            on_address_type: Message::BtcScriptTypeChosen,
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
            on_word25: Message::BtcWord25Chosen,
            mode: state.btc_import_mode,
            enc_field: SecureField::BtcEncryption,
            enc: &state.btc_encryption_input,
            enc_reveal: state.btc_encryption_reveal,
            on_mode: Message::BtcImportModeChanged,
            on_back: Message::BtcBackClicked,
            on_submit: Message::BtcImportSubmitClicked,
        },
    )
}

pub fn create(state: &AppState) -> Element<'_, Message> {
    wallet_setup::setup_screen(
        state,
        SetupScreenParams {
            flow: SetupFlow::Create,
            chain: SetupChain::Btc,
            title: "Create wallet",
            mark: Mark::Btc,
            task: state.btc_setup_task,
            address_types: &BtcScriptType::CREATE,
            address_type: create_type(state.btc_script_type),
            on_address_type: Message::BtcScriptTypeChosen,
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
            on_word25: Message::BtcWord25Chosen,
            mode: state.btc_import_mode,
            enc_field: SecureField::BtcEncryption,
            enc: &state.btc_encryption_input,
            enc_reveal: state.btc_encryption_reveal,
            on_mode: Message::BtcImportModeChanged,
            on_back: Message::BtcBackClicked,
            on_submit: Message::BtcCreateSubmitClicked,
        },
    )
}


pub fn create_type(chosen: Option<BtcScriptType>) -> Option<BtcScriptType> {
    chosen.filter(|t| BtcScriptType::CREATE.contains(t))
}

#[cfg(test)]
mod tests {
    use super::create_type;
    use crate::btc_script_type::BtcScriptType;

    #[test]
    fn create_never_derives_a_type_it_does_not_offer() {
        assert_eq!(create_type(Some(BtcScriptType::Taproot)), Some(BtcScriptType::Taproot));
        assert_eq!(create_type(Some(BtcScriptType::NativeSegwit)), Some(BtcScriptType::NativeSegwit));
        assert_eq!(create_type(Some(BtcScriptType::Legacy)), None);
        assert_eq!(create_type(Some(BtcScriptType::NestedSegwit)), None);
        assert_eq!(create_type(None), None);
    }
}
