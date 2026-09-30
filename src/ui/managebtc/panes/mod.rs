//! BTC's pane faces beyond the shared chrome (2026-09-10): the `wallet`
//! pane and its `restore key` face, the `transactions` pane with its
//! `modify fee` face, and the two read-only instruments of the second
//! handoff — `mempool` and `block intervals`. The old screens stay parked
//! in `managebtc/mod.rs` until the panes are user-tested.
//!
//! Every face draws an existing flow through the same state, messages and
//! controller paths that flow uses; what a pane adds is layout. The rows,
//! buttons, scroller and sign block are `components::grid`'s, shared with
//! XRP's panes — same words on both chains.

pub mod intervals;
pub mod mempool;
pub mod transactions;
pub mod wallet;
