//! The pane grid behind the XRP dashboard (`ui/managexrp/xrpdashboard.rs`) —
//! phase 1 of the pane system, 2026-09-09.
//!
//! Chain-agnostic on purpose: BTC gets the same skeleton on its own handoff,
//! so nothing here knows what a pane draws. What lives here is the MODEL —
//! which panes are open and how the window is split between them — and the
//! rules the handoff fixes:
//!
//! - **The grid is a binary split tree, not a cell grid.** Every region has
//!   exactly one owner, so there is no representable state with a hole in it.
//! - **Closing a pane collapses its split**: the sibling subtree inherits the
//!   whole rectangle. iced's `State::close` does exactly that; the one case it
//!   will not do — the last pane — becomes the **Empty branch** (`panes:
//!   None`), which the view draws as the one `+` in the design.
//! - **Which neighbour grows is the tree, not visual adjacency**, which reads
//!   correctly only because every split is born from splitting an existing
//!   pane. There is no other way to make one.
//! - **Per-pane UI state is keyed by [`PaneKind`], never by `Pane`.** Presets
//!   and reset rebuild the tree from a configuration, which mints new
//!   handles; anything keyed by handle would be silently lost.
//!
//! There is no undo (removed 2026-09-12, user). The stack it kept covered
//! close, preset and reset only, so `ctrl+z` after the add, drag, resize or
//! split people actually reach for it after did nothing at all. `Reset to
//! default` is the way back.
//!
//! Sizes are LOGICAL pixels before the app's own `scale()`; every threshold
//! here is multiplied by it at the call site, the way every screen does.

use std::time::{Duration, Instant};

use iced::widget::pane_grid::{self, Axis, Configuration, DragEvent, Node, Pane, ResizeEvent, Split, Target};
use serde_json::{json, Value};
use iced::{Size, Task};

use crate::channel::CHANNEL;
use crate::controller::app_state::{AppState, BtcView, ChartPeriod, Tab, XrpView};
use crate::controller::message::Message;

/// Which chain a grid belongs to (2026-09-10, when BTC got its own). The
/// tree logic is chain-blind; the kind lists, the default layout, the
/// settings key and the message a view fires are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chain {
    Xrp,
    Btc,
}

// ── Geometry ────────────────────────────────────────────────────────────────

/// The hairline between panes. The grid's background shows through it.
pub const SPACING: f32 = 1.0;
/// Grab room either side of a hairline: iced's pick region is `spacing +
/// leeway`, centred, so this is the handoff's 7px hit area on a 1px line.
pub const LEEWAY: f32 = 6.0;
/// A pane may not resize below this — the title strip plus one row.
pub const MIN_W: f32 = 200.0;
pub const MIN_H: f32 = 88.0;
/// The top bar's height, which the view draws and the grid size subtracts.
pub const BAR_H: f32 = 34.0;
/// The dock's height as `ui/dashboard.rs` builds it: 2px bar + 6 + an 11px
/// label at line height 1.3 + 8. Approximate by a pixel; nothing here needs
/// better than that.
const DOCK_H: f32 = 30.3;

/// The grid's own size — the window less the chrome above and below it.
///
/// Needed in `update`, where no layout exists: the resize floor and "split
/// the largest pane" both need pixels, and the window size is the one
/// measurement the app already tracks. A hairline off is fine for a 200px
/// floor.
pub fn grid_size(state: &AppState) -> Size {
    let s = state.scale();
    let chrome = BAR_H * s + 1.0 + DOCK_H * s + 1.0;
    Size::new(state.window_width.max(1.0), (state.window_height - chrome).max(1.0))
}

// ── Kinds ───────────────────────────────────────────────────────────────────

/// What a pane draws. `Empty` is the well a split leaves behind — a `+` that
/// opens the panels menu, filled by the next panel checked, closable like any
/// other pane. It is never in the menu and never in the canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneKind {
    Balance,
    Chart,
    Send,
    Receive,
    Network,
    // Phase 2 (design handoff `design_handoff_xrp_panes`, 2026-09-09): the
    // rest of the XRP screens as panes — content in boxes that already work.
    Wallet,
    Ticket,
    Book,
    Depth,
    Transactions,
    Orders,
    Tokens,
    AvailableTokens,
    // BTC (design handoff `design_handoff_btc_dashboard`, 2026-09-10): the
    // fee ladder and the block train. `Balance`, `Chart`, `Send`, `Wallet`
    // and `Receive` are the same kinds drawn by the other chain's dashboard.
    Fees,
    Blocks,
    // BTC, the second handoff (`btc-panes`, 2026-09-10): the mempool by fee
    // band and the last twelve block gaps. Neither is in the default.
    Mempool,
    Intervals,
    Empty,
}

/// Clear the form a pane carries, if it carries one.
///
/// **Why this exists (2026-09-12).** Before the grid, every one of these forms
/// lived on its own screen and `Back` cleared it on the way out. The panes
/// migration dropped that: a form's buffers live in [`AppState`], the pane
/// lives in the tree, and closing the pane only edited the tree — so a pasted
/// 24-word mnemonic outlived the pane it was typed into, with no way to reach
/// it again except to reopen the pane and backspace the phrase out by hand.
///
/// It clears **buffers only**. Nothing here removes a key, a wallet or a watch
/// channel — closing a pane is a view action, and the wallet operations are
/// the deliberate, confirmed paths they have always been.
fn clear_pane_form(state: &mut AppState, chain: Chain, kind: PaneKind) {
    use crate::controller::{btc, xrp};
    match (chain, kind) {
        (Chain::Xrp, PaneKind::Send) => xrp::clear_send_form(state),
        // The receive pane's buffers are its tag form; the tag itself stays.
        (Chain::Xrp, PaneKind::Receive) => xrp::clear_receive_form(state),
        // `clear_trade_inputs`, NOT `clear_trade_form`: the latter also drops
        // the pair and fires `UnsubscribeBook`. The book is gated by the
        // wallet, not by the pair or the pane, so closing the ticket must not
        // move a subscription (user, 2026-09-12).
        (Chain::Xrp, PaneKind::Ticket) => xrp::clear_trade_inputs(state),
        // The offer-cancel stack lives inside `orders`.
        (Chain::Xrp, PaneKind::Orders) => xrp::clear_cancel_form(state),
        // The TrustSet stack is reached from `available tokens`.
        (Chain::Xrp, PaneKind::AvailableTokens) => xrp::clear_enable_form(state),
        // Its mirror, the limit-0 TrustSet, from the held `tokens` pane.
        (Chain::Xrp, PaneKind::Tokens) => {
            xrp::clear_disable_form(state);
            state.disable_token = None;
        }
        (Chain::Xrp, PaneKind::Wallet) => xrp::clear_reimport_form(state),
        (Chain::Btc, PaneKind::Send) => btc::clear_btc_send_form(state),
        // The RBF fee bump lives inside BTC's `transactions`.
        (Chain::Btc, PaneKind::Transactions) => btc::clear_btc_bump_form(state),
        (Chain::Btc, PaneKind::Wallet) => btc::clear_btc_reimport_form(state),
        _ => {}
    }
}

/// Every kind that carries a form on this chain — what `reset` empties.
fn form_kinds(chain: Chain) -> &'static [PaneKind] {
    match chain {
        Chain::Xrp => &[
            PaneKind::Send,
            PaneKind::Receive,
            PaneKind::Ticket,
            PaneKind::Orders,
            PaneKind::AvailableTokens,
            PaneKind::Tokens,
            PaneKind::Wallet,
        ],
        Chain::Btc => &[PaneKind::Send, PaneKind::Transactions, PaneKind::Wallet],
    }
}

/// The form-bearing kinds on this chain's screen right now. Only these matter
/// to [`clear_forms_of_removed`], so it walks [`form_kinds`] rather than the
/// tree — a `chart` coming and going has no buffers to answer for.
fn present_kinds(state: &mut AppState, chain: Chain) -> Vec<PaneKind> {
    let g = grid_mut(state, chain);
    form_kinds(chain).iter().copied().filter(|k| g.has(*k)).collect()
}

/// Clear the forms of every pane that a tree change just removed.
///
/// One rule for every way a pane can leave — the close `×`, a `panels +`
/// toggle, a preset swap — rather than three call sites that a fourth removal
/// path would quietly not be added to. `after` is read from the tree once the
/// change has landed.
fn clear_forms_of_removed(state: &mut AppState, chain: Chain, before: Vec<PaneKind>) {
    let gone: Vec<PaneKind> = before
        .into_iter()
        .filter(|k| !grid_mut(state, chain).has(*k))
        .collect();
    for kind in gone {
        clear_pane_form(state, chain, kind);
    }
}

impl PaneKind {
    /// The kinds a chain's menu lists, in order. A saved layout may hold
    /// only these — `fees` never opens on XRP, `network` never on BTC.
    pub fn canonical(chain: Chain) -> &'static [PaneKind] {
        match chain {
            Chain::Xrp => &Self::CANONICAL,
            Chain::Btc => &Self::BTC_CANONICAL,
        }
    }

    /// BTC's menu: the seven of the default layout, then the panes the user
    /// adds — `transactions` (with the fee bump inside it), `mempool`,
    /// `block intervals`.
    pub const BTC_CANONICAL: [PaneKind; 10] = [
        PaneKind::Balance,
        PaneKind::Fees,
        PaneKind::Chart,
        PaneKind::Send,
        PaneKind::Wallet,
        PaneKind::Receive,
        PaneKind::Blocks,
        PaneKind::Transactions,
        PaneKind::Mempool,
        PaneKind::Intervals,
    ];

    /// The seven BTC's default layout opens with, in tree order.
    pub const BTC_DEFAULT: [PaneKind; 7] = [
        PaneKind::Balance,
        PaneKind::Fees,
        PaneKind::Chart,
        PaneKind::Send,
        PaneKind::Wallet,
        PaneKind::Receive,
        PaneKind::Blocks,
    ];

    /// The order presets deal panes into columns, and the order the menu
    /// lists them. The default five first, then phase 2's eight in the
    /// handoff's build order.
    pub const CANONICAL: [PaneKind; 13] = [
        PaneKind::Balance,
        PaneKind::Chart,
        PaneKind::Send,
        PaneKind::Receive,
        PaneKind::Network,
        PaneKind::Wallet,
        PaneKind::Ticket,
        PaneKind::Book,
        PaneKind::Depth,
        PaneKind::Transactions,
        PaneKind::Orders,
        PaneKind::Tokens,
        PaneKind::AvailableTokens,
    ];

    /// The six the default layout opens with — what `Reset to default`
    /// restores and what the first test pins. Tree order, not menu order.
    pub const DEFAULT: [PaneKind; 6] = [
        PaneKind::Balance,
        PaneKind::Chart,
        PaneKind::Send,
        PaneKind::Wallet,
        PaneKind::Receive,
        PaneKind::Network,
    ];

    /// The title-strip word (lowercase source text; the strip upper-cases).
    pub fn label(self) -> &'static str {
        match self {
            PaneKind::Balance => "balance",
            PaneKind::Chart => "chart",
            PaneKind::Send => "send",
            PaneKind::Receive => "receive",
            PaneKind::Network => "network",
            PaneKind::Wallet => "wallet",
            PaneKind::Ticket => "ticket",
            PaneKind::Book => "order book",
            PaneKind::Depth => "depth",
            PaneKind::Transactions => "transactions",
            PaneKind::Orders => "orders",
            PaneKind::Tokens => "tokens",
            PaneKind::AvailableTokens => "available tokens",
            PaneKind::Fees => "fees",
            PaneKind::Blocks => "blocks",
            PaneKind::Mempool => "mempool",
            PaneKind::Intervals => "block intervals",
            PaneKind::Empty => "",
        }
    }

    /// The name a pane is saved under. Stable on purpose — a strip word or a
    /// menu row can be reworded, this cannot, or every saved layout that
    /// holds the pane falls back to the default.
    pub fn key(self) -> &'static str {
        match self {
            PaneKind::Balance => "balance",
            PaneKind::Chart => "chart",
            PaneKind::Send => "send",
            PaneKind::Receive => "receive",
            PaneKind::Network => "network",
            PaneKind::Wallet => "wallet",
            PaneKind::Ticket => "ticket",
            PaneKind::Book => "book",
            PaneKind::Depth => "depth",
            PaneKind::Transactions => "transactions",
            PaneKind::Orders => "orders",
            PaneKind::Tokens => "tokens",
            PaneKind::AvailableTokens => "available_tokens",
            PaneKind::Fees => "fees",
            PaneKind::Blocks => "blocks",
            PaneKind::Mempool => "mempool",
            PaneKind::Intervals => "intervals",
            PaneKind::Empty => "empty",
        }
    }

    /// The saved name back to a kind — one of `chain`'s own, or the well.
    pub fn from_key(chain: Chain, key: &str) -> Option<Self> {
        Self::canonical(chain)
            .iter()
            .copied()
            .chain(std::iter::once(PaneKind::Empty))
            .find(|k| k.key() == key)
    }

    /// The menu row.
    pub fn menu_label(self) -> &'static str {
        match self {
            PaneKind::Balance => "Balance",
            PaneKind::Chart => "Chart",
            PaneKind::Send => "Send",
            PaneKind::Receive => "Receive",
            PaneKind::Network => "Network",
            PaneKind::Wallet => "Wallet",
            PaneKind::Ticket => "Ticket",
            PaneKind::Book => "Order book",
            PaneKind::Depth => "Depth",
            PaneKind::Transactions => "Transactions",
            PaneKind::Orders => "Orders",
            PaneKind::Tokens => "Tokens",
            PaneKind::AvailableTokens => "Available tokens",
            PaneKind::Fees => "Fees",
            PaneKind::Blocks => "Blocks",
            PaneKind::Mempool => "Mempool",
            PaneKind::Intervals => "Block intervals",
            PaneKind::Empty => "",
        }
    }
}

// ── The tree ────────────────────────────────────────────────────────────────

/// Our own mirror of the split tree — the undo snapshot, and the shape a
/// preset builds. Serialising THIS rather than the framework's configuration
/// type is what keeps a settings format from moving when the framework does
/// (persistence is the next pass; the type is ready for it).
#[derive(Debug, Clone, PartialEq)]
pub enum PaneTree {
    Split {
        axis: Axis,
        /// The first child's share.
        ratio: f32,
        a: Box<PaneTree>,
        b: Box<PaneTree>,
    },
    Pane(PaneKind),
}

impl PaneTree {
    /// A chain's default layout.
    pub fn default_layout_for(chain: Chain) -> Self {
        match chain {
            Chain::Xrp => Self::default_layout(),
            Chain::Btc => Self::btc_default_layout(),
        }
    }

    /// BTC's default (`design_handoff_btc_dashboard`, 2026-09-10): the same
    /// three columns, the left one THREE deep — `balance` over `fees` over
    /// `chart`, the chart taking more than the other two together because
    /// it is the one with a plot in it; the middle `send` over `wallet` as
    /// on XRP but at 0.71 (the BTC send is taller — user, 2026-09-10: the
    /// wallet pane is in the default here too); the right `receive` over
    /// `blocks`, even.
    ///
    /// The nesting decides who inherits on close: `fees` and `chart` are a
    /// pair split off under `balance`, so closing either hands the other
    /// its height and `balance` keeps its own; closing `balance` hands the
    /// pair the whole column.
    pub fn btc_default_layout() -> Self {
        let leaf = |k| Box::new(PaneTree::Pane(k));
        PaneTree::Split {
            axis: Axis::Vertical,
            ratio: 1.0 / 3.0,
            // Balance grew to hero + four rows (2026-09-16: confirmed /
            // unconfirmed / on master / on other addresses), so it takes
            // 0.30 of the column; fees keeps the height its four rungs had
            // (~0.23 of the column) and the chart gives up the difference —
            // it is the one pane that scales down without losing a row.
            a: Box::new(PaneTree::Split {
                axis: Axis::Horizontal,
                ratio: 0.30,
                a: leaf(PaneKind::Balance),
                b: Box::new(PaneTree::Split {
                    axis: Axis::Horizontal,
                    ratio: 0.33,
                    a: leaf(PaneKind::Fees),
                    b: leaf(PaneKind::Chart),
                }),
            }),
            b: Box::new(PaneTree::Split {
                axis: Axis::Vertical,
                ratio: 0.5,
                a: Box::new(PaneTree::Split {
                    axis: Axis::Horizontal,
                    // 0.71, not XRP's 0.68: the BTC send is a chip row and a
                    // fee line taller, and at 0.68 the wallet covered
                    // `Send payment` by about the button's own height
                    // (user, 2026-09-10). The wallet scrolls if it is short.
                    ratio: 0.71,
                    a: leaf(PaneKind::Send),
                    b: leaf(PaneKind::Wallet),
                }),
                b: Box::new(PaneTree::Split {
                    axis: Axis::Horizontal,
                    ratio: 0.5,
                    a: leaf(PaneKind::Receive),
                    b: leaf(PaneKind::Blocks),
                }),
            }),
        }
    }

    /// The handoff's default: three equal columns, the left one `balance` over
    /// `chart` at 0.398, the right one `receive` over `network` at half — and,
    /// since phase 2 (user, 2026-09-09: "it fits perfectly"), the middle one
    /// `send` over `wallet` at 0.68, the wallet at its content height.
    /// `Axis::Vertical` is a vertical LINE (left/right), the handoff's `V`.
    ///
    /// This exact shape matters — it decides who inherits space on close:
    /// the send column was split off the receive column, so closing both
    /// send and wallet hands that column its width and `balance` keeps its
    /// own; closing one of them hands the other the column's height.
    pub fn default_layout() -> Self {
        let leaf = |k| Box::new(PaneTree::Pane(k));
        PaneTree::Split {
            axis: Axis::Vertical,
            ratio: 1.0 / 3.0,
            a: Box::new(PaneTree::Split {
                axis: Axis::Horizontal,
                ratio: 0.398,
                a: leaf(PaneKind::Balance),
                b: leaf(PaneKind::Chart),
            }),
            b: Box::new(PaneTree::Split {
                axis: Axis::Vertical,
                ratio: 0.5,
                a: Box::new(PaneTree::Split {
                    axis: Axis::Horizontal,
                    ratio: 0.68,
                    a: leaf(PaneKind::Send),
                    b: leaf(PaneKind::Wallet),
                }),
                b: Box::new(PaneTree::Split {
                    axis: Axis::Horizontal,
                    ratio: 0.5,
                    a: leaf(PaneKind::Receive),
                    b: leaf(PaneKind::Network),
                }),
            }),
        }
    }

    fn configuration(&self) -> Configuration<PaneKind> {
        match self {
            PaneTree::Split { axis, ratio, a, b } => Configuration::Split {
                axis: *axis,
                ratio: *ratio,
                a: Box::new(a.configuration()),
                b: Box::new(b.configuration()),
            },
            PaneTree::Pane(kind) => Configuration::Pane(*kind),
        }
    }

    /// A fresh grid state in this shape. Mints new `Pane` handles.
    pub fn build(&self) -> pane_grid::State<PaneKind> {
        pane_grid::State::with_configuration(self.configuration())
    }

    /// The live tree, ratios included, as the framework holds it now.
    pub fn mirror(state: &pane_grid::State<PaneKind>) -> Self {
        Self::from_node(state.layout(), state)
    }

    fn from_node(node: &Node, state: &pane_grid::State<PaneKind>) -> Self {
        match node {
            Node::Split { axis, ratio, a, b, .. } => PaneTree::Split {
                axis: *axis,
                ratio: *ratio,
                a: Box::new(Self::from_node(a, state)),
                b: Box::new(Self::from_node(b, state)),
            },
            Node::Pane(pane) => PaneTree::Pane(state.get(*pane).copied().unwrap_or(PaneKind::Empty)),
        }
    }

    /// The tree as it is saved: `{"axis":"v"|"h","ratio":r,"a":…,"b":…}` for
    /// a split, `{"pane":"balance"}` for a leaf. Pane names are
    /// [`PaneKind::key`]; ratios are the raw ones the tree holds.
    pub fn to_json(&self) -> Value {
        match self {
            PaneTree::Split { axis, ratio, a, b } => json!({
                "axis": match axis { Axis::Vertical => "v", Axis::Horizontal => "h" },
                "ratio": ratio,
                "a": a.to_json(),
                "b": b.to_json(),
            }),
            PaneTree::Pane(k) => json!({ "pane": k.key() }),
        }
    }

    /// The inverse. `None` on anything it does not understand — an unknown
    /// pane name (or another chain's), a ratio outside (0, 1), a missing
    /// child — and the caller falls back to the default layout rather than
    /// guessing at a tree.
    pub fn from_json(chain: Chain, v: &Value) -> Option<Self> {
        if let Some(name) = v.get("pane").and_then(Value::as_str) {
            return PaneKind::from_key(chain, name).map(PaneTree::Pane);
        }
        let axis = match v.get("axis").and_then(Value::as_str)? {
            "v" => Axis::Vertical,
            "h" => Axis::Horizontal,
            _ => return None,
        };
        let ratio = v.get("ratio").and_then(Value::as_f64)? as f32;
        if !(ratio > 0.0 && ratio < 1.0) {
            return None;
        }
        let a = Self::from_json(chain, v.get("a")?)?;
        let b = Self::from_json(chain, v.get("b")?)?;
        Some(PaneTree::Split { axis, ratio, a: Box::new(a), b: Box::new(b) })
    }

    /// The panes in tree order (left/top first).
    pub fn kinds(&self) -> Vec<PaneKind> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<PaneKind>) {
        match self {
            PaneTree::Split { a, b, .. } => {
                a.collect(out);
                b.collect(out);
            }
            PaneTree::Pane(k) => out.push(*k),
        }
    }

    /// A column preset: `kinds` dealt down `columns` columns, balanced — the
    /// first `len % columns` columns take one more. Equal column widths, equal
    /// row heights. `None` when the preset cannot be satisfied (fewer panes
    /// than columns, or none at all); the menu draws those faint.
    pub fn preset(kinds: &[PaneKind], columns: usize) -> Option<Self> {
        if kinds.is_empty() || columns == 0 || kinds.len() < columns {
            return None;
        }
        let base = kinds.len() / columns;
        let extra = kinds.len() % columns;
        let mut cols = Vec::with_capacity(columns);
        let mut at = 0;
        for i in 0..columns {
            let n = base + usize::from(i < extra);
            cols.push(Self::stack(&kinds[at..at + n]));
            at += n;
        }
        Some(Self::join(cols, Axis::Vertical))
    }

    /// One column: the panes stacked top to bottom, equal heights.
    fn stack(kinds: &[PaneKind]) -> Self {
        let panes: Vec<Self> = kinds.iter().map(|k| PaneTree::Pane(*k)).collect();
        Self::join(panes, Axis::Horizontal)
    }

    /// `parts` laid along `axis` with equal shares: the first takes `1/n`,
    /// the rest recurse. `parts` must not be empty.
    fn join(mut parts: Vec<Self>, axis: Axis) -> Self {
        let first = parts.remove(0);
        if parts.is_empty() {
            return first;
        }
        let n = parts.len() + 1;
        PaneTree::Split {
            axis,
            ratio: 1.0 / n as f32,
            a: Box::new(first),
            b: Box::new(Self::join(parts, axis)),
        }
    }
}

// ── Presets ─────────────────────────────────────────────────────────────────

/// The `LAYOUT` section of the menu. Each rebuilds the tree from the open
/// panes in canonical order. `Grid 2 × 2` from the mock is dropped on purpose
/// (the handoff README): it is exactly "two columns with four panes open".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Three,
    Two,
    One,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Three, Preset::Two, Preset::One];

    pub fn columns(self) -> usize {
        match self {
            Preset::Three => 3,
            Preset::Two => 2,
            Preset::One => 1,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::Three => "Three columns",
            Preset::Two => "Two columns",
            Preset::One => "One column",
        }
    }
}

// ── The grid ────────────────────────────────────────────────────────────────

/// One chain's grid. Each chain owns one, so closing `send` on one never
/// closes it on the other.
#[derive(Debug)]
pub struct Grid {
    pub chain: Chain,
    /// `None` is the Empty branch — the last pane was closed. A `pane_grid`
    /// cannot be empty, so this is a different view, not a grid with no panes.
    pub panes: Option<pane_grid::State<PaneKind>>,
    /// Which preset is checked at rest. `None` once the tree was touched by
    /// hand — a resize, a split, a drag, a close.
    pub preset: Option<Preset>,
    pub menu_open: bool,
    /// The chart pane's `1h` / `1d` pill — keyed by kind, so it survives a
    /// rebuild of the tree.
    pub chart_period: ChartPeriod,
    /// When the layout last changed and has not been written since. The
    /// grid is saved once the change has SETTLED ([`SETTLE`]) — a drag stamps
    /// this continuously and produces one write after the pointer is let go;
    /// an untouched layout never writes. Checked on every `Sync`, which
    /// costs one clock comparison.
    pub dirty: Option<Instant>,
}

/// How long a change has to stand before it is written. Two seconds (user,
/// 2026-09-09): longer than any drag-look-approve-close sequence is short,
/// and the only thing at risk is the last two seconds before a crash.
pub const SETTLE: Duration = Duration::from_secs(2);

/// The settings keys the two grids are saved under — one each, so a layout
/// built on one chain never opens on the other.
pub const XRP_KEY: &str = "xrp_grid";
pub const BTC_KEY: &str = "btc_grid";

/// The XRP grid at its default — what the phase-1 tests pin.
impl Default for Grid {
    fn default() -> Self {
        Self::new(Chain::Xrp)
    }
}

impl Grid {
    /// A chain's grid at its default layout.
    pub fn new(chain: Chain) -> Self {
        Self {
            chain,
            panes: Some(PaneTree::default_layout_for(chain).build()),
            preset: Some(Preset::Three),
            menu_open: false,
            chart_period: ChartPeriod::OneDay,
            dirty: None,
        }
    }

    /// The saved layout under `key`, or `None` when there is none or it
    /// does not parse — the caller opens the default and the next settled
    /// change overwrites whatever was there.
    pub fn load(chain: Chain, key: &str) -> Option<Self> {
        let json = crate::bridge::json_storage::read_json::<Value>("settings.json").ok()?;
        Self::from_json(chain, json.get(key)?)
    }

    /// `{"tree": …|null, "chart_period": "1h"|"1d"}`. `null` is the Empty
    /// branch — the user closed every pane, and that is a layout too.
    pub fn to_json(&self) -> Value {
        json!({
            "tree": self.panes.as_ref().map(PaneTree::mirror).map(|t| t.to_json()).unwrap_or(Value::Null),
            "chart_period": match self.chart_period { ChartPeriod::OneHour => "1h", ChartPeriod::OneDay => "1d" },
        })
    }

    pub fn from_json(chain: Chain, v: &Value) -> Option<Self> {
        let tree = v.get("tree")?;
        let panes = if tree.is_null() { None } else { Some(PaneTree::from_json(chain, tree)?.build()) };
        let chart_period = match v.get("chart_period").and_then(Value::as_str) {
            Some("1h") => ChartPeriod::OneHour,
            _ => ChartPeriod::OneDay,
        };
        Some(Self { chain, panes, preset: None, menu_open: false, chart_period, dirty: None })
    }

    /// Something about the saved shape changed.
    fn touch(&mut self) {
        self.dirty = Some(Instant::now());
    }

    /// Write the layout if a change has settled. Returns whether it wrote.
    pub fn flush_if_settled(&mut self, key: &str) -> bool {
        let Some(at) = self.dirty else { return false };
        if at.elapsed() < SETTLE {
            return false;
        }
        let snapshot = self.to_json();
        let _ = crate::bridge::json_storage::update_json::<Value>("settings.json", |json| {
            if let Some(obj) = json.as_object_mut() {
                obj.insert(key.to_string(), snapshot);
            }
        });
        self.dirty = None;
        true
    }

    /// The open panes in canonical order — the menu's checks, and what a
    /// preset deals out. `Empty` wells are not panes.
    pub fn open_kinds(&self) -> Vec<PaneKind> {
        let Some(p) = &self.panes else { return Vec::new() };
        PaneKind::canonical(self.chain)
            .iter()
            .copied()
            .filter(|k| p.iter().any(|(_, x)| x == k))
            .collect()
    }

    pub fn has(&self, kind: PaneKind) -> bool {
        self.find(kind).is_some()
    }

    fn find(&self, kind: PaneKind) -> Option<Pane> {
        self.panes
            .as_ref()?
            .iter()
            .find(|(_, k)| **k == kind)
            .map(|(pane, _)| *pane)
    }

    pub fn preset_available(&self, preset: Preset) -> bool {
        let n = self.open_kinds().len();
        n > 0 && n >= preset.columns()
    }

    /// `✓` in the menu and `×` on the pane are the same action: a pane that
    /// is open closes, one that is not is added.
    pub fn toggle(&mut self, kind: PaneKind, size: Size, scale: f32) {
        match self.find(kind) {
            Some(pane) => self.close_pane(pane),
            None => self.insert(kind, size, scale),
        }
    }

    /// Add a pane. An `Empty` well takes it first; otherwise the LARGEST open
    /// pane is split along its longer axis — predictable, and it never starves
    /// anything: the framework floors every region at the minimum. On the
    /// Empty branch the new pane is the whole grid.
    pub fn insert(&mut self, kind: PaneKind, size: Size, scale: f32) {
        self.preset = None;
        self.touch();
        if self.panes.is_none() {
            self.panes = Some(pane_grid::State::new(kind).0);
            return;
        }
        let Some(p) = self.panes.as_mut() else { return };
        let well = p.iter().find(|(_, k)| **k == PaneKind::Empty).map(|(pane, _)| *pane);
        if let Some(well) = well {
            if let Some(slot) = p.get_mut(well) {
                *slot = kind;
            }
            return;
        }
        // A maximised pane is a view mode; adding to the tree ends it so the
        // new pane is visible where it landed.
        p.restore();
        let regions = p.layout().pane_regions(SPACING, MIN_H * scale, size);
        let largest = regions
            .iter()
            .max_by(|a, b| {
                let aa = a.1.width * a.1.height;
                let bb = b.1.width * b.1.height;
                aa.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(pane, r)| (*pane, r.width >= r.height));
        if let Some((pane, wider)) = largest {
            let axis = if wider { Axis::Vertical } else { Axis::Horizontal };
            let _ = p.split(axis, pane, kind);
        }
    }

    /// Close → collapse. The sibling subtree inherits the rectangle (the
    /// framework's `close`); the last pane closed is the Empty branch.
    pub fn close_pane(&mut self, pane: Pane) {
        let exists = self.panes.as_ref().is_some_and(|p| p.get(pane).is_some());
        if !exists {
            return;
        }
        self.preset = None;
        self.touch();
        let last = self.panes.as_ref().is_some_and(|p| p.len() <= 1);
        if last {
            self.panes = None;
            return;
        }
        if let Some(p) = self.panes.as_mut() {
            let _ = p.close(pane);
        }
    }

    /// `⤢`: a view mode, not a layout change — the tree and its ratios are
    /// untouched, and the glyph flips to restore.
    pub fn toggle_maximize(&mut self, pane: Pane) {
        let Some(p) = self.panes.as_mut() else { return };
        if p.maximized() == Some(pane) {
            p.restore();
        } else if p.get(pane).is_some() {
            p.maximize(pane);
        }
    }

    /// The two title-strip glyphs: the pane halves along `axis` and the new
    /// half is an `Empty` well. Nothing while maximised — there is no
    /// rectangle to halve.
    pub fn split(&mut self, pane: Pane, axis: Axis) {
        let Some(p) = self.panes.as_mut() else { return };
        if p.maximized().is_some() {
            return;
        }
        if p.split(axis, pane, PaneKind::Empty).is_some() {
            self.preset = None;
            self.touch();
        }
    }

    /// A splitter drag. The framework floors both axes at [`MIN_H`] on its
    /// own; the wider [`MIN_W`] floor is ours: a drag that would leave a pane
    /// narrower than it — and narrower than the narrowest pane already was —
    /// is refused, and the split stays where it was.
    pub fn resize(&mut self, ev: ResizeEvent, size: Size, scale: f32) {
        let Some(p) = self.panes.as_mut() else { return };
        let min = MIN_H * scale;
        let floor = MIN_W * scale;
        let narrowest = |node: &Node| {
            node.pane_regions(SPACING, min, size)
                .values()
                .map(|r| r.width)
                .fold(f32::INFINITY, f32::min)
        };
        let before = narrowest(p.layout());
        // The node's own ratio, not `split_regions`' — that one is derived
        // back from whole pixels, so restoring it would move the split by a
        // rounding error on every refused drag.
        let old = raw_ratio(p.layout(), ev.split);
        p.resize(ev.split, ev.ratio);
        let after = narrowest(p.layout());
        if after < floor && after < before {
            if let Some(ratio) = old {
                p.resize(ev.split, ratio);
            }
            return;
        }
        self.preset = None;
        self.touch();
    }

    /// Drop a pane on another to SWAP them. Edges are not targets: a drop
    /// outside every pane cancels, and no drop ever creates a split.
    pub fn drag(&mut self, ev: DragEvent) {
        if let DragEvent::Dropped { pane, target: Target::Pane(target, _) } = ev {
            if let Some(p) = self.panes.as_mut() {
                p.swap(pane, target);
                self.preset = None;
                self.touch();
            }
        }
    }

    pub fn apply_preset(&mut self, preset: Preset) {
        let kinds = self.open_kinds();
        let Some(tree) = PaneTree::preset(&kinds, preset.columns()) else { return };
        self.panes = Some(tree.build());
        self.preset = Some(preset);
        self.touch();
    }

    /// `Reset to default`: the chain's handoff tree and its default panes.
    pub fn reset(&mut self) {
        self.panes = Some(PaneTree::default_layout_for(self.chain).build());
        self.preset = Some(Preset::Three);
        self.touch();
    }

}

/// The ratio a split holds, as the tree stores it.
fn raw_ratio(node: &Node, split: Split) -> Option<f32> {
    match node {
        Node::Split { id, ratio, a, b, .. } => {
            if *id == split {
                Some(*ratio)
            } else {
                raw_ratio(a, split).or_else(|| raw_ratio(b, split))
            }
        }
        Node::Pane(_) => None,
    }
}

// ── The ledger tape ─────────────────────────────────────────────────────────

/// One validated ledger, as the network pane's tape draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerBar {
    pub index: u64,
    pub txns: u64,
    /// The open-ledger fee was above the base fee when this ledger closed —
    /// the same "escalation has begun" edge the fee verdict calls `busy`.
    pub escalated: bool,
}

pub const TAPE_CAP: usize = 128;

/// Fold the current node frame into the tape: one bar per validated ledger,
/// newest last. Display-only, derived from frames this session has seen;
/// the node frame itself carries no history.
pub fn note_ledger(state: &mut AppState) {
    let frame = CHANNEL.xrp_node_rx.borrow().clone();
    let Some(index) = frame.ledger_index else { return };
    if state.ledger_tape.back().is_some_and(|b| b.index >= index) {
        return;
    }
    let escalated = matches!(
        (frame.open_ledger_fee, frame.fee_base),
        (Some(open), Some(base)) if base > 0 && open > base
    );
    state.ledger_tape.push_back(LedgerBar { index, txns: frame.txn_count.unwrap_or(0), escalated });
    while state.ledger_tape.len() > TAPE_CAP {
        state.ledger_tape.pop_front();
    }
}

// ── Messages ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum GridMsg {
    /// `panels +` in the top bar, and the `+` in an empty well.
    MenuToggled,
    /// Outside click, `Esc`, or an item that closes the menu.
    MenuDismissed,
    /// A row in the menu's PANELS checklist.
    Toggle(PaneKind),
    Close(Pane),
    Maximize(Pane),
    Split(Pane, Axis),
    Resized(ResizeEvent),
    Dragged(DragEvent),
    Preset(Preset),
    Reset,
    Period(ChartPeriod),
    /// The send pane's `Send payment`.
    SendSign,
    /// The ticket pane's `Buy XRP` / `Sell XRP`.
    TicketSign,
    /// The pair chip in the top bar: the chip becomes the search field.
    /// There is no way back to no pair — you switch, you do not clear.
    PairSearchOpened,
    /// A click outside the field and its results, or `Esc`: the query
    /// goes and the chip is back.
    PairSearchDismissed,
    /// Enter in the field: the first live row of the results is the pick.
    PairSearchSubmitted,
}

/// Whether `chain`'s grid is the screen the user is looking at — the keyboard
/// shortcuts route here from anywhere and must act nowhere else.
fn on_screen(state: &AppState, chain: Chain) -> bool {
    let tab = match chain {
        Chain::Xrp => state.selected_tab == Tab::Xrp && state.xrp_view == XrpView::Menu,
        Chain::Btc => state.selected_tab == Tab::Btc && state.btc_view == BtcView::Menu,
    };
    tab && state.activity_log.is_none()
}

/// The top bar's pair search closes and forgets its query. Harmless on BTC,
/// which has no pair to search for.
pub fn dismiss_pair_search(state: &mut AppState) {
    state.trade_pair_search_open = false;
    state.trade_pair_query.clear();
}

fn grid_mut(state: &mut AppState, chain: Chain) -> &mut Grid {
    match chain {
        Chain::Xrp => &mut state.xrp_grid,
        Chain::Btc => &mut state.btc_grid,
    }
}

/// `Ctrl+Z` / `Ctrl+0` from the keyboard, which cannot know which grid is
/// up: they act on the grid on screen, and nowhere else.
pub fn handle_shortcut(state: &mut AppState, msg: GridMsg) -> Task<Message> {
    if on_screen(state, Chain::Xrp) {
        handle(state, Chain::Xrp, msg)
    } else if on_screen(state, Chain::Btc) {
        handle(state, Chain::Btc, msg)
    } else {
        Task::none()
    }
}

pub fn handle(state: &mut AppState, chain: Chain, msg: GridMsg) -> Task<Message> {
    let size = grid_size(state);
    let scale = state.scale();
    match msg {
        GridMsg::MenuToggled => {
            // One overlay at a time: the pair search folds when the menu
            // opens over it.
            dismiss_pair_search(state);
            let g = grid_mut(state, chain);
            g.menu_open = !g.menu_open;
        }
        GridMsg::MenuDismissed => grid_mut(state, chain).menu_open = false,
        // The menu stays open across toggles, so three panels cost three
        // clicks, not six.
        // Toggling a panel off is a close by another control, so it drops the
        // form the same way the `×` does.
        GridMsg::Toggle(kind) => {
            let before = present_kinds(state, chain);
            grid_mut(state, chain).toggle(kind, size, scale);
            clear_forms_of_removed(state, chain, before);
        }
        GridMsg::Close(pane) => {
            let before = present_kinds(state, chain);
            grid_mut(state, chain).close_pane(pane);
            clear_forms_of_removed(state, chain, before);
        }
        GridMsg::Maximize(pane) => grid_mut(state, chain).toggle_maximize(pane),
        GridMsg::Split(pane, axis) => grid_mut(state, chain).split(pane, axis),
        GridMsg::Resized(ev) => grid_mut(state, chain).resize(ev, size, scale),
        GridMsg::Dragged(ev) => grid_mut(state, chain).drag(ev),
        GridMsg::Preset(preset) => {
            let before = present_kinds(state, chain);
            let g = grid_mut(state, chain);
            g.apply_preset(preset);
            g.menu_open = false;
            clear_forms_of_removed(state, chain, before);
        }
        GridMsg::Reset => {
            if on_screen(state, chain) {
                let g = grid_mut(state, chain);
                g.reset();
                g.menu_open = false;
                // Reset empties EVERY form on the chain, not just the ones
                // whose panes went away (user, 2026-09-12). "As installed"
                // is a statement about the whole screen, and a phrase left
                // sitting in a pane that happens to be in the default layout
                // would be the one thing reset did not put back.
                for kind in form_kinds(chain) {
                    clear_pane_form(state, chain, *kind);
                }
                // Reset means "as installed": on XRP the pair goes with the
                // layout, on disk too — the one way back to no pair (user,
                // 2026-09-11; the chip has no `×` and the results no `none`
                // row on purpose).
                if chain == Chain::Xrp {
                    crate::controller::xrp::clear_trade_pair(state);
                }
            }
        }
        GridMsg::Period(period) => {
            let g = grid_mut(state, chain);
            g.chart_period = period;
            g.touch();
        }
        GridMsg::SendSign => match chain {
            Chain::Xrp => {
                // The pane's one button is send v3's two presses: the SAME
                // validation `Sign transaction` runs (it re-checks every fact
                // the CTA lit on), then — only if it passed — the SAME submit
                // the stack's CTA fires. The pane is always the compose step,
                // so the step counter is set to what that handler expects;
                // the submit clears the form and resets it. Nothing in the
                // signer changes.
                state.send_step = 1;
                let checked = crate::controller::xrp::handle(state, Message::SendContinueClicked);
                if state.send_step == 2 {
                    return crate::controller::xrp::handle(state, Message::SendSubmitClicked);
                }
                return checked;
            }
            Chain::Btc => {
                // The BTC twin, to the letter: the send screen's Continue
                // (address, amount, fee against the live floor, amount plus
                // fee against the eligible coins) and then its submit. The
                // planner, the signer and the dispatch are untouched.
                state.btc_send_step = 1;
                let checked = crate::controller::btc::handle(state, Message::BtcSendContinueClicked);
                if state.btc_send_step == 2 {
                    return crate::controller::btc::handle(state, Message::BtcSendSubmitClicked);
                }
                return checked;
            }
        },
        GridMsg::TicketSign => {
            if chain != Chain::Xrp {
                return Task::none();
            }
            // Same shape as SendSign: the ticket's Continue re-checks every
            // fact the button lit on and moves to step 2 only if it passed;
            // then the stack's own submit fires. The pane is always the
            // ticket, so the step is pinned at 1 first.
            state.trade_step = 1;
            let checked = crate::controller::xrp::handle(state, Message::TradeContinueClicked);
            if state.trade_step == 2 {
                let sent = crate::controller::xrp::handle(state, Message::TradeSubmitClicked);
                // Whatever submit did, the pane is the ticket again — a
                // failed gate must not leave the step parked at 2.
                state.trade_step = 1;
                return sent;
            }
            return checked;
        }
        GridMsg::PairSearchOpened => {
            if chain != Chain::Xrp {
                return Task::none();
            }
            state.trade_pair_search_open = true;
            state.trade_pair_query.clear();
        }
        GridMsg::PairSearchDismissed => dismiss_pair_search(state),
        GridMsg::PairSearchSubmitted => {
            if chain != Chain::Xrp {
                return Task::none();
            }
            // The first row Enter would land on: the listing's order, the
            // same rows the panel draws. No book is skipped for its depth —
            // every pair is pickable; depth gates Market on the ticket.
            let query = state.trade_pair_query.clone();
            let (rows, _) = crate::ui::managexrp::xrptrade::picker::listing(&query);
            if let Some((b, q)) = rows.into_iter().next() {
                return crate::controller::xrp::handle(state, Message::TradePairSelected(b.to_string(), q.to_string()));
            }
        }
    }
    // The ticket pane is always on its compose step; the books follow the
    // pair it holds. XRP's business alone.
    if chain == Chain::Xrp {
        crate::controller::xrp::trade_grid_init(state);
    }
    Task::none()
}

#[cfg(test)]
mod tests {

    use crate::controller::message::SecureField;

    /// Closing a pane empties the credential it was carrying.
    ///
    /// The bug this pins (2026-09-12): before the grid, every one of these
    /// forms had a `Back` that cleared it. The panes migration dropped that,
    /// so a pasted 24-word mnemonic outlived the pane it was typed into and
    /// the only way to reach it again was to backspace it out by hand.
    #[test]
    fn closing_a_pane_empties_the_credential_it_carried() {
        const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon \
                              abandon abandon abandon abandon abandon abandon abandon abandon \
                              abandon abandon abandon abandon abandon abandon abandon art";
        for (chain, kind, field) in [
            (Chain::Xrp, PaneKind::Send, SecureField::SendSeed),
            (Chain::Xrp, PaneKind::Ticket, SecureField::TradeSeed),
            (Chain::Xrp, PaneKind::Orders, SecureField::CancelSeed),
            (Chain::Xrp, PaneKind::AvailableTokens, SecureField::EnableSeed),
            (Chain::Xrp, PaneKind::Tokens, SecureField::DisableSeed),
            (Chain::Xrp, PaneKind::Wallet, SecureField::ReimportSeed),
            (Chain::Btc, PaneKind::Send, SecureField::BtcSendSeed),
            (Chain::Btc, PaneKind::Transactions, SecureField::BtcBumpSeed),
            (Chain::Btc, PaneKind::Wallet, SecureField::BtcReimportSeed),
        ] {
            let mut state = AppState::default();
            state.replace_secure_field(field, PHRASE.to_string());
            assert!(
                state.secure_field_len(field) > 0,
                "{kind:?} on {chain:?}: the phrase did not go in",
            );
            clear_pane_form(&mut state, chain, kind);
            assert_eq!(
                state.secure_field_len(field),
                0,
                "{kind:?} on {chain:?} kept its phrase after the pane closed",
            );
        }
    }

    /// Every kind `reset` sweeps is a kind close knows how to clear, and vice
    /// versa — a form added to one list and not the other is a buffer that
    /// survives one of the two paths.
    #[test]
    fn reset_sweeps_exactly_the_kinds_close_can_clear() {
        for chain in [Chain::Xrp, Chain::Btc] {
            for kind in form_kinds(chain) {
                let mut state = AppState::default();
                // The receive pane holds no credential — its one buffer is
                // the plain destination tag, checked on its own.
                if (chain, *kind) == (Chain::Xrp, PaneKind::Receive) {
                    state.receive_tag = "7".to_string();
                    state.receive_tag_draft = "12345".to_string();
                    state.receive_tag_editing = true;
                    clear_pane_form(&mut state, chain, *kind);
                    assert!(state.receive_tag_draft.is_empty());
                    assert!(!state.receive_tag_editing);
                    // The tag itself is the wallet's, not the pane's.
                    assert_eq!(state.receive_tag, "7");
                    continue;
                }
                let field = match (chain, kind) {
                    (Chain::Xrp, PaneKind::Send) => SecureField::SendSeed,
                    (Chain::Xrp, PaneKind::Ticket) => SecureField::TradeSeed,
                    (Chain::Xrp, PaneKind::Orders) => SecureField::CancelSeed,
                    (Chain::Xrp, PaneKind::AvailableTokens) => SecureField::EnableSeed,
                    (Chain::Xrp, PaneKind::Tokens) => SecureField::DisableSeed,
                    (Chain::Xrp, PaneKind::Wallet) => SecureField::ReimportSeed,
                    (Chain::Btc, PaneKind::Send) => SecureField::BtcSendSeed,
                    (Chain::Btc, PaneKind::Transactions) => SecureField::BtcBumpSeed,
                    (Chain::Btc, PaneKind::Wallet) => SecureField::BtcReimportSeed,
                    _ => panic!("{kind:?} on {chain:?} is in form_kinds with no credential mapped"),
                };
                state.replace_secure_field(field, "x".to_string());
                clear_pane_form(&mut state, chain, *kind);
                assert_eq!(state.secure_field_len(field), 0);
            }
        }
    }

    /// Closing the ticket does NOT drop the pair or move a book subscription.
    ///
    /// `clear_trade_form` does both, which is right when a trade completes and
    /// wrong for a view action: the book is gated by the wallet, and `Reset`
    /// is deliberately the one way back to no pair. Wiring pane close to the
    /// wrong one of the two would unsubscribe a book behind the user's back.
    #[test]
    fn closing_the_ticket_keeps_the_pair() {
        let mut state = AppState::default();
        state.trade_pay_asset = "XRP".to_string();
        state.trade_receive_asset = "RLUSD".to_string();
        state.trade_amount = "5".to_string();
        state.replace_secure_field(SecureField::TradeSeed, "x".to_string());

        clear_pane_form(&mut state, Chain::Xrp, PaneKind::Ticket);

        assert_eq!(state.secure_field_len(SecureField::TradeSeed), 0, "the credential stayed");
        assert_eq!(state.trade_amount, "", "the ticket's own input stayed");
        assert_eq!(state.trade_pay_asset, "XRP", "closing the ticket dropped the pair");
        assert_eq!(state.trade_receive_asset, "RLUSD", "closing the ticket dropped the pair");
    }

    use super::*;

    /// The handoff's default window: grid inner width 898, height 494.
    const SIZE: Size = Size { width: 898.0, height: 494.0 };

    fn pane_of(state: &pane_grid::State<PaneKind>, kind: PaneKind) -> Pane {
        state.iter().find(|(_, k)| **k == kind).map(|(p, _)| *p).expect("pane present")
    }

    fn root_ratio(tree: &PaneTree) -> f32 {
        match tree {
            PaneTree::Split { ratio, .. } => *ratio,
            PaneTree::Pane(_) => panic!("root is a pane"),
        }
    }

    /// Columns = the chain of vertical splits down the `b` side.
    fn columns(tree: &PaneTree) -> usize {
        match tree {
            PaneTree::Split { axis: Axis::Vertical, b, .. } => 1 + columns(b),
            _ => 1,
        }
    }

    /// Rows in one column = the chain of horizontal splits down its `b` side.
    fn rows(tree: &PaneTree) -> usize {
        match tree {
            PaneTree::Split { axis: Axis::Horizontal, b, .. } => 1 + rows(b),
            _ => 1,
        }
    }

    /// The tree at the top of the handoff, and a build → mirror round trip
    /// that keeps every ratio.
    #[test]
    fn the_default_tree_is_the_handoffs() {
        let tree = PaneTree::default_layout();
        assert_eq!(tree.kinds(), PaneKind::DEFAULT.to_vec());
        assert_eq!(columns(&tree), 3);
        let built = tree.build();
        assert_eq!(built.len(), 6);
        assert_eq!(PaneTree::mirror(&built), tree);
    }

    /// Step 2 of the reference: close `chart` and `balance` takes the left
    /// column's full height; the column keeps its width.
    #[test]
    fn closing_chart_hands_balance_the_column() {
        let mut g = Grid::default();
        let chart = pane_of(g.panes.as_ref().unwrap(), PaneKind::Chart);
        g.close_pane(chart);
        let tree = PaneTree::mirror(g.panes.as_ref().unwrap());
        match &tree {
            PaneTree::Split { axis: Axis::Vertical, ratio, a, .. } => {
                assert!((ratio - 1.0 / 3.0).abs() < 1e-6, "the column width moved");
                assert_eq!(**a, PaneTree::Pane(PaneKind::Balance));
            }
            other => panic!("unexpected tree {other:?}"),
        }
        assert_eq!(g.preset, None);
    }

    /// Step 3: close `send` and `wallet` takes the middle column's height;
    /// close `wallet` too and the RECEIVE column takes the width — the
    /// sibling in the tree, not the neighbour that looks adjacent. Its own
    /// 50/50 survives.
    #[test]
    fn closing_send_hands_the_receive_column_its_width() {
        let mut g = Grid::default();
        let send = pane_of(g.panes.as_ref().unwrap(), PaneKind::Send);
        g.close_pane(send);
        {
            let PaneTree::Split { b, .. } = PaneTree::mirror(g.panes.as_ref().unwrap()) else { panic!() };
            let PaneTree::Split { a: middle, .. } = *b else { panic!() };
            assert_eq!(*middle, PaneTree::Pane(PaneKind::Wallet), "wallet takes send's column");
        }
        let wallet = pane_of(g.panes.as_ref().unwrap(), PaneKind::Wallet);
        g.close_pane(wallet);
        let tree = PaneTree::mirror(g.panes.as_ref().unwrap());
        let PaneTree::Split { axis: Axis::Vertical, ratio, a, b } = &tree else {
            panic!("unexpected tree {tree:?}");
        };
        assert!((ratio - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(rows(a), 2, "the balance column is untouched");
        assert_eq!(
            **b,
            PaneTree::Split {
                axis: Axis::Horizontal,
                ratio: 0.5,
                a: Box::new(PaneTree::Pane(PaneKind::Receive)),
                b: Box::new(PaneTree::Pane(PaneKind::Network)),
            }
        );
    }

    /// Steps 4 and 5: the survivor fills the window, and the last close is
    /// the Empty branch — which `insert` grows back into the whole grid.
    #[test]
    fn the_last_close_is_the_empty_branch() {
        let mut g = Grid::default();
        for kind in [PaneKind::Chart, PaneKind::Send, PaneKind::Wallet, PaneKind::Balance, PaneKind::Receive] {
            let p = pane_of(g.panes.as_ref().unwrap(), kind);
            g.close_pane(p);
        }
        assert_eq!(g.open_kinds(), vec![PaneKind::Network]);
        let last = pane_of(g.panes.as_ref().unwrap(), PaneKind::Network);
        g.close_pane(last);
        assert!(g.panes.is_none(), "the last close is the Empty branch");
        // Adding to the Empty branch is the whole grid.
        g.insert(PaneKind::Send, SIZE, 1.0);
        assert_eq!(g.open_kinds(), vec![PaneKind::Send]);
    }

    /// Presets deal the open panes down columns, balanced: five over three is
    /// 2 · 2 · 1, four over three is 2 · 1 · 1, and fewer panes than columns
    /// is no preset at all.
    #[test]
    fn presets_balance_down_columns() {
        let five = &PaneKind::DEFAULT[..5];
        let tree = PaneTree::preset(five, 3).unwrap();
        assert_eq!(tree.kinds(), five.to_vec());
        assert_eq!(columns(&tree), 3);
        let PaneTree::Split { a, b, ratio, .. } = &tree else { panic!() };
        assert!((ratio - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(rows(a), 2);
        let PaneTree::Split { a: col2, b: col3, ratio, .. } = &**b else { panic!() };
        assert!((ratio - 0.5).abs() < 1e-6);
        assert_eq!(rows(col2), 2);
        assert_eq!(rows(col3), 1);

        let four = &five[..4];
        let tree = PaneTree::preset(four, 3).unwrap();
        let PaneTree::Split { a, b, .. } = &tree else { panic!() };
        assert_eq!(rows(a), 2);
        let PaneTree::Split { a: col2, b: col3, .. } = &**b else { panic!() };
        assert_eq!(rows(col2), 1);
        assert_eq!(rows(col3), 1);

        assert!(PaneTree::preset(&five[..2], 3).is_none());
        assert!(PaneTree::preset(&[], 1).is_none());
        assert_eq!(rows(&PaneTree::preset(five, 1).unwrap()), 5);
    }

    /// Checking a panel splits the LARGEST open pane along its longer axis;
    /// an `Empty` well is filled first.
    #[test]
    fn a_new_pane_splits_the_largest_along_its_longer_axis() {
        let tree = PaneTree::Split {
            axis: Axis::Vertical,
            ratio: 1.0 / 3.0,
            a: Box::new(PaneTree::Pane(PaneKind::Balance)),
            b: Box::new(PaneTree::Pane(PaneKind::Send)),
        };
        let mut g = Grid { panes: Some(tree.build()), ..Grid::default() };
        // `send` is ~599 × 494: wider than tall, so it halves left/right.
        g.insert(PaneKind::Chart, SIZE, 1.0);
        let after = PaneTree::mirror(g.panes.as_ref().unwrap());
        let PaneTree::Split { b, .. } = &after else { panic!() };
        assert_eq!(
            **b,
            PaneTree::Split {
                axis: Axis::Vertical,
                ratio: 0.5,
                a: Box::new(PaneTree::Pane(PaneKind::Send)),
                b: Box::new(PaneTree::Pane(PaneKind::Chart)),
            }
        );
        // A split glyph leaves a well, and the next panel checked takes it.
        let send = pane_of(g.panes.as_ref().unwrap(), PaneKind::Send);
        g.split(send, Axis::Horizontal);
        assert_eq!(g.panes.as_ref().unwrap().len(), 4);
        g.insert(PaneKind::Network, SIZE, 1.0);
        assert_eq!(g.panes.as_ref().unwrap().len(), 4, "the well was filled, not split");
        assert!(g.has(PaneKind::Network));
        assert!(!g.panes.as_ref().unwrap().iter().any(|(_, k)| *k == PaneKind::Empty));
    }

    /// A drag is a ratio; undo brings the ratio back, not just the pane.
    #[test]
    fn a_starving_resize_is_refused() {
        let mut g = Grid::default();
        let root = {
            let p = g.panes.as_ref().unwrap();
            let splits = p.layout().split_regions(SPACING, MIN_H, SIZE);
            *splits
                .iter()
                .find(|(_, (_, r, _))| r.width == SIZE.width && r.height == SIZE.height)
                .map(|(s, _)| s)
                .expect("root split")
        };
        // 0.5 leaves the two right columns 224 each — over the 200 floor.
        // (0.6 would leave them 179: refused, correctly — the first draft
        // of this test asked for it and was wrong.)
        g.resize(ResizeEvent { split: root, ratio: 0.5 }, SIZE, 1.0);
        assert!((root_ratio(&PaneTree::mirror(g.panes.as_ref().unwrap())) - 0.5).abs() < 1e-6);
        assert_eq!(g.preset, None);

        // 5% of 898 is 45px for the balance column: refused, ratio unmoved —
        // and unmoved EXACTLY, not to the pixel-rounded ratio iced reports.
        g.resize(ResizeEvent { split: root, ratio: 0.05 }, SIZE, 1.0);
        assert!((root_ratio(&PaneTree::mirror(g.panes.as_ref().unwrap())) - 0.5).abs() < 1e-6);

    }

    /// The saved shape round-trips: kinds, axes and raw ratios, and the
    /// Empty branch as `null`. Anything unknown is refused whole.
    #[test]
    fn the_layout_round_trips_through_json_and_refuses_the_unknown() {
        let g = Grid::default();
        let back = Grid::from_json(Chain::Xrp, &g.to_json()).expect("parses");
        assert_eq!(PaneTree::mirror(back.panes.as_ref().unwrap()), PaneTree::default_layout());
        assert_eq!(back.chart_period, ChartPeriod::OneDay);

        let empty = Grid { panes: None, ..Grid::default() };
        assert!(Grid::from_json(Chain::Xrp, &empty.to_json()).unwrap().panes.is_none());

        let mut json = g.to_json();
        json["tree"]["a"]["a"]["pane"] = serde_json::json!("order");
        assert!(Grid::from_json(Chain::Xrp, &json).is_none(), "an unknown pane name is the default layout");
        let mut json = g.to_json();
        json["tree"]["ratio"] = serde_json::json!(1.5);
        assert!(Grid::from_json(Chain::Xrp, &json).is_none(), "a ratio outside (0, 1) is refused");
        assert!(Grid::from_json(Chain::Xrp, &serde_json::json!({})).is_none());
    }

    /// BTC's default: three columns, the left one three deep, `wallet` under
    /// `send`. Closing `fees` hands `chart` its height and `balance` keeps
    /// its own — the pair was split off under it.
    #[test]
    fn the_btc_default_tree_is_the_handoffs_with_the_wallet() {
        let tree = PaneTree::btc_default_layout();
        assert_eq!(tree.kinds(), PaneKind::BTC_DEFAULT.to_vec());
        assert_eq!(columns(&tree), 3);
        let PaneTree::Split { a, .. } = &tree else { panic!() };
        assert_eq!(rows(a), 3, "the left column is three deep");

        let mut g = Grid::new(Chain::Btc);
        assert_eq!(g.chain, Chain::Btc);
        // The default seven are the first seven of the menu, in its order;
        // transactions, mempool and intervals are added by the user.
        assert_eq!(g.open_kinds(), PaneKind::BTC_DEFAULT.to_vec());
        assert_eq!(&PaneKind::BTC_CANONICAL[..7], &PaneKind::BTC_DEFAULT[..]);
        assert_eq!(PaneKind::from_key(Chain::Btc, "intervals"), Some(PaneKind::Intervals));
        assert_eq!(PaneKind::from_key(Chain::Xrp, "mempool"), None);
        let fees = pane_of(g.panes.as_ref().unwrap(), PaneKind::Fees);
        g.close_pane(fees);
        let after = PaneTree::mirror(g.panes.as_ref().unwrap());
        let PaneTree::Split { a, .. } = &after else { panic!() };
        assert_eq!(
            **a,
            PaneTree::Split {
                axis: Axis::Horizontal,
                ratio: 0.30,
                a: Box::new(PaneTree::Pane(PaneKind::Balance)),
                b: Box::new(PaneTree::Pane(PaneKind::Chart)),
            }
        );
        g.reset();
        assert_eq!(PaneTree::mirror(g.panes.as_ref().unwrap()), PaneTree::btc_default_layout());
    }

    /// A saved layout holds only its own chain's panes: BTC's shape never
    /// opens on XRP, and XRP's never on BTC — each falls back to its default.
    #[test]
    fn a_saved_layout_only_holds_its_own_chains_panes() {
        let btc = Grid::new(Chain::Btc);
        let back = Grid::from_json(Chain::Btc, &btc.to_json()).expect("parses");
        assert_eq!(back.chain, Chain::Btc);
        assert_eq!(PaneTree::mirror(back.panes.as_ref().unwrap()), PaneTree::btc_default_layout());
        assert!(Grid::from_json(Chain::Xrp, &btc.to_json()).is_none(), "fees and blocks are not XRP panes");
        let xrp = Grid::default();
        assert!(Grid::from_json(Chain::Btc, &xrp.to_json()).is_none(), "network is not a BTC pane");
        assert_eq!(PaneKind::from_key(Chain::Btc, "fees"), Some(PaneKind::Fees));
        assert_eq!(PaneKind::from_key(Chain::Xrp, "fees"), None);
        assert_eq!(PaneKind::from_key(Chain::Btc, "empty"), Some(PaneKind::Empty));
    }

    /// A change stamps the grid dirty; nothing is written until it settles.
    #[test]
    fn a_change_is_dirty_until_it_settles() {
        let mut g = Grid::default();
        assert!(g.dirty.is_none());
        let chart = pane_of(g.panes.as_ref().unwrap(), PaneKind::Chart);
        g.close_pane(chart);
        assert!(g.dirty.is_some());
        // Not settled: no write, still dirty.
        assert!(!g.flush_if_settled("test_grid_never_written"));
        assert!(g.dirty.is_some());
    }

    /// Reset is the handoff's tree again, with `Three columns` checked.
    #[test]
    fn reset_is_the_default_and_checks_three_columns() {
        let mut g = Grid::default();
        let send = pane_of(g.panes.as_ref().unwrap(), PaneKind::Send);
        g.close_pane(send);
        g.apply_preset(Preset::Two);
        assert_eq!(g.preset, Some(Preset::Two));
        assert!(!g.preset_available(Preset::Three) || g.open_kinds().len() >= 3);
        g.reset();
        assert_eq!(PaneTree::mirror(g.panes.as_ref().unwrap()), PaneTree::default_layout());
        assert_eq!(g.preset, Some(Preset::Three));
    }
}
