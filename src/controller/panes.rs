
use std::time::{Duration, Instant};

use iced::widget::pane_grid::{self, Axis, Configuration, DragEvent, Node, Pane, ResizeEvent, Split, Target};
use serde_json::{json, Value};
use iced::{Size, Task};

use crate::channel::CHANNEL;
use crate::controller::app_state::{AppState, BtcView, ChartPeriod, Tab, XrpView};
use crate::controller::message::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chain {
    Xrp,
    Btc,
}


pub const SPACING: f32 = 1.0;
pub const LEEWAY: f32 = 6.0;
pub const MIN_W: f32 = 200.0;
pub const MIN_H: f32 = 88.0;
pub const BAR_H: f32 = 34.0;
const DOCK_H: f32 = 30.3;

pub fn grid_size(state: &AppState) -> Size {
    let s = state.scale();
    let chrome = BAR_H * s + 1.0 + DOCK_H * s + 1.0;
    Size::new(state.window_width.max(1.0), (state.window_height - chrome).max(1.0))
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneKind {
    Balance,
    Chart,
    Send,
    Receive,
    Network,
    Wallet,
    Ticket,
    Book,
    Depth,
    Transactions,
    Orders,
    Tokens,
    AvailableTokens,
    Fees,
    Blocks,
    Mempool,
    Intervals,
    Empty,
}

fn clear_pane_form(state: &mut AppState, chain: Chain, kind: PaneKind) {
    use crate::controller::{btc, xrp};
    match (chain, kind) {
        (Chain::Xrp, PaneKind::Send) => xrp::clear_send_form(state),
        (Chain::Xrp, PaneKind::Receive) => xrp::clear_receive_form(state),
        (Chain::Xrp, PaneKind::Ticket) => xrp::clear_trade_inputs(state),
        (Chain::Xrp, PaneKind::Orders) => xrp::clear_cancel_form(state),
        (Chain::Xrp, PaneKind::AvailableTokens) => xrp::clear_enable_form(state),
        (Chain::Xrp, PaneKind::Tokens) => {
            xrp::clear_disable_form(state);
            state.disable_token = None;
        }
        (Chain::Xrp, PaneKind::Wallet) => xrp::clear_reimport_form(state),
        (Chain::Btc, PaneKind::Send) => btc::clear_btc_send_form(state),
        (Chain::Btc, PaneKind::Transactions) => btc::clear_btc_bump_form(state),
        (Chain::Btc, PaneKind::Wallet) => btc::clear_btc_reimport_form(state),
        (Chain::Btc, PaneKind::Receive) => btc::clear_btc_receive_line(state),
        _ => {}
    }
}

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
        Chain::Btc => &[PaneKind::Send, PaneKind::Transactions, PaneKind::Wallet, PaneKind::Receive],
    }
}

fn present_kinds(state: &mut AppState, chain: Chain) -> Vec<PaneKind> {
    let g = grid_mut(state, chain);
    form_kinds(chain).iter().copied().filter(|k| g.has(*k)).collect()
}

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
    pub fn canonical(chain: Chain) -> &'static [PaneKind] {
        match chain {
            Chain::Xrp => &Self::CANONICAL,
            Chain::Btc => &Self::BTC_CANONICAL,
        }
    }

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

    pub const BTC_DEFAULT: [PaneKind; 7] = [
        PaneKind::Balance,
        PaneKind::Fees,
        PaneKind::Chart,
        PaneKind::Send,
        PaneKind::Wallet,
        PaneKind::Receive,
        PaneKind::Blocks,
    ];

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

    pub const DEFAULT: [PaneKind; 6] = [
        PaneKind::Balance,
        PaneKind::Chart,
        PaneKind::Send,
        PaneKind::Wallet,
        PaneKind::Receive,
        PaneKind::Network,
    ];

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

    pub fn from_key(chain: Chain, key: &str) -> Option<Self> {
        Self::canonical(chain)
            .iter()
            .copied()
            .chain(std::iter::once(PaneKind::Empty))
            .find(|k| k.key() == key)
    }

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


#[derive(Debug, Clone, PartialEq)]
pub enum PaneTree {
    Split {
        axis: Axis,
        ratio: f32,
        a: Box<PaneTree>,
        b: Box<PaneTree>,
    },
    Pane(PaneKind),
}

impl PaneTree {
    pub fn default_layout_for(chain: Chain) -> Self {
        match chain {
            Chain::Xrp => Self::default_layout(),
            Chain::Btc => Self::btc_default_layout(),
        }
    }

    pub fn btc_default_layout() -> Self {
        let leaf = |k| Box::new(PaneTree::Pane(k));
        PaneTree::Split {
            axis: Axis::Vertical,
            ratio: 1.0 / 3.0,
            a: Box::new(PaneTree::Split {
                axis: Axis::Horizontal,
                ratio: 0.20,
                a: leaf(PaneKind::Balance),
                b: Box::new(PaneTree::Split {
                    axis: Axis::Horizontal,
                    ratio: 0.29,
                    a: leaf(PaneKind::Fees),
                    b: leaf(PaneKind::Chart),
                }),
            }),
            b: Box::new(PaneTree::Split {
                axis: Axis::Vertical,
                ratio: 0.5,
                a: Box::new(PaneTree::Split {
                    axis: Axis::Horizontal,
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

    pub fn default_layout() -> Self {
        let leaf = |k| Box::new(PaneTree::Pane(k));
        PaneTree::Split {
            axis: Axis::Vertical,
            ratio: 1.0 / 3.0,
            a: Box::new(PaneTree::Split {
                axis: Axis::Horizontal,
                ratio: 0.33,
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

    pub fn build(&self) -> pane_grid::State<PaneKind> {
        pane_grid::State::with_configuration(self.configuration())
    }

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

    fn stack(kinds: &[PaneKind]) -> Self {
        let panes: Vec<Self> = kinds.iter().map(|k| PaneTree::Pane(*k)).collect();
        Self::join(panes, Axis::Horizontal)
    }

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


#[derive(Debug)]
pub struct Grid {
    pub chain: Chain,
    pub panes: Option<pane_grid::State<PaneKind>>,
    pub preset: Option<Preset>,
    pub menu_open: bool,
    pub chart_period: ChartPeriod,
    pub dirty: Option<Instant>,
}

pub const SETTLE: Duration = Duration::from_secs(2);

pub const XRP_KEY: &str = "xrp_grid";
pub const BTC_KEY: &str = "btc_grid";

impl Default for Grid {
    fn default() -> Self {
        Self::new(Chain::Xrp)
    }
}

impl Grid {
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

    pub fn load(chain: Chain, key: &str) -> Option<Self> {
        let json = crate::bridge::json_storage::read_json::<Value>("settings.json").ok()?;
        Self::from_json(chain, json.get(key)?)
    }

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

    fn touch(&mut self) {
        self.dirty = Some(Instant::now());
    }

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

    pub fn toggle(&mut self, kind: PaneKind, size: Size, scale: f32) {
        match self.find(kind) {
            Some(pane) => self.close_pane(pane),
            None => self.insert(kind, size, scale),
        }
    }

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

    pub fn toggle_maximize(&mut self, pane: Pane) {
        let Some(p) = self.panes.as_mut() else { return };
        if p.maximized() == Some(pane) {
            p.restore();
        } else if p.get(pane).is_some() {
            p.maximize(pane);
        }
    }

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

    pub fn reset(&mut self) {
        self.panes = Some(PaneTree::default_layout_for(self.chain).build());
        self.preset = Some(Preset::Three);
        self.touch();
    }

}

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


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerBar {
    pub index: u64,
    pub txns: u64,
    pub escalated: bool,
}

pub const TAPE_CAP: usize = 128;

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


#[derive(Debug, Clone)]
pub enum GridMsg {
    MenuToggled,
    MenuDismissed,
    Toggle(PaneKind),
    Close(Pane),
    Maximize(Pane),
    Split(Pane, Axis),
    Resized(ResizeEvent),
    Dragged(DragEvent),
    Preset(Preset),
    Reset,
    Period(ChartPeriod),
    SendSign,
    TicketSign,
    PairSearchOpened,
    PairSearchDismissed,
    PairSearchSubmitted,
}

fn on_screen(state: &AppState, chain: Chain) -> bool {
    let tab = match chain {
        Chain::Xrp => state.selected_tab == Tab::Xrp && state.xrp_view == XrpView::Menu,
        Chain::Btc => state.selected_tab == Tab::Btc && state.btc_view == BtcView::Menu,
    };
    tab && state.activity_log.is_none()
}

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
            dismiss_pair_search(state);
            let g = grid_mut(state, chain);
            g.menu_open = !g.menu_open;
        }
        GridMsg::MenuDismissed => grid_mut(state, chain).menu_open = false,
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
                for kind in form_kinds(chain) {
                    clear_pane_form(state, chain, *kind);
                }
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
                state.send_step = 1;
                let checked = crate::controller::xrp::handle(state, Message::SendContinueClicked);
                if state.send_step == 2 {
                    return crate::controller::xrp::handle(state, Message::SendSubmitClicked);
                }
                return checked;
            }
            Chain::Btc => {
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
            state.trade_step = 1;
            let checked = crate::controller::xrp::handle(state, Message::TradeContinueClicked);
            if state.trade_step == 2 {
                let sent = crate::controller::xrp::handle(state, Message::TradeSubmitClicked);
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
            let query = state.trade_pair_query.clone();
            let (rows, _) = crate::ui::managexrp::xrptrade::picker::listing(&query);
            if let Some((b, q)) = rows.into_iter().next() {
                return crate::controller::xrp::handle(state, Message::TradePairSelected(b.to_string(), q.to_string()));
            }
        }
    }
    if chain == Chain::Xrp {
        crate::controller::xrp::trade_grid_init(state);
    }
    Task::none()
}

#[cfg(test)]
mod tests {

    use crate::controller::message::SecureField;

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

    #[test]
    fn reset_sweeps_exactly_the_kinds_close_can_clear() {
        for chain in [Chain::Xrp, Chain::Btc] {
            for kind in form_kinds(chain) {
                let mut state = AppState::default();
                if (chain, *kind) == (Chain::Xrp, PaneKind::Receive) {
                    state.receive_tag = "7".to_string();
                    state.receive_tag_draft = "12345".to_string();
                    state.receive_tag_editing = true;
                    clear_pane_form(&mut state, chain, *kind);
                    assert!(state.receive_tag_draft.is_empty());
                    assert!(!state.receive_tag_editing);
                    assert_eq!(state.receive_tag, "7");
                    continue;
                }
                if (chain, *kind) == (Chain::Btc, PaneKind::Receive) {
                    state.btc_receive_pool = vec!["a".to_string(), "b".to_string()];
                    state.btc_receive_address = Some("b".to_string());
                    state.btc_drop_confirm = true;
                    state.btc_pool_refusal = Some(crate::bridge::btc_receive_rotation::PoolRefusal::Full);
                    state.btc_copy_feedback = true;
                    clear_pane_form(&mut state, chain, *kind);
                    assert!(!state.btc_drop_confirm);
                    assert!(state.btc_pool_refusal.is_none());
                    assert!(!state.btc_copy_feedback);
                    assert_eq!(state.btc_receive_pool.len(), 2);
                    assert_eq!(state.btc_receive_address.as_deref(), Some("b"));
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

    fn columns(tree: &PaneTree) -> usize {
        match tree {
            PaneTree::Split { axis: Axis::Vertical, b, .. } => 1 + columns(b),
            _ => 1,
        }
    }

    fn rows(tree: &PaneTree) -> usize {
        match tree {
            PaneTree::Split { axis: Axis::Horizontal, b, .. } => 1 + rows(b),
            _ => 1,
        }
    }

    #[test]
    fn the_default_tree_is_the_handoffs() {
        let tree = PaneTree::default_layout();
        assert_eq!(tree.kinds(), PaneKind::DEFAULT.to_vec());
        assert_eq!(columns(&tree), 3);
        let built = tree.build();
        assert_eq!(built.len(), 6);
        assert_eq!(PaneTree::mirror(&built), tree);
    }

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
        g.insert(PaneKind::Send, SIZE, 1.0);
        assert_eq!(g.open_kinds(), vec![PaneKind::Send]);
    }

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

    #[test]
    fn a_new_pane_splits_the_largest_along_its_longer_axis() {
        let tree = PaneTree::Split {
            axis: Axis::Vertical,
            ratio: 1.0 / 3.0,
            a: Box::new(PaneTree::Pane(PaneKind::Balance)),
            b: Box::new(PaneTree::Pane(PaneKind::Send)),
        };
        let mut g = Grid { panes: Some(tree.build()), ..Grid::default() };
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
        let send = pane_of(g.panes.as_ref().unwrap(), PaneKind::Send);
        g.split(send, Axis::Horizontal);
        assert_eq!(g.panes.as_ref().unwrap().len(), 4);
        g.insert(PaneKind::Network, SIZE, 1.0);
        assert_eq!(g.panes.as_ref().unwrap().len(), 4, "the well was filled, not split");
        assert!(g.has(PaneKind::Network));
        assert!(!g.panes.as_ref().unwrap().iter().any(|(_, k)| *k == PaneKind::Empty));
    }

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
        g.resize(ResizeEvent { split: root, ratio: 0.5 }, SIZE, 1.0);
        assert!((root_ratio(&PaneTree::mirror(g.panes.as_ref().unwrap())) - 0.5).abs() < 1e-6);
        assert_eq!(g.preset, None);

        g.resize(ResizeEvent { split: root, ratio: 0.05 }, SIZE, 1.0);
        assert!((root_ratio(&PaneTree::mirror(g.panes.as_ref().unwrap())) - 0.5).abs() < 1e-6);

    }

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

    #[test]
    fn the_btc_default_tree_is_the_handoffs_with_the_wallet() {
        let tree = PaneTree::btc_default_layout();
        assert_eq!(tree.kinds(), PaneKind::BTC_DEFAULT.to_vec());
        assert_eq!(columns(&tree), 3);
        let PaneTree::Split { a, .. } = &tree else { panic!() };
        assert_eq!(rows(a), 3, "the left column is three deep");

        let mut g = Grid::new(Chain::Btc);
        assert_eq!(g.chain, Chain::Btc);
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
                ratio: 0.20,
                a: Box::new(PaneTree::Pane(PaneKind::Balance)),
                b: Box::new(PaneTree::Pane(PaneKind::Chart)),
            }
        );
        g.reset();
        assert_eq!(PaneTree::mirror(g.panes.as_ref().unwrap()), PaneTree::btc_default_layout());
    }

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

    #[test]
    fn a_change_is_dirty_until_it_settles() {
        let mut g = Grid::default();
        assert!(g.dirty.is_none());
        let chart = pane_of(g.panes.as_ref().unwrap(), PaneKind::Chart);
        g.close_pane(chart);
        assert!(g.dirty.is_some());
        assert!(!g.flush_if_settled("test_grid_never_written"));
        assert!(g.dirty.is_some());
    }

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
