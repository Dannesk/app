pub mod btc;
pub mod global;
pub mod history;
pub mod xrp;

pub use btc::*;
pub use global::*;
pub use history::*;
pub use xrp::*;

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::watch;

pub type RateHistoryMap = HashMap<String, Vec<(u64, f32)>>;

/// One upstream component's state as reported by the server that owns it.
///
/// `last_data_ms` is epoch millis of the last data that server saw from it (0 if
/// never). It is carried but NOT displayed: for most components it measures
/// activity rather than health — wallet updates arrive when the user transacts,
/// blocks every ~10 minutes, a quiet order book not at all — so a climbing
/// number would read as a fault while everything is fine. It exists so the
/// SERVER can later downgrade a silent-but-connected feed to `up: false`, which
/// is the side that knows each component's expected cadence.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ComponentHealth {
    pub up: bool,
    pub last_data_ms: u64,
}

/// What we can say about one component right now.
///
/// `Unknown` is NOT the same as `Down`: it means the transport is up but that
/// server hasn't told us about this component yet (a brief window right after
/// connecting). Rendering it as Down would flash a failure that isn't one;
/// rendering it as Up would be the very lie this whole mechanism removes. It
/// gets its own neutral treatment — "SYNCING", not "OFFLINE".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Up,
    Down,
    Unknown,
}

impl Health {
    pub fn is_up(self) -> bool {
        self == Health::Up
    }
}

/// The Rates list header's verdict. Distinct from [`Health`] because the list is
/// a claim about a *set* of prices, so it has a state no single component has:
/// `Degraded` — the socket is up and most rows are current, but at least one
/// asset is being shown at its last-known value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatesStatus {
    Live,
    Degraded,
    /// Connected, but the server hasn't sent its first status frame yet.
    Checking,
    Offline,
}

/// The four rows Settings ▸ Services shows. Fewer than there are components:
/// the three price feeds are one statement about whether prices are current,
/// and each node absorbs the process that sits over it (`bookd` broadcasts the
/// XRPL node, `indexd` owns bitcoind).
///
/// `RelayServer` is the relay's link alone. It used to be keyed on `relay:redis`,
/// the one thing in the relay that could die on its own while the socket stayed
/// open; since 2026-09-14 the relay's wallet store and push bus are in-process
/// (no Redis on either chain), so nothing behind a live link can fail by itself,
/// and the row says whether the relay can reach us — through the transport fold
/// in [`Channel::health`], which takes every `relay:*` key down with the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    PriceFeeds,
    XrpNode,
    BitcoinNode,
    RelayServer,
}

/// One service row's verdict. See [`Channel::service_state`] for why there are
/// three of these and not four.
///
/// It is the SERVICE's state and nothing else (user, 2026-09-20): up when the
/// service is up, down when it is literally down. Whether this client is
/// subscribed to it — whether it has a wallet at all — is internal, is never a
/// state here, and never changes one. The proxy tells every client the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Up,
    /// Some of this still works. Reserved for real partial capability — never
    /// used to soften a total outage.
    Degraded,
    Down,
}

// Wire keys, named once. The order book came out of rates on 2026-09-04
// (workspace/TRANSPORT-SPEC.md §4.3): its key moved from `rates:books` to
// `bookd:xrpld` here and in the transport fold below, and no caller moved.
pub const PRICE_FEEDS: [&str; 3] = ["rates:binance", "rates:kraken", "rates:gemini"];
pub const ORDER_BOOK: &str = "bookd:xrpld";
pub const XRP_NODE: &str = "relay:xrp";
// Bitcoin reports from its own process since 2026-09-14 (indexd's relay, link
// `btc`): its keys moved from `relay:*` to `btc:*` here and in the transport
// fold below, and no caller moved.
pub const BTC_NODE: &str = "btc:node";
pub const BTC_INDEX: &str = "btc:indexd";

/// Upstream health for all three servers in one map, keyed by the
/// server-namespaced component name the wire uses (`relay:xrp`,
/// `rates:binance`, `bookd:xrpld`, …). One map
/// because every read is "is component X up" and the caller does not care which
/// process owns it — the same reason `tokens` and `orderbook` are single maps.
///
/// Read it through [`Channel::health`], never directly: that method folds in the
/// transport booleans, which is what makes "my socket to this server is down ⟹
/// everything behind it is down" structural rather than something each call site
/// has to remember.
#[derive(Debug, Clone, Default)]
pub struct ServiceHealth {
    pub components: HashMap<String, ComponentHealth>,
    /// Assets the rates server says it can no longer price currently. Derived
    /// SERVER-side from its feed graph (an asset can bridge two feeds), so this
    /// is a plain list here and never re-derived.
    pub stale_assets: HashSet<String>,
}

/// Per-token ledger state, keyed by canonical token code ("RLUSD", "EUROP",
/// "XSGD", …) in the `tokens` channel map. Tuple is
/// `(balance, has_trustline, trustline_limit)` — the same shape the old
/// per-token channels carried, now in one map so adding a token needs no new
/// channel. Read with `CHANNEL.token(code)`, write with `set_token*`.
pub type TokenState = (f64, bool, Option<f64>);

/// One aggregated DEX book snapshot as published by the rates server: up to
/// 40 price levels per side, best first, funded amounts only. Prices are
/// quote per base for the pair the map keys it under ("XRP/RLUSD" → RLUSD
/// per XRP), amounts are in base units (XRP). Mid/spread are derived in the
/// view. `amm` is the pair's pool as the same frame saw it — invisible to
/// `book_offers`, and on XRP/RLUSD several times deeper than the whole CLOB
/// — which `utils::orderbook` slices into levels alongside the offers.
#[derive(Debug, Clone, Default)]
pub struct OrderBook {
    pub ledger: u64,
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
    pub amm: Option<Amm>,
    /// The token issuer's `lsfRequireAuth` / `lsfGlobalFreeze`, as rates read
    /// them once per connection. `None` means NOT KNOWN — never "restricted":
    /// an answer still in flight must not black out a market. Only a `Some(true)`
    /// gates anything. This replaces the per-order `tec` pre-flight that went
    /// with the node dry run (spec §12); a miss costs one fee.
    pub require_auth: Option<bool>,
    pub global_freeze: Option<bool>,
    /// The issuer's `TickSize`: how many significant digits the ledger rounds
    /// this book's offer QUALITIES to. `None` is the common case and means full
    /// precision. A cushion input, not trivia — see `xrp::trade_delta_min`.
    pub tick_size: Option<u32>,
}

/// bookd's per-book depth summary — the trade picker's tradeability
/// measurement, pushed unsolicited for every pair so a market can be judged
/// before its book is held. Same fields as bookd's `MarketSummary`; every
/// figure is XRP except `mid` (token per XRP) and the spread. The thresholds
/// that turn these into Live / Thin / None live in `utils::liquidity`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MarketSummary {
    pub ledger: u64,
    pub mid: f64,
    pub spread_pct: f64,
    pub bid1: f64,
    pub ask1: f64,
    pub bid5: f64,
    pub ask5: f64,
    pub amm_xrp: f64,
    pub amm1: f64,
    /// bookd's window over its last ~20 readings (2026-09-15): medians of
    /// the figures above and the count of HOLES among them — readings whose
    /// mid broke from the median or whose side emptied. `win_n == 0` is an
    /// old bookd or the first frame; the instantaneous figures stand in.
    pub win_n: u32,
    pub win_mid: f64,
    pub win_spread_pct: f64,
    pub win_bid1: f64,
    pub win_ask1: f64,
    pub win_bid5: f64,
    pub win_ask5: f64,
    pub win_amm1: f64,
    pub holes: u32,
}

/// An XRPL AMM pool's state for an XRP/token pair: the two reserves and the
/// trading fee in percent (`TradingFee` 205 ⇒ 0.205). Constant product; the
/// fee is charged on the input side.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Amm {
    pub xrp: f64,
    pub token: f64,
    pub fee_pct: f64,
}


/// How the active wallet's key is stored on this device. Cold / watch-only is
/// already carried by the `private_key_deleted` bool in the wallet tuples, so
/// with the hardware tier gone this has exactly one inhabitant: a key that is
/// present is a passphrase-encrypted file.
///
/// It is kept as a type rather than deleted because it is the fourth element of
/// both wallet watch-channel tuples, and because the *question* it answers —
/// how is this key held — is the one a second storage method would answer
/// differently. Collapsing it would rewrite ~90 unrelated destructuring sites
/// to save a discriminant that is never read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyMode {
    #[default]
    Standard,
}

impl KeyMode {
    /// Maps the persisted `method` discriminator to a mode. Every value —
    /// `"standard"`, `"cold"`, absent, or a `"hot"` left by an older build —
    /// reads as Standard; a cold wallet is identified by its deleted-key flag,
    /// not by this.
    pub fn from_method(_method: &str) -> Self {
        KeyMode::Standard
    }
}

/// What has arrived since launch: the local wallet records, and each chain's
/// first balance from its relay. Each flag goes up once and stays up. A total
/// waits for them: the prices land with the connect, a relay round trip ahead
/// of the balances, and a sum taken before then reads 0.00, or part of the
/// total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Loaded {
    /// `wallet::load_wallets` has read the local records: each chain's
    /// identity is in its channel by now, or there is none.
    pub wallets: bool,
    /// The XRP relay's cached-balance reply has been applied: the balance,
    /// the account and the tokens together.
    pub xrp: bool,
    /// The Bitcoin wallet's coins have arrived, so its balance is the
    /// chain's figure.
    pub btc: bool,
}

/// How the launch went: the first connect's outcome, from the socket task,
/// and when the app started, for the limit. A screen reads it to tell a
/// figure that is still on its way from one that cannot be had — at launch
/// only. After [`FETCH_LIMIT`] the launch is over, whatever happened, and the
/// link bools alone say what can be had, as they did before 2026-10-07.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Launch {
    pub phase: Phase,
    pub since: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The first attempt is still running: TCP, then the handshake, then the
    /// proxy's first link report.
    Connecting,
    /// The first attempt reached the proxy and its link report is in; what
    /// each figure waits for now is its own service's answer.
    Connected,
    /// The first attempt failed. The socket keeps trying on its backoff, but
    /// the screens read the dash until something arrives.
    Failed,
}

/// The longest a figure reads as "on its way" after launch. The socket gives
/// one attempt ten seconds each for the connect and the handshake; a figure
/// still missing after this cannot be had, until it arrives.
pub const FETCH_LIMIT: Duration = Duration::from_secs(10);

impl Launch {
    fn new() -> Self {
        Launch { phase: Phase::Connecting, since: Instant::now() }
    }

    /// Whether a figure is still on its way: at launch, until the first
    /// attempt resolves; then, connected, while its service is up and its
    /// data has not arrived; never past the limit, never after a failed
    /// first attempt. `link_up` is the figure's transport bool, `arrived`
    /// whether its data is in.
    pub fn fetching(&self, link_up: bool, arrived: bool) -> bool {
        if arrived || self.since.elapsed() >= FETCH_LIMIT {
            return false;
        }
        match self.phase {
            Phase::Connecting => true,
            Phase::Connected => link_up,
            Phase::Failed => false,
        }
    }
}

pub static CHANNEL: LazyLock<Channel> = LazyLock::new(Channel::new);

pub struct Channel {
    //global related channels
    pub rates_tx: watch::Sender<HashMap<String, f32>>,
    pub rates_rx: watch::Receiver<HashMap<String, f32>>,
    pub rate_history_tx: watch::Sender<RateHistoryMap>,
    pub rate_history_rx: watch::Receiver<RateHistoryMap>,
    pub rate_history_long_tx: watch::Sender<RateHistoryMap>,
    pub rate_history_long_rx: watch::Receiver<RateHistoryMap>,

    pub activity_tx: watch::Sender<Option<ActivityLogState>>,
    pub activity_rx: watch::Receiver<Option<ActivityLogState>>,
    // TRANSPORT ONLY — "can an answer from that server reach us". Since the
    // single-socket change (2026-09-04) each is `socket up AND the proxy's
    // link to that service up`, written by the socket task from the proxy's
    // link frame. They say nothing about whether the exchanges, chain nodes or
    // Redis behind a service are alive; that is what `service_health` carries.
    // Reading a transport bool where an upstream fact was meant is exactly the
    // bug this set used to cause, which is why they are no longer named after
    // what they were once used to imply.
    pub rates_ws_status_tx: watch::Sender<bool>,
    pub rates_ws_status_rx: watch::Receiver<bool>,
    pub relay_ws_status_tx: watch::Sender<bool>,
    pub relay_ws_status_rx: watch::Receiver<bool>,
    pub btc_ws_status_tx: watch::Sender<bool>,
    pub btc_ws_status_rx: watch::Receiver<bool>,
    pub book_ws_status_tx: watch::Sender<bool>,
    /// The one client socket to the proxy itself, links aside. With it up, a
    /// link that is down is a SERVICE that is down — the proxy could not reach
    /// it — not our network.
    pub proxy_ws_status_tx: watch::Sender<bool>,
    pub proxy_ws_status_rx: watch::Receiver<bool>,
    /// See [`Loaded`]. Set through `send_if_modified`, so its readers wake
    /// once per flag, not on every balance push.
    pub loaded_tx: watch::Sender<Loaded>,
    pub loaded_rx: watch::Receiver<Loaded>,
    /// See [`Launch`]. Written by the socket task once, on its first attempt.
    pub launch_tx: watch::Sender<Launch>,
    pub launch_rx: watch::Receiver<Launch>,
    pub book_ws_status_rx: watch::Receiver<bool>,

    // Upstream health for both servers, merged into one map. See [`ServiceHealth`].
    pub service_health_tx: watch::Sender<ServiceHealth>,
    pub service_health_rx: watch::Receiver<ServiceHealth>,

    // Issued-token state, one map keyed by token code. One watch channel for
    // all tokens; `send_modify` updates a single key without disturbing the
    // rest (same pattern as `rates`). Adding a token = a registry entry, no new
    // channel here.
    pub tokens_tx: watch::Sender<HashMap<String, TokenState>>,
    pub tokens_rx: watch::Receiver<HashMap<String, TokenState>>,

    // DEX order books, one map keyed by pair ("XRP/RLUSD"). Each wire frame is
    // a complete snapshot of one book, so `send_modify` replaces that pair's
    // entry wholesale — no per-level diffing — without disturbing other books.
    // Adding a book on the server needs no change here.
    pub orderbook_tx: watch::Sender<HashMap<String, OrderBook>>,
    pub orderbook_rx: watch::Receiver<HashMap<String, OrderBook>>,
    // Every XRP-leg market's depth summary, keyed by pair, replaced whole on
    // each `markets` frame. Never gated on a subscription — it is what says
    // whether subscribing is worth it.
    pub markets_tx: watch::Sender<HashMap<String, MarketSummary>>,
    pub markets_rx: watch::Receiver<HashMap<String, MarketSummary>>,
    pub pending_trade_tx: watch::Sender<Option<PendingTrade>>,
    pub pending_trade_rx: watch::Receiver<Option<PendingTrade>>,

    //xrp channels
    pub wallet_balance_tx: watch::Sender<(f64, Option<String>, bool, KeyMode)>,
    pub wallet_balance_rx: watch::Receiver<(f64, Option<String>, bool, KeyMode)>,
    pub transactions_tx: watch::Sender<TransactionState>,
    pub transactions_rx: watch::Receiver<TransactionState>,
  
    //bitcoin related channels
    // See [`BtcSendDispatches`]. Its own channel rather than a reading of the
    // activity log: the log is a display of backend comms, and inferring a
    // control decision from which of its steps went red would couple the send
    // flow to a string id in a surface that exists to be looked at.
    pub btc_send_dispatched_tx: watch::Sender<BtcSendDispatches>,
    pub btc_send_dispatched_rx: watch::Receiver<BtcSendDispatches>,

    pub bitcoin_wallet_tx: watch::Sender<(f64, Option<String>, bool, KeyMode)>,
    pub bitcoin_wallet_rx: watch::Receiver<(f64, Option<String>, bool, KeyMode)>,
    pub btc_transactions_tx: watch::Sender<BtcTransactionState>,
    pub btc_transactions_rx: watch::Receiver<BtcTransactionState>,
    // The wallet's UTXO set, pushed whole by the relay (cached copy on app
    // open, fresh copy on every wallet event / subscribe / reconnect). Signing
    // builds from this — the blockchain is the arbiter and the set is
    // re-derived server-side from indexd, never maintained here.
    pub btc_utxos_tx: watch::Sender<(Option<String>, Vec<BtcUtxo>)>,
    pub btc_utxos_rx: watch::Receiver<(Option<String>, Vec<BtcUtxo>)>,
    // Our node's own state + the fee market. NOT per-wallet: one frame shared
    // by every client, so it rides its own channel rather than the wallet tuple.
    pub btc_node_tx: watch::Sender<BtcNodeStats>,
    pub btc_node_rx: watch::Receiver<BtcNodeStats>,
    // XRPL twin: ledger index, fee market, reserves, peers — one frame for all.
    pub xrp_node_tx: watch::Sender<XrpNodeStats>,
    pub xrp_node_rx: watch::Receiver<XrpNodeStats>,
    // Per-wallet: next sequence + owner count. Signing reads this, never fetches.
    pub xrp_account_tx: watch::Sender<XrpAccount>,
    pub xrp_account_rx: watch::Receiver<XrpAccount>,
}

impl Channel {
    pub fn new() -> Self {
        //global related
        let (rates_tx, rates_rx) = watch::channel(HashMap::new());
        let (rate_history_tx, rate_history_rx) = watch::channel(HashMap::new());
        let (rate_history_long_tx, rate_history_long_rx) = watch::channel(HashMap::new());
        let (activity_tx, activity_rx) = watch::channel(None);
        let (btc_send_dispatched_tx, btc_send_dispatched_rx) = watch::channel(0);
        let (rates_ws_status_tx, rates_ws_status_rx) = watch::channel(false);
        let (relay_ws_status_tx, relay_ws_status_rx) = watch::channel(false);
        let (btc_ws_status_tx, btc_ws_status_rx) = watch::channel(false);
        let (book_ws_status_tx, book_ws_status_rx) = watch::channel(false);
        let (proxy_ws_status_tx, proxy_ws_status_rx) = watch::channel(false);
        let (loaded_tx, loaded_rx) = watch::channel(Loaded::default());
        let (launch_tx, launch_rx) = watch::channel(Launch::new());
        let (service_health_tx, service_health_rx) = watch::channel(ServiceHealth::default());
       
        //token balances (single map, keyed by token code)
        let (tokens_tx, tokens_rx) = watch::channel(HashMap::new());

        //dex order books (single map, keyed by pair)
        let (orderbook_tx, orderbook_rx) = watch::channel(HashMap::new());
        let (markets_tx, markets_rx) = watch::channel(HashMap::new());
        let (pending_trade_tx, pending_trade_rx) = watch::channel(None);

        //xrp related
        let (wallet_balance_tx, wallet_balance_rx) = watch::channel((0.0, None, false, KeyMode::Standard));

        let (transactions_tx, transactions_rx) = watch::channel(TransactionState::default());

        //btc related
        let (bitcoin_wallet_tx, bitcoin_wallet_rx) = watch::channel((0.0, None, false, KeyMode::Standard));

        let (btc_transactions_tx, btc_transactions_rx) = watch::channel(BtcTransactionState::default());

        let (btc_utxos_tx, btc_utxos_rx) = watch::channel((None, Vec::new()));

        let (btc_node_tx, btc_node_rx) = watch::channel(BtcNodeStats::default());
        let (xrp_node_tx, xrp_node_rx) = watch::channel(XrpNodeStats::default());
        let (xrp_account_tx, xrp_account_rx) = watch::channel(XrpAccount::default());


        Channel {

            rates_tx,
            rates_rx,
            rate_history_tx,
            rate_history_rx,
            rate_history_long_tx,
            rate_history_long_rx,
            btc_send_dispatched_tx,
            btc_send_dispatched_rx,
            activity_tx,
            activity_rx,
            rates_ws_status_tx,
            rates_ws_status_rx,
            relay_ws_status_tx,
            relay_ws_status_rx,
            btc_ws_status_tx,
            btc_ws_status_rx,
            book_ws_status_tx,
            proxy_ws_status_tx,
            proxy_ws_status_rx,
            loaded_tx,
            loaded_rx,
            launch_tx,
            launch_rx,
            book_ws_status_rx,
            service_health_tx,
            service_health_rx,


            tokens_tx,
            tokens_rx,

            orderbook_tx,
            orderbook_rx,
            markets_tx,
            markets_rx,
            pending_trade_tx,
            pending_trade_rx,

            wallet_balance_tx,
            wallet_balance_rx,
            transactions_tx,
            transactions_rx,
           

            bitcoin_wallet_tx,
            bitcoin_wallet_rx,
            btc_transactions_tx,
            btc_utxos_tx,
            btc_utxos_rx,
            btc_transactions_rx,
            btc_node_tx,
            btc_node_rx,
            xrp_node_tx,
            xrp_node_rx,
            xrp_account_tx,
            xrp_account_rx,

        }
    }

    /// Snapshot of the published book for a pair, in either orientation —
    /// "XRP/RLUSD" and "RLUSD/XRP" are the same book, keyed directionally by
    /// the server. Returns the canonical key too so the view can label the
    /// ladder in the orientation the prices are actually quoted in.
    pub fn book(&self, base: &str, quote: &str) -> Option<(String, OrderBook)> {
        let books = self.orderbook_rx.borrow();
        for key in [format!("{base}/{quote}"), format!("{quote}/{base}")] {
            if let Some(b) = books.get(&key) {
                return Some((key, b.clone()));
            }
        }
        None
    }

    /// Snapshot of one token's `(balance, has_trustline, limit)`. Returns the
    /// zero default when the token has no entry yet, so callers never branch on
    /// presence.
    pub fn token(&self, code: &str) -> TokenState {
        self.tokens_rx
            .borrow()
            .get(code)
            .copied()
            .unwrap_or((0.0, false, None))
    }

    /// Replace one token's full state in place, leaving every other token's
    /// entry untouched (single-key `send_modify`, no full-map clone).
    pub fn set_token(&self, code: &str, state: TokenState) {
        self.tokens_tx
            .send_modify(|m| { m.insert(code.to_string(), state); });
    }

    /// Update only a token's balance, preserving its trustline flag and limit.
    pub fn set_token_balance(&self, code: &str, balance: f64) {
        self.tokens_tx.send_modify(|m| {
            m.entry(code.to_string()).or_insert((0.0, false, None)).0 = balance;
        });
    }

    /// Reset every token to its zero default — used on wallet removal / wipe.
    /// Generic over the token set, so it needs no edit when tokens are added.
    /// Also clears the `"XRP"` existence entry (see [`Self::set_xrp_exists`]).
    pub fn clear_tokens(&self) {
        self.tokens_tx.send_modify(|m| m.clear());
        // The account's sequence/owner count belong to the wallet being removed.
        self.clear_xrp_account();
    }

    /// XRP account existence rides the token map as the `"XRP"` entry's `has`
    /// flag — the same statement every token's flag makes ("the ledger object
    /// backing this asset exists"): a trustline for issued tokens, the
    /// AccountRoot itself for XRP. The entry's balance/limit slots are unused
    /// (XRP balance lives on `wallet_balance`), and `"XRP"` is not in the token
    /// registry, so registry iterations never see this entry.
    /// Fold a relay frame's `sequence` / `owner_count` into the account watch.
    /// `null` (old relay, legacy hash) leaves the current value alone — a
    /// missing fact must never erase a known one. Wallet change clears both
    /// via [`Self::clear_xrp_account`].
    pub fn apply_xrp_account(&self, data: &serde_json::Value) {
        let seq = data.get("sequence").and_then(|v| v.as_u64()).and_then(|v| u32::try_from(v).ok());
        let oc = data.get("owner_count").and_then(|v| v.as_u64()).and_then(|v| u32::try_from(v).ok());
        if seq.is_none() && oc.is_none() {
            return;
        }
        self.xrp_account_tx.send_if_modified(|a| {
            let before = *a;
            if seq.is_some() { a.sequence = seq; }
            if oc.is_some() { a.owner_count = oc; }
            *a != before
        });
    }

    pub fn clear_xrp_account(&self) {
        let _ = self.xrp_account_tx.send(XrpAccount::default());
    }

    pub fn set_xrp_exists(&self, exists: bool) {
        self.tokens_tx.send_modify(|m| {
            m.entry("XRP".to_string()).or_insert((0.0, false, None)).1 = exists;
        });
    }

    /// State of one upstream component, keyed as the wire names it
    /// (`relay:xrp`, `btc:node`, `btc:indexd`, `rates:binance`,
    /// `rates:kraken`, `rates:gemini`, `bookd:xrpld`).
    ///
    /// The transport fold is the point of this method. A component's reported
    /// state is only meaningful while the socket that reports it is open, so a
    /// dead socket forces `Down` no matter what the last frame said. Doing it
    /// here rather than clearing the map on disconnect means there is no
    /// invalidation step anyone can forget, and no window where a stale `up`
    /// from before a drop reads as healthy.
    pub fn health(&self, key: &str) -> Health {
        if !self.link_up(key) {
            return Health::Down;
        }
        match self.service_health_rx.borrow().components.get(key) {
            Some(c) if c.up => Health::Up,
            Some(_) => Health::Down,
            None => Health::Unknown,
        }
    }

    /// The transport bool behind a wire key: whether an answer from the service
    /// that owns that namespace can reach us right now — socket AND link, as
    /// the socket task publishes it. The activity log reads this for the flow
    /// it is narrating; everything else reads it through [`Self::health`].
    pub fn link_up(&self, key: &str) -> bool {
        match key.split(':').next() {
            Some("relay") => *self.relay_ws_status_rx.borrow(),
            Some("btc") => *self.btc_ws_status_rx.borrow(),
            Some("rates") => *self.rates_ws_status_rx.borrow(),
            Some("bookd") => *self.book_ws_status_rx.borrow(),
            // An unnamespaced or unknown key can't be vouched for by any socket.
            _ => false,
        }
    }

    /// The transport bool behind a wire key as a receiver, for anything that
    /// must WAIT on it rather than read it — the activity watchdog, which
    /// counts only the time an answer could have arrived. `None` or an
    /// unnamespaced key falls back to the socket itself: a flow that named no
    /// link still needs one to be up.
    pub fn link_rx(&self, key: Option<&str>) -> watch::Receiver<bool> {
        match key.and_then(|k| k.split(':').next()) {
            Some("relay") => self.relay_ws_status_rx.clone(),
            Some("btc") => self.btc_ws_status_rx.clone(),
            Some("rates") => self.rates_ws_status_rx.clone(),
            Some("bookd") => self.book_ws_status_rx.clone(),
            _ => self.proxy_ws_status_rx.clone(),
        }
    }

    /// Overall verdict for the Rates list header. Deliberately driven by the
    /// server's `stale_assets` rather than by counting down feeds: the two are
    /// not the same statement (a feed can be down without making any displayed
    /// asset stale, and one dead feed can strand assets quoted by others), and
    /// only the server knows the difference.
    ///
    /// Four states, not three, because "connected but not yet told" and "some
    /// prices are stale" are genuinely different and collapsing them would flash
    /// a Degraded warning on every connect.
    pub fn rates_status(&self) -> RatesStatus {
        if !*self.rates_ws_status_rx.borrow() {
            return RatesStatus::Offline;
        }
        let h = self.service_health_rx.borrow();
        if h.components.is_empty() {
            return RatesStatus::Checking;
        }
        if h.stale_assets.is_empty() {
            RatesStatus::Live
        } else {
            RatesStatus::Degraded
        }
    }

    /// What one Settings ▸ Services row reports. Three states, deliberately:
    /// `Checking` was dropped because the app cannot distinguish "not reported
    /// yet" from "nobody is coming", and narrating an investigation we may not
    /// be running is a claim for the status page, not the wallet. So
    /// [`Health::Unknown`] folds into `Down` here — safe now that down renders
    /// as a `faint` dot and a `muted` word rather than red.
    ///
    /// The ramp is non-monotonic in alarm on purpose: gray down, amber
    /// degraded, green up. **Down is self-evident** — the BTC screen is
    /// visibly broken — while **degraded is the state you would otherwise
    /// miss.** Amber earns the colour by being the non-obvious one.
    pub fn service_state(&self, svc: Service) -> ServiceState {
        match svc {
            Service::PriceFeeds => {
                // Total failure is read off the components, partial failure off
                // `stale_assets`. Both are needed: with every feed dead the
                // server still reports Degraded (some asset is merely stale),
                // and "all my prices are gone" is not a partial outage. Below
                // that line the component count is the WRONG signal and is not
                // consulted — binance dying strands AUD and SGD even though
                // kraken and gemini are up, because both bridges divide through
                // BTC/USD (`rates/src/rate_engine.rs`). Only the server knows
                // that graph, which is why it ships the answer and not the
                // inputs.
                if !PRICE_FEEDS.iter().any(|k| self.health(k).is_up()) {
                    return ServiceState::Down;
                }
                match self.rates_status() {
                    RatesStatus::Live => ServiceState::Up,
                    RatesStatus::Degraded => ServiceState::Degraded,
                    RatesStatus::Checking | RatesStatus::Offline => ServiceState::Down,
                }
            }

            // The book is the node's own broadcast, so it is reported as part
            // of it rather than as a peer: it has no way to fail on its own
            // while the node is healthy. Losing it is genuinely partial —
            // import, balances and sends all still work, only DEX depth is
            // gone — which is the one place amber belongs.
            //
            // Two ways to lose the book, both amber, both only judged while
            // our own socket to the proxy is up. bookd running but its
            // doorbell to xrpld gone: `bookd:xrpld` reports down. bookd itself
            // gone: the proxy's `link:book` drops — and behind a live socket a
            // dead link IS bookd, not our network (the proxy is what could not
            // reach it). With the socket down we cannot see the book and say
            // nothing about it, which is what keeps a dropped connection from
            // painting "XRP node degraded" about a healthy node.
            //
            // `link:book` is bookd's OWN state, which the proxy reports to
            // every client from its watcher on that service (2026-09-20) — not
            // whether this client is subscribed to a book. A client with no
            // wallet holds no book and still reads bookd as up, because it is.
            Service::XrpNode => {
                if !self.health(XRP_NODE).is_up() {
                    return ServiceState::Down;
                }
                let socket_up = *self.proxy_ws_status_rx.borrow();
                let book_link = *self.book_ws_status_rx.borrow();
                if socket_up && (!book_link || !self.health(ORDER_BOOK).is_up()) {
                    return ServiceState::Degraded;
                }
                ServiceState::Up
            }

            // No degraded state, and the asymmetry with XRP is the point.
            // `indexd` sits over bitcoind and owns every balance and every bit
            // of history; with it down nothing on the BTC side works at all.
            // Amber would say "some of this still works", which is false, so
            // the row reports Down and the description names which half died.
            // Nothing else to fold in: since 2026-09-14 the Bitcoin relay has no
            // Redis — its store is in-process, so it cannot fail on its own
            // behind a live link — and its link loss takes both keys down.
            Service::BitcoinNode => {
                if self.health(BTC_NODE).is_up() && self.health(BTC_INDEX).is_up() {
                    ServiceState::Up
                } else {
                    ServiceState::Down
                }
            }

            // The link, not a component: the relay reports nothing that can
            // fail on its own behind a live link (see the `Service` docs).
            Service::RelayServer => {
                if self.link_up(XRP_NODE) {
                    ServiceState::Up
                } else {
                    ServiceState::Down
                }
            }
        }
    }

    /// Replace the components reported by one server, leaving the other's
    /// entries alone — the two push independently and neither knows the other's
    /// keys. `stale_assets` is only carried by the rates server, so `None`
    /// means "this frame says nothing about staleness", not "nothing is stale".
    pub fn apply_health(
        &self,
        components: HashMap<String, ComponentHealth>,
        stale_assets: Option<HashSet<String>>,
    ) {
        self.service_health_tx.send_modify(|h| {
            h.components.extend(components);
            if let Some(stale) = stale_assets {
                h.stale_assets = stale;
            }
        });
    }

    /// A link fell: every component that service reported goes DOWN now, not
    /// when the service next says so — it cannot, it is unreachable. The
    /// timestamps are kept, so "down since" still reads true. Without this the
    /// map held each service's last word (usually `up`) for as long as the
    /// outage lasted, and Settings ▸ Services read green through it.
    pub fn mark_link_down(&self, prefix: &str) {
        self.service_health_tx.send_modify(|h| {
            for (key, c) in h.components.iter_mut() {
                if key.starts_with(prefix) {
                    c.up = false;
                }
            }
        });
    }

    /// Chain-asserted XRP account existence (see [`Self::set_xrp_exists`]).
    /// `false` also covers "no assertion received yet" (map default) — parse
    /// sites fold the legacy balance-threshold inference in at write time, so
    /// a cached funded wallet reads `true` before the relay reasserts it.
    pub fn xrp_exists(&self) -> bool {
        self.token("XRP").1
    }
}
