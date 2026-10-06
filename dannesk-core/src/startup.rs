//! Process-wide one-offs that must run before any socket opens.
//!
//! The version check that used to live here (a `reqwest` to the landing
//! page's `version.json`, gating the whole app behind an "Update Available"
//! wall) is GONE, 2026-09-05. A self-custody wallet must never refuse to open
//! on a version string our server chose, and apt is the update path on the
//! launch platform. A version floor, if one is ever needed, belongs in the
//! proxy hello where the server can say so in-band; a notice belongs on the
//! notification path, not a wall.

pub fn init_globals() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
}
