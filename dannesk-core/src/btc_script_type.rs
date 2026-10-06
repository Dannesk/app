//! The BTC wallet's **address type** — which of the four standard script
//! families a seed derives to, chosen once at import (or create) and stored
//! in btc.json as `script_type`. One wallet is one type: a seed that was used
//! both ways elsewhere is two imports here (the Sparrow model), never a
//! four-way scan.
//!
//! What the type decides:
//! - the BIP-32 purpose the account key hangs under (44' / 49' / 84' / 86'),
//! - how a derived public key is encoded into an address,
//! - the input weight the fee quote budgets for a coin of this wallet,
//! - the size of the change output (this wallet's own script).
//!
//! What it does NOT decide: how an input is signed. The signer reads the
//! owner address of every coin and branches on the script the crate parses
//! out of it ([`BtcScriptType::of_address`]) — the address is
//! self-describing, so signing never trusts a stored field. A test pins the
//! two views to each other.
//!
//! The frozen bc1q path is untouched: every existing derivation site still
//! runs its literal `m/84'/0'/0'/0/0` block as it always has, and this module
//! only takes over for a wallet whose stored type is NOT native segwit.
//! Native is the default for a btc.json that has no `script_type` field, so
//! every wallet that exists today reads as exactly what it was.

use bitcoin::address::AddressType;
use bitcoin::bip32::DerivationPath;
use bitcoin::key::UntweakedPublicKey;
use bitcoin::secp256k1::{Secp256k1, Verification};
use bitcoin::{Address, CompressedPublicKey, Network};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BtcScriptType {
    /// BIP-84, `bc1q…` (P2WPKH). The default, and the only type that existed
    /// before 2026-09-14.
    #[default]
    NativeSegwit,
    /// BIP-86, `bc1p…` (P2TR key-path).
    Taproot,
    /// BIP-49, `3…` (P2SH-wrapped P2WPKH).
    NestedSegwit,
    /// BIP-44, `1…` (P2PKH).
    Legacy,
}

impl BtcScriptType {
    /// What import offers, in display order: the modern default first, then
    /// the ones people arrive with.
    pub const IMPORT: [Self; 4] = [Self::NativeSegwit, Self::Taproot, Self::NestedSegwit, Self::Legacy];

    /// What create offers. A new wallet has no history to match, so the two
    /// old encodings are not on the menu: they cost more per input and mark
    /// the wallet as pre-2017 software.
    pub const CREATE: [Self; 2] = [Self::NativeSegwit, Self::Taproot];

    /// The BIP-43 purpose field.
    pub fn purpose(self) -> u32 {
        match self {
            Self::NativeSegwit => 84,
            Self::Taproot => 86,
            Self::NestedSegwit => 49,
            Self::Legacy => 44,
        }
    }

    /// The btc.json value. Stable — a rename here strands every wallet of
    /// that type as "native" and its addresses stop verifying at signing.
    pub fn tag(self) -> &'static str {
        match self {
            Self::NativeSegwit => "native",
            Self::Taproot => "taproot",
            Self::NestedSegwit => "nested",
            Self::Legacy => "legacy",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::IMPORT.into_iter().find(|t| t.tag() == tag)
    }

    /// The address prefix — what people actually recognise a wallet by, and
    /// the picker's segment label.
    pub fn prefix(self) -> &'static str {
        match self {
            Self::NativeSegwit => "bc1q",
            Self::Taproot => "bc1p",
            Self::NestedSegwit => "3\u{2026}",
            Self::Legacy => "1\u{2026}",
        }
    }

    /// The type's name — the picker card's second line.
    pub fn name(self) -> &'static str {
        match self {
            Self::NativeSegwit => "native segwit",
            Self::Taproot => "taproot",
            Self::NestedSegwit => "nested segwit",
            Self::Legacy => "legacy",
        }
    }

    /// The BIP that defines the type — the picker eyebrow's right slot.
    pub fn bip(self) -> &'static str {
        match self {
            Self::NativeSegwit => "bip-84",
            Self::Taproot => "bip-86",
            Self::NestedSegwit => "bip-49",
            Self::Legacy => "bip-44",
        }
    }

    /// The #0 path, for the recovery-phrase eyebrow. Native's copy is pinned
    /// to the frozen deriver's literal by `btcsetup`'s test.
    pub fn path(self) -> &'static str {
        match self {
            Self::NativeSegwit => "m/84'/0'/0'/0/0",
            Self::Taproot => "m/86'/0'/0'/0/0",
            Self::NestedSegwit => "m/49'/0'/0'/0/0",
            Self::Legacy => "m/44'/0'/0'/0/0",
        }
    }

    /// `m/{purpose}'/0'/0'` — where the account xpub lives.
    pub fn account_path(self) -> DerivationPath {
        DerivationPath::from_str(&format!("m/{}'/0'/0'", self.purpose()))
            .expect("account path is a literal")
    }

    /// `m/{purpose}'/0'/0'/{chain}/{index}` — one member address.
    pub fn member_path(self, chain: u32, index: u32) -> DerivationPath {
        DerivationPath::from_str(&format!("m/{}'/0'/0'/{}/{}", self.purpose(), chain, index))
            .expect("member path is a literal with two indices")
    }

    /// Encode a derived compressed public key as this type's address.
    pub fn address<C: Verification>(self, secp: &Secp256k1<C>, pk: &CompressedPublicKey) -> Address {
        match self {
            Self::NativeSegwit => Address::p2wpkh(pk, Network::Bitcoin),
            Self::Taproot => {
                let internal: UntweakedPublicKey = pk.0.into();
                Address::p2tr(secp, internal, None, Network::Bitcoin)
            }
            Self::NestedSegwit => Address::p2shwpkh(pk, Network::Bitcoin),
            Self::Legacy => Address::p2pkh(pk, Network::Bitcoin),
        }
    }

    /// The type an address string is, as the crate parses it. `None` for
    /// anything this wallet cannot own (P2WSH, P2A, a foreign network, or
    /// not an address at all).
    pub fn of_address(address: &str) -> Option<Self> {
        let addr = Address::from_str(address).ok()?.require_network(Network::Bitcoin).ok()?;
        match addr.address_type()? {
            AddressType::P2wpkh => Some(Self::NativeSegwit),
            AddressType::P2tr => Some(Self::Taproot),
            AddressType::P2sh => Some(Self::NestedSegwit),
            AddressType::P2pkh => Some(Self::Legacy),
            _ => None,
        }
    }

    /// This type's scriptPubKey length — what a change output costs.
    pub fn spk_len(self) -> usize {
        match self {
            Self::NativeSegwit => 22,
            Self::Taproot => 34,
            Self::NestedSegwit => 23,
            Self::Legacy => 25,
        }
    }

    /// Bytes an input of this type adds to the transaction's non-witness
    /// serialisation beyond the 41 fixed ones (outpoint 36, scriptSig length
    /// 1, sequence 4): the scriptSig itself.
    pub fn script_sig_len(self) -> u64 {
        match self {
            // Empty scriptSig.
            Self::NativeSegwit | Self::Taproot => 0,
            // One push of the 22-byte redeem script (`0014{hash}`).
            Self::NestedSegwit => 23,
            // Worst-case 73-byte DER signature (incl. sighash byte) and the
            // 33-byte key, each behind a push byte.
            Self::Legacy => 108,
        }
    }

    /// Bytes an input of this type adds to the witness: item count plus each
    /// item behind its length byte. Worst case, so the paid rate lands at or
    /// above the one picked.
    pub fn witness_len(self) -> u64 {
        match self {
            // 1 + (1 + 73) + (1 + 33)
            Self::NativeSegwit | Self::NestedSegwit => 109,
            // 1 + (1 + 64): a Schnorr signature is fixed-size, and the
            // default sighash type is implied rather than appended.
            Self::Taproot => 66,
            Self::Legacy => 0,
        }
    }
}

/// The type of the wallet on this device, from btc.json. Absent field, or no
/// wallet at all, reads as native segwit — the type every wallet written
/// before this field existed actually is.
pub fn stored() -> BtcScriptType {
    crate::bridge::json_storage::read_json::<serde_json::Value>("btc.json")
        .ok()
        .and_then(|j| j.get("script_type").and_then(|v| v.as_str()).and_then(BtcScriptType::from_tag))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bip39::{Language, Mnemonic};
    use bitcoin::bip32::Xpriv;

    /// The BIP-39 test mnemonic every BIP's own vectors use.
    const ABANDON: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn first_address(t: BtcScriptType) -> String {
        let secp = Secp256k1::new();
        let seed = Mnemonic::parse_in(Language::English, ABANDON).unwrap().to_seed("");
        let master = Xpriv::new_master(Network::Bitcoin, &seed).unwrap();
        let child = master.derive_priv(&secp, &t.member_path(0, 0)).unwrap();
        let pk = CompressedPublicKey(child.to_priv().public_key(&secp).inner);
        t.address(&secp, &pk).to_string()
    }

    /// The first receive address of each type, against the vectors the BIPs
    /// (84, 86) and the reference implementations (44, 49) publish for the
    /// `abandon … about` mnemonic. A wallet imported with the wrong encoding
    /// lands on an address that exists but holds nothing — this is the test
    /// that says we land where Electrum, Sparrow and the hardware wallets do.
    #[test]
    fn first_addresses_match_the_published_vectors() {
        assert_eq!(first_address(BtcScriptType::NativeSegwit), "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu");
        assert_eq!(
            first_address(BtcScriptType::Taproot),
            "bc1p5cyxnuxmeuwuvkwfem96lqzszd02n6xdcjrs20cac6yqjjwudpxqkedrcr"
        );
        assert_eq!(first_address(BtcScriptType::NestedSegwit), "37VucYSaXLCAsxYyAPfbSi9eh4iEcbShgf");
        assert_eq!(first_address(BtcScriptType::Legacy), "1LqBGSKuX5yYUonjxT5qGfpUsXKYYWeabA");
    }

    /// The stored type and the parsed type agree for every address the
    /// wallet can derive — the signer branches on the second, the deriver on
    /// the first, and a disagreement would be a coin shown but unsignable.
    #[test]
    fn a_derived_address_parses_back_to_its_type() {
        for t in BtcScriptType::IMPORT {
            assert_eq!(BtcScriptType::of_address(&first_address(t)), Some(t), "{t:?}");
        }
    }

    #[test]
    fn spk_lengths_are_the_real_scripts() {
        for t in BtcScriptType::IMPORT {
            let addr = Address::from_str(&first_address(t)).unwrap().assume_checked();
            assert_eq!(addr.script_pubkey().len(), t.spk_len(), "{t:?}");
        }
    }

    #[test]
    fn tags_round_trip_and_native_is_the_default() {
        for t in BtcScriptType::IMPORT {
            assert_eq!(BtcScriptType::from_tag(t.tag()), Some(t));
        }
        assert_eq!(BtcScriptType::from_tag("p2wpkh"), None);
        assert_eq!(BtcScriptType::default(), BtcScriptType::NativeSegwit);
    }

    #[test]
    fn paths_carry_the_purpose() {
        for t in BtcScriptType::IMPORT {
            assert_eq!(t.path(), format!("m/{}'/0'/0'/0/0", t.purpose()));
            assert_eq!(t.member_path(0, 0).to_string(), t.path().trim_start_matches("m/"));
        }
    }
}
