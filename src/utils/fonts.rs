//! The two faces the app carries, and the system's fonts behind them.
//!
//! Inter 18pt (Regular and Light) is the voice of everything read; JetBrains
//! Mono is the grid and every ticking value (`components/tui.rs` for why).
//! iced matches a font by the family name written INSIDE the file and by exact
//! weight — never by the file's name. A miss is silent: the text is drawn by
//! whichever system font comes first in cosmic-text's fallback list (Noto Sans,
//! then DejaVu Sans). 0.1.0 asked for "Inter" while the files say "Inter 18pt",
//! so it shipped drawing Noto Sans.
//!
//! ## Why the system fonts are indexed late
//!
//! iced opens every font installed on the machine — 2,652 faces and 832 MB on
//! the workstation — TWICE before its first frame is on screen: once for the
//! text font system, and once more in the SVG renderer on the first icon it
//! draws (`iced_wgpu/src/image/vector.rs`, a database of its own). Warm, 50 ms
//! each; on the first launch after a boot, with those files not in the page
//! cache, whichever runs first costs 4 s. Skipping only the first moves the
//! wait into the first frame, behind a window with nothing drawn (tried
//! 2026-10-02). Our own faces register in microseconds.
//!
//! Both scans are fontdb's `load_system_fonts`, and both obey `FONTCONFIG_FILE`.
//! So [`index_before_window`] points that variable, for the life of the
//! process, at a config that names no fonts: the text font system starts with
//! our faces alone and the SVG renderer with none (our icons hold no text).
//! Once the first frame is out, [`system_fonts_task`] reads the font
//! directories on a thread of its own, merges the faces in for fallback (other
//! scripts, emoji) and has the text on screen re-shaped.
//!
//! Until the merge lands, every glyph comes from the carried faces, and
//! nothing the UI draws itself needs more: check marks are Inter's, the carets,
//! box drawing and header arrows are JetBrains Mono's. Only user- or
//! ledger-supplied text in another script shows as boxes for those moments.
//!
//! ⚠️ A child process would inherit the variable and find no fonts. The app
//! starts none; one that ever does must `env_remove("FONTCONFIG_FILE")`.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::advanced::graphics::text::{self as text, cosmic_text::fontdb};
use iced::{Font, Task, font::{Family, Weight}};

use crate::controller::message::Message;

/// The family name inside `Inter-Light.ttf` and `Inter_Regular.ttf`.
pub const SANS_FAMILY: &str = "Inter 18pt";

/// The app default: Inter 18pt Regular.
pub const SANS: Font = Font::new(SANS_FAMILY);

pub const MONO: Font = Font {
    family: Family::Name("JetBrains Mono"),
    weight: Weight::Normal,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};

pub const LIGHT: Font = Font {
    family: Family::Name(SANS_FAMILY),
    weight: Weight::Light,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};

/// The system's fonts are still to be read: set by [`index_before_window`],
/// taken by [`system_fonts_task`].
static SYSTEM_FONTS_OWED: AtomicBool = AtomicBool::new(false);

/// The config that names no fonts: per process, in the runtime directory.
fn skip_config() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("dannesk-fonts-{}.conf", std::process::id()))
}

/// Turns both of iced's system-font scans into no-ops and builds the text font
/// system, before any window exists. If the scan could not be skipped it runs
/// here, the slow way, as iced itself would have run it, and nothing is owed.
///
/// fontdb reads `FONTCONFIG_FILE` first and scans only the `<dir>`s that file
/// names, so a config naming one directory that does not exist yields an empty
/// index. (An empty config, or a missing file, makes it fall back to scanning
/// /usr/share/fonts.) The variable stays set and the file stays in place: the
/// SVG renderer's scan runs later, in the first frame. Changing the
/// environment is only sound while the process has a single thread — call
/// this before the runtime is built.
pub fn index_before_window() {
    let conf = skip_config();
    let nowhere = conf.with_extension("d");
    let config = format!(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"fonts.dtd\">\n<fontconfig>\n  <dir>{}</dir>\n</fontconfig>\n",
        nowhere.display()
    );
    if std::fs::write(&conf, config).is_err() {
        let _ = text::font_system();
        return;
    }

    // SAFETY: single-threaded at this point; nothing else reads the environment.
    unsafe { std::env::set_var("FONTCONFIG_FILE", &conf) };
    let faces = text::font_system().write().expect("Write font system").raw().db().len();

    // Only iced's own icon face means the skip worked. Thousands means fontdb
    // scanned anyway, and merging the same faces again would be wrong.
    SYSTEM_FONTS_OWED.store(faces < 16, Ordering::Release);
}

/// Removes the config [`index_before_window`] wrote. For the way out of `main`.
pub fn remove_skip_config() {
    let _ = std::fs::remove_file(skip_config());
}

/// Whether the first frame is still awaited to start the read
/// (`controller/subscription.rs`).
pub fn system_fonts_pending() -> bool {
    SYSTEM_FONTS_OWED.load(Ordering::Acquire)
}

/// Starts the deferred read, once; the task ends in `SystemFontsIndexed`. For
/// the first frame's message: reading 350 MB of a cold disk must not compete
/// with that frame.
///
/// The read runs on a plain detached thread, NOT the runtime's blocking pool.
/// A runtime waits for its blocking tasks when it is dropped, so a read still
/// going held a closed window on screen until it finished — 4 s on a cold
/// start (2026-10-02). A detached thread simply ends with the process.
pub fn system_fonts_task() -> Task<Message> {
    if !SYSTEM_FONTS_OWED.swap(false, Ordering::AcqRel) {
        return Task::none();
    }
    let (done, merged) = tokio::sync::oneshot::channel::<()>();
    let spawned = std::thread::Builder::new().name("system-fonts".into()).spawn(move || {
        index_system_fonts();
        let _ = done.send(());
    });
    if spawned.is_err() {
        return Task::none();
    }
    Task::future(async move {
        let _ = merged.await;
        Message::SystemFontsIndexed
    })
}

/// Where fonts are installed. fontconfig's own list cannot be asked for (the
/// variable above answers for it); on the workstation it adds only TeX's
/// directories to these, 111 faces of 2,652, none of them a fallback font.
fn system_font_dirs() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = vec!["/usr/share/fonts".into(), "/usr/local/share/fonts".into()];
    let mut add = |dir: PathBuf| {
        if !found.contains(&dir) {
            found.push(dir);
        }
    };
    if let Ok(data_dirs) = std::env::var("XDG_DATA_DIRS") {
        for dir in data_dirs.split(':').filter(|d| !d.is_empty()) {
            add(PathBuf::from(dir).join("fonts"));
        }
    }
    if let Some(dir) = dirs::font_dir() {
        add(dir);
    }
    if let Some(home) = dirs::home_dir() {
        add(home.join(".fonts"));
    }
    found
}

/// The scan iced skipped, then the merge: every system face goes into the live
/// font system for fallback, and a version bump marks every paragraph on
/// screen stale, so the next layout re-shapes it. The scan is the slow part
/// and touches nothing shared; the merge holds the font system for a few
/// milliseconds.
fn index_system_fonts() {
    crate::startup_trace::stamp("system font scan started");
    let mut system = fontdb::Database::new();
    for dir in system_font_dirs() {
        system.load_fonts_dir(dir);
    }

    let mut font_system = text::font_system().write().expect("Write font system");
    let db = font_system.raw().db_mut();
    for face in system.faces() {
        let mut face = face.clone();
        face.id = fontdb::ID::dummy();
        db.push_face_info(face);
    }

    // `load_font` is the one way to move the font system's version, which is
    // what tells iced's paragraphs their shaping is stale. Zero bytes parse as
    // no font at all, so nothing is added — only the version moves.
    font_system.load_font(Cow::Owned(Vec::new()));
    drop(font_system);
    crate::startup_trace::stamp("system fonts merged");
}
