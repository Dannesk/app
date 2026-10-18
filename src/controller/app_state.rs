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


fn byte_index(s: &str, i: usize) -> usize {
    s.char_indices().nth(i).map_or(s.len(), |(b, _)| b)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum XrpView {
    #[default]
    Menu,
    Import,
    Create,

}


#[derive(Debug, Clone, PartialEq, Default)]
pub enum XrpTokenTab {
    #[default]
    Xrp,
    Token(&'static str),
}

impl XrpTokenTab {
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


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BtcFeeTier {
    
    Minimum,
    Low,
    #[default]
    Med,
    High,
    Custom,
}

impl BtcFeeTier {
    pub const ALL: [BtcFeeTier; 5] = [
        BtcFeeTier::Minimum,
        BtcFeeTier::Low,
        BtcFeeTier::Med,
        BtcFeeTier::High,
        BtcFeeTier::Custom,
    ];

    
    pub fn label(self) -> &'static str {
        match self {
            BtcFeeTier::Minimum => "min",
            BtcFeeTier::Low => "low",
            BtcFeeTier::Med => "med",
            BtcFeeTier::High => "high",
            BtcFeeTier::Custom => "custom",
        }
    }

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


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BaseCcy {
    #[default]
    Usd,
    Eur,
    Sgd,
    Aud,
}

impl BaseCcy {
   
    pub const ALL: [BaseCcy; 4] = [BaseCcy::Usd, BaseCcy::Eur, BaseCcy::Sgd, BaseCcy::Aud];

    pub fn code(self) -> &'static str {
        match self {
            BaseCcy::Usd => "USD",
            BaseCcy::Eur => "EUR",
            BaseCcy::Sgd => "SGD",
            BaseCcy::Aud => "AUD",
        }
    }

   
    pub fn symbol(self) -> &'static str {
        match self {
            BaseCcy::Usd => "$",
            BaseCcy::Eur => "\u{20ac}",
            BaseCcy::Sgd => "S$",
            BaseCcy::Aud => "A$",
        }
    }

   
    pub fn from_code(s: &str) -> Self {
        match s {
            "EUR" => BaseCcy::Eur,
            "SGD" => BaseCcy::Sgd,
            "AUD" => BaseCcy::Aud,
            _ => BaseCcy::Usd,
        }
    }

}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeAnchor {
    Pay,
    Receive,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SendAnchor {
    #[default]
    Amount,
    Fiat,
}


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
  
    pub fn detect(key_deleted: bool, _key_mode: crate::channel::KeyMode) -> Self {
        if key_deleted {
            EnableInputMode::Seed
        } else {
            EnableInputMode::Passphrase
        }
    }
}

pub use crate::wallet::ImportMode;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Paper,
    #[serde(alias = "Light")]
    Chalk,
    #[serde(alias = "Dark")]
    Obsidian,
    Graphite,
}

impl Theme {
   
    pub const SELECTABLE: &'static [(Theme, &'static str)] =
        &[(Theme::Paper, "paper"), (Theme::Chalk, "chalk"), (Theme::Graphite, "graphite"), (Theme::Obsidian, "obsidian")];

    pub fn build(self) -> iced::Theme {
        match self {
            Theme::Paper => crate::utils::theme::paper_theme(),
            Theme::Chalk => crate::utils::theme::chalk_theme(),
            Theme::Obsidian => crate::utils::theme::obsidian_theme(),
            Theme::Graphite => crate::utils::theme::graphite_theme(),
        }
    }

  
    pub fn of(theme: &iced::Theme) -> Theme {
        if theme.to_string().contains("Graphite") {
            Theme::Graphite
        } else if crate::utils::theme::is_dark(theme) {
            Theme::Obsidian
        } else if theme.to_string().contains("Chalk") {
            Theme::Chalk
        } else {
            Theme::Paper
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    pub selected_tab: Tab,
  
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
    pub btc_word25: bool,
  
    pub btc_script_type: crate::btc_script_type::BtcScriptType,
    pub xrp_token_tab: XrpTokenTab,
    pub send_asset_picker_open: bool,
    pub enable_passphrase: SecureString,
    pub enable_passphrase_reveal: bool,
    pub enable_bip39: SecureString,
    pub enable_bip39_reveal: bool,
    pub enable_seed_text: SecureString,
    pub enable_seed_reveal: bool,
    pub enable_error: Option<String>,
    
    pub show_enable: bool,

    pub disable_token: Option<&'static str>,
    pub disable_passphrase: SecureString,
    pub disable_passphrase_reveal: bool,
    pub disable_bip39: SecureString,
    pub disable_bip39_reveal: bool,
    pub disable_seed_text: SecureString,
    pub disable_seed_reveal: bool,
    pub disable_error: Option<String>,

    pub key_mgmt_restoring: bool,
    pub btc_key_mgmt_restoring: bool,
  
    pub reimport_step: u8,
 
    pub reimport_word25: bool,
 
    pub entropy_hold: Option<f64>,
 
    pub key_purge_requested: bool,
    pub btc_key_purge_requested: bool,

    pub btc_tx_selected: Option<String>,
    pub btc_txid_copied: Option<String>,

    pub remove_wallet_requested: bool,
    pub btc_remove_wallet_requested: bool,
    pub xrp_seed_text: SecureString,
    pub xrp_seed_reveal: bool,
    pub xrp_encryption_input: SecureString,
    pub xrp_encryption_reveal: bool,
    pub xrp_import_mode: ImportMode,
    pub xrp_bip39_input: SecureString,
    pub xrp_bip39_reveal: bool,
    pub xrp_word25: bool,
    pub send_step: u8,
    pub send_recipient: String,
 
    pub send_destination_tag: String,
    pub send_tag_editing: bool,
    pub send_tag_draft: String,
    pub receive_tag: String,
  
    pub receive_tag_editing: bool,
    pub receive_tag_draft: String,
 
    pub xrp_tag_copied: bool,
 
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
    pub tx_hash_copied: Option<String>,
    pub activity_log: Option<ActivityLogState>,
    pub activity_tick: std::time::Instant,
    pub theme: iced::Theme,
    pub hide_balance: bool,
    pub btc_balance_sats: bool,
    pub xrp_copy_feedback: bool,
    pub btc_copy_feedback: bool,
  
    pub btc_master_copy_feedback: bool,
    pub xrp_wallet_copy_feedback: bool,

    pub btc_receive_address: Option<String>,
  
    pub btc_receive_pool: Vec<String>,
   
    pub btc_drop_confirm: bool,
  
    pub btc_pool_refusal: Option<crate::bridge::btc_receive_rotation::PoolRefusal>,
  
    pub settings_open: bool,
    pub prefs_erase_confirm: bool,
    pub btc_send_step: u8,
    pub btc_send_recipient: String,
    pub btc_send_amount: String,
    pub btc_send_fiat_amount: String,
    pub btc_send_anchor: SendAnchor,
  
    pub btc_send_fee: String,
    pub btc_send_fee_tier: BtcFeeTier,
    pub btc_send_error: Option<String>,
    pub btc_send_passphrase: SecureString,
    pub btc_send_passphrase_reveal: bool,
    pub btc_send_bip39: SecureString,
    pub btc_send_bip39_reveal: bool,
    pub btc_send_seed_text: SecureString,
    pub btc_send_seed_reveal: bool,
   
    pub btc_bump_txid: Option<String>,
    pub btc_bump_fee_tier: BtcFeeTier,
    pub btc_bump_fee: String,
    pub btc_bump_passphrase: SecureString,
    pub btc_bump_passphrase_reveal: bool,
    pub btc_bump_bip39: SecureString,
    pub btc_bump_bip39_reveal: bool,
    pub btc_bump_seed_text: SecureString,
    pub btc_bump_seed_reveal: bool,
    pub trade_step: u8,

    pub trade_pay_asset: String,
    pub trade_receive_asset: String,
    pub trade_amount: String,
    pub trade_limit_price: String,
    pub trade_flags: Vec<String>,
   
    pub trade_contract: Option<TradeContract>,
    
    pub trade_side: TradeSide,
   
    pub trade_side_chosen: bool,

    pub trade_pair_search_open: bool,
    pub trade_anchor: TradeAnchor,

    pub trade_receive: String,
    pub trade_pair_query: String,
    pub trade_error: Option<String>,
 
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
    pub btc_reimport_seed_text: SecureString,
    pub btc_reimport_seed_reveal: bool,
    pub btc_reimport_passphrase: SecureString,
    pub btc_reimport_passphrase_reveal: bool,
    pub btc_reimport_bip39: SecureString,
    pub btc_reimport_bip39_reveal: bool,
    pub btc_reimport_error: Option<String>,
    pub btc_reimport_spinning: bool,
    pub window_width: f32,
    pub window_height: f32,
    pub pin_wallet_identities: Vec<(String, String)>,
    pub gate_unlocked: bool,
    pub gate_exists: bool,
    pub gate_enabled: bool,
    pub gate_pin_draft: Option<String>,
    pub gate_pin_input: String,
    pub gate_error: Option<String>,
    pub gate_error_at: Option<std::time::Instant>,
    pub gate_wiping: bool,
    pub rate_history_short: HashMap<String, VecDeque<(u64, f32)>>,
    pub rate_history_long: HashMap<String, VecDeque<(u64, f32)>>,
    pub xrp_grid: crate::controller::panes::Grid,
    pub btc_grid: crate::controller::panes::Grid,
    pub btc_utxo_fingerprint: u64,
    pub ledger_tape: VecDeque<crate::controller::panes::LedgerBar>,
}

impl AppState {
    pub fn scale(&self) -> f32 {
        (self.window_width / 900.0)
            .clamp(1.0, 1.3)
            .min((self.window_height / 800.0).clamp(0.85, 1.3))
            * Self::SCALE_LIFT
    }

    const SCALE_LIFT: f32 = 1.10;

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

    pub fn apply_secure_edit(&mut self, field: SecureField, op: SecureOp) {
        let cap = field.char_cap();
        let (buf, reveal) = self.secure_field_mut(field);
        match op {
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

    pub fn apply_plain_edit(&mut self, field: PlainField, op: SecureOp) -> bool {
        let accepts = |s: &str, at: usize, add: &str| -> bool {
            match field {
                PlainField::BtcSendFee
                | PlainField::BtcBumpFee
                | PlainField::XrpSendTag
                | PlainField::XrpReceiveTag => {
                    add.chars().all(|c| c.is_ascii_digit())
                }
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
                PlainField::BtcSendRecipient | PlainField::XrpSendRecipient => {
                    let _ = at;
                    add.chars().all(|c| c.is_ascii_alphanumeric())
                }
                PlainField::TradePairQuery => {
                    add.chars().all(|c| c.is_ascii_alphanumeric() || c == '/' || c == ' ')
                }
            }
        };

        let cap = match field {
            PlainField::XrpReceiveTag => Some(10),
            _ => None,
        };

        let buf = self.plain_field_mut(field);
        match op {
            SecureOp::Insert(i, c) => {
                if cap.is_some_and(|n| buf.chars().count() >= n) {
                    return false;
                }
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
                let mut clean: String = text
                    .chars()
                    .filter(|c| {
                        let mut b = [0u8; 4];
                        accepts(buf, i, c.encode_utf8(&mut b))
                    })
                    .collect();
                if let Some(n) = cap {
                    let room = n.saturating_sub(buf.chars().count());
                    clean = clean.chars().take(room).collect();
                }
                if clean.is_empty() {
                    return false;
                }
                let at = byte_index(buf, i);
                buf.insert_str(at, &clean);
                true
            }
            SecureOp::ToggleReveal => false,
        }
    }

    pub fn replace_plain_field(&mut self, field: PlainField, text: String) {
        self.plain_field_mut(field).clear();
        self.apply_plain_edit(field, SecureOp::Paste(0, text));
    }

    pub fn hold_entropy(&mut self, field: SecureField) {
        let bits = {
            let (buf, _) = self.secure_field_mut(field);
            crate::utils::entropy::bits(buf.as_str().trim())
        };
        self.entropy_hold = Some(bits);
    }

    pub fn release_entropy(&mut self) {
        self.entropy_hold = None;
    }

    pub fn reset_settings(&mut self) {
        let (theme, hide_balance, base_currency, btc_balance_sats) = load_settings();
        self.theme = theme;
        self.hide_balance = hide_balance;
        self.base_currency = base_currency;
        self.btc_balance_sats = btc_balance_sats;
        self.gate_enabled = crate::gate::enabled();
        self.xrp_grid = crate::controller::panes::Grid::load(crate::controller::panes::Chain::Xrp, crate::controller::panes::XRP_KEY).unwrap_or_default();
        self.btc_grid = crate::controller::panes::Grid::load(crate::controller::panes::Chain::Btc, crate::controller::panes::BTC_KEY)
            .unwrap_or_else(|| crate::controller::panes::Grid::new(crate::controller::panes::Chain::Btc));
        crate::controller::xrp::clear_trade_pair(self);
    }

    pub fn secure_field_len(&mut self, field: SecureField) -> usize {
        self.secure_field_mut(field).0.char_len()
    }

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
        let (theme, hide_balance, base_currency, btc_balance_sats) = load_settings();
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
            btc_txid_copied: None,
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
            xrp_tag_copied: false,
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
            tx_hash_copied: None,
            activity_log: None,
            activity_tick: std::time::Instant::now(),
            theme,
            hide_balance,
            btc_balance_sats,
            xrp_copy_feedback: false,
            btc_copy_feedback: false,
            btc_master_copy_feedback: false,
            xrp_wallet_copy_feedback: false,
            btc_receive_address: None,
            btc_receive_pool: Vec::new(),
            btc_drop_confirm: false,
            btc_pool_refusal: None,
            settings_open: false,
            prefs_erase_confirm: false,
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
            btc_reimport_seed_text: SecureString::input(),
            btc_reimport_seed_reveal: false,
            btc_reimport_passphrase: SecureString::input(),
            btc_reimport_passphrase_reveal: false,
            btc_reimport_bip39: SecureString::input(),
            btc_reimport_bip39_reveal: false,
            btc_reimport_error: None,
            btc_reimport_spinning: false,
            window_width: 900.0,
            window_height: 800.0,
            xrp_grid: crate::controller::panes::Grid::load(crate::controller::panes::Chain::Xrp, crate::controller::panes::XRP_KEY).unwrap_or_default(),
            btc_grid: crate::controller::panes::Grid::load(crate::controller::panes::Chain::Btc, crate::controller::panes::BTC_KEY)
                .unwrap_or_else(|| crate::controller::panes::Grid::new(crate::controller::panes::Chain::Btc)),
            btc_utxo_fingerprint: 0,
            ledger_tape: VecDeque::new(),
            pin_wallet_identities: load_pin_wallet_identities(),
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

pub fn default_settings() -> serde_json::Value {
    serde_json::json!({ "theme": Theme::Paper, "is_hidden": false, "base_currency": "USD", "btc_sats": false })
}

fn load_settings() -> (iced::Theme, bool, BaseCcy, bool) {
    use crate::bridge::json_storage;
    use serde_json::Value;

    if let Some(json) = json_storage::read_json::<Value>("settings.json").ok().filter(Value::is_object) {
        let ct = json.get("theme")
            .and_then(|v| serde_json::from_value::<Theme>(v.clone()).ok())
            .unwrap_or(Theme::Paper);
        let hide = json.get("is_hidden").and_then(|v| v.as_bool()).unwrap_or(false);
        let base = json.get("base_currency")
            .and_then(|v| v.as_str())
            .map(BaseCcy::from_code)
            .unwrap_or_default();
        let sats = json.get("btc_sats").and_then(|v| v.as_bool()).unwrap_or(false);
        (ct.build(), hide, base, sats)
    } else {
        let _ = json_storage::write_json("settings.json", &default_settings());
        (crate::utils::theme::paper_theme(), false, BaseCcy::default(), false)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(normalise_phrase(clean), clean);
    }

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
