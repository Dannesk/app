// src/ws/mod.rs
pub mod commands;
pub mod config;
pub mod rates;
pub mod relay;
pub mod socket;

pub use socket::run_websocket;

use tokio::sync::mpsc;
use crate::channel::WSCommand;
use std::sync::OnceLock;
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

// Channels for communicating with the socket task
pub static CRYPTO_COMMANDS_TX: OnceLock<mpsc::Sender<WSCommand>> = OnceLock::new();

// Outgoing relay payloads (bare JSON text; the socket task wraps and tags them)
pub static CRYPTO_OUTGOING_TX: OnceLock<mpsc::Sender<Message>> = OnceLock::new();

// One socket, one shutdown.
pub static WS_SHUTDOWN_TX: OnceLock<mpsc::Sender<()>> = OnceLock::new();

/// What the app asks rates and bookd for. Books are opt-in per connection —
/// only a client with the trade screen open has any use for them, and they are
/// the only heavy stream — so the trade flow says which pair it is showing and
/// the socket task keeps bookd told, across link drops too. The `history` ask
/// to rates is not a command here: it is what opens the rates stream, and the
/// socket task sends it itself on every connection once a wallet exists.
#[derive(Debug, Clone)]
pub enum RatesCommand {
    /// Start receiving one book, by its server key ("XRP/RLUSD").
    SubscribeBook(String),
    UnsubscribeBook(String),
}

pub static RATES_COMMANDS_TX: OnceLock<mpsc::Sender<RatesCommand>> = OnceLock::new();

/// Hand a command to the socket task without waiting. Dropped silently if the
/// task isn't up yet or its queue is full — every command here is idempotent
/// state the task re-derives on its next chance, never a one-off the flow
/// depends on.
pub fn rates_send(cmd: RatesCommand) {
    if let Some(tx) = RATES_COMMANDS_TX.get() {
        let _ = tx.try_send(cmd);
    }
}

/// The app's start-up trace, registered only by a `startup-trace` build of the
/// desktop app; everywhere else it stays unset and [`trace`] does nothing.
pub static TRACE: OnceLock<fn(&'static str)> = OnceLock::new();

/// Records one stage of the connect in the app's start-up trace, if one runs.
pub(crate) fn trace(stage: &'static str) {
    if let Some(stamp) = TRACE.get() {
        stamp(stage);
    }
}

/// Parse a `{"type":"node_stats"}` frame into the shared BTC node channel.
///
/// Absent and `null` are the SAME statement here — "indexd could not measure
/// this" — so every field goes through `as_f64()`/`as_u64()` and simply lands
/// as `None` either way. Nothing is defaulted to zero: a fee tier of 0.0 would
/// render as a real, free-looking price, and a sync meter of 0.0 as a node
/// that is catastrophically behind rather than one we haven't heard from.
///
/// `tiers` is all-or-nothing on purpose. A partial array would let the priority
/// table show three real numbers beside an invented one, and the user cannot
/// tell which is which.
pub fn apply_node_stats_frame(data: &Value) {
    let f32_of = |k: &str| data.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);

    let tiers = data.get("tiers").and_then(|v| v.as_array()).and_then(|a| {
        if a.len() != 4 {
            return None;
        }
        let mut out = [0f32; 4];
        for (i, v) in a.iter().enumerate() {
            out[i] = v.as_f64()? as f32;
        }
        Some(out)
    });

    // The train: a list, read in order into the fixed slots; a slot past the
    // list's end — or a malformed entry — stays `None` and the pane skips it.
    let mut blocks = [None; crate::channel::BLOCK_TRAIN];
    if let Some(arr) = data.get("blocks").and_then(|v| v.as_array()) {
        for (slot, b) in blocks.iter_mut().zip(arr.iter()) {
            *slot = (|| {
                Some(crate::channel::BtcBlock {
                    height: b.get("height")?.as_u64()?,
                    at: b.get("at")?.as_u64()?,
                    txs: b.get("txs")?.as_u64()?,
                    weight: b.get("weight")?.as_u64()?,
                    feerate: b.get("feerate")?.as_f64()? as f32,
                })
            })();
        }
    }

    // The histogram: exactly `BANDS` well-formed entries or nothing.
    let bands = data.get("bands").and_then(|v| v.as_array()).and_then(|arr| {
        if arr.len() != crate::channel::BANDS {
            return None;
        }
        let mut out = [crate::channel::BtcBand { from: 0.0, vsize: 0 }; crate::channel::BANDS];
        for (slot, b) in out.iter_mut().zip(arr.iter()) {
            *slot = crate::channel::BtcBand {
                from: b.get("from")?.as_f64()? as f32,
                vsize: b.get("vsize")?.as_u64()?,
            };
        }
        Some(out)
    });

    let stats = crate::channel::BtcNodeStats {
        sync: f32_of("sync"),
        peers: data.get("peers").and_then(|v| v.as_u64()).map(|v| v as u32),
        tip_height: data.get("tip_height").and_then(|v| v.as_u64()),
        tip_at: data.get("tip_at").and_then(|v| v.as_u64()),
        avg_interval: f32_of("avg_interval"),
        tiers,
        fee_scale_top: f32_of("fee_scale_top"),
        incremental_sat_vb: f32_of("incremental_sat_vb"),
        mempool_txs: data.get("mempool_txs").and_then(|v| v.as_u64()),
        next_block_txs: data.get("next_block_txs").and_then(|v| v.as_u64()),
        pending_vsize: data.get("pending_vsize").and_then(|v| v.as_u64()),
        bands,
        blocks,
        node_height: data.get("node_height").and_then(|v| v.as_u64()),
        headers: data.get("headers").and_then(|v| v.as_u64()),
        behind_secs: data.get("behind_secs").and_then(|v| v.as_u64()),
        tip_time: data.get("tip_time").and_then(|v| v.as_u64()),
        epoch_start_time: data.get("epoch_start_time").and_then(|v| v.as_u64()),
    };

    // `send_if_modified` rather than `send`: the frame is re-sent on every
    // reconnect and on each client's first tick, and an unchanged value waking
    // the whole view tree is work for nothing.
    crate::channel::CHANNEL.btc_node_tx.send_if_modified(|cur| {
        if *cur == stats {
            false
        } else {
            *cur = stats;
            true
        }
    });
}

/// Parse a `{"type":"xrp_node_stats"}` frame into the shared XRP node channel.
/// Same rules as [`apply_node_stats_frame`]: absent and `null` both land as
/// `None`, nothing defaults to zero, publish only on change.
pub fn apply_xrp_node_stats_frame(data: &Value) {
    let u64_of = |k: &str| data.get(k).and_then(|v| v.as_u64());
    let stats = crate::channel::XrpNodeStats {
        ledger_index: u64_of("ledger_index"),
        fee_base: u64_of("fee_base"),
        open_ledger_fee: u64_of("open_ledger_fee"),
        reserve_base: u64_of("reserve_base"),
        reserve_inc: u64_of("reserve_inc"),
        txn_count: u64_of("txn_count"),
        peers: u64_of("peers"),
        synced: data.get("synced").and_then(|v| v.as_bool()),
        state: data.get("state").and_then(|v| v.as_str()).map(str::to_string),
    };
    crate::channel::CHANNEL.xrp_node_tx.send_if_modified(|cur| {
        if *cur == stats {
            false
        } else {
            *cur = stats;
            true
        }
    });
}

/// Parse a `{"type":"status"}` frame into the shared health map.
///
/// ONE parser, deliberately: relay, rates and bookd all emit this exact frame,
/// and that agreement is the whole contract. Components are namespaced by the
/// server that sends them, so merging all of them into one map is safe.
/// `stale_assets` is optional: only the rates server has a derivation graph,
/// and its absence means "this frame says nothing about staleness", never
/// "nothing is stale".
///
/// The proxy's own `link:*` frame has the same shape but is NOT routed here —
/// the socket task reads it itself and folds it into the transport bools.
pub fn apply_status_frame(data: &Value) {
    let Some(components) = data.get("components").and_then(|v| v.as_object()) else {
        return;
    };

    let parsed = components
        .iter()
        .map(|(key, c)| {
            (
                key.clone(),
                crate::channel::ComponentHealth {
                    up: c.get("up").and_then(|v| v.as_bool()).unwrap_or(false),
                    last_data_ms: c.get("last_data_ms").and_then(|v| v.as_u64()).unwrap_or(0),
                },
            )
        })
        .collect();

    let stale = data.get("stale_assets").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    });

    crate::channel::CHANNEL.apply_health(parsed, stale);
}
