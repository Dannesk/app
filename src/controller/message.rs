use crate::controller::app_state::{
    BaseCcy, BtcFeeTier, ImportMode, Tab, TradeSide, XrpTokenTab,
};
use crate::channel::ActivityLogState;


/// Identifies a secret input buffer in `AppState`. Every memory-hardened field
/// (passphrase / bip39 / seed across all flows) gets a variant here, and a
/// matching arm in `AppState::secure_field_mut`. This lets one generic
/// `Message::SecureEdit` drive them all instead of ~4 messages per field.
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
    /// Whether this buffer holds a **recovery phrase** — a sequence of BIP39
    /// words, where whitespace is a separator and nothing else.
    ///
    /// The one thing that distinguishes these from every other secret here.
    /// A key, a 25th word or a PIN is arbitrary text whose spaces are part of
    /// the secret: collapsing a double space inside a key would seal a wallet
    /// under something the user cannot type back. A phrase has no such reading
    /// — two spaces between two words are a copy artefact, and normalising
    /// them is the only way a messy clipboard drag produces the same 24 words
    /// as a clean one.
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

/// The most characters a 25th word may have, on the setup flows.
///
/// A ceiling, not a target. Fifty random printable characters is ~330 bits —
/// past every rung the entropy ladder has — so nothing is lost above it, and
/// a bound is what lets the field, the meter and the derivation agree on the
/// biggest word they will ever meet. It is also where hardware wallets stop
/// (Trezor caps the passphrase at 50), so a word chosen here restores
/// elsewhere. BIP39 itself sets no limit; this app does.
pub const WORD25_MAX: usize = 50;

/// The most characters a storage key may have, on the setup flows. The same
/// ceiling for the same reason: fifty random characters is past the top of the
/// ladder, and the meter, the field and Argon2id then agree on the largest key
/// they will meet. The signing-time twin of this field stays unbounded.
pub const KEY_MAX: usize = 50;

impl SecureField {
    /// The most characters this field accepts, if it is bounded. Enforced by
    /// the widget (a keystroke past it is dropped, a paste is cut to fit) and
    /// again by `AppState::apply_secure_edit`, so no path around the widget
    /// can overfill the buffer.
    pub fn char_cap(self) -> Option<usize> {
        match self {
            Self::XrpBip39 | Self::BtcBip39 => Some(WORD25_MAX),
            Self::XrpEncryption | Self::BtcEncryption => Some(KEY_MAX),
            _ => None,
        }
    }
}

/// An edit applied to a [`SecureField`]'s buffer (or its reveal flag).
#[derive(Debug, Clone)]
pub enum SecureOp {
    Insert(usize, char),
    Remove(usize),
    Paste(usize, String),
    ToggleReveal,
}

/// Which pane's destination tag a [`Message::TagEdit`] and its siblings act
/// on. The send tag is the one payment's; the receive tag is the wallet's,
/// kept in its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagPane {
    Send,
    Receive,
}

/// Identifies a **non-secret** input buffer that is drawn on the character
/// grid — an address, an amount, a fee.
///
/// Same shape as [`SecureField`], and deliberately so: a field inside a `tui`
/// box has to lay itself out on the grid and wrap where the frame says, which
/// is machinery `SecureInput` already owns. Reusing it means the edits arrive
/// as the same positional ops, so these get their own field id rather than
/// riding `SecureField` — nothing here is a secret, and nothing here belongs in
/// an mlocked buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlainField {
    BtcSendRecipient,
    BtcSendAmount,
    BtcSendFiat,
    BtcSendFee,
    /// The fee-bump stack's custom fee, absolute satoshis.
    BtcBumpFee,
    XrpSendRecipient,
    XrpSendAmount,
    XrpSendFiat,
    /// The send pane's tag form — a DRAFT; `TagSet` commits it.
    XrpSendTag,
    /// The receive pane's tag form — a draft likewise. The tag itself, once
    /// set, is what the wallet's own address is shown with as an X-address.
    XrpReceiveTag,
    /// The trade card's amount — what you pay.
    TradeAmount,
    /// The trade card's receive amount, when that is the side being typed.
    TradeReceive,
    /// The trade card's limit price, in Limit Order mode.
    TradeLimit,
    /// The pair picker's search box.
    TradePairQuery,
}

impl PlainField {
    /// Which flow owns the field — the controller routes the after-edit work
    /// (the fiat cross, a tier selecting itself) to that flow's handler.
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

/// Why the activity watchdog stopped waiting. Two different facts, so two
/// different sentences on the log (`controller::mod`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityVerdict {
    /// The link was up for `ACTIVITY_BUDGET` in total and nothing landed.
    Silent,
    /// The link has been down for `LINK_DOWN_CAP` without coming back.
    LinkDown,
}

#[derive(Debug, Clone)]
pub enum Message {
    Sync,
    /// The system's fonts were indexed behind the window and merged in
    /// (`utils/fonts.rs`). Carries nothing: its arrival rebuilds the view, and
    /// that layout re-shapes the text with the full fallback list.
    SystemFontsIndexed,
    /// The first frame is on screen. Starts the read of the system's fonts,
    /// which must not compete with that frame for the disk (`utils/fonts.rs`).
    FirstFrame,
    /// Generic edit for any memory-hardened secret field.
    SecureEdit(SecureField, SecureOp),
    /// `paste ›` on a secure field. Separate from [`SecureOp::Paste`] because
    /// the clipboard read is a `Task` — `SecureEdit` applies its op inline —
    /// and because a paste *button* replaces the field rather than inserting
    /// at the caret.
    SecurePasteRequested(SecureField),
    /// `Clear ›` on a secure field: the buffer is emptied and zeroised.
    SecureClearRequested(SecureField),
    /// `Clear ›` on a pane's whole sign block: all three credential buffers
    /// are emptied and zeroised, and their reveals go dark.
    ///
    /// One message rather than three `SecureClearRequested`s because this is
    /// one user act — *I changed my mind* — and a clear that left the 25th
    /// word behind would be a half-answer to it. The fields ride the message
    /// so the six sign blocks need no per-caller wiring: the block builds it
    /// from the `SignFields` it was already handed.
    ///
    /// It does NOT touch the recipient, amount or ticket around it. Those are
    /// not secrets, they cost real effort to retype, and the thing the user
    /// wants gone is the phrase.
    SignCredentialCleared {
        secret: SecureField,
        phrase: SecureField,
        bip39:  SecureField,
    },
    /// The clipboard came back. `None` when it held nothing readable.
    SecurePasted(SecureField, Option<String>),
    /// Generic edit for a non-secret field drawn on the character grid.
    PlainEdit(PlainField, SecureOp),
    /// `paste ›` on a plain field — replaces the field's contents.
    PlainPasteRequested(PlainField),
    /// The clipboard came back. `None` when it held nothing readable.
    PlainPasted(PlainField, Option<String>),
    /// PIN view-gate: the 6-digit entry was submitted (enter OR create/confirm,
    /// resolved from state).
    /// A digit typed on the launch gate. The PIN is a plain `String` and always
    /// will be: this gate stops someone glancing at an unattended screen, and
    /// nothing else. It cannot protect a seed — the seed is encrypted at rest
    /// and only the sign-time credential opens it — so hardening the buffer
    /// would buy nothing and imply a guarantee the screen does not make.
    GateDigit(char),
    GateBackspace,
    /// The back circle on the confirm step — returns to `choose a pin` with the first
    /// PIN restored in the field, never cleared. A mismatch must not dead-end.
    GateBack,
    GatePinSubmitted,
    /// Settings hub: launch gate on/off. Off forgets the PIN entirely; on with
    /// no gate relocks into set-up.
    GateEnabledToggled(bool),
    /// Self-wipe finished its on-screen beat; quit the app.
    GateWipeExit,
    TabChanged(Tab),
    ImportWalletClicked,
    CreateWalletClicked,
    GenerateMnemonic,
    ImportModeChanged(ImportMode),
    /// The setup screen's 25th-word segmented: `Yes` (there is a 25th word) or
    /// `No`. `No` empties the 25th-word buffer, so a word typed and then
    /// disowned cannot derive the wallet.
    Word25Chosen(bool),
    ImportSubmitClicked,
    ImportCompleted,
    ImportFailed,
    ActivityChanged(Option<ActivityLogState>),
    ActivityTick(std::time::Instant),
    ActivityDismiss,
    /// The watchdog gave up on the operation whose `activity_gen` this is.
    ActivityTimeout(u64, ActivityVerdict),
    BackClicked,
    DeleteXrpKey,
    RemoveXrpWallet,
    XrpWalletRemoved,
    CopyAddress,
    CopyAddressFeedback,
    /// The XRP `wallet` pane's glyph — its own beat, so the receive copy on
    /// the same screen does not light for a click it did not get.
    WalletCopyAddress,
    WalletCopyAddressFeedback,
    CopyMnemonic,
    FocusNext,
    XrpTokenTabChanged(XrpTokenTab),
    /// The chip in the send pane's amount field: swap the pane's face for the
    /// asset list, and back.
    SendAssetPickerToggled,
    /// `set destination tag ›` / `change ›`: the pane's face becomes the tag
    /// form, its draft seeded from what is set.
    TagEdit(TagPane),
    /// `Set tag` on the form: the draft becomes the pane's tag (an empty
    /// draft = no tag) and the face swaps back.
    TagSet(TagPane),
    /// `Cancel` on the form: the draft is dropped, the face swaps back.
    TagCancelled(TagPane),
    /// Tap an AVAILABLE row in the Tokens modal → set the token and open the
    /// trustline-enable signing surface (a second modal layer).
    EnableTokenClicked(&'static str),
    /// Back / dismiss from the enable card → return to the Tokens modal.
    EnableDismissed,
    EnableSubmitClicked,
    /// `disable` on a HELD row at a zero balance → arm the token and swap the
    /// tokens pane for the trustline-removal signing face.
    DisableTokenClicked(&'static str),
    /// `Cancel` on the disable face → back to the held list, buffers dropped.
    DisableDismissed,
    DisableSubmitClicked,
    /// `max ›` on the XRP amount row: the whole available balance in the chosen
    /// asset, less the network fee when that asset is XRP itself.
    SendMaxClicked,
    /// `max ›` on the ticket's PAY field: the whole balance of the pay asset
    /// (less fee and, for a resting order, the reserve, when that asset is
    /// XRP), to the ledger's last digit. The anchor moves to pay.
    TradeMaxClicked,
    /// The ticket's unit chip: the one amount field flips between the
    /// base and the quote asset. The unit IS the anchor — the side you type
    /// in is the exact side — so this flips `trade_anchor` and nothing else
    /// (2026-09-16; before it, Amount and Total were two fields and the
    /// anchor was whichever one you typed in last).
    TradeUnitToggled,
    SendContinueClicked,
    SendSubmitClicked,
    SendCompleted,
    SendFailed(String),
    TxCardToggled(String),
    /// The settled tail line of an expanded XRP row: hash to the clipboard.
    CopyTxHash(String),
    CreateSubmitClicked,
    CreateCompleted,
    /// Settings ▸ Appearance ▸ Theme. A SET, not a flip: the picker is an
    /// N-option segment driven by `Theme::SELECTABLE`, so a third palette costs
    /// one array entry rather than a new control.
    ThemeSet(crate::controller::app_state::Theme),
    ToggleHideBalance,
    /// Settings ▸ Display currency: "this is my currency". Sets the active base
    /// AND the appended 4th tab together — a non-major becomes the 4th, a major
    /// clears it (which is how the row returns to the three defaults).
    DisplayCcySet(BaseCcy),
    OpenSettings,
    CloseSettings,
    // Rates — global lookup modal (fired from any balance screen, not a tab)
    // BTC
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
    /// Issue another receive address into the pool (capped, gap-guarded).
    BtcGenerateReceiveAddress,
    /// Drop one address from the pool; the shorter live list unsubscribes it.
    BtcRemoveReceiveAddress(String),
    /// Show this pool address on the receive card.
    BtcSelectReceiveAddress(String),
    /// Swap the receive card between the QR and the pool list (a pane FACE).
    BtcTogglePoolFace,
    /// Copy one pool address — the row's own glyph, not the shown one.
    BtcCopyPoolAddress(String),
    /// Esc pressed: close whichever overlay is open — the panels menu, the
    /// asset picker, the pair search.
    EscapeDismiss,
    /// A fee tier was picked on send step 2. `Custom` hands the sats field to
    /// the user; the other four are computed from the live node tiers.
    BtcSendFeeTierSelected(BtcFeeTier),
    /// `max ›` — fill the amount with everything the fee leaves behind.
    /// A signed transaction has been handed to the outgoing channel — the one
    /// irreversible moment in the send flow. See
    /// [`crate::channel::BtcSendDispatches`] for why there is no failing twin.
    BtcSendDispatched,
    BtcSendContinueClicked,
    BtcSendSubmitClicked,
    BtcSendCompleted,
    BtcSendFailed(String),
    /// `modify fee ›` on a pending outgoing row: arms the bump stack over
    /// the transactions panel and asks the relay to describe the transaction.
    BtcBumpClicked(String),
    BtcBumpDismissed,
    BtcBumpFeeTierSelected(BtcFeeTier),
    BtcBumpSubmitClicked,
    BtcBumpCompleted,
    BtcBumpFailed(String),
    CopyMnemonicFeedback,
    BtcCopyMnemonicFeedback,
    BtcCopyAddressFeedback,
    /// The `wallet` pane's glyph: copies #0, the master address, always —
    /// never the rotating receive address.
    BtcCopyMasterAddress,
    BtcCopyMasterAddressFeedback,
    WindowResized(f32, f32),
    /// The XRP pane grid — every layout action, the menu, the chart pane's
    /// period pill and the send pane's `Send payment`. One sub-enum,
    /// routed to `controller::panes`, so the grid can grow without growing
    /// this list.
    Grid(crate::controller::panes::GridMsg),
    /// The BTC grid's twin of `Grid` — its own variant so the dispatch is
    /// explicit (a BTC message must never fall through to an XRP handler).
    BtcGrid(crate::controller::panes::GridMsg),
    /// `Ctrl+Z` / `Ctrl+0` from the keyboard, which cannot know which grid
    /// is up: `panes::handle_shortcut` acts on the one on screen.
    GridShortcut(crate::controller::panes::GridMsg),
    TradeContinueClicked,
    /// `Sell` / `Buy` — the ticket's side. Swaps pay and receive underneath
    /// and carries the amount and total with their assets.
    TradeSideSet(TradeSide),
    TradeOrderOptionSet(u8),
    /// A pair was chosen: `(base, quote)` as the ticket shows it.
    TradePairSelected(String, String),
    /// Price mode: `Market Order` (the book's top becomes the limit).
    TradeMarketSelected,
    /// Price mode: `Limit Order` (the typed price is the limit).
    TradeLimitSelected,
    /// A book price was clicked under Limit: it becomes the limit price. The
    /// book is inert under Market (the click is gated, never mode-changing).
    TradeBookPriceClicked(String),
    TradeSubmitClicked,
    /// The order left the app. NOT "the order filled" — see the handler.
    /// There is deliberately no `TradeFailed` twin: an outcome is a fact about
    /// the ledger, it arrives on the relay socket rather than as the result of
    /// this task, and it belongs in the activity log rather than on the trade
    /// surface. The variant that used to sit here had no emitter and a handler
    /// that re-rendered the sign step over a form the dispatch had emptied.
    TradeCompleted,
    CancelOrderClicked(u32),
    CancelDismissed,
    CancelSubmitClicked,
    CancelCompleted,
    /// Settings ▸ Data ▸ `erase`. Wipes immediately — there is no confirm
    /// step, by design: this is an emergency button and a stack is the opposite
    /// of one. Granular removal lives in Key management.
    PrefsEraseConfirmed,
    RateHistoryReceived(crate::channel::RateHistoryMap),
    RateHistoryLongReceived(crate::channel::RateHistoryMap),
    // ── Key management page ──────────────────────────────────────────────────
    // A routed page in the import/send grammar, reached from the
    // `Key management · …` chip. Removing the seed is the chain's existing
    // delete message; `Reimport*` drive the 3-step restore flow (phrase →
    // 25th word → encryption key), which ends in the chain's reimport confirm.
    // The twins differ only in which chain's state they flip.
    ReimportNext,
    ReimportBack,
    /// Restore step 2's radio: did this wallet use a 25th word?
    ReimportWord25Chosen(bool),
    ReimportSpinnerTick,
    ReimportConfirm,
    ReimportSuccess,
    ReimportFailed(String),
    KeyMgmtRestoreToggled,
    BtcKeyMgmtRestoreToggled,
    BtcReimportSpinnerTick,
    BtcReimportConfirm,
    BtcReimportSuccess,
    BtcReimportFailed(String),
    /// Toggle a row of the transactions pane open, by txid.
    BtcTxSelected(String),
    BtcCopyTxid(String),
}