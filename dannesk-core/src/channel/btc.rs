use serde::{Deserialize, Serialize};
use std::collections::HashMap;


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BitcoinTransactionStatus {
    Success,
    Failed,
    Pending,
    Cancelled,
    /// Left the mempool WITHOUT being mined — replaced, evicted, conflicted,
    /// or expired after bitcoind's 14-day `mempoolexpiry`. Reported by indexd
    /// off bitcoind's ZMQ `sequence` topic, whose `R` label means exactly this
    /// and cannot be raised by a confirmation.
    ///
    /// Kept as a real state rather than deleted: an evicted transaction is a
    /// payment that did not happen, and it is written to Redis like any other
    /// record so it survives closing the app. A user who was told "sent" and
    /// comes back to an empty list would have no way to learn otherwise.
    Dropped,
    /// Superseded by a fee bump this wallet signed. Written by the relay off
    /// the replacement's broadcast verdict, never inferred from the mempool —
    /// the node's own `R` event cannot tell a replacement from an eviction.
    /// Files among the settled like a drop, but it is not a failure: the
    /// payment is still on its way, under `replaced_by`.
    Replaced,
}

/// A pending transaction as a fee bump rebuilds it, assembled from the row's
/// own record plus the node frame — never fetched. Inputs carry their value
/// and owner because those coins left the wallet's UTXO set the moment the
/// original was accepted.
#[derive(Debug, Clone, PartialEq)]
pub struct BtcRbfInfo {
    pub txid: String,
    /// Measured by the node (`getmempoolentry.vsize`), not estimated.
    pub vsize: u64,
    /// Paid by the original, in satoshis.
    pub fee_sats: u64,
    pub inputs: Vec<BtcRbfInput>,
    pub outputs: Vec<BtcRbfOutput>,
    /// The node's `incrementalrelayfee`, sat/vB — BIP125 rule 4's floor on
    /// how much MORE the replacement must pay, per vbyte of its own size.
    pub incremental_sat_vb: f32,
    /// The node's live `mempoolminfee`, sat/vB.
    pub min_sat_vb: f32,
    /// Mempool transactions that spend this one's outputs. A bump evicts them
    /// with the original, so any non-zero count refuses the bump.
    pub descendants: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BtcRbfInput {
    pub txid: String,
    pub vout: u32,
    pub sats: u64,
    /// The prevout's address — which of our keys signs it. `None` is a script
    /// the node could not name, which cannot be ours.
    pub address: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BtcRbfOutput {
    pub vout: u32,
    pub sats: u64,
    pub address: Option<String>,
    /// scriptPubKey, hex, when the source carried it (a send this client just
    /// signed). Absent on the relay's record, where the planner rebuilds it
    /// from the address — the same bytes for every standard script.
    #[serde(default)]
    pub spk: Option<String>,
}


/// Our Bitcoin node's own state and the fee market, as measured by indexd and
/// forwarded verbatim by the relay. Shared by every client — this is the node's
/// state, not the wallet's.
///
/// Every field is `Option` and **a field we cannot measure stays `None`**: the
/// UI draws `—` and an empty gauge rather than a stale reading. That is
/// structural, not politeness — a fee tier is a number the user spends money on,
/// so a plausible-but-old one is worse than none. The relay clears the whole
/// frame when it loses indexd for the same reason.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BtcNodeStats {
    /// Index height ÷ node height, 0.0..=1.0. NOT `verificationprogress`, which
    /// is pinned at 1 on our node and would be a dead meter.
    pub sync: Option<f32>,
    pub peers: Option<u32>,
    pub tip_height: Option<u64>,
    /// indexd's own arrival stamp for the tip, epoch seconds — not the block
    /// header's time, which is not monotonic across blocks.
    pub tip_at: Option<u64>,
    /// Average block interval in **SECONDS**, over 144 blocks. Every screen
    /// that shows it displays minutes, so each divides by 60 at the point of
    /// display — the wire unit stays seconds because that is what indexd
    /// measures (a subtraction of two header timestamps).
    pub avg_interval: Option<f32>,
    /// floor / low / med / high(next block), sat/vB.
    pub tiers: Option<[f32; 4]>,
    /// The tip block's realised feerate (`totalfee ÷ vsize`), sat/vB.
    ///
    /// **Not currently rendered.** It was meant to be the fee gauge's
    /// self-calibrating scale top, but one block is far too narrow a scale —
    /// any current fee above that single block pegged the bar full. The gauge
    /// now reads fixed severity bands instead (see `managebtc::node::FEE_BANDS`).
    /// Kept because it is one cheap call per block and is the honest input for
    /// a future "what blocks actually paid" view.
    pub fee_scale_top: Option<f32>,
    /// The node's `incrementalrelayfee`, sat/vB — what a fee bump must add
    /// per vbyte on top of the original's fee (BIP125 rule 4).
    pub incremental_sat_vb: Option<f32>,
    /// Transactions in the mempool at indexd's last walk, and how many of
    /// the best-paying fill the next block by vsize — the blocks pane's
    /// candidate card (2026-09-10). Both are read off the same snapshot as
    /// the tiers, so they move when the tiers move and never on their own.
    pub mempool_txs: Option<u64>,
    pub next_block_txs: Option<u64>,
    /// Vsize queued in the mempool at the walk — the `mempool` pane's
    /// `pending`, and what its bands add up to.
    pub pending_vsize: Option<u64>,
    /// The mempool by fee band, cheapest first, edges derived by indexd at
    /// walk time (log-spaced from the relay floor to the rate at its top
    /// read; the last band is open). All-or-nothing: a partial histogram
    /// is not a histogram.
    pub bands: Option<[BtcBand; BANDS]>,
    /// The last mined blocks, newest first — indexd's ring, forwarded
    /// verbatim. `None` past what it has measured. A fixed array rather
    /// than a `Vec` keeps the frame `Copy`, which every reader relies on.
    pub blocks: [Option<BtcBlock>; BLOCK_TRAIN],
    /// bitcoind's own block height and best-header height. Headers ahead
    /// of blocks is the one honest sign the node is behind the network —
    /// it knows of a block it has not applied. A quiet chain produces
    /// neither, however long the gap.
    pub node_height: Option<u64>,
    pub headers: Option<u64>,
    /// How long headers have run ahead of blocks, seconds, as indexd timed
    /// it — `Some(0)` while they agree. The status word waits a minute of
    /// this (`managebtc::node::BEHIND_FLOOR_SECS`) so a block being fetched and
    /// validated, which takes seconds, never reads as a stall.
    pub behind_secs: Option<u64>,
    /// The tip block's header time and the header time of the first block
    /// of the current difficulty epoch, epoch seconds — the `block
    /// intervals` pane's retarget estimate is their ratio against the
    /// blocks between them.
    pub tip_time: Option<u64>,
    pub epoch_start_time: Option<u64>,
}

/// How many mined blocks the frame carries (indexd's `TRAIN`): the block
/// train draws five, the intervals pane needs twelve gaps.
pub const BLOCK_TRAIN: usize = 13;

/// Fee bands in the mempool histogram (indexd's `BANDS`).
pub const BANDS: usize = 14;

/// One band of the mempool histogram: its lower edge in sat/vB and the
/// vsize queued in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BtcBand {
    pub from: f32,
    pub vsize: u64,
}

/// One mined block, as the blocks pane's train draws it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BtcBlock {
    pub height: u64,
    /// When it landed, epoch seconds: indexd's own clock for a block it
    /// watched arrive, else the header time clamped to now. Header times
    /// are not monotonic across blocks, so ages derived from a run of them
    /// are clamped monotonic at the point of display.
    pub at: u64,
    pub txs: u64,
    /// Weight units; 4,000,000 is a full block.
    pub weight: u64,
    /// `totalfee ÷ vsize`, sat/vB — what getting into this block cost.
    pub feerate: f32,
}

/// One coin of the wallet's UTXO set, exactly as the relay pushes it
/// (`btc_utxos` frames and the `utxos` field of the cached-balance reply).
/// indexd's tracker is the sole maintainer of this set; the client only ever
/// receives it whole and signs from it — there is no per-signing fetch any
/// more (see project_btc_utxo_in_redis).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtcUtxo {
    pub txid: String,
    pub vout: u32,
    pub sats: u64,
    /// The owning address. HD wallets watch several addresses of one seed and
    /// the channel carries their UNION — signing derives each input's key from
    /// this tag, so an untagged coin cannot be represented, let alone signed.
    pub address: String,
    /// 0 = mempool-created, unconfirmed. Coin selection excludes FOREIGN
    /// height-0 coins (an RBF'able incoming payment is not ours to spend yet);
    /// our own pending change is spendable — the sender-side check in
    /// `bitcoin_payment::eligible_utxos` tells the two apart.
    pub height: u64,
    /// The mempool transaction spending this coin, when one does. The coin
    /// stays in the set until a block spends it — the chain still holds it, so
    /// the confirmed figure counts it — and nothing offers it to a signer. Set
    /// by the relay's push and the whole-wallet fetch (`spent` beside `utxos`),
    /// and by our own dispatch moments before the push arrives.
    pub spent_by: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BtcTransactionState {
    pub transactions: HashMap<String, BtcTransactionData>,
    /// The `transactions` pane's paging. A pending row is never paged: the
    /// startup reply carries every one of them.
    pub page: super::history::HistoryPage,
}

impl BtcTransactionState {
    /// Settled records this client holds — the `offset` a page asks from,
    /// and what the end row compares against the server's total. Records,
    /// not the rows drawn: a replaced original folds into its replacement on
    /// screen, and the server counts records.
    pub fn held(&self) -> usize {
        self.transactions
            .values()
            .filter(|t| t.status != BitcoinTransactionStatus::Pending)
            .count()
    }

    /// A reply with rows landed: merge them (a repeat rewrites itself) and
    /// take the facts. `page` says whether this reply answers the page
    /// request; the startup reply does not.
    pub fn apply_reply(&mut self, rows: Vec<BtcTransactionData>, reply: &serde_json::Value, page: bool) {
        for tx in rows {
            self.transactions.insert(tx.txid.clone(), tx);
        }
        self.page.facts(super::history::HistoryList::BtcTransactions, reply);
        if page {
            self.page.done();
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Deserialize)]
pub struct BtcTransactionData {
    pub txid: String,                     // Transaction ID (txid)
    pub status: BitcoinTransactionStatus, // Pending, Success, Failed, or Cancelled
    /// Direction-aware amount in **BTC**, formatted to 8 dp — relay divides by
    /// 1e8 before sending (`bitcoin_handler/events.rs`). Sending, it sums the
    /// outputs that are NOT ours (so change is excluded); receiving, the outputs
    /// that are. NOTE the unit disagrees with `fees` in the same struct.
    pub amount: String,
    /// Fee in **satoshis**.
    ///
    /// Two sources now. Pending, it is `getmempoolentry.fees.base × 1e8`, which
    /// misses — and is written `"0"` — for a transaction first seen at block
    /// connect. Confirmed, it is the block's own per-transaction `fee`, which
    /// is measured and always present inside the prune window. So a `"0"` here
    /// still means "unknown" rather than "free", but it is now confined to
    /// pending rows and repaired the moment the transaction confirms.
    pub fees: String,
    pub receiver_addresses: Vec<String>, // List of recipient addresses
    /// Every input address. This is the ONLY reliable direction signal:
    /// `receiver_addresses` excludes our own address, so on an incoming
    /// transaction we are absent from it.
    pub sender_addresses: Vec<String>,
    /// **First seen**, epoch seconds as a decimal string — NOT ISO 8601, and
    /// NOT the confirmation time. It is the relay's own wall clock at mempool
    /// observation and confirmation deliberately does not move it: how long a
    /// transaction waited is a fact worth keeping, and a block cannot
    /// reconstruct it. A transaction the mempool never surfaced has no earlier
    /// stamp to keep, so this equals [`Self::confirmed_at`] for those.
    pub timestamp: String,
    /// The carrying block's own time, epoch seconds, once a block carried it.
    ///
    /// Separate from `timestamp` because they answer different questions and a
    /// transaction that sat three hours at a low fee has two very different
    /// honest times. A confirmed row must show the one it settled at.
    #[serde(default)]
    pub confirmed_at: Option<String>,
    /// Epoch seconds bitcoind reported it gone from the mempool without being
    /// mined. Set only on a [`BitcoinTransactionStatus::Dropped`] record, and
    /// it is what such a row sorts and reads by: a drop resolves a transaction
    /// exactly as a block does, so it files by when it FAILED rather than by
    /// when it was sent.
    #[serde(default)]
    pub dropped_at: Option<String>,
    /// Height of the block that carried it. `None` while pending — never `0`,
    /// which would read as a real height.
    ///
    /// Confirmations are NOT carried: they are `tip - height + 1` against the
    /// node frame the dashboard already holds, and a count on the wire would
    /// be stale one block after it was sent.
    #[serde(default)]
    pub block_height: Option<String>,
    /// The txid of the fee bump that superseded this one. Present exactly on a
    /// [`BitcoinTransactionStatus::Replaced`] record.
    #[serde(default)]
    pub replaced_by: Option<String>,
    /// The transaction's own body, from indexd's mempool frame via the relay's
    /// record (or from the signed transaction itself, for a send this client
    /// just made): outpoints with values and owners, outputs, measured vsize.
    /// Present on every pending row written since 2026-09-06; a fee bump
    /// rebuilds the replacement from these alone. Empty on an older row,
    /// which therefore cannot be bumped.
    #[serde(default)]
    pub inputs: Vec<BtcRbfInput>,
    #[serde(default)]
    pub outputs: Vec<BtcRbfOutput>,
    #[serde(default)]
    pub vsize: Option<u64>,
}

/// Bumped the instant a signed Bitcoin transaction is handed to the outgoing
/// channel. The send screen tears its composition down on the change.
///
/// **There is no counterpart for failure, and that is the design.** Everything
/// that can go wrong before this point goes wrong *locally* — argon2id runs
/// offline on a blocking thread, ~0.4s, deliberately ordered ahead of the
/// network, and the build reads coins from a watch channel — so a send that
/// does not reach here never left the device. Nothing is in flight, nothing
/// needs undoing, and the screen has nothing to do but stay where it is. The
/// activity log the bridge already opened says why.
///
/// It is bumped BEFORE the send rather than after: once the bytes are queued,
/// whether they landed is not knowable here, and a send that *might* have
/// landed has to be treated exactly like one that did.
pub type BtcSendDispatches = u64;
