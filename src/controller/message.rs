use crate::controller::app_state::{
    BaseCcy, BtcFeeTier, ImportMode, Tab, TradeSide, XrpTokenTab,
};
use crate::channel::ActivityLogState;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecureField {
    SendPassphrase,
    SendBip39,
    SendSeed,
    CancelPassphrase,
    CancelBip39,
    CancelSeed,
    TradePassphrase,
    TradeBip39,
    TradeSeed,
    EnablePassphrase,
    EnableBip39,
    EnableSeed,
    DisablePassphrase,
    DisableBip39,
    DisableSeed,
    ReimportPassphrase,
    ReimportBip39,
    ReimportSeed,
    BtcSendPassphrase,
    BtcSendBip39,
    BtcSendSeed,
    BtcBumpPassphrase,
    BtcBumpBip39,
    BtcBumpSeed,
    BtcReimportPassphrase,
    BtcReimportBip39,
    BtcReimportSeed,
    XrpImportSeed,
    XrpEncryption,
    XrpBip39,
    CreateSeed,
    BtcImportSeed,
    BtcEncryption,
    BtcBip39,
}

impl SecureField {
    pub fn is_phrase(self) -> bool {
        matches!(
            self,
            Self::SendSeed
                | Self::CancelSeed
                | Self::TradeSeed
                | Self::EnableSeed
                | Self::DisableSeed
                | Self::ReimportSeed
                | Self::BtcSendSeed
                | Self::BtcBumpSeed
                | Self::BtcReimportSeed
                | Self::XrpImportSeed
                | Self::BtcImportSeed
                | Self::CreateSeed
        )
    }
}

pub const WORD25_MAX: usize = 50;

pub const KEY_MAX: usize = 50;

impl SecureField {
    pub fn char_cap(self) -> Option<usize> {
        match self {
            Self::XrpBip39 | Self::BtcBip39 => Some(WORD25_MAX),
            Self::XrpEncryption | Self::BtcEncryption => Some(KEY_MAX),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SecureOp {
    Insert(usize, char),
    Remove(usize),
    Paste(usize, String),
    ToggleReveal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagPane {
    Send,
    Receive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlainField {
    BtcSendRecipient,
    BtcSendAmount,
    BtcSendFiat,
    BtcSendFee,
    BtcBumpFee,
    XrpSendRecipient,
    XrpSendAmount,
    XrpSendFiat,
    XrpSendTag,
    XrpReceiveTag,
    TradeAmount,
    TradeReceive,
    TradeLimit,
    TradePairQuery,
}

impl PlainField {
    pub fn is_btc(self) -> bool {
        matches!(
            self,
            Self::BtcSendRecipient
                | Self::BtcSendAmount
                | Self::BtcSendFiat
                | Self::BtcSendFee
                | Self::BtcBumpFee
        )
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Sync,
    SystemFontsIndexed,
    FirstFrame,
    SecureEdit(SecureField, SecureOp),
    SecurePasteRequested(SecureField),
    SecureClearRequested(SecureField),
    SignCredentialCleared {
        secret: SecureField,
        phrase: SecureField,
        bip39:  SecureField,
    },
    SecurePasted(SecureField, Option<String>),
    PlainEdit(PlainField, SecureOp),
    PlainPasteRequested(PlainField),
    PlainPasted(PlainField, Option<String>),
    GateDigit(char),
    GateBackspace,
    GateBack,
    GatePinSubmitted,
    GateEnabledToggled(bool),
    GateWipeExit,
    TabChanged(Tab),
    ImportWalletClicked,
    CreateWalletClicked,
    GenerateMnemonic,
    ImportModeChanged(ImportMode),
    Word25Chosen(bool),
    ImportSubmitClicked,
    ImportCompleted,
    ImportFailed,
    ActivityChanged(Option<ActivityLogState>),
    ActivityTick(std::time::Instant),
    FetchTick,
    ActivityDismiss,
    BackClicked,
    DeleteXrpKey,
    RemoveXrpWallet,
    XrpWalletRemoved,
    CopyAddress,
    CopyAddressFeedback,
    WalletCopyAddress,
    WalletCopyAddressFeedback,
    CopyMnemonic,
    FocusNext,
    XrpTokenTabChanged(XrpTokenTab),
    SendAssetPickerToggled,
    TagEdit(TagPane),
    TagSet(TagPane),
    TagCancelled(TagPane),
    TagCleared(TagPane),
    CopyReceiveTag,
    CopyReceiveTagFeedback,
    EnableTokenClicked(&'static str),
    EnableDismissed,
    EnableSubmitClicked,
    DisableTokenClicked(&'static str),
    DisableDismissed,
    DisableSubmitClicked,
    SendMaxClicked,
    TradeMaxClicked,
    TradeUnitToggled,
    SendContinueClicked,
    SendSubmitClicked,
    SendCompleted,
    SendFailed(String),
    TxCardToggled(String),
    CopyTxHash(String),
    CopyTxHashFeedback,
    HistoryLoadMore(crate::channel::HistoryList),
    CreateSubmitClicked,
    CreateCompleted,
    ThemeSet(crate::controller::app_state::Theme),
    ToggleHideBalance,
    DisplayCcySet(BaseCcy),
    OpenSettings,
    CloseSettings,
    BtcImportWalletClicked,
    BtcCreateWalletClicked,
    BtcGenerateMnemonic,
    BtcImportModeChanged(ImportMode),
    BtcWord25Chosen(bool),
    BtcScriptTypeChosen(crate::btc_script_type::BtcScriptType),
    BtcImportSubmitClicked,
    BtcImportCompleted,
    BtcImportFailed,
    BtcCreateSubmitClicked,
    BtcCopyMnemonic,
    BtcCreateCompleted,
    BtcCreateFailed,
    BtcBackClicked,
    BtcDeleteKey,
    BtcRemoveWallet,
    BtcWalletRemoved,
    BtcCopyAddress,
    BtcGenerateReceiveAddress,
    BtcRemoveReceiveAddress(String),
    BtcSelectReceiveAddress(String),
    BtcDropAsked,
    BtcDropDismissed,
    BtcBalanceUnitToggled,
    EscapeDismiss,
    BtcSendFeeTierSelected(BtcFeeTier),
    BtcSendDispatched,
    BtcSendContinueClicked,
    BtcSendSubmitClicked,
    BtcSendCompleted,
    BtcSendFailed(String),
    BtcBumpClicked(String),
    BtcBumpDismissed,
    BtcBumpFeeTierSelected(BtcFeeTier),
    BtcBumpSubmitClicked,
    BtcBumpCompleted,
    BtcBumpFailed(String),
    CopyMnemonicFeedback,
    BtcCopyMnemonicFeedback,
    BtcCopyAddressFeedback,
    BtcCopyMasterAddress,
    BtcCopyMasterAddressFeedback,
    WindowResized(f32, f32),
    Grid(crate::controller::panes::GridMsg),
    BtcGrid(crate::controller::panes::GridMsg),
    GridShortcut(crate::controller::panes::GridMsg),
    TradeContinueClicked,
    TradeSideSet(TradeSide),
    TradeOrderOptionSet(u8),
    TradePairSelected(String, String),
    TradeMarketSelected,
    TradeLimitSelected,
    TradeBookPriceClicked(String),
    TradeSubmitClicked,
    TradeCompleted,
    CancelOrderClicked(u32),
    CancelDismissed,
    CancelSubmitClicked,
    CancelCompleted,
    PrefsEraseAsked,
    PrefsEraseDismissed,
    PrefsEraseConfirmed,
    RateHistoryReceived(crate::channel::RateHistoryMap),
    RateHistoryLongReceived(crate::channel::RateHistoryMap),
    ReimportNext,
    ReimportBack,
    ReimportWord25Chosen(bool),
    ReimportConfirm,
    ReimportSuccess,
    ReimportFailed(String),
    KeyMgmtRestoreToggled,
    BtcKeyMgmtRestoreToggled,
    BtcReimportConfirm,
    BtcReimportSuccess,
    BtcReimportFailed(String),
    BtcTxSelected(String),
    BtcCopyTxid(String),
    BtcCopyTxidFeedback,
}