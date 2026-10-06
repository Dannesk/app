// ONE socket since 2026-09-04 (workspace/TRANSPORT-SPEC.md): the proxy fans it
// to relay, indexd's wallet listener (the Bitcoin relay, tag 0x04), rates and
// bookd.
//
// Which proxy is chosen by the build profile, like the hardening in
// `secure.rs` (2026-09-29). A debug build (`cargo run`) dials a proxy on this
// machine, run in plain mode (`proxy --plain 127.0.0.1:8443`). A release build
// (`cargo run --release`, and what the .deb ships) dials production: the same
// path on TLS 443, behind Cloudflare.
#[cfg(debug_assertions)]
pub const WS_URL: &str = "ws://127.0.0.1:8443/ws";
#[cfg(not(debug_assertions))]
pub const WS_URL: &str = "wss://proxy.dannesk.com/ws";

// Stream tags — byte 0 of every frame in both directions. Byte 1 is flags
// (bit 0 = zstd), bytes 2… the payload exactly as the service sends or reads it.
pub const TAG_PROXY: u8 = 0x00;
pub const TAG_RELAY: u8 = 0x01;
pub const TAG_RATES: u8 = 0x02;
pub const TAG_BOOK: u8 = 0x03;
// Bitcoin left relay for its own process on 2026-09-14 (stage 1 of the BTC
// transport split); same envelope and gate, its own stream and link.
pub const TAG_BTC: u8 = 0x04;
pub const FLAG_ZSTD: u8 = 0x01;
