

use dannesk_btc_codec::bip39::Mnemonic;

use crate::controller::app_state::EnableInputMode;
use crate::controller::message::{Message, SecureField};
use crate::secure::SecureString;


pub const PASSPHRASE_MIN: usize = 6;

pub const PHRASE_WORDS: usize = 24;


pub struct SignFields<'a> {
    pub mode: EnableInputMode,
    pub secret: SecureField,
    pub secret_buf: &'a SecureString,
    pub secret_reveal: bool,
    pub phrase: SecureField,
    pub phrase_buf: &'a SecureString,
    pub phrase_reveal: bool,
    pub bip39: SecureField,
    pub bip39_buf: &'a SecureString,
    pub bip39_reveal: bool,
    pub on_submit: Message,
}

impl SignFields<'_> {

    pub fn can_submit(&self) -> bool {
        match self.mode {
            EnableInputMode::Passphrase => self.secret_buf.trimmed_char_len() >= PASSPHRASE_MIN,
            EnableInputMode::Seed => phrase_checksum(self.phrase_buf.as_str()) == Some(true),
        }
    }
}

fn phrase_checksum(phrase: &str) -> Option<bool> {
    (phrase.split_whitespace().count() == PHRASE_WORDS)
        .then(|| Mnemonic::parse(phrase).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::components::wallet_setup::KEY_MIN;


    #[test]
    fn the_gate_matches_what_storage_accepts() {
        assert_eq!(
            PASSPHRASE_MIN, KEY_MIN,
            "the sign gate and the storage floor have drifted"
        );
    }


    #[test]
    fn the_checksum_is_silent_until_the_phrase_is_whole() {
        assert_eq!(phrase_checksum(""), None);
        assert_eq!(phrase_checksum("abandon abandon abandon"), None);
        let short = "abandon ".repeat(PHRASE_WORDS - 1);
        assert_eq!(phrase_checksum(short.trim()), None);
        let bad = "abandon ".repeat(PHRASE_WORDS);
        assert_eq!(phrase_checksum(bad.trim()), Some(false));
        let good = format!("{}art", "abandon ".repeat(PHRASE_WORDS - 1));
        assert_eq!(phrase_checksum(&good), Some(true));
    }
}
