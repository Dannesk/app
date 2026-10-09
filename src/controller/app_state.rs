use zeroize::Zeroize;
use crate::channel::ActivityLogState;
use crate::controller::message::{PlainField, SecureField, SecureOp};
use crate::secure::SecureString;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};


#[derive(Debug, Clone, PartialEq, Default)]
pub enum Tab {
    #[default]
    Balance,
    Xrp,
    Btc,
}

/// Byte offset of the `i`th **character**, clamped to the end.
///
/// Grid fields address themselves in characters (that is what a column is), and
/// `String` addresses itself in bytes. Everything these fields hold is ASCII
/// today, but a pasted address with a stray non-ASCII character would make the
/// two disagree and panic on a byte boundary — so the conversion is explicit
/// and there is exactly one of it.
fn byte_index(s: &str, i: usize) -> usize {
    s.char_indices().nth(i).map_or(s.len(), |(b, _)| b)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum XrpView {
    #[default]
    Menu,
    Import,
    Create,
    // Send, Trade, Transactions, KeyMgmt and RemoveWallet were routes of the
    // old balance screen; every one is a pane or a pane face on the grid
    // now (deleted 2026-09-09). `Menu` IS the grid.
}

/// Which asset tab is active on the manage-XRP screen. Issued tokens are
/// identified by their registry code (`"RLUSD"`, …) rather than a per-token
/// variant, so adding a token needs no change here.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum XrpTokenTab {
    #[default]
    Xrp,
    Token(&'static str),
}

impl XrpTokenTab {
    /// The registry code for an issued-token tab; `None` for the native XRP tab.
    pub fn code(&self) -> Option<&'static str> {
        match self {
            XrpTokenTab::Xrp => None,
            XrpTokenTab::Token(c) => Some(*c),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum BtcView {
    #[default]
    Menu,
    Import,
    Create,
}

/// A fee tier on send step 2 — the SAME four the dashboard's priority table
/// draws, plus a hand-typed one.
///
/// The four named tiers are not our numbers: they are `btc_node_rx.tiers`,
/// which is the one field both screens read. A send screen that estimated its
/// own would eventually disagree with the gauge the user just looked at, and
/// the two sit one click apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BtcFeeTier {
    /// `tiers[0]` — the relay floor. Named for what it *is*: the least the
    /// network will carry. An aspirational name ("no rush") would have hidden
    /// that this is a floor and not a slow-but-normal option.
    Minimum,
    /// `tiers[1]`.
    Low,
    #[default]
    /// `tiers[2]`.
    Med,
    /// `tiers[3]` — next block.
    High,
    /// Whatever was typed into the sats field.
    Custom,
}

impl BtcFeeTier {
    /// Row order, left to right.
    pub const ALL: [BtcFeeTier; 5] = [
        BtcFeeTier::Minimum,
        BtcFeeTier::Low,
        BtcFeeTier::Med,
        BtcFeeTier::High,
        BtcFeeTier::Custom,
    ];

    /// The word in the tier row. Lower case — it is a value on a grid, not a
    /// title.
    pub fn label(self) -> &'static str {
        match self {
            BtcFeeTier::Minimum => "min",
            BtcFeeTier::Low => "low",
            BtcFeeTier::Med => "med",
            BtcFeeTier::High => "high",
            BtcFeeTier::Custom => "custom",
        }
    }

    /// Index into `BtcNodeStats::tiers`, or `None` for the typed one.
    pub fn index(self) -> Option<usize> {
        match self {
            BtcFeeTier::Minimum => Some(0),
            BtcFeeTier::Low => Some(1),
            BtcFeeTier::Med => Some(2),
            BtcFeeTier::High => Some(3),
            BtcFeeTier::Custom => None,
        }
    }
}

/// App-wide base currency: the unit every total / chart / rate is quoted in.
/// Drives the Balance hero, the Rates "Quoted in" selector, and Rates row
/// quotes. Persisted to settings.json as its `code()` (e.g. `"SGD"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BaseCcy {
    #[default]
    Usd,
    Eur,
    Sgd,
    Aud,
}

impl BaseCcy {
    /// Every currency the base can be — the Settings ▸ Display currency picker's
    /// option list, and the ONLY place a currency is chosen: no screen carries
    /// tabs of its own (2026-08-29), every total and every `XRP/…`, `BTC/…`
    /// rate simply reads in the chosen base. USD by default.
    pub const ALL: [BaseCcy; 4] = [BaseCcy::Usd, BaseCcy::Eur, BaseCcy::Sgd, BaseCcy::Aud];

    /// Asset / rate-key code, used for price lookups, display, and persistence.
    pub fn code(self) -> &'static str {
        match self {
            BaseCcy::Usd => "USD",
            BaseCcy::Eur => "EUR",
            BaseCcy::Sgd => "SGD",
            BaseCcy::Aud => "AUD",
        }
    }

    /// The symbol a figure wears when the design puts one in front of the
    /// number (`$2.00`) rather than a code after it (`2.00 USD`). Both
    /// grammars are in the handoffs: symbol in a review row, code in a list
    /// row.
    pub fn symbol(self) -> &'static str {
        match self {
            BaseCcy::Usd => "$",
            BaseCcy::Eur => "\u{20ac}",
            BaseCcy::Sgd => "S$",
            BaseCcy::Aud => "A$",
        }
    }

    /// Parse a stored code; unknown / missing falls back to USD. A settings.json
    /// still holding `"BRL"` (dropped 2026-09-04) lands here too.
    pub fn from_code(s: &str) -> Self {
        match s {
            "EUR" => BaseCcy::Eur,
            "SGD" => BaseCcy::Sgd,
            "AUD" => BaseCcy::Aud,
            _ => BaseCcy::Usd,
        }
    }

}


/// The exact side of a trade — see `AppState::trade_anchor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeAnchor {
    Pay,
    Receive,
}

/// Which of the send form's two amounts the user typed last. The other is
/// derived from it at the live rate — on the keystroke, and again on every
/// rate tick (`resync_send_twin`), so the pair never drifts apart while the
/// market moves. The anchor itself is never rewritten: it is the field being
/// typed, and rates tick every 250ms. Typed 1 XRP, 1 XRP goes and the fiat
/// readout follows the market; typed 100 USD, 100 USD worth goes and the
/// crypto amount follows the market until it is signed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SendAnchor {
    #[default]
    Amount,
    Fiat,
}

/// Which side of the market the order is on — the trader's word for it. The
/// engine works in pay/receive; `Sell` pays the market's base asset and
/// receives the quote, `Buy` the reverse. See `xrp::trade_market_pair`.
/// Market (the walked bound is the price) or Limit (the typed price is).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeContract {
    Market,
    Limit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSide {
    Sell,
    Buy,
}

#[derive(Copy, Debug, Clone, PartialEq, Default)]
pub enum ChartPeriod {
    #[default]
    OneHour,
    OneDay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnableInputMode {
    Passphrase,
    Seed,
}

impl EnableInputMode {
    /// The credential is detected, not chosen (twin of mobile's
    /// `rememberSigningMode` in SigningFields.kt): a wallet was stored exactly
    /// one way, so the signing screens render only that mode's input — the
    /// other could only ever produce a failure. A purged key is cold, and cold
    /// wallets sign from the phrase itself.
    pub fn detect(key_deleted: bool, _key_mode: crate::channel::KeyMode) -> Self {
        if key_deleted {
            EnableInputMode::Seed
        } else {
            EnableInputMode::Passphrase
        }
    }
}

pub use crate::wallet::ImportMode;

/// The persisted theme choice. `Dark` is obsidian — the name is what
/// settings.json has always written and is not worth migrating.
///
/// **`Graphite` is not offered by the picker yet.** The variant, its
/// constructor and its persistence all exist so that turning it on is one entry
/// in [`Theme::SELECTABLE`] — which happened 2026-09-04 with trade v3, the last
/// compact screen. Surfaces still on `AppPalette` (the transactions and ledger
/// modals, cancel, the balance chart, the update card) draw the obsidian ramp
/// over graphite's window until they get their compact pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Light,
    Dark,
    Graphite,
}

impl Theme {
    /// The Appearance ▸ Theme option set, in the order the segment draws them.
    /// Add `(Theme::Graphite, "graphite")` here and the picker grows a third
    /// option; nothing else changes.
    pub const SELECTABLE: &'static [(Theme, &'static str)] =
        &[(Theme::Dark, "obsidian"), (Theme::Light, "light"), (Theme::Graphite, "graphite")];

    pub fn build(self) -> iced::Theme {
        match self {
            Theme::Dark => crate::utils::theme::dark_theme(),
            Theme::Light => crate::utils::theme::light_theme(),
            Theme::Graphite => crate::utils::theme::graphite_theme(),
        }
    }

    /// Which variant an `iced::Theme` came from, so the picker can mark the
    /// active segment. Matches on the custom theme's name, the same handle
    /// `theme::compact` resolves palettes by.
    pub fn of(theme: &iced::Theme) -> Theme {
        if theme.to_string().contains("Graphite") {
            Theme::Graphite
        } else if crate::utils::theme::is_dark(theme) {
            Theme::Dark
        } else {
            Theme::Light
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    pub selected_tab: Tab,
    /// The display currency — Settings ▸ Display currency — which every
    /// total, rate and chart reads in. Persisted to settings.json. There is
    /// no per-screen override any more.
    pub base_currency: BaseCcy,
    pub xrp_view: XrpView,
    pub btc_view: BtcView,
    pub btc_seed_text: SecureString,
    pub btc_seed_reveal: bool,
    pub btc_encryption_input: SecureString,
    pub btc_encryption_reveal: bool,
    pub btc_import_mode: ImportMode,
    pub btc_bip39_input: SecureString,
    pub btc_bip39_reveal: bool,
    /// Step 2's answer: is there a 25th word? `false` is the default and the
    /// gate for an empty buffer — `true` with nothing typed cannot continue.
    pub btc_word25: bool,
    /// The address type the import/create picker holds. Default bc1q; reset
    /// with the rest of the setup form.
    pub btc_script_type: crate::btc_script_type::BtcScriptType,
    pub xrp_token_tab: XrpTokenTab,
    /// Step-2 Send field's asset-picker dropdown is open (the chip → registry list).
    pub send_asset_picker_open: bool,
    pub enable_passphrase: SecureString,
    pub enable_passphrase_reveal: bool,
    pub enable_bip39: SecureString,
    pub enable_bip39_reveal: bool,
    pub enable_seed_text: SecureString,
    pub enable_seed_reveal: bool,
    pub enable_error: Option<String>,
    /// The send v3 sign stack is floated over the Tokens page, armed by an
    /// `enable` link. The page stays open UNDERNEATH — the stack is an overlay
    /// on it, not a screen that replaces it — so dismissing this leaves the
    /// list exactly where it was. The asset rides `xrp_token_tab`.
    pub show_enable: bool,
    /// The held-tokens pane's face is the trustline-removal form for this
    /// token: a TrustSet with limit 0, which the ledger only honours (line
    /// deleted, reserve refunded) at a zero balance. `None` = the list. Its
    /// own slot, not `xrp_token_tab`: the send pane's chip must not be left
    /// pointing at a line that is about to stop existing.
    pub disable_token: Option<&'static str>,
    pub disable_passphrase: SecureString,
    pub disable_passphrase_reveal: bool,
    pub disable_bip39: SecureString,
    pub disable_bip39_reveal: bool,
    pub disable_seed_text: SecureString,
    pub disable_seed_reveal: bool,
    pub disable_error: Option<String>,
    /// The key-management page's cold face has been swapped for the 3-step
    /// restore flow. Meaningless while the key is present; reset whenever the
    /// page opens or closes.
    pub key_mgmt_restoring: bool,
    pub btc_key_mgmt_restoring: bool,
    /// Restore step: 1 = recovery phrase, 2 = 25th word, 3 = encryption key —
    /// the import flow's own split, because restore IS importing without the
    /// backend call.
    pub reimport_step: u8,
    /// Restore step 2's radio: `true` = this wallet used a 25th word. The
    /// answer was fixed at import — a wrong pick here surfaces as a master-key
    /// mismatch at submit, arbitrated by the bridge.
    pub reimport_word25: bool,
    /// The ENCRYPTION gauge's last reading, held across a submit.
    ///
    /// Every step-2 surface — import, create, restore, both chains — MOVES its
    /// key out to the bridge rather than copying it (see `SecureString::take`),
    /// so the buffer the gauge measures is empty the instant submit fires. The
    /// screen stays mounted while the bridge works, and re-measuring an emptied
    /// buffer redrew the bar at `0 bits` in danger red: a green reading falling
    /// to red at the exact moment the user committed, which reads as their key
    /// being judged and found wanting.
    ///
    /// A bit count is not the secret — it is the number already on screen — so
    /// holding it costs nothing the display had not already spent. `None` means
    /// nothing is in flight and the gauge measures the live buffer.
    pub entropy_hold: Option<f64>,
    /// True from the moment "Remove seed" is tapped until the channel confirms
    /// the key is gone — guards the button against a double-fire through the
    /// async gap (the button draws disabled while it is set).
    pub key_purge_requested: bool,
    pub btc_key_purge_requested: bool,
    /// Which transaction row is expanded in the transactions pane, by txid.
    /// `None` — every row collapsed — is the opening state; clicking the open
    /// row collapses it again. A txid, not an index: the list re-sorts as
    /// rows settle, and the open row should stay the same transaction.
    pub btc_tx_selected: Option<String>,
    /// `Remove wallet` has been tapped on the removal page and the bridge has
    /// not yet reported back — the button draws disabled through the async gap
    /// so a second tap cannot dispatch a second unsubscribe.
    pub remove_wallet_requested: bool,
    pub btc_remove_wallet_requested: bool,
    pub xrp_seed_text: SecureString,
    pub xrp_seed_reveal: bool,
    pub xrp_encryption_input: SecureString,
    pub xrp_encryption_reveal: bool,
    pub xrp_import_mode: ImportMode,
    pub xrp_bip39_input: SecureString,
    pub xrp_bip39_reveal: bool,
    /// See `btc_word25`.
    pub xrp_word25: bool,
    pub send_step: u8,
    pub send_recipient: String,
    /// Manually-entered XRP destination tag (r-address path). Raw text; parsed to
    /// u32 at submit. Ignored when the recipient is an X-address.
    /// The payment's destination tag as SET — what the review shows and the
    /// Payment carries. Digits or empty; `TagSet` is the only writer.
    pub send_destination_tag: String,
    /// The send pane's tag form: up, and what is typed in it.
    pub send_tag_editing: bool,
    pub send_tag_draft: String,
    /// The receive pane's destination tag as set: empty = the classic
    /// address is shown; digits = the address is shown as an X-address
    /// carrying this tag. Persisted in xrp.json (`receive_tag`) — a tag is
    /// handed to a payer, and retyping it each launch invites the one wrong
    /// digit that misroutes. The wallet's file, so remove wallet takes it.
    pub receive_tag: String,
    /// The receive pane's tag form: up, and what is typed in it.
    pub receive_tag_editing: bool,
    pub receive_tag_draft: String,
    /// Whether the recipient screen's "Advanced" disclosure (home of the tag
    /// field) is expanded.
    pub send_amount: String,
    pub send_fiat_amount: String,
    pub send_anchor: SendAnchor,
    pub send_passphrase: SecureString,
    pub send_passphrase_reveal: bool,
    pub send_bip39: SecureString,
    pub send_bip39_reveal: bool,
    pub send_seed_text: SecureString,
    pub send_seed_reveal: bool,
    pub send_error: Option<String>,
    pub create_mnemonic: SecureString,
    pub create_seed_reveal: bool,
    pub tx_expanded: Option<String>,
    pub activity_log: Option<ActivityLogState>,
    pub activity_tick: std::time::Instant,
    pub theme: iced::Theme,
    pub hide_balance: bool,
    pub xrp_copy_feedback: bool,
    pub btc_copy_feedback: bool,
    /// The `wallet` pane's copy glyph beat — its own flag, because the
    /// receive pane's copy can be on screen at the same time and must not
    /// show `copied` for a click it did not get.
    pub btc_master_copy_feedback: bool,
    /// The XRP `wallet` pane's copy glyph beat — same reason as the BTC one.
    pub xrp_wallet_copy_feedback: bool,
    /// The pool address the receive card is CURRENTLY SHOWING. The user picks
    /// it by clicking a row in the pool face; it defaults to the most recently
    /// generated one. Deliberately not persisted — a QR is something you hold
    /// up to someone for a minute, not a setting. `None` = no pool yet (legacy
    /// btc.json without an account xpub), and the card falls back to #0.
    pub btc_receive_address: Option<String>,
    /// The receive pool as curated by the user, in issue order
    /// (`bridge::btc_receive_rotation::receive_pool`). Rebuilt by
    /// `refresh_receive_address`; the face lists it, the live list carries it.
    pub btc_receive_pool: Vec<String>,
    /// The receive card is showing the pool list instead of the QR. A pane
    /// FACE, the same idiom as the XRP tag and the send asset picker — never a
    /// dropdown, never a modal.
    pub btc_pool_face: bool,
    /// Why the last generate was refused, so the button can name what it is
    /// waiting on. Cleared on any pool change.
    pub btc_pool_refusal: Option<crate::bridge::btc_receive_rotation::PoolRefusal>,
    /// Which pool row was just copied, for that row's glyph.
    pub btc_pool_copied: Option<String>,
    /// Settings lives OFF the dock (reached from the top-left circle). When true
    /// it replaces the active tab's content; the dock stays visible with no cell
    /// highlighted.
    pub settings_open: bool,
    pub btc_send_step: u8,
    pub btc_send_recipient: String,
    pub btc_send_amount: String,
    pub btc_send_fiat_amount: String,
    pub btc_send_anchor: SendAnchor,
    /// The fee in **absolute satoshis**, as typed or as computed from the
    /// selected tier. NOT sat/vB — `construct_transaction` subtracts this flat,
    /// so this string is what gets spent.
    pub btc_send_fee: String,
    pub btc_send_fee_tier: BtcFeeTier,
    pub btc_send_error: Option<String>,
    pub btc_send_passphrase: SecureString,
    pub btc_send_passphrase_reveal: bool,
    pub btc_send_bip39: SecureString,
    pub btc_send_bip39_reveal: bool,
    pub btc_send_seed_text: SecureString,
    pub btc_send_seed_reveal: bool,
    /// The pending transaction a fee bump is armed on — the bump stack
    /// floats over the transactions panel while this is set, like XRP's
    /// `cancel_offer_sequence`. The stack prices from the row's own record.
    pub btc_bump_txid: Option<String>,
    pub btc_bump_fee_tier: BtcFeeTier,
    /// The hand-typed fee, absolute satoshis, when the tier is `custom`.
    pub btc_bump_fee: String,
    pub btc_bump_passphrase: SecureString,
    pub btc_bump_passphrase_reveal: bool,
    pub btc_bump_bip39: SecureString,
    pub btc_bump_bip39_reveal: bool,
    pub btc_bump_seed_text: SecureString,
    pub btc_bump_seed_reveal: bool,
    pub trade_step: u8,
    /// The ENGINE's orientation: what this order pays and what it receives —
    /// `TakerGets` / `TakerPays` in the ticket's assets. Under Sell the pay
    /// side is the market's base, under Buy its quote. Named for what they
    /// are since 2026-09-16; as `trade_base_asset` / `trade_quote_asset`
    /// they read as the market pair and, under Buy, as the pair backwards
    /// (user: "in our model it's buy usd/btc"). The TICKET's pair is
    /// `xrp::trade_market_pair`, and it never flips.
    pub trade_pay_asset: String,
    pub trade_receive_asset: String,
    pub trade_amount: String,
    pub trade_limit_price: String,
    pub trade_flags: Vec<String>,
    /// The contract — the user's click, and ONLY the user's (2026-09-15).
    /// The book never changes it: a Market ticket over a book that stops
    /// being walkable stays a Market ticket with a dark button and a dim
    /// segment; the user switches to Limit, or waits. It used to flip to
    /// Limit and back on the book's say-so, rewriting the ticket each time.
    ///
    /// The ticket OPENS on `Limit` (user, 2026-09-16). It opened on no
    /// contract at all for a day, which left the ticket half-lit — a side
    /// chosen, a contract not — and nothing to explain which click the dark
    /// button was waiting for. Limit is the answer that cannot go wrong:
    /// it prices at a number the user typed, so the worst it does is not
    /// fill. Still `Option` because the type is what keeps the book from
    /// inventing a contract; nothing sets it back to `None`.
    pub trade_contract: Option<TradeContract>,
    /// Sell or Buy the market's base asset. The pay/receive pair above is the
    /// engine's orientation; this is the ticket's.
    ///
    /// **Opens on the side the wallet can pay for** (user, 2026-09-16): a
    /// wallet holding XRP and no RLUSD has exactly one trade on XRP/RLUSD,
    /// and it is a Sell; opening on Buy put the first click on the one side
    /// it could not afford. `xrp::trade_default_side` decides at pair pick,
    /// at restore, on clear — and, while the ticket is untouched, on Sync,
    /// so a wallet that lands a second after the app opens is followed.
    pub trade_side: TradeSide,
    /// The user has clicked something on the ticket — a side, a contract,
    /// the unit, a tier. From then on the side is theirs and the wallet
    /// stops choosing it (typing pins it too, by the fields being non-empty).
    pub trade_side_chosen: bool,
    /// The top bar's pair chip has been pressed and the search field is in
    /// its place (2026-09-11). Irrelevant while no pair is held — the field
    /// shows on its own then, there being nothing to open it from.
    pub trade_pair_search_open: bool,
    /// Which side of the order is the exact one — the field the user typed.
    /// `Pay` sells exactly `trade_amount` (signed with `tfSell`, receive is a
    /// `≥`); `Receive` buys exactly `trade_receive` (no `tfSell`, pay is a `≤`).
    pub trade_anchor: TradeAnchor,
    /// The receive amount. Typed when `trade_anchor` is `Receive`; otherwise
    /// the twin of `trade_amount`, rewritten from it on every `Sync` (see
    /// `xrp::refresh_trade_twin`) — the same arrangement as send's USD twin,
    /// so both sides are always live fields and one click lands in either.
    pub trade_receive: String,
    pub trade_pair_query: String,
    pub trade_error: Option<String>,
    /// Server books this screen holds open on the rates socket
    /// (`xrp::sync_trade_books`).
    pub trade_books: Vec<String>,
    pub trade_passphrase: SecureString,
    pub trade_passphrase_reveal: bool,
    pub trade_bip39: SecureString,
    pub trade_bip39_reveal: bool,
    pub trade_seed_text: SecureString,
    pub trade_seed_reveal: bool,
    pub cancel_offer_sequence: Option<u32>,
    pub cancel_passphrase: SecureString,
    pub cancel_passphrase_reveal: bool,
    pub cancel_bip39: SecureString,
    pub cancel_bip39_reveal: bool,
    pub cancel_seed_text: SecureString,
    pub cancel_seed_reveal: bool,
    pub cancel_error: Option<String>,
    pub reimport_seed_text: SecureString,
    pub reimport_seed_reveal: bool,
    pub reimport_passphrase: SecureString,
    pub reimport_passphrase_reveal: bool,
    pub reimport_bip39: SecureString,
    pub reimport_bip39_reveal: bool,
    pub reimport_error: Option<String>,
    pub reimport_spinning: bool,
    pub reimport_spinner_angle: f32,
    pub btc_reimport_seed_text: SecureString,
    pub btc_reimport_seed_reveal: bool,
    pub btc_reimport_passphrase: SecureString,
    pub btc_reimport_passphrase_reveal: bool,
    pub btc_reimport_bip39: SecureString,
    pub btc_reimport_bip39_reveal: bool,
    pub btc_reimport_error: Option<String>,
    pub btc_reimport_spinning: bool,
    pub btc_reimport_spinner_angle: f32,
    pub window_width: f32,
    pub window_height: f32,
    // (chain "XRP"|"BTC", truncated address "rPj7…MjqA") — loaded once at startup
    pub pin_wallet_identities: Vec<(String, String)>,
    /// View-gate (see [`crate::gate`]): passed for this session.
    pub gate_unlocked: bool,
    /// A gate is registered on disk (launch shows "enter", not "create").
    pub gate_exists: bool,
    /// Launch gate enabled (settings toggle). The credential still exists when
    /// off — this only decides whether launch demands it.
    pub gate_enabled: bool,
    /// First PIN of the create→confirm step, awaiting re-entry.
    /// The first PIN of a create/confirm pair, held while the second is typed.
    pub gate_pin_draft: Option<String>,
    /// The 6-digit launch-gate PIN. A plain `String` by decision, not oversight:
    /// see [`Message::GateDigit`]. Six digits is trivially brute-forced and the
    /// gate guards against snooping, not against an attacker with the file — so
    /// a hardened buffer here would be theatre, and theatre that implies a
    /// promise the screen cannot keep.
    pub gate_pin_input: String,
    /// Inline gate feedback ("Patterns didn't match", "Incorrect pattern", …).
    pub gate_error: Option<String>,
    /// When the current gate error landed. Drives the shake and the red field
    /// for one second off the redraw pump — no timer message, no tween state.
    pub gate_error_at: Option<std::time::Instant>,
    /// Self-wipe in progress (too many wrong attempts) — show the farewell beat.
    pub gate_wiping: bool,
    /// Per-asset USD price history, **1 hour** at 30s buckets (120 samples) —
    /// backs the 1H chart. `short`/`long` are only relative to each other; the
    /// spans are the thing, so read them here rather than inferring from the
    /// names. Anything labelled `24h` wants the other one.
    pub rate_history_short: HashMap<String, VecDeque<(u64, f32)>>,
    /// Per-asset USD price history, **24 hours** at 5-min buckets (288 samples)
    /// — backs the 1D chart and the dashboards' `24h` change.
    pub rate_history_long: HashMap<String, VecDeque<(u64, f32)>>,
    /// The XRP dashboard's pane grid (2026-09-09) — see `controller::panes`.
    /// Per chain: BTC gets its own when its handoff lands.
    pub xrp_grid: crate::controller::panes::Grid,
    /// The BTC tab's pane grid (2026-09-10) — its own tree, its own settings
    /// key, so closing `send` on one chain never closes it on the other.
    pub btc_grid: crate::controller::panes::Grid,
    /// What the BTC receive pane's rotating address was last derived
    /// against: a fingerprint of the pushed UTXO set. The pane is on screen
    /// all day, so rotation cannot run "on open" as the receive page did —
    /// it runs when this changes (a payment landed, or was spent) and on
    /// every switch to the tab. See `controller::btc::refresh_receive_address`.
    pub btc_utxo_fingerprint: u64,
    /// The network pane's tape: one bar per validated ledger this session
    /// has seen, newest last. Display-only, derived from the node frames.
    pub ledger_tape: VecDeque<crate::controller::panes::LedgerBar>,
}

impl AppState {
    /// App-wide UI scale: the LESSER of the width and height ratios against the
    /// 900×800 reference viewport, then lifted by [`Self::SCALE_LIFT`]. Width
    /// alone was wrong — the iced default window is 1024×768, and 1024/900
    /// inflated every vertical size by ~14% in a window that got no taller,
    /// pushing fixed layouts (the XRP ledger-state grid's third row) below the
    /// fold. Shorter windows shrink instead of clipping, floored at 0.85 where
    /// the smallest text stops being legible.
    pub fn scale(&self) -> f32 {
        (self.window_width / 900.0)
            .clamp(1.0, 1.3)
            .min((self.window_height / 800.0).clamp(0.85, 1.3))
            * Self::SCALE_LIFT
    }

    /// The global lift on top of the viewport ratio (2026-09-01, user call
    /// after living with import + send v3): the compact spec's type is right
    /// to be compact, but at the reference ratio it sat a step small while
    /// screens had room to spare, so everything rides 10% up — the whole
    /// range, not just the floor. One knob, applied after the clamps, so the
    /// geometry semantics above are untouched and every screen (old surfaces
    /// included — they're due for the compact pass anyway) moves together.
    const SCALE_LIFT: f32 = 1.10;

    /// Maps a [`SecureField`] to its backing buffer and reveal flag. The one
    /// place the field set is enumerated; add a flow's fields here as it's
    /// migrated to the secure widgets.
    fn secure_field_mut(&mut self, field: SecureField) -> (&mut SecureString, &mut bool) {
        match field {
            SecureField::SendPassphrase => (&mut self.send_passphrase, &mut self.send_passphrase_reveal),
            SecureField::SendBip39 => (&mut self.send_bip39, &mut self.send_bip39_reveal),
            SecureField::SendSeed => (&mut self.send_seed_text, &mut self.send_seed_reveal),
            SecureField::CancelPassphrase => (&mut self.cancel_passphrase, &mut self.cancel_passphrase_reveal),
            SecureField::CancelBip39 => (&mut self.cancel_bip39, &mut self.cancel_bip39_reveal),
            SecureField::CancelSeed => (&mut self.cancel_seed_text, &mut self.cancel_seed_reveal),
            SecureField::TradePassphrase => (&mut self.trade_passphrase, &mut self.trade_passphrase_reveal),
            SecureField::TradeBip39 => (&mut self.trade_bip39, &mut self.trade_bip39_reveal),
            SecureField::TradeSeed => (&mut self.trade_seed_text, &mut self.trade_seed_reveal),
            SecureField::EnablePassphrase => (&mut self.enable_passphrase, &mut self.enable_passphrase_reveal),
            SecureField::EnableBip39 => (&mut self.enable_bip39, &mut self.enable_bip39_reveal),
            SecureField::EnableSeed => (&mut self.enable_seed_text, &mut self.enable_seed_reveal),
            SecureField::DisablePassphrase => (&mut self.disable_passphrase, &mut self.disable_passphrase_reveal),
            SecureField::DisableBip39 => (&mut self.disable_bip39, &mut self.disable_bip39_reveal),
            SecureField::DisableSeed => (&mut self.disable_seed_text, &mut self.disable_seed_reveal),
            SecureField::ReimportPassphrase => (&mut self.reimport_passphrase, &mut self.reimport_passphrase_reveal),
            SecureField::ReimportBip39 => (&mut self.reimport_bip39, &mut self.reimport_bip39_reveal),
            SecureField::ReimportSeed => (&mut self.reimport_seed_text, &mut self.reimport_seed_reveal),
            SecureField::BtcSendPassphrase => (&mut self.btc_send_passphrase, &mut self.btc_send_passphrase_reveal),
            SecureField::BtcSendBip39 => (&mut self.btc_send_bip39, &mut self.btc_send_bip39_reveal),
            SecureField::BtcSendSeed => (&mut self.btc_send_seed_text, &mut self.btc_send_seed_reveal),
            SecureField::BtcBumpPassphrase => (&mut self.btc_bump_passphrase, &mut self.btc_bump_passphrase_reveal),
            SecureField::BtcBumpBip39 => (&mut self.btc_bump_bip39, &mut self.btc_bump_bip39_reveal),
            SecureField::BtcBumpSeed => (&mut self.btc_bump_seed_text, &mut self.btc_bump_seed_reveal),
            SecureField::BtcReimportPassphrase => (&mut self.btc_reimport_passphrase, &mut self.btc_reimport_passphrase_reveal),
            SecureField::BtcReimportBip39 => (&mut self.btc_reimport_bip39, &mut self.btc_reimport_bip39_reveal),
            SecureField::BtcReimportSeed => (&mut self.btc_reimport_seed_text, &mut self.btc_reimport_seed_reveal),
            SecureField::XrpImportSeed => (&mut self.xrp_seed_text, &mut self.xrp_seed_reveal),
            SecureField::XrpEncryption => (&mut self.xrp_encryption_input, &mut self.xrp_encryption_reveal),
            SecureField::XrpBip39 => (&mut self.xrp_bip39_input, &mut self.xrp_bip39_reveal),
            SecureField::CreateSeed => (&mut self.create_mnemonic, &mut self.create_seed_reveal),
            SecureField::BtcImportSeed => (&mut self.btc_seed_text, &mut self.btc_seed_reveal),
            SecureField::BtcEncryption => (&mut self.btc_encryption_input, &mut self.btc_encryption_reveal),
            SecureField::BtcBip39 => (&mut self.btc_bip39_input, &mut self.btc_bip39_reveal),
        }
    }

    /// Apply a generic secure-field edit. Drives every memory-hardened input.
    pub fn apply_secure_edit(&mut self, field: SecureField, op: SecureOp) {
        let cap = field.char_cap();
        let (buf, reveal) = self.secure_field_mut(field);
        match op {
            // The widget already refuses past the cap; this is the backstop
            // for any path that reaches the buffer without going through it.
            SecureOp::Insert(i, c) => {
                if cap.is_none_or(|cap| buf.char_len() < cap) {
                    buf.insert(i, c);
                }
            }
            SecureOp::Remove(i) => buf.remove(i),
            SecureOp::Paste(i, mut text) => {
                if field.is_phrase() {
                    let mut clean = normalise_phrase(&text);
                    buf.insert_str(i, &clean);
                    clean.zeroize();
                } else if let Some(cap) = cap {
                    let room = cap.saturating_sub(buf.char_len());
                    let mut cut: String = text.chars().take(room).collect();
                    buf.insert_str(i, &cut);
                    cut.zeroize();
                } else {
                    buf.insert_str(i, &text);
                }
                text.zeroize();
            }
            SecureOp::ToggleReveal => *reveal = !*reveal,
        }
    }

    fn plain_field_mut(&mut self, field: PlainField) -> &mut String {
        match field {
            PlainField::BtcSendRecipient => &mut self.btc_send_recipient,
            PlainField::BtcSendAmount => &mut self.btc_send_amount,
            PlainField::BtcSendFiat => &mut self.btc_send_fiat_amount,
            PlainField::BtcSendFee => &mut self.btc_send_fee,
            PlainField::BtcBumpFee => &mut self.btc_bump_fee,
            PlainField::XrpSendRecipient => &mut self.send_recipient,
            PlainField::XrpSendAmount => &mut self.send_amount,
            PlainField::XrpSendFiat => &mut self.send_fiat_amount,
            PlainField::XrpSendTag => &mut self.send_tag_draft,
            PlainField::XrpReceiveTag => &mut self.receive_tag_draft,
            PlainField::TradeAmount => &mut self.trade_amount,
            PlainField::TradeReceive => &mut self.trade_receive,
            PlainField::TradeLimit => &mut self.trade_limit_price,
            PlainField::TradePairQuery => &mut self.trade_pair_query,
        }
    }

    /// Apply an edit to a non-secret grid field, rejecting anything the field
    /// does not accept.
    ///
    /// The filter lives here rather than in the widget because it is a property
    /// of the *value*, not of the way it is drawn: an amount takes digits and
    /// one decimal point, a fee takes whole satoshis, an address takes what a
    /// base58/bech32 address is made of. A rejected keystroke is dropped
    /// silently — the field simply does not change, which is what every text
    /// input everywhere does with a character it cannot hold.
    ///
    /// Returns whether the buffer actually changed, so the caller can skip the
    /// per-field follow-up work (the fiat cross, mainly) on a no-op.
    pub fn apply_plain_edit(&mut self, field: PlainField, op: SecureOp) -> bool {
        let accepts = |s: &str, at: usize, add: &str| -> bool {
            match field {
                // A whole number of satoshis. No sign, no point — the fee is an
                // absolute amount and a fractional satoshi is not a thing.
                // A destination tag is a u32: digits only, likewise.
                PlainField::BtcSendFee
                | PlainField::BtcBumpFee
                | PlainField::XrpSendTag
                | PlainField::XrpReceiveTag => {
                    add.chars().all(|c| c.is_ascii_digit())
                }
                // Digits and at most one point, counting what is already there.
                PlainField::BtcSendAmount
                | PlainField::BtcSendFiat
                | PlainField::XrpSendAmount
                | PlainField::XrpSendFiat
                | PlainField::TradeAmount
                | PlainField::TradeReceive
                | PlainField::TradeLimit => {
                    let dots = s.chars().filter(|c| *c == '.').count()
                        + add.chars().filter(|c| *c == '.').count();
                    dots <= 1 && add.chars().all(|c| c.is_ascii_digit() || c == '.')
                }
                // Addresses are alphanumeric (base58 and bech32 both, and an
                // X-address too); a pasted one often drags whitespace, and
                // `1` and `bc1` are both covered by this.
                PlainField::BtcSendRecipient | PlainField::XrpSendRecipient => {
                    let _ = at;
                    add.chars().all(|c| c.is_ascii_alphanumeric())
                }
                // A symbol, an issuer name, or pair syntax (`xrp/rl`, `xrp rl`).
                PlainField::TradePairQuery => {
                    add.chars().all(|c| c.is_ascii_alphanumeric() || c == '/' || c == ' ')
                }
            }
        };

        let buf = self.plain_field_mut(field);
        match op {
            SecureOp::Insert(i, c) => {
                let mut b = [0u8; 4];
                if !accepts(buf, i, c.encode_utf8(&mut b)) {
                    return false;
                }
                let at = byte_index(buf, i);
                buf.insert(at, c);
                true
            }
            SecureOp::Remove(i) => {
                if i >= buf.chars().count() {
                    return false;
                }
                let at = byte_index(buf, i);
                buf.remove(at);
                true
            }
            SecureOp::Paste(i, text) => {
                // A paste is filtered rather than rejected: clipboard text
                // arrives with newlines and spaces around an otherwise perfectly
                // good address, and dropping the whole paste over a trailing
                // `\n` would be maddening.
                let clean: String = text
                    .chars()
                    .filter(|c| {
                        let mut b = [0u8; 4];
                        accepts(buf, i, c.encode_utf8(&mut b))
                    })
                    .collect();
                if clean.is_empty() {
                    return false;
                }
                let at = byte_index(buf, i);
                buf.insert_str(at, &clean);
                true
            }
            // Nothing here is masked, so there is nothing to reveal.
            SecureOp::ToggleReveal => false,
        }
    }

    /// Replace a plain field outright — what the `paste ›` link does.
    pub fn replace_plain_field(&mut self, field: PlainField, text: String) {
        self.plain_field_mut(field).clear();
        self.apply_plain_edit(field, SecureOp::Paste(0, text));
    }

    /// Freeze the ENCRYPTION gauge at what `field` currently measures, called
    /// immediately before that secret is moved out to the bridge.
    pub fn hold_entropy(&mut self, field: SecureField) {
        let bits = {
            let (buf, _) = self.secure_field_mut(field);
            crate::utils::entropy::bits(buf.as_str().trim())
        };
        self.entropy_hold = Some(bits);
    }

    /// Release the gauge back to the live buffer. Called from the same places
    /// that clear a setup form — opening one, and finishing one.
    pub fn release_entropy(&mut self) {
        self.entropy_hold = None;
    }

    /// Replace a secure field wholesale — what a paste *button* means, as
    /// against [`SecureOp::Paste`], which inserts at the caret. Clicking
    /// `paste ›` on a recovery phrase means "this is the phrase", not "add
    /// these words to the ones already in the box".
    ///
    /// A phrase is [`normalise_phrase`]d rather than trimmed; everything else
    /// keeps the trim it always had.
    /// How many characters one secure field holds. Takes `&mut self` so it can
    /// go through [`Self::secure_field_mut`] — the single place the field set
    /// is enumerated — rather than standing up a second copy of that match.
    pub fn secure_field_len(&mut self, field: SecureField) -> usize {
        self.secure_field_mut(field).0.char_len()
    }

    /// Set one secure field's reveal flag.
    ///
    /// Deliberately separate from [`Self::replace_secure_field`], which leaves
    /// the flag where it found it: replacing a field's contents is not a
    /// statement about whether they should be on screen, but clearing one on
    /// purpose is.
    pub fn set_secure_reveal(&mut self, field: SecureField, revealed: bool) {
        let (_, reveal) = self.secure_field_mut(field);
        *reveal = revealed;
    }

    pub fn replace_secure_field(&mut self, field: SecureField, mut text: String) {
        let cap = field.char_cap();
        let (buf, _) = self.secure_field_mut(field);
        buf.clear();
        if field.is_phrase() {
            let mut clean = normalise_phrase(&text);
            buf.insert_str(0, &clean);
            clean.zeroize();
        } else if let Some(cap) = cap {
            let mut cut: String = text.trim().chars().take(cap).collect();
            buf.insert_str(0, &cut);
            cut.zeroize();
        } else {
            buf.insert_str(0, text.trim());
        }
        text.zeroize();
    }
}

/// Rebuild a pasted recovery phrase from its words.
///
/// Whitespace in a mnemonic is a separator and never content, so a phrase
/// arriving from the clipboard is reassembled rather than trimmed. `.trim()`
/// alone only cleaned the ends: a drag-selection that picked up a leading space
/// — or a line break sitting between two words — left real separator slots in
/// the buffer, and the grid had to draw them. The field rendered with an
/// indent, and revealing it dropped the row that indent began.
///
/// Collapsing every run — spaces, tabs, newlines, leading, trailing and
/// interior alike — is what makes a messy copy land as the same 24 words a
/// clean one does. Only [`SecureField::is_phrase`] buffers get this: in a key
/// or a 25th word the spaces ARE the secret, and collapsing one would seal a
/// wallet under something the user cannot type back.
fn normalise_phrase(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

impl Default for AppState {
    fn default() -> Self {
        let (theme, hide_balance, base_currency) = load_settings();
        let gate_enabled = crate::gate::enabled();
        let mut app = Self {
            selected_tab: Tab::default(),
            base_currency,
            xrp_view: XrpView::default(),
            btc_view: BtcView::default(),
            btc_seed_text: SecureString::input(),
            btc_seed_reveal: false,
            btc_encryption_input: SecureString::input(),
            btc_encryption_reveal: false,
            btc_import_mode: ImportMode::default(),
            btc_bip39_input: SecureString::input(),
            btc_bip39_reveal: false,
            btc_word25: false,
            btc_script_type: Default::default(),
            xrp_token_tab: XrpTokenTab::default(),
            send_asset_picker_open: false,
            enable_passphrase: SecureString::input(),
            enable_passphrase_reveal: false,
            enable_bip39: SecureString::input(),
            enable_bip39_reveal: false,
            enable_seed_text: SecureString::input(),
            enable_seed_reveal: false,
            enable_error: None,
            show_enable: false,
            disable_token: None,
            disable_passphrase: SecureString::input(),
            disable_passphrase_reveal: false,
            disable_bip39: SecureString::input(),
            disable_bip39_reveal: false,
            disable_seed_text: SecureString::input(),
            disable_seed_reveal: false,
            disable_error: None,
            key_mgmt_restoring: false,
            btc_key_mgmt_restoring: false,
            reimport_step: 0,
            reimport_word25: false,
            entropy_hold: None,
            key_purge_requested: false,
            btc_key_purge_requested: false,
            btc_tx_selected: None,
            remove_wallet_requested: false,
            btc_remove_wallet_requested: false,
            xrp_seed_text: SecureString::input(),
            xrp_seed_reveal: false,
            xrp_encryption_input: SecureString::input(),
            xrp_encryption_reveal: false,
            xrp_import_mode: ImportMode::default(),
            xrp_bip39_input: SecureString::input(),
            xrp_bip39_reveal: false,
            xrp_word25: false,
            send_step: 0,
            send_recipient: String::new(),
            send_destination_tag: String::new(),
            send_tag_editing: false,
            send_tag_draft: String::new(),
            receive_tag: String::new(),
            receive_tag_editing: false,
            receive_tag_draft: String::new(),
            send_amount: String::new(),
            send_fiat_amount: String::new(),
            send_anchor: SendAnchor::Amount,
            send_passphrase: SecureString::input(),
            send_passphrase_reveal: false,
            send_bip39: SecureString::input(),
            send_bip39_reveal: false,
            send_seed_text: SecureString::input(),
            send_seed_reveal: false,
            send_error: None,
            create_mnemonic: SecureString::input(),
            create_seed_reveal: false,
            tx_expanded: None,
            activity_log: None,
            activity_tick: std::time::Instant::now(),
            theme,
            hide_balance,
            xrp_copy_feedback: false,
            btc_copy_feedback: false,
            btc_master_copy_feedback: false,
            xrp_wallet_copy_feedback: false,
            btc_receive_address: None,
            btc_receive_pool: Vec::new(),
            btc_pool_face: false,
            btc_pool_refusal: None,
            btc_pool_copied: None,
            settings_open: false,
            btc_send_step: 0,
            btc_send_recipient: String::new(),
            btc_send_amount: String::new(),
            btc_send_fiat_amount: String::new(),
            btc_send_anchor: SendAnchor::Amount,
            btc_send_fee: String::new(),
            btc_send_fee_tier: BtcFeeTier::default(),
            btc_send_error: None,
            btc_send_passphrase: SecureString::input(),
            btc_send_passphrase_reveal: false,
            btc_bump_txid: None,
            btc_bump_fee_tier: BtcFeeTier::default(),
            btc_bump_fee: String::new(),
            btc_bump_passphrase: SecureString::input(),
            btc_bump_passphrase_reveal: false,
            btc_bump_bip39: SecureString::input(),
            btc_bump_bip39_reveal: false,
            btc_bump_seed_text: SecureString::input(),
            btc_bump_seed_reveal: false,
            btc_send_bip39: SecureString::input(),
            btc_send_bip39_reveal: false,
            btc_send_seed_text: SecureString::input(),
            btc_send_seed_reveal: false,
            trade_step: 0,
            trade_pay_asset: String::new(),
            trade_receive_asset: String::new(),
            trade_amount: String::new(),
            trade_limit_price: String::new(),
            trade_flags: Vec::new(),
            // Limit · Buy is what the ticket opens on (user-decided
            // 2026-09-16): a wallet buys its first token before it sells
            // one, and a limit order is the one contract that cannot cost
            // more than the number on the screen. Under Buy the base amount
            // is the engine's RECEIVE side, so the anchor sits there.
            trade_contract: Some(TradeContract::Limit),
            trade_side: TradeSide::Buy,
            trade_side_chosen: false,
            trade_pair_search_open: false,
            trade_anchor: TradeAnchor::Receive,
            trade_receive: String::new(),
            trade_pair_query: String::new(),
            trade_error: None,
            trade_books: Vec::new(),
            trade_passphrase: SecureString::input(),
            trade_passphrase_reveal: false,
            trade_bip39: SecureString::input(),
            trade_bip39_reveal: false,
            trade_seed_text: SecureString::input(),
            trade_seed_reveal: false,
            cancel_offer_sequence: None,
            cancel_passphrase: SecureString::input(),
            cancel_passphrase_reveal: false,
            cancel_bip39: SecureString::input(),
            cancel_bip39_reveal: false,
            cancel_seed_text: SecureString::input(),
            cancel_seed_reveal: false,
            cancel_error: None,
            reimport_seed_text: SecureString::input(),
            reimport_seed_reveal: false,
            reimport_passphrase: SecureString::input(),
            reimport_passphrase_reveal: false,
            reimport_bip39: SecureString::input(),
            reimport_bip39_reveal: false,
            reimport_error: None,
            reimport_spinning: false,
            reimport_spinner_angle: 0.0,
            btc_reimport_seed_text: SecureString::input(),
            btc_reimport_seed_reveal: false,
            btc_reimport_passphrase: SecureString::input(),
            btc_reimport_passphrase_reveal: false,
            btc_reimport_bip39: SecureString::input(),
            btc_reimport_bip39_reveal: false,
            btc_reimport_error: None,
            btc_reimport_spinning: false,
            btc_reimport_spinner_angle: 0.0,
            window_width: 900.0,
            // Both dimensions default to their scale-reference values so the
            // first frame (before the initial Resized event) renders at 1.0.
            window_height: 800.0,
            // The layout as it was last left; the default the first time,
            // or when the saved one does not parse.
            xrp_grid: crate::controller::panes::Grid::load(crate::controller::panes::Chain::Xrp, crate::controller::panes::XRP_KEY).unwrap_or_default(),
            btc_grid: crate::controller::panes::Grid::load(crate::controller::panes::Chain::Btc, crate::controller::panes::BTC_KEY)
                .unwrap_or_else(|| crate::controller::panes::Grid::new(crate::controller::panes::Chain::Btc)),
            btc_utxo_fingerprint: 0,
            ledger_tape: VecDeque::new(),
            pin_wallet_identities: load_pin_wallet_identities(),
            // Gate disabled (settings toggle) → skip straight in, even if no
            // gate file exists (a full "forget" leaves enabled=false with no
            // gate). Enabled → locked: enter if registered, first-run set-up if
            // not (fresh installs default to enabled).
            gate_unlocked: !gate_enabled,
            gate_exists: crate::gate::exists(),
            gate_enabled,
            gate_pin_draft: None,
            gate_pin_input: String::new(),
            gate_error: None,
            gate_error_at: None,
            gate_wiping: false,
            rate_history_short: HashMap::new(),
            rate_history_long: HashMap::new(),
        };
        // The last pair picked on the XRP grid comes back with the layout
        // (user, 2026-09-11): reopening the app lands on the market it was
        // closed on. A Redis-free, node-free read — settings.json only.
        crate::controller::xrp::restore_trade_pair(&mut app);
        crate::controller::xrp::restore_receive_tag(&mut app);
        app
    }
}

fn load_pin_wallet_identities() -> Vec<(String, String)> {
    use crate::bridge::json_storage;
    use serde_json::Value;

    let truncate = |addr: &str| -> String {
        if addr.len() <= 8 { return addr.to_string(); }
        let tail = addr.char_indices().rev().nth(3).map(|(i, _)| &addr[i..]).unwrap_or(&addr[addr.len()-4..]);
        format!("{}…{}", &addr[..4], tail)
    };

    let mut ids = Vec::new();
    if let Ok(json) = json_storage::read_json::<Value>("xrp.json") {
        if let Some(addr) = json.get("address").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            ids.push(("XRP".to_string(), truncate(addr)));
        }
    }
    if let Ok(json) = json_storage::read_json::<Value>("btc.json") {
        // v2: record 0 of `addresses` is #0, the identity. v1 fallback: the
        // single `address` field (pre-migration file, first run).
        let addr = json
            .get("addresses")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("address"))
            .and_then(|v| v.as_str())
            .or_else(|| json.get("address").and_then(|v| v.as_str()));
        if let Some(addr) = addr.filter(|s| !s.is_empty()) {
            ids.push(("BTC".to_string(), truncate(addr)));
        }
    }
    ids
}

fn load_settings() -> (iced::Theme, bool, BaseCcy) {
    use crate::bridge::json_storage;
    use serde_json::Value;

    if let Ok(json) = json_storage::read_json::<Value>("settings.json") {
        let ct = json.get("theme")
            .and_then(|v| serde_json::from_value::<Theme>(v.clone()).ok())
            .unwrap_or(Theme::Dark);
        let hide = json.get("is_hidden").and_then(|v| v.as_bool()).unwrap_or(false);
        // A `hero_unit` left by an older build (the dashboards' `BTC | USD`
        // pill, gone with the pane grid) is simply not read.
        // `base_currency` is the whole model now; a `local_currency` left by an
        // older build (the appended 4th tab) is simply not read.
        let base = json.get("base_currency")
            .and_then(|v| v.as_str())
            .map(BaseCcy::from_code)
            .unwrap_or_default();
        (ct.build(), hide, base)
    } else {
        let defaults = serde_json::json!({ "theme": Theme::Dark, "is_hidden": false, "base_currency": "USD" });
        let _ = json_storage::write_json("settings.json", &defaults);
        (crate::utils::theme::dark_theme(), false, BaseCcy::default())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A 25th word stops at [`WORD25_MAX`] whichever way it arrives — typed,
    /// pasted at the caret, or pasted over the field — and an unbounded field
    /// (the same word at signing time) does not.
    #[test]
    fn a_25th_word_stops_at_its_cap() {
        use crate::controller::message::WORD25_MAX;
        let mut state = AppState::default();
        let long: String = "x".repeat(WORD25_MAX + 20);

        state.replace_secure_field(SecureField::XrpBip39, long.clone());
        assert_eq!(state.xrp_bip39_input.char_len(), WORD25_MAX);
        state.apply_secure_edit(SecureField::XrpBip39, SecureOp::Insert(WORD25_MAX, 'y'));
        assert_eq!(state.xrp_bip39_input.char_len(), WORD25_MAX, "a keystroke got past the cap");
        state.apply_secure_edit(SecureField::XrpBip39, SecureOp::Remove(0));
        state.apply_secure_edit(SecureField::XrpBip39, SecureOp::Paste(0, "abc".to_string()));
        assert_eq!(state.xrp_bip39_input.char_len(), WORD25_MAX, "a paste was not cut to fit");

        state.replace_secure_field(SecureField::SendBip39, long.clone());
        assert_eq!(state.send_bip39.char_len(), WORD25_MAX + 20, "the signing field must stay unbounded");

        // The storage key, both chains: same ceiling, same three paths.
        use crate::controller::message::KEY_MAX;
        let key_fields: [(SecureField, fn(&AppState) -> usize); 2] = [
            (SecureField::XrpEncryption, |s| s.xrp_encryption_input.char_len()),
            (SecureField::BtcEncryption, |s| s.btc_encryption_input.char_len()),
        ];
        for (field, len) in key_fields {
            state.replace_secure_field(field, long.clone());
            assert_eq!(len(&state), KEY_MAX, "{field:?} overfilled on replace");
            state.apply_secure_edit(field, SecureOp::Insert(KEY_MAX, 'y'));
            assert_eq!(len(&state), KEY_MAX, "{field:?} let a keystroke past the cap");
            state.apply_secure_edit(field, SecureOp::Remove(0));
            state.apply_secure_edit(field, SecureOp::Paste(0, "abc".to_string()));
            assert_eq!(len(&state), KEY_MAX, "{field:?} did not cut a paste to fit");
        }
        state.replace_secure_field(SecureField::SendPassphrase, long.clone());
        assert_eq!(state.send_passphrase.char_len(), WORD25_MAX + 20, "the signing key must stay unbounded");
    }

    /// The whole point: a clean copy and a messy one have to produce the same
    /// buffer, because the grid draws whatever separator slots survive.
    #[test]
    fn a_messy_phrase_lands_exactly_as_a_clean_one() {
        let clean = "abandon abandon ability able about above absent absorb \
                     abstract absurd abuse access accident account accuse \
                     achieve acid acoustic acquire across act action actor \
                     actress";
        for messy in [
            format!(" {clean}"),
            format!("{clean} "),
            format!("\n\t{clean}\r\n"),
            clean.replace("ability able", "ability  able"),
            clean.replace("absorb abstract", "absorb\nabstract"),
        ] {
            assert_eq!(normalise_phrase(&messy), clean, "{messy:?} did not normalise");
        }
        // A clean phrase is left exactly as it is.
        assert_eq!(normalise_phrase(clean), clean);
    }

    /// A phrase gets rebuilt; nothing else does. A key's spaces are the secret,
    /// and a key that normalised on paste but not on typing would seal a file
    /// under something its owner cannot re-enter.
    #[test]
    fn only_phrase_fields_are_normalised() {
        for f in [
            SecureField::XrpImportSeed,
            SecureField::BtcImportSeed,
            SecureField::ReimportSeed,
            SecureField::BtcReimportSeed,
            SecureField::CreateSeed,
            SecureField::SendSeed,
        ] {
            assert!(f.is_phrase(), "{f:?} holds a phrase");
        }
        for f in [
            SecureField::XrpEncryption,
            SecureField::BtcEncryption,
            SecureField::ReimportPassphrase,
            SecureField::ReimportBip39,
            SecureField::XrpBip39,
        ] {
            assert!(!f.is_phrase(), "{f:?} is not a phrase — its spaces are content");
        }
    }
}
