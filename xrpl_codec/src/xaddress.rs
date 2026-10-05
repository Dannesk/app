//! Single resolution point for an XRP send recipient. Accepts either a classic
//! r-address (optionally paired with a separately-entered destination tag) or an
//! X-address (which bundles the r-address + tag + network into one string), and
//! collapses both to the same `(classic address, Option<tag>)` the Payment needs.
//!
//! X-addresses exist precisely so users can't forget the destination tag that
//! exchanges require to route a pooled-wallet deposit to the right customer, so
//! decoding the tag back out is the whole point — surface it, never drop it.
//!
//! Both address forms are Base58Check (double-SHA256, 4-byte checksum) over
//! the ripple alphabet — the same encoding an account's own r-address is
//! derived with. Decoding and encoding are written by hand: the payload
//! layouts are fixed by the ledger and fit in a screen of code. [`encode`] is
//! the receive side — the wallet's own address with a tag of the user's
//! choosing, so a payer who routes by tag can be handed one string.
//!
//! ```text
//! classic:   0x00 ‖ account_id[20]                              (21 bytes)
//! X-address: prefix[2] ‖ account_id[20] ‖ flag ‖ tag[8, LE]     (31 bytes)
//! ```
//!
//! where prefix = 05 44 (mainnet) / 04 93 (testnet), flag 0 = no tag (tag
//! bytes must be zero), 1 = 32-bit tag, 2 = 64-bit tag (rejected — the ledger
//! has no 64-bit tags).

const XRPL_ALPHABET: bs58::Alphabet =
    bs58::Alphabet::new_unwrap(b"rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz");
const XADDRESS_PREFIX_MAIN: [u8; 2] = [0x05, 0x44];
const XADDRESS_PREFIX_TEST: [u8; 2] = [0x04, 0x93];

/// Base58Check-decode with the ripple alphabet; checksum failures are `None`.
fn decode_check(s: &str) -> Option<Vec<u8>> {
    bs58::decode(s)
        .with_alphabet(&XRPL_ALPHABET)
        .with_check(None)
        .into_vec()
        .ok()
}

/// True for a well-formed classic address: valid checksum, 0x00 type byte,
/// 20-byte account id.
pub fn is_valid_classic_address(s: &str) -> bool {
    matches!(decode_check(s).as_deref(), Some([0x00, rest @ ..]) if rest.len() == 20)
}

/// The 20-byte account id inside a classic address, which is what a
/// transaction carries in its AccountID fields ([`crate::account`]). `None`
/// for anything that is not a classic address.
pub fn account_id(classic: &str) -> Option<[u8; 20]> {
    match decode_check(classic)?.as_slice() {
        [0x00, rest @ ..] => rest.try_into().ok(),
        _ => None,
    }
}

fn encode_classic_address(account_id: &[u8]) -> String {
    let mut payload = [0u8; 21];
    payload[1..].copy_from_slice(account_id);
    bs58::encode(payload)
        .with_alphabet(&XRPL_ALPHABET)
        .with_check()
        .into_string()
}

/// Encode a wallet's own classic address, with or without a destination
/// tag, as a MAINNET X-address — the tagged form of a receive address. The
/// inverse of what [`resolve`] decodes, over the same 31-byte layout: `05 44` ‖
/// account_id ‖ flag ‖ tag as eight little-endian bytes (flag 1 = a 32-bit
/// tag, the upper four bytes zero; flag 0 = no tag, all eight zero). `None`
/// when `classic` is not a valid r-address.
pub fn encode(classic: &str, tag: Option<u32>) -> Option<String> {
    let bytes = decode_check(classic.trim())?;
    let [0x00, account_id @ ..] = bytes.as_slice() else {
        return None;
    };
    if account_id.len() != 20 {
        return None;
    }
    let mut payload = [0u8; 31];
    payload[..2].copy_from_slice(&XADDRESS_PREFIX_MAIN);
    payload[2..22].copy_from_slice(account_id);
    if let Some(t) = tag {
        payload[22] = 1;
        payload[23..27].copy_from_slice(&t.to_le_bytes());
    }
    Some(
        bs58::encode(payload)
            .with_alphabet(&XRPL_ALPHABET)
            .with_check()
            .into_string(),
    )
}

/// Decode a MAINNET X-address to (classic address, tag). `None` for a bad
/// checksum, a testnet prefix, an unknown prefix, a 64-bit tag, or a no-tag
/// address whose tag bytes are not all zero.
fn xaddress_to_classic(s: &str) -> Option<(String, Option<u32>)> {
    let bytes = decode_check(s)?;
    if bytes.len() != 31 {
        return None;
    }
    let (prefix, rest) = bytes.split_at(2);
    if prefix == XADDRESS_PREFIX_TEST || prefix != XADDRESS_PREFIX_MAIN {
        return None;
    }
    let (account_id, tail) = rest.split_at(20);
    let flag = tail[0];
    let tag_bytes = &tail[1..9];
    let tag = match flag {
        0 => {
            if tag_bytes.iter().any(|&b| b != 0) {
                return None;
            }
            None
        }
        1 => {
            // 32-bit tag, little-endian, upper four bytes must be zero.
            if tag_bytes[4..].iter().any(|&b| b != 0) {
                return None;
            }
            Some(u32::from_le_bytes([tag_bytes[0], tag_bytes[1], tag_bytes[2], tag_bytes[3]]))
        }
        _ => return None,
    };
    Some((encode_classic_address(account_id), tag))
}

/// A recipient string resolved to what the Payment actually carries.
pub struct Resolved {
    /// Classic r-address to use as the Payment destination.
    pub classic: String,
    /// Destination tag. For an X-address this is decoded from the address; for a
    /// plain r-address it is whatever the caller supplies (None here).
    pub tag: Option<u32>,
    /// True when the input was an X-address — the tag is baked in and must not be
    /// overridden by a manually-entered one.
    pub from_xaddress: bool,
}

/// True if `s` looks like a mainnet X-address ('X' prefix). A cheap prefix check
/// for UI branching; real validity comes from `resolve`. Testnet X-addresses ('T'
/// prefix) are deliberately not recognised — this is a mainnet-only crate, and a
/// 'T' string decodes to a real classic address that would receive real funds.
pub fn is_xaddress(s: &str) -> bool {
    s.trim().starts_with('X')
}

/// Resolve a user-entered recipient to its classic address + tag. Returns `None`
/// for anything that isn't a valid XRP destination. The manual-tag path for a
/// plain r-address is left to the caller (this returns `tag: None` for those);
/// only X-addresses carry a tag out of here.
pub fn resolve(input: &str) -> Option<Resolved> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if is_xaddress(s) {
        let (classic, tag) = xaddress_to_classic(s)?;
        Some(Resolved {
            classic,
            tag,
            from_xaddress: true,
        })
    } else if s.starts_with('r') {
        if !is_valid_classic_address(s) {
            return None;
        }
        Some(Resolved {
            classic: s.to_string(),
            tag: None,
            from_xaddress: false,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage() {
        assert!(resolve("").is_none());
        assert!(resolve("not-an-address").is_none());
        assert!(resolve("0xdeadbeef").is_none());
    }

    #[test]
    fn accepts_plain_r_address_without_tag() {
        let r = "rPEPPER7kfTD9w2To4CQk6UCfuHM9c6GDY";
        let got = resolve(r).expect("valid classic address");
        assert_eq!(got.classic, r);
        assert_eq!(got.tag, None);
        assert!(!got.from_xaddress);
    }

    #[test]
    fn account_id_is_the_classic_address_payload() {
        let r = "rPEPPER7kfTD9w2To4CQk6UCfuHM9c6GDY";
        let id = account_id(r).expect("valid classic address");
        assert_eq!(encode_classic_address(&id), r);
        // A bad checksum or an X-address is not a classic address.
        assert!(account_id("rPEPPER7kfTD9w2To4CQk6UCfuHM9c6GDZ").is_none());
        assert!(account_id("X7AcgcsBL6XDcUb289X4mJ8djcdyKaB5hJDWMArnXr61cqZ").is_none());
    }

    // Reference vectors from the XRPL address-codec test suite (shared by
    // xrpl.js / xrpl-py / xrpl-rust): one account, four tags.
    const CLASSIC: &str = "r9cZA1mLK5R5Am25ArfXFmqgNwjZgnfk59";

    #[test]
    fn decodes_xaddress_to_classic_and_tag() {
        for (x, tag) in [
            ("X7AcgcsBL6XDcUb289X4mJ8djcdyKaB5hJDWMArnXr61cqZ", None),
            ("X7AcgcsBL6XDcUb289X4mJ8djcdyKaGZMhc9YTE92ehJ2Fu", Some(1)),
            ("X7AcgcsBL6XDcUb289X4mJ8djcdyKaGo2K5VpXpmCqbV2gS", Some(14)),
            ("X7AcgcsBL6XDcUb289X4mJ8djcdyKaLFuhLRuNXPrDeJd9A", Some(11747)),
        ] {
            let got = resolve(x).expect("valid x-address");
            assert!(got.from_xaddress);
            assert_eq!(got.classic, CLASSIC, "{x}");
            assert_eq!(got.tag, tag, "{x}");
            assert!(is_valid_classic_address(&got.classic));
        }
        // A second account, so the account-id slice is not a fixed-vector fluke.
        let got = resolve("XVZVpQj8YSVpNyiwXYSqvQoQqgBttTxAZwMcuJd4xteQHyt").unwrap();
        assert_eq!(got.classic, "rLczgQHxPhWtjkaQqn3Q6UM8AbRbbRvs5K");
        assert_eq!(got.tag, None);
    }

    #[test]
    fn encodes_classic_and_tag_to_the_reference_xaddress() {
        for (x, tag) in [
            ("X7AcgcsBL6XDcUb289X4mJ8djcdyKaB5hJDWMArnXr61cqZ", None),
            ("X7AcgcsBL6XDcUb289X4mJ8djcdyKaGZMhc9YTE92ehJ2Fu", Some(1)),
            ("X7AcgcsBL6XDcUb289X4mJ8djcdyKaGo2K5VpXpmCqbV2gS", Some(14)),
            ("X7AcgcsBL6XDcUb289X4mJ8djcdyKaLFuhLRuNXPrDeJd9A", Some(11747)),
        ] {
            assert_eq!(encode(CLASSIC, tag).as_deref(), Some(x), "tag {tag:?}");
        }
        assert_eq!(
            encode("rLczgQHxPhWtjkaQqn3Q6UM8AbRbbRvs5K", None).as_deref(),
            Some("XVZVpQj8YSVpNyiwXYSqvQoQqgBttTxAZwMcuJd4xteQHyt")
        );
        // The largest tag the ledger has: every one of the four tag bytes set,
        // the upper four still zero, and the decoder reads it straight back.
        let x = encode(CLASSIC, Some(u32::MAX)).unwrap();
        let back = resolve(&x).unwrap();
        assert_eq!(back.classic, CLASSIC);
        assert_eq!(back.tag, Some(u32::MAX));
    }

    #[test]
    fn encode_refuses_anything_but_a_classic_address() {
        assert!(encode("", Some(1)).is_none());
        assert!(encode("r9cZA1mLK5R5Am25ArfXFmqgNwjZgnfk58", Some(1)).is_none());
        // An X-address is not a classic address: no double-wrapping.
        assert!(encode("X7AcgcsBL6XDcUb289X4mJ8djcdyKaB5hJDWMArnXr61cqZ", None).is_none());
    }

    #[test]
    fn rejects_testnet_xaddress() {
        // Same account, testnet prefix. Refused at the 'T' prefix by
        // `is_xaddress`, and by the prefix bytes inside the decoder too.
        let t = "T719a5UwUCnEs54UsxG9CJYYDhwmFCqkr7wxCcNcfZ6p5GZ";
        assert!(resolve(t).is_none());
        assert!(xaddress_to_classic(t).is_none());
    }

    #[test]
    fn rejects_bad_checksum() {
        // Flip the last character of a valid X-address and a valid r-address.
        assert!(resolve("X7AcgcsBL6XDcUb289X4mJ8djcdyKaB5hJDWMArnXr61cqY").is_none());
        assert!(!is_valid_classic_address("r9cZA1mLK5R5Am25ArfXFmqgNwjZgnfk58"));
        assert!(resolve("r9cZA1mLK5R5Am25ArfXFmqgNwjZgnfk58").is_none());
    }

    #[test]
    fn classic_encode_roundtrips() {
        let bytes = decode_check(CLASSIC).unwrap();
        assert_eq!(bytes.len(), 21);
        assert_eq!(bytes[0], 0x00);
        assert_eq!(encode_classic_address(&bytes[1..]), CLASSIC);
    }
}
