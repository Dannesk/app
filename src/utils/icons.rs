use iced::widget::svg;
use std::sync::LazyLock;


// ── Brand mark (the launch gate) ────────────────────────────────────────────
/// The two bars of `src/icon.svg` **without its #1E2227 plate**, cropped to the
/// same 700 box the landing page's nav mark uses (`landing/src/components/
/// Nav.astro`) so the two are the same shape at any size. Plateless because the
/// plate is only ever a window colour: tinted `cp.text` by the caller it lands
/// correctly on all three themes, where a plated disc would be a dark badge on
/// the light one. Change the logo and this, `icon.svg` and Nav.astro all move.
///
/// A second mark (`dannesk-triad`) was tried here against this one on
/// 2026-09-15 and lost; the user deleted it the same day.
pub static LOGO: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("logo.svg").as_ref())
});

// ── Reveal toggle (secure fields) ────────────────────────────────────────────
pub static EYE_OPEN: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("eye-open.svg").as_ref())
});

pub static EYE_CLOSED: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("eye-closed.svg").as_ref())
});

// ── Balance ─────────────────────────────────────────────────────────────────
pub static SLIDERS: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("sliders.svg").as_ref())
});

// ── Back ────────────────────────────────────────────────────────────────────
/// The back chevron, cut in the sliders' own stroke (24 box, 1.9, round) so
/// the two read as a pair in the same corner circle.
pub static CHEVRON_LEFT: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("chevron-left.svg").as_ref())
});

// ── Empty list (pane grid) ──────────────────────────────────────────────────
/// The one asset the XRP panes handoff (2026-09-09) added: a 22 × 22 line
/// glyph, `fill: none`, 1px `currentColor` stroke, tinted `faint` by the
/// caller. Drawn for that handoff, not cut from any sprite — do not re-cut
/// it and do not thicken the stroke: at 22px with 1px it is deliberately
/// quieter than the toolbar glyphs.
pub static EMPTY_LIST: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("empty-list.svg").as_ref())
});

// ── Pair search (XRP top bar) ───────────────────────────────────────────────
/// The one asset the pair-selection handoff (2026-09-11) added: a 12 × 12
/// magnifier, `fill: none`, 1.3px `currentColor` stroke, drawn at 11 × 11
/// and tinted `muted` by the caller. It rides in the trailing slot of the
/// top bar's pair search field and nowhere else — not on the chip, and not
/// as a toolbar glyph (those are cut at 22).
pub static SEARCH: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("search.svg").as_ref())
});

// ── Copy (wallet panes) ─────────────────────────────────────────────────────
/// The copy glyph beside the address on both chains' `wallet` panes
/// (2026-09-14): two offset rounded squares, cut in the chevron's own stroke
/// (24 box, 1.9, round), tinted `muted` and `text` on hover by the caller.
pub static COPY: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("copy.svg").as_ref())
});

// ── No-wallet gate ──────────────────────────────────────────────────────────
/// The XRP mark on the no-wallet orbit's plate. The file is filled white;
/// the gate tints it `dim` so it sits in the same ink as BTC's `₿`.
pub static XRP: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("xrp.svg").as_ref())
});
