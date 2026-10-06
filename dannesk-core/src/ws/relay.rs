//! The relay half of the socket — relay AND the Bitcoin relay inside indexd, which speak the same
//! contract on two links since 2026-09-14: what we send them, what they send
//! us, and the little state that the re-sync on a link rise needs. Was `crypto.rs` +
//! `connection.rs` until 2026-09-04; the socket itself now lives in
//! `socket.rs` and is shared with rates and bookd.

use crate::channel::{CHANNEL, HistoryList, WSCommand};
use crate::ws::commands::Command;
use crate::ws::config::{TAG_BTC, TAG_RELAY};
use serde::Serialize;
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

/// The relay's contract: every payload rides inside this envelope. It carried
/// a shared `auth_token` until 2026-09-20 — a string in a public binary gates
/// nothing, so it is gone; the session's gate is the `hello` in `socket.rs`.
#[derive(Serialize)]
struct WsMessage {
    payload: Value,
}

/// `{"payload": <json>}` around a bare JSON payload, plus
/// the stream it belongs on: the six Bitcoin commands go to the Bitcoin relay (inside indexd), its
/// own process since 2026-09-14, everything else to relay. A Bitcoin command
/// missing here lands on the XRPL relay as "unknown" and its flow times out. `None` if the
/// payload isn't JSON — nothing we send ever isn't.
pub fn wrap(payload: &str) -> Option<(u8, String)> {
    let payload_json: Value = serde_json::from_str(payload).ok()?;
    let tag = match payload_json.get("command").and_then(|c| c.as_str()).and_then(Command::from_str) {
        Some(Command::ImportBitcoinWallet)
        | Some(Command::SubscribeBitcoinAddresses)
        | Some(Command::GetBitcoinBalance)
        | Some(Command::DeleteBitcoinWallet)
        | Some(Command::SubmitBitcoinTransaction)
        | Some(Command::GetBitcoinHistory) => TAG_BTC,
        _ => TAG_RELAY,
    };
    let wrapped = serde_json::to_string(&WsMessage { payload: payload_json }).ok()?;
    Some((tag, wrapped))
}

/// A payload the socket task could not send because its link was down. Only a
/// signing payload is anyone's business here: it belongs to the one flow on
/// the activity log, which is told — in words that are certain, because the
/// bytes are still in our hands. A history page is told too — its end row
/// reads `couldn't load · retry ›` rather than `loading…` through a backoff,
/// and the retry is the user's. Everything else (balance asks, imports) is
/// re-asked by the link-rise re-sync or by the user. Returns whether it was one.
pub fn report_unsent(payload: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(payload) else { return false };
    match v.get("command").and_then(|c| c.as_str()) {
        Some("submit_transaction") => {
            crate::ws::commands::submit_transaction::report_unsent();
            true
        }
        Some("submit_bitcoin_transaction") => {
            crate::ws::commands::bitcoin_submit_transaction::report_unsent();
            true
        }
        Some("get_history") => {
            let list = match v.get("kind").and_then(|k| k.as_str()) {
                Some("orders") => HistoryList::XrpOrders,
                _ => HistoryList::XrpTransactions,
            };
            CHANNEL.transactions_tx.send_modify(|state| state.page_mut(list).fail_any());
            true
        }
        Some("get_bitcoin_history") => {
            CHANNEL.btc_transactions_tx.send_modify(|state| state.page.fail_any());
            true
        }
        _ => false,
    }
}

/// What relay and the Bitcoin relay have been told about us on their links, kept so
/// each can be told again when its link comes back. A service forgets a
/// connection the moment it drops; these are the only two facts to re-state.
#[derive(Default)]
pub struct RelayState {
    current_wallet: String,
    /// The BTC wallet. Element 0 is the primary (#0) — a SUCCESSFUL import
    /// resets the list to it (`note_arrival`), cached-balance appends, delete
    /// clears. Since 2026-09-20 an HD wallet only ever asks for its primary
    /// (the whole wallet rides that one ask), so the list is one long; it
    /// stays a list for the per-address fallback in `resync_btc_payloads`.
    bitcoin_current_wallets: Vec<String>,
    /// The BTC live list last put on the wire, sorted. Several things ask for
    /// the list to be sent (a whole-wallet frame, a rotation, the coin set
    /// moving) and most of the time it has not changed — at import it went out
    /// four times, twice as a pure repeat. Forgotten whenever the far side may
    /// have forgotten US: a new socket or a `link:btc` rise (the Bitcoin
    /// relay's push routing is per connection), a fresh import (the proxy
    /// rebinds the session to #0 alone), and Remove wallet.
    btc_live_list: Vec<String>,
    /// Removals that may not have landed, as `(command, wallet)`.
    ///
    /// `delete_wallet` / `delete_bitcoin_wallet` are fire-and-forget and get no
    /// reply, and the proxy DISCARDS a hub frame outright while the link is
    /// down (`command_while_down`). The app has erased the wallet files by
    /// then, so nothing on disk can ever name that address again — the removal
    /// would simply never happen and the relay would keep the wallet forever.
    /// So the address is held HERE, in memory only, for as long as this process
    /// lives, and re-sent on every link rise until an import of the same
    /// address supersedes it. Both commands are idempotent, so a repeat that
    /// did land costs nothing.
    ///
    /// This is a retry, not a record: it is never written to disk, it dies with
    /// the process, and the relay's own dormancy sweep is what covers the case
    /// where the user quits before the link ever comes back.
    pending_deletes: Vec<(String, String)>,
}

impl RelayState {
    /// Whether this command still needs to go out. Only the BTC live list is
    /// ever redundant: the same list as last time is dropped here.
    pub fn is_news(&mut self, cmd: &WSCommand) -> bool {
        if cmd.command != "subscribe_bitcoin_addresses" {
            return true;
        }
        let mut list = cmd.scan.clone().unwrap_or_default();
        list.sort();
        if list == self.btc_live_list {
            return false;
        }
        self.btc_live_list = list;
        true
    }

    /// The far side may have forgotten us — the next live list goes out even
    /// if it is the same one.
    pub fn forget_live_list(&mut self) {
        self.btc_live_list.clear();
    }

    /// Update the re-sync facts from an outgoing command.
    ///
    /// An import or create is NOT recorded here: until its reply lands the
    /// wallet does not exist, and one that fails never will. Recording it at
    /// command time left a FAILED import in these fields — re-synced on the
    /// next link rise, counted by `has_wallet`, and handed to the signing
    /// validation as the identity to compare against. `note_arrival` records
    /// it when the reply has made it this client's wallet.
    pub fn track(&mut self, cmd: &WSCommand) {
        if cmd.command == "get_cached_balance" {
            if let Some(w) = &cmd.wallet { self.current_wallet = w.clone(); }
        } else if cmd.command == "get_bitcoin_cached_balance" {
            if let Some(w) = &cmd.wallet
                && !self.bitcoin_current_wallets.contains(w)
            {
                self.bitcoin_current_wallets.push(w.clone());
            }
        } else if cmd.command == "delete_wallet" {
            // Deletion is tracked as carefully as arrival: this string is the
            // only thing the re-sync consults, so leaving it set would re-sync
            // a deleted wallet on the next link rise — the relay answers with
            // its cached balance and the wallet reappears.
            self.current_wallet.clear();
            self.note_delete(cmd);
        } else if cmd.command == "delete_bitcoin_wallet" {
            self.bitcoin_current_wallets.clear();
            self.btc_live_list.clear();
            self.note_delete(cmd);
        }
    }

    /// Hold a removal for retry until an import supersedes it. One entry per
    /// address: removing the same wallet twice is still one thing to undo.
    fn note_delete(&mut self, cmd: &WSCommand) {
        let Some(wallet) = cmd.wallet.as_deref().filter(|w| !w.is_empty()) else { return };
        if !self
            .pending_deletes
            .iter()
            .any(|(c, w)| c == &cmd.command && w == wallet)
        {
            self.pending_deletes.push((cmd.command.clone(), wallet.to_string()));
        }
    }

    /// An import of this address landed, so the address is live again and its
    /// held removal must NOT be replayed on the next rise — that would wipe
    /// the wallet the user just imported.
    fn clear_delete_for(&mut self, wallet: &str) {
        self.pending_deletes.retain(|(_, w)| w != wallet);
    }

    /// The held removals for one command, as payloads. They go out AHEAD of
    /// the re-sync asks: deleting and then re-asking would leave the relay
    /// answering for a wallet we just told it to forget.
    fn delete_payloads(&self, command: &str) -> Vec<String> {
        self.pending_deletes
            .iter()
            .filter(|(c, _)| c == command)
            .map(|(c, w)| serde_json::json!({ "command": c, "wallet": w }).to_string())
            .collect()
    }

    /// Whether this client holds a wallet of either chain — the gate on the
    /// rates stream: with no wallet there is nothing to price.
    pub fn has_wallet(&self) -> bool {
        !self.current_wallet.is_empty() || !self.bitcoin_current_wallets.is_empty()
    }

    /// Run the command's bridge work (build, sign, hand to `CRYPTO_OUTGOING_TX`)
    /// off the socket task.
    pub fn spawn_command(&self, cmd: WSCommand) {
        if let Some(command) = Command::from_str(&cmd.command) {
            let wallet = self.current_wallet.clone();
            // Commands see the PRIMARY — the identity the signing validation
            // compares against, not whichever member address subscribed last.
            let btc_wallet = self.bitcoin_current_wallets.first().cloned().unwrap_or_default();
            tokio::spawn(async move {
                let _ = command.execute(wallet, btc_wallet, cmd).await;
            });
        }
    }

    /// The bare payloads that re-establish this client with a freshly
    /// connected relay: one cached-balance ask for the XRP wallet, which the
    /// relay answers AND uses to rebuild its wallet→client mapping.
    pub fn resync_relay_payloads(&self) -> Vec<String> {
        let mut out = self.delete_payloads("delete_wallet");
        if !self.current_wallet.is_empty() {
            out.push(serde_json::json!({ "command": "get_cached_balance", "wallet": self.current_wallet }).to_string());
        }
        out
    }

    /// The same for the Bitcoin relay: ONE whole-wallet ask carrying the
    /// account xpub (`getbitcoincachedbalance::fetch_payload`) — its reply is
    /// what sends the live list again. A wallet with no account key yet falls
    /// back to one ask per address it holds, each naming its group's primary
    /// (element 0) as the live ask does; that is a pre-HD wallet's one address.
    pub fn resync_btc_payloads(&self) -> Vec<String> {
        let mut out = self.delete_payloads("delete_bitcoin_wallet");
        let primary = self.bitcoin_current_wallets.first().cloned().unwrap_or_default();
        if let Some(payload) = crate::ws::commands::getbitcoincachedbalance::fetch_payload(&primary) {
            out.push(payload);
            return out;
        }
        out.extend(self.bitcoin_current_wallets.iter().map(|wallet| {
            serde_json::json!({ "command": "get_bitcoin_cached_balance", "wallet": wallet, "primary": primary })
                .to_string()
        }));
        out
    }

    /// One frame from the relay. Unsolicited frames (`status`, the two node
    /// frames) carry no `command` and are handled before the command dispatch
    /// rather than being forced into the Command enum.
    pub async fn handle_frame(&mut self, text: String) {
        let Ok(data) = serde_json::from_str::<Value>(&text) else { return };
        match data.get("type").and_then(|t| t.as_str()) {
            Some("status") => { crate::ws::apply_status_frame(&data); return; }
            Some("node_stats") => { crate::ws::apply_node_stats_frame(&data); return; }
            Some("xrp_node_stats") => { crate::ws::apply_xrp_node_stats_frame(&data); return; }
            _ => {}
        }
        let cmd_str = data.get("command").and_then(|c| c.as_str());
        if let Some(command) = cmd_str.and_then(Command::from_str) {
            let btc_primary = self.bitcoin_current_wallets.first().cloned().unwrap_or_default();
            let _ = command
                .process_response(Message::text(text), &self.current_wallet, &btc_primary)
                .await;
            self.note_arrival(&data);
        }
    }

    /// Record a wallet once its import/create reply has been processed AND it
    /// is the wallet this client now holds. The channel is the test, not the
    /// handler's return value: every failure path leaves the channel alone
    /// (several of them return `Ok`), and only the success path writes the
    /// reply's wallet into it. BTC create rides `import_bitcoin_wallet`.
    fn note_arrival(&mut self, reply: &Value) {
        let Some(wallet) = reply.get("wallet").and_then(|w| w.as_str()) else { return };
        match reply.get("command").and_then(|c| c.as_str()) {
            Some("import_wallet") | Some("create_wallet") => {
                if CHANNEL.wallet_balance_rx.borrow().1.as_deref() == Some(wallet) {
                    self.current_wallet = wallet.to_string();
                    self.clear_delete_for(wallet);
                }
            }
            Some("import_bitcoin_wallet") => {
                // New wallet identity — the list starts over at its primary.
                // The member asks the import queued are tracked after this
                // returns, so they append behind it.
                if CHANNEL.bitcoin_wallet_rx.borrow().1.as_deref() == Some(wallet)
                    && self.bitcoin_current_wallets.first().map(String::as_str) != Some(wallet)
                {
                    self.bitcoin_current_wallets = vec![wallet.to_string()];
                }
                if CHANNEL.bitcoin_wallet_rx.borrow().1.as_deref() == Some(wallet) {
                    self.clear_delete_for(wallet);
                }
                // The import rebound the proxy session to #0 alone.
                self.btc_live_list.clear();
            }
            _ => {}
        }
    }
}
