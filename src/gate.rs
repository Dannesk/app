//! The app's startup view-gate secret — a **6-digit PIN**, always: a salted
//! SHA-256 of the digits in `gate.json`.
//!
//! Pure theatre, and the reason that is acceptable is that this gate never
//! protects wallet key material. Signing is a passphrase (Standard) or the
//! phrase itself (Cold); nothing here decrypts anything. Its only job is the
//! "nobody opens my app without unlocking it" expectation.
//!
//! So the hash is not a KDF and does not want to be. Six digits is a 10^6
//! space — Argon2id, scrypt and a bare SHA are equally breakable across it, and
//! a slow KDF would only add a blocking stall at every launch to protect a
//! screen that hides a balance. The real crypto is elsewhere, on a secret with
//! real entropy behind it.
//!
//! The rule this must keep is the one that made the *old* SHA gate dangerous:
//! never store anything offline-crackable that equals a signing credential. It
//! holds by construction now — the gate PIN unlocks a view and nothing else.

use crate::bridge::json_storage;
use sha2::{Digest, Sha256};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rand::RngExt;
use serde::{Deserialize, Serialize};

const FILE: &str = "gate.json";

/// Exact digits in the gate PIN. Fixed-length, which is what lets the last digit
/// submit the form (see the `GatePin` arm of `Message::SecureEdit`).
pub const PIN_DIGITS: usize = 6;

/// Wrong attempts allowed before the app self-wipes. On the `MAX_FAILS`-th miss
/// all wallet data (and this gate) is erased — a snooper on a lost device gets
/// an empty app; the real owner just re-imports from their 24-word seed.
pub const MAX_FAILS: u32 = 5;

#[derive(Serialize, Deserialize)]
struct GateData {
    hash: String, // Base64 SHA-256(salt || PIN)
    salt: String, // Base64 random salt
    /// Consecutive wrong attempts since the last success. Persisted (co-located
    /// with the hash) so a restart can't reset the snooper's count.
    #[serde(default)]
    fails: u32,
}

fn digest_str(secret: &str, salt: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(salt);
    h.update(secret.as_bytes());
    h.finalize().to_vec()
}

/// Read a *usable* gate record: one that carries a hash we could actually check
/// a PIN against.
///
/// The emptiness test is what retires the TPM-backed gate. That gate stored an
/// empty hash and salt on purpose — the chip was the verifier — so a record left
/// behind by the old build is a file this module cannot check anything against.
/// Read naively it is worse than useless: `digest_str(...) == []` is false for
/// every input, so the correct PIN reads as wrong five times and
/// [`register_fail`] erases the wallets. Treating it as *no gate* sends the user
/// to "create a PIN" instead, which is the honest outcome — the old secret lived
/// in a chip we no longer talk to, and nothing on disk can verify it.
fn read() -> Option<GateData> {
    let data = json_storage::read_json::<GateData>(FILE).ok()?;
    (!data.hash.is_empty() && !data.salt.is_empty()).then_some(data)
}

/// A gate has been registered (so the launch screen is "enter", not "create").
pub fn exists() -> bool {
    read().is_some()
}

/// Register a 6-digit PIN gate, clearing any prior fail state.
pub fn set_pin(pin: &str) -> Result<(), String> {
    let salt: [u8; 16] = rand::rng().random();
    let data = GateData {
        hash: BASE64.encode(digest_str(pin, &salt)),
        salt: BASE64.encode(salt),
        fails: 0,
    };
    json_storage::write_json(FILE, &data).map_err(|e| e.to_string())
}

/// Check an entered 6-digit PIN against the stored SHA hash.
///
/// `Ok(true)` verified · `Ok(false)` **wrong digits, positively identified** ·
/// `Err` = the record could not be read, which is not evidence about the PIN.
///
/// The three-way return exists because `Ok(false)` is what [`register_fail`]
/// counts toward erasing every wallet on the device, so it may only be returned
/// when the stored hash was actually read and actually disagreed. This used to
/// be a bare `bool` that answered `false` for an unreadable `gate.json` — so a
/// truncated file (and [`register_fail`] rewrites that file on every miss, with
/// `fs::write` truncate-in-place) turned the *correct* PIN into a wrong one.
pub fn verify_pin(pin: &str) -> Result<bool, String> {
    let data = read().ok_or("Could not read the gate record.")?;
    let (Ok(stored), Ok(salt)) = (BASE64.decode(&data.hash), BASE64.decode(&data.salt)) else {
        return Err("The gate record is corrupt (its hash could not be decoded).".into());
    };
    Ok(digest_str(pin, &salt).as_slice() == stored.as_slice())
}

/// Record a wrong attempt; returns attempts remaining before the wipe. `0` means
/// the limit is reached and the caller must erase everything.
///
/// ## Unreadable means *full credit*, never *wipe now*
///
/// `0` is not a neutral "unknown" — it is the caller's instruction to erase
/// every wallet on the device. So the only path that may return it is one that
/// read the record and found the streak exhausted.
///
/// This previously opened with `else { return 0 }` on a read failure, which
/// meant an unreadable `gate.json` erased everything on the *first* attempt,
/// correct PIN included. That is not a remote possibility: this function
/// rewrites that same file on every miss via `fs::write`, which truncates in
/// place with no temp file, no rename and no fsync, and it discards the result.
/// A full disk, a read-only filesystem or a power cut during the rewrite leaves
/// exactly the zero-length file that used to trigger the wipe — and the file is
/// rewritten most often precisely when the user is already failing to get in.
pub fn register_fail() -> u32 {
    let Some(mut data) = read() else {
        // Cannot read the streak, so cannot know it is exhausted. Grant the
        // full allowance: a snooper gains at most a few extra guesses at a
        // view-gate that protects no key material, while the owner keeps their
        // coins. That asymmetry is not close.
        return MAX_FAILS;
    };
    data.fails = (data.fails + 1).min(MAX_FAILS);
    let _ = json_storage::write_json(FILE, &data);
    MAX_FAILS.saturating_sub(data.fails)
}

/// Remove the gate entirely (self-wipe / erase-all / gate-off reset; next
/// launch starts at "create a PIN" if the gate is enabled).
pub fn delete() {
    let _ = json_storage::remove_json(FILE);
}

/// Launch-gate toggle from settings.json (default ON, so fresh installs run
/// first-launch set-up).
pub fn enabled() -> bool {
    json_storage::read_json::<serde_json::Value>("settings.json")
        .ok()
        .and_then(|j| j.get("gate_enabled").and_then(|v| v.as_bool()))
        .unwrap_or(true)
}

/// Clear the fail streak after a correct entry.
pub fn clear_attempts() {
    let Some(mut data) = read() else { return };
    if data.fails == 0 {
        return; // nothing to rewrite
    }
    data.fails = 0;
    let _ = json_storage::write_json(FILE, &data);
}
