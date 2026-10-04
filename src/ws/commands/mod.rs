use crate::channel::WSCommand;
use tokio_tungstenite::tungstenite::Message;

pub mod balances;
pub mod bitcoin_auth;
pub mod bitcoin_bump_transaction;
pub mod bitcoin_delete_wallet;
pub mod bitcoin_import_wallet;
pub mod bitcoin_payment;
pub mod bitcoin_submit_transaction;
pub mod bitcoin_subscribe;
pub mod bitcoin_transaction_sender;
pub mod bitcoin_validation;
pub mod create_wallet;
pub mod delete_wallet;
pub mod get_bitcoin_balance;
pub mod get_bitcoin_history;
pub mod get_btc_transaction;
pub mod get_btc_utxos;
pub mod get_history;
pub mod get_transaction;
pub mod getbitcoincachedbalance;
pub mod getcachedbalance;
pub mod import_wallet;
pub mod offer_cancel;
pub mod offer_create;
pub mod payment;
pub mod submit_transaction;
pub mod transaction_builder;
pub mod transaction_sender;
pub mod trustline;
pub mod trustset;
pub mod validation;
pub mod wallet_auth;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    ImportWallet,
    CreateWallet,
    DeleteWallet,
    SubmitTransaction,
    GetBalance,
    GetTransaction,
    /// One page of history — `load 20 more ›` (get_history.rs).
    GetHistory,
    GetXRPBalance,
    /// Live balance for any registry token (which one is read from the command).
    GetTokenBalance,
    ImportBitcoinWallet,
    /// The wallet's live list, sent whole (bitcoin_subscribe.rs). Outgoing
    /// only — the relay does not reply to it.
    SubscribeBitcoinAddresses,
    GetBitcoinBalance,
    DeleteBitcoinWallet,
    SubmitBitcoinTransaction,
    /// Fee bump: sign and send the replacement, rebuilt from the row's own
    /// record. Its reply is the ordinary `submit_bitcoin_transaction_response`.
    BumpBitcoinTransaction,
    GetBTCBalance,
    /// The relay pushing the wallet's whole UTXO set (`btc_utxos`).
    GetBtcUtxos,
    GetBitcoinTransaction,
    /// One page of BTC history — the twin of `GetHistory` (get_bitcoin_history.rs).
    GetBitcoinHistory,
    /// Trustline limit for any registry token (which one is read from the command).
    GetTokenTrustline,
}

impl Command {
    pub fn from_str(command_name: &str) -> Option<Self> {
        // Registry tokens are recognized by their per-token command strings, so
        // adding a token needs no new arm here.
        if crate::utils::tokens::by_balance_cmd(command_name).is_some() {
            return Some(Command::GetTokenBalance);
        }
        if crate::utils::tokens::by_trustline_cmd(command_name).is_some() {
            return Some(Command::GetTokenTrustline);
        }
        match command_name {
            "import_wallet" => Some(Command::ImportWallet),
            "create_wallet" => Some(Command::CreateWallet),
            "delete_wallet" => Some(Command::DeleteWallet),
            "submit_transaction" | "submit_transaction_response" => {
                Some(Command::SubmitTransaction)
            }
            "get_balance" | "get_cached_balance" => Some(Command::GetBalance),
            "get_transaction" => Some(Command::GetTransaction),
            "get_history" => Some(Command::GetHistory),
            "xrp_balance" => Some(Command::GetXRPBalance),
            "import_bitcoin_wallet" => Some(Command::ImportBitcoinWallet),
            "subscribe_bitcoin_addresses" => Some(Command::SubscribeBitcoinAddresses),
            "get_bitcoin_cached_balance" => Some(Command::GetBitcoinBalance),
            "delete_bitcoin_wallet" => Some(Command::DeleteBitcoinWallet),
            "bitcoin_submit_transaction"
            | "submit_bitcoin_transaction"
            | "submit_bitcoin_transaction_response" => Some(Command::SubmitBitcoinTransaction),
            "bitcoin_bump_transaction" => Some(Command::BumpBitcoinTransaction),
            "btc_balance" => Some(Command::GetBTCBalance),
            "btc_utxos" => Some(Command::GetBtcUtxos),
            "get_bitcoin_transaction" => Some(Command::GetBitcoinTransaction),
            "get_bitcoin_history" => Some(Command::GetBitcoinHistory),
            _ => None,
        }
    }

    pub async fn execute(
        &self,
        current_wallet: String,
        bitcoin_current_wallet: String,
        cmd: WSCommand,
    ) -> Result<(), String> {
        match self {
            Command::ImportWallet => import_wallet::execute(current_wallet, cmd).await,
            Command::CreateWallet => create_wallet::execute(current_wallet, cmd).await,
            Command::DeleteWallet => delete_wallet::execute(current_wallet, cmd).await,
            Command::SubmitTransaction => {
                submit_transaction::execute(current_wallet, cmd).await
            }
            Command::GetBalance => getcachedbalance::execute(current_wallet, cmd).await,
            Command::GetTokenTrustline => {
                trustline::execute(current_wallet, cmd).await
            }
            Command::GetTransaction => {
                get_transaction::execute(current_wallet, cmd).await
            }
            Command::GetHistory => get_history::execute(current_wallet, cmd).await,

            Command::GetTokenBalance
            | Command::GetXRPBalance => balances::execute(current_wallet, cmd).await,

            Command::ImportBitcoinWallet => {
                bitcoin_import_wallet::execute(bitcoin_current_wallet, cmd).await
            }
            Command::SubscribeBitcoinAddresses => {
                bitcoin_subscribe::execute(bitcoin_current_wallet, cmd).await
            }
            Command::GetBitcoinBalance => {
                getbitcoincachedbalance::execute(bitcoin_current_wallet, cmd).await
            }
            Command::DeleteBitcoinWallet => {
                bitcoin_delete_wallet::execute(bitcoin_current_wallet, cmd).await
            }
            Command::SubmitBitcoinTransaction => {
                bitcoin_submit_transaction::execute(bitcoin_current_wallet, cmd).await
            }
            Command::BumpBitcoinTransaction => {
                bitcoin_bump_transaction::execute(bitcoin_current_wallet, cmd).await
            }
            Command::GetBTCBalance => {
                get_bitcoin_balance::execute(bitcoin_current_wallet, cmd).await
            }
            Command::GetBtcUtxos => {
                get_btc_utxos::execute(bitcoin_current_wallet, cmd).await
            }
            Command::GetBitcoinTransaction => {
                get_btc_transaction::execute(bitcoin_current_wallet, cmd).await
            }
            Command::GetBitcoinHistory => {
                get_bitcoin_history::execute(bitcoin_current_wallet, cmd).await
            }
        }
    }

    pub async fn process_response(
        &self,
        message: Message,
        current_wallet: &str,
        bitcoin_current_wallet: &str,
    ) -> Result<(), String> {
        match self {
            Command::ImportWallet => import_wallet::process_response(message, current_wallet).await,
            Command::CreateWallet => create_wallet::process_response(message, current_wallet).await,
            Command::DeleteWallet => delete_wallet::process_response(message, current_wallet).await,
            Command::SubmitTransaction => {
                submit_transaction::process_response(message, current_wallet).await
            }
            Command::GetBalance => {
                getcachedbalance::process_response(message, current_wallet).await
            }
            Command::GetTokenTrustline => {
                trustline::process_response(message, current_wallet).await
            }
            Command::GetTransaction => {
                get_transaction::process_response(message, current_wallet).await
            }
            Command::GetHistory => get_history::process_response(message, current_wallet).await,

            Command::GetTokenBalance
            | Command::GetXRPBalance => balances::process_response(message, current_wallet).await,

            Command::ImportBitcoinWallet => {
                bitcoin_import_wallet::process_response(message, bitcoin_current_wallet).await
            }
            // Outgoing only: the relay never answers under this name.
            Command::SubscribeBitcoinAddresses => Ok(()),
            Command::GetBitcoinBalance => {
                getbitcoincachedbalance::process_response(message, bitcoin_current_wallet).await
            }
            Command::DeleteBitcoinWallet => {
                bitcoin_delete_wallet::process_response(message, bitcoin_current_wallet).await
            }
            Command::SubmitBitcoinTransaction => {
                bitcoin_submit_transaction::process_response(message, bitcoin_current_wallet).await
            }
            // Its reply is `submit_bitcoin_transaction_response`, routed above.
            Command::BumpBitcoinTransaction => Ok(()),
            Command::GetBTCBalance => {
                get_bitcoin_balance::process_response(message, bitcoin_current_wallet).await
            }
            Command::GetBtcUtxos => {
                get_btc_utxos::process_response(message, bitcoin_current_wallet).await
            }
            Command::GetBitcoinTransaction => {
                get_btc_transaction::process_response(message, bitcoin_current_wallet).await
            }
            Command::GetBitcoinHistory => {
                get_bitcoin_history::process_response(message, bitcoin_current_wallet).await
            }
        }
    }
}
