//! Copying the recovery phrase, marked secret.
//!
//! The create screens' phrase is the one secret the app puts on the clipboard,
//! and people copy it however often they are told to write it down. iced's
//! clipboard write cannot mark what it copies, so the phrase went out like an
//! address — and KDE's Klipper, or any other history keeper, recorded it
//! (Klipper keeps its history across sessions by default). This copy goes
//! through arboard, the crate, version and features iced already links, with
//! the mark clipboard managers honour (`x-kde-passwordManagerHint: secret`):
//! they keep it out of their history, and arboard never hands secret-marked
//! data to an X11 clipboard manager when the app quits.
//!
//! That is all it does. It is an ordinary copy otherwise: it stays until the
//! user copies something else or the app quits. The app never reads the
//! clipboard back and never clears it — the clipboard is the user's (user,
//! 2026-10-03). Addresses and transaction ids are public and stay ordinary
//! `iced::clipboard::write` copies.

use std::sync::Mutex;

#[cfg(target_os = "linux")]
use arboard::SetExtLinux;
use zeroize::Zeroizing;

/// Kept for the life of the process: on X11 a copy is served only while an
/// arboard `Clipboard` exists.
static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// Puts `text` on the clipboard, marked secret.
///
/// On a detached thread: the first copy connects to the display server, and
/// where Wayland's data-control protocol is missing arboard falls back to X11,
/// which can mean starting XWayland. Never `spawn_blocking` (ICED.md).
pub fn copy_secret(text: String) {
    // Wiped when dropped, whichever way this ends; arboard makes the one copy
    // it serves from.
    let text = Zeroizing::new(text);
    let _ = std::thread::Builder::new().name("secret-copy".into()).spawn(move || {
        let Ok(mut slot) = CLIPBOARD.lock() else { return };
        if slot.is_none() {
            *slot = arboard::Clipboard::new().ok();
        }
        let Some(clipboard) = slot.as_mut() else { return };
        // Windows and macOS have their own marks (`SetExtWindows`: history,
        // cloud and monitoring; `SetExtApple`: history) — wire them with those
        // ports.
        let set = clipboard.set();
        #[cfg(target_os = "linux")]
        let set = set.exclude_from_history();
        let _ = set.text(text.as_str());
    });
}
