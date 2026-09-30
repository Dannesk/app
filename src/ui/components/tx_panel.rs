//! The **row vocabulary** both chains' transaction lists share: a value is a
//! run of [`Seg`]s, a detail line is a key and its runs, and an expanded row
//! closes on a [`Tail`]. The chain files (`xrptransactions`,
//! `btctransactions`) resolve their channel into these; the pane files draw
//! them.
//!
//! **Type by role.** Inter for anything read — a qualifier, a sentence;
//! mono for working data — amounts, units, status codes, timestamps, hashes,
//! addresses, counts. The chain files decide which a value is by building
//! [`Seg::mono`] or [`Seg::word`].
//!
//! This was the 420px transactions modal (XRP transactions v3 handoff,
//! 2026-09-05) until both chains moved their lists into panes; the panel
//! itself went with the BTC old chain on 2026-09-10 and the types stayed.

use iced::Color;

use crate::controller::message::Message;

/// One span of a value: its text, its ink, and which voice it is in.
#[derive(Debug, Clone)]
pub struct Seg {
    pub s: String,
    pub color: Color,
    pub mono: bool,
}

impl Seg {
    /// Working data — amounts, hashes, addresses, codes, counts.
    pub fn mono(s: impl Into<String>, color: Color) -> Self {
        Self { s: s.into(), color, mono: true }
    }

    /// Something read — a qualifier, a sentence.
    pub fn word(s: impl Into<String>, color: Color) -> Self {
        Self { s: s.into(), color, mono: false }
    }
}

/// One detail line: a fixed key and a value built from runs.
pub type Line = (&'static str, Vec<Seg>);

/// The line that closes an expanded row.
pub enum Tail {
    /// A live control — `copy hash ›`, `cancel order ›`. Drawn mono-caps in
    /// `color` with the `neutral` wash on hover.
    Link { label: &'static str, color: Color, msg: Message },
    /// Something that cannot act, said plainly and without the `›`.
    Prose { s: &'static str, color: Color },
}

pub struct Detail {
    pub lines: Vec<Line>,
    pub tail: Tail,
}
