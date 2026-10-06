//! Dannesk's core: everything under the user interface, shared by the desktop
//! app and Android. The one socket to the proxy and what its frames mean
//! (`ws`), the state bus an interface subscribes to (`channel`), wallet
//! creation, import, sending, signing and storage (`bridge`, `wallet`,
//! `encrypt`, `decrypt`, `secure`), the launch-PIN gate (`gate`) and the
//! chain-neutral helpers in `utils`.
//!
//! An app calls [`init`] once, before anything else, and names what the core
//! cannot know on its own: its version and the directory its files live in.
//! Everything platform-shaped stays in the app: the window or the Activity,
//! fonts, the clipboard, and where that directory is.

pub mod bridge;
pub mod btc_script_type;
pub mod channel;
pub mod decrypt;
pub mod encrypt;
pub mod gate;
pub mod secure;
mod startup;
pub mod utils;
pub mod wallet;
pub mod ws;

use std::path::PathBuf;
use std::sync::OnceLock;

/// What an app tells the core about itself, once, through [`init`].
#[derive(Debug, Clone)]
pub struct App {
    /// The app's own version, sent to the proxy in the session's hello frame.
    /// It is the app's `CARGO_PKG_VERSION`, never this crate's: the core has a
    /// version of its own, and it is not the one a user reports.
    pub version: &'static str,
    /// The directory every file of the app lives in: the wallet records, the
    /// encrypted phrases, the gate and the settings. Desktop names `Dannesk`
    /// under the user's config directory; Android passes its files directory
    /// in from Kotlin. The core never guesses one.
    pub data_dir: PathBuf,
}

static APP: OnceLock<App> = OnceLock::new();

/// Registers the app and installs the TLS provider. Call it first, before the
/// runtime starts and before any file is read, and only once: a second call
/// changes nothing and hands the argument back as the error.
pub fn init(app: App) -> Result<(), App> {
    APP.set(app)?;
    startup::init_globals();
    Ok(())
}

/// The registered app, once [`init`] has run.
pub fn app() -> Option<&'static App> {
    APP.get()
}
