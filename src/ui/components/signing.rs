//! The signing **contract** — what a screen must hold, and must ask, before it
//! can broadcast something irreversible.
//!
//! ## What this module is, and is not
//!
//! It is no longer a card. Signing used to float as a modal with two faces, and
//! this file carried that card's whole vocabulary — frame, eyebrow, summary,
//! wells, footer, review rows. The send flows now sign on a step of their own
//! (see [`crate::ui::components::review_sign`]), drawn from the import screen's
//! widgets, and all of that vocabulary went with the modal.
//!
//! What survives is the part that was never about drawing: [`SignFields`], the
//! bundle of buffers a signing surface holds, and the **gate** it answers to.
//! That is the security-relevant half, and it is shared for exactly one reason
//! — six screens asking for a credential six slightly different ways is six
//! chances for one of them to ask for less than the others.
//!
//! ## The gate asks one question
//!
//! A key of at least [`PASSPHRASE_MIN`] characters, or 24 words with a valid
//! checksum. That is all, and it is deliberately all: whether the key is
//! *correct* is decided by the thing that decrypts, on submit, because argon2id
//! is slow by design and nothing can be checked while typing. A screen that
//! claimed to know would be guessing.
//!
//! The cold-storage checksum is the one live verdict, and it earns its place:
//! it is cheap math, it is the same parse the bridge runs, and it catches a
//! mistyped word before a round trip that could not have succeeded.
//!
//! No character count is ever shown for a key — that hands its exact length to
//! anyone glancing at the screen. (At import the count *is* the advice, because
//! there the key is being chosen rather than recalled.)
//!
//! ## Detected, never chosen
//!
//! [`EnableInputMode::detect`] reads what this wallet actually takes. There is
//! no picker: offering "key / mnemonic" would invite someone to try the wrong
//! one and read the failure as a wrong key.

use bip39::{Language, Mnemonic};

use crate::controller::app_state::EnableInputMode;
use crate::controller::message::{Message, SecureField};
use crate::secure::SecureString;

// ── Gates ───────────────────────────────────────────────────────────────────

/// Characters a sign-time key must reach.
///
/// The same number import, create and reimport enforce, and that is the whole
/// argument. A key this app stored was set through one of those screens, so it
/// is at least this long; a gate any lower only lets someone press a button
/// with a key that cannot possibly be the right one, and spends a decryption
/// attempt to say so.
///
/// The gate is silent — the field's meta line names the key rather than a
/// minimum, because the minimum was import's advice and repeating it here reads
/// as a second rule. The button simply stays in its disabled clothes until the
/// count is reachable, which leaks nothing: every key is at least this long.
///
/// It follows that this number can never exceed
/// [`crate::ui::components::wallet_setup::KEY_MIN`] — a gate above it would
/// refuse to let someone type back a key this app itself agreed to store, which
/// is a wallet locked by its own setup screen and unopenable except by
/// re-importing the phrase. The test at the bottom of this file pins the two
/// together.
pub const PASSPHRASE_MIN: usize = 6;

/// Words in a recovery phrase.
pub const PHRASE_WORDS: usize = 24;

// ── The fields ──────────────────────────────────────────────────────────────

/// Everything a signing surface needs, gathered so the callers' signatures stay
/// about what is being signed rather than about how it is entered.
pub struct SignFields<'a> {
    pub mode: EnableInputMode,
    /// The key buffer. Cold storage ignores it.
    pub secret: SecureField,
    pub secret_buf: &'a SecureString,
    pub secret_reveal: bool,
    /// The 24-word buffer, used by cold storage only.
    pub phrase: SecureField,
    pub phrase_buf: &'a SecureString,
    pub phrase_reveal: bool,
    pub bip39: SecureField,
    pub bip39_buf: &'a SecureString,
    pub bip39_reveal: bool,
    pub on_submit: Message,
}

impl SignFields<'_> {
    /// Whether the credential is complete enough to try.
    ///
    /// The key check is only a length gate — whether it is *correct* is decided
    /// by the thing that decrypts, and a screen that claimed to know would be
    /// guessing. The phrase check is the full checksum, because that one *can*
    /// be known here, and a submit with a bad checksum is a doomed round trip.
    pub fn can_submit(&self) -> bool {
        match self.mode {
            EnableInputMode::Passphrase => self.secret_buf.trimmed_char_len() >= PASSPHRASE_MIN,
            EnableInputMode::Seed => phrase_checksum(self.phrase_buf.as_str()) == Some(true),
        }
    }
}

/// `None` until all 24 words are down; then whether the checksum resolves.
///
/// The *same* parse the bridge runs when it derives — `parse_in` splits on any
/// whitespace, so a phrase this returns `Some(true)` for is one signing will
/// accept. Checked only at the full word count: a red verdict against a phrase
/// someone is still typing would read as an error in the middle of an answer.
/// The parsed value is dropped on the spot; nothing here derives from it.
fn phrase_checksum(phrase: &str) -> Option<bool> {
    (phrase.split_whitespace().count() == PHRASE_WORDS)
        .then(|| Mnemonic::parse_in(Language::English, phrase).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::components::wallet_setup::KEY_MIN;

    /// The sign-time gate can never sit above the storage minimum — see
    /// [`PASSPHRASE_MIN`]. A wallet whose key this screen refuses to accept is
    /// one its own setup screen locked and only a re-import can open.
    #[test]
    fn the_gate_matches_what_storage_accepts() {
        assert_eq!(
            PASSPHRASE_MIN, KEY_MIN,
            "the sign gate and the storage floor have drifted"
        );
    }

    /// The checksum readout is the one live verdict on the sign step, and it
    /// has to agree with the parse the bridge runs — including staying silent
    /// until all 24 words are down.
    #[test]
    fn the_checksum_is_silent_until_the_phrase_is_whole() {
        assert_eq!(phrase_checksum(""), None);
        assert_eq!(phrase_checksum("abandon abandon abandon"), None);
        let short = "abandon ".repeat(PHRASE_WORDS - 1);
        assert_eq!(phrase_checksum(short.trim()), None);
        // 24 words, deliberately not a valid checksum.
        let bad = "abandon ".repeat(PHRASE_WORDS);
        assert_eq!(phrase_checksum(bad.trim()), Some(false));
        // The BIP39 test vector: 23 `abandon` and the word that closes it.
        let good = format!("{}art", "abandon ".repeat(PHRASE_WORDS - 1));
        assert_eq!(phrase_checksum(&good), Some(true));
    }
}
