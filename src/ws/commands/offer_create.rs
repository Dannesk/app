// ws/commands/offer_create.rs
use crate::channel::WSCommand;
use crate::ws::commands::wallet_auth::Bip44Wallet;
use rippled_binary_codec::serialize::serialize_tx;
use serde_json::{json, Value};

// OfferCreate `Flags` bits. https://xrpl.org/offercreate.html#offercreate-flags
const TF_IMMEDIATE_OR_CANCEL: u32 = 0x0002_0000;
const TF_FILL_OR_KILL: u32 = 0x0004_0000;
const TF_SELL: u32 = 0x0008_0000;

// Manual signing and hashing
use bitcoin::secp256k1::{Message, Secp256k1};
use sha2::{Digest, Sha512};

// --- HIGH PRECISION HELPERS ---

fn xrp_str_to_drops(xrp_str: &str) -> Result<String, String> {
    if xrp_str.is_empty()
        || !xrp_str
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
    {
        return Err(
            "Invalid XRP amount format: must be numeric with optional decimal.".to_string(),
        );
    }

    let negative = if xrp_str.starts_with('-') { -1 } else { 1 };
    let abs_str = xrp_str.trim_start_matches('-');

    let parts: Vec<&str> = abs_str.split('.').collect();
    if parts.len() > 2 {
        return Err("Invalid XRP amount: too many decimal points.".to_string());
    }

    let integer_part = parts[0].trim_start_matches('0'); // Remove leading zeros for safety
    let integer_str = if integer_part.is_empty() {
        "0"
    } else {
        integer_part
    };

    let mut fractional_part = String::new();
    if parts.len() == 2 {
        fractional_part = parts[1].to_string();
    }

    // Pad or truncate fractional to exactly 6 digits (XRPL XRP precision)
    while fractional_part.len() < 6 {
        fractional_part.push('0');
    }
    if fractional_part.len() > 6 {
        fractional_part.truncate(6);
    }

    // Parse to u128 for safety (u64 max covers XRPL limits: ~10^17 drops)
    let integer_drops: u128 = integer_str
        .parse()
        .map_err(|_| "Invalid integer part.".to_string())?;
    let fractional_drops: u128 = fractional_part
        .parse()
        .map_err(|_| "Invalid fractional part.".to_string())?;

    let total_drops = integer_drops * 1_000_000 + fractional_drops;
    if negative < 0 && total_drops > 0 {
        return Err("Negative XRP amounts not supported.".to_string());
    }

    Ok(total_drops.to_string())
}

/// Validate and normalize a positive decimal amount string for an XRPL issued
/// currency, without round-tripping through f64.
///
/// Going through `f64` injects representation noise into the value string —
/// "1000000.005" becomes "1000000.005000000004657" — producing an over-precise
/// mantissa that XRPL rejects or silently re-rounds. XRPL issued currencies are
/// 15-significant-digit decimals, so we keep the user's digits exactly and
/// reject anything that exceeds that precision. (Kept in sync with the twin in
/// payment.rs.)
fn normalize_issued_value(amount_str: &str) -> Result<String, String> {
    let s = amount_str.trim();
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err("Invalid amount format: must be a positive decimal number.".to_string());
    }

    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() > 2 {
        return Err("Invalid amount: too many decimal points.".to_string());
    }
    let integer_part = parts[0];
    let fractional_part = if parts.len() == 2 { parts[1] } else { "" };

    // Significant digits = all digits with leading and trailing zeros removed.
    let sig_digits = format!("{}{}", integer_part, fractional_part)
        .trim_start_matches('0')
        .trim_end_matches('0')
        .len();
    if sig_digits == 0 {
        return Err("Amount must be greater than zero.".to_string());
    }
    if sig_digits > 15 {
        return Err(
            "Amount has too many significant digits (max 15 for issued currencies).".to_string(),
        );
    }

    // Reassemble canonical form: drop leading integer zeros and trailing
    // fractional zeros.
    let int_norm = integer_part.trim_start_matches('0');
    let int_out = if int_norm.is_empty() { "0" } else { int_norm };
    let frac_norm = fractional_part.trim_end_matches('0');
    Ok(if frac_norm.is_empty() {
        int_out.to_string()
    } else {
        format!("{}.{}", int_out, frac_norm)
    })
}

fn get_asset_config(symbol: &str) -> Option<(&'static str, &'static str)> {
    crate::utils::tokens::by_code(symbol).map(|t| (t.currency_hex, t.issuer))
}

/// Amount in the JSON shape the binary codec expects: XRP is a bare drops
/// string, an issued currency is `{ currency, issuer, value }`.
fn to_xrpl_amount(amount_str: &str, currency: &str) -> Result<Value, String> {
    if currency == "XRP" {
        let drops = xrp_str_to_drops(amount_str)?;
        if drops == "0" {
            return Err("Amount must be greater than zero.".to_string());
        }
        Ok(Value::String(drops))
    } else {
        let (hex, issuer) = get_asset_config(currency)
            .ok_or_else(|| format!("Unsupported currency: {}", currency))?;

        let value = normalize_issued_value(amount_str)?;

        Ok(json!({
            "currency": hex,
            "issuer": issuer,
            "value": value,
        }))
    }
}

/// Fold the command's flag names into the `Flags` bitmask. Unknown names are
/// ignored, as before.
fn offer_flags(names: Option<&Vec<String>>) -> u32 {
    let mut flags = 0u32;
    if let Some(names) = names {
        for flag in names {
            match flag.as_str() {
                "tfFillOrKill" => flags |= TF_FILL_OR_KILL,
                "tfImmediateOrCancel" => flags |= TF_IMMEDIATE_OR_CANCEL,
                // Sell the whole TakerGets at the ratio or better, rather
                // than stopping once TakerPays is received. Set when the
                // user typed the pay side — "pay" then means pay.
                "tfSell" => flags |= TF_SELL,
                _ => (),
            }
        }
    }
    flags
}

// --- CORE LOGIC ---

pub async fn construct_blob(
    wallet_obj: &Bip44Wallet,
    cmd: &WSCommand,
    sequence: u32,
    fee: String,
    last_ledger_sequence: u32,
) -> Result<String, String> {
    let taker_pays_raw = cmd.taker_pays.as_ref().ok_or("Missing taker_pays")?;
    let taker_gets_raw = cmd.taker_gets.as_ref().ok_or("Missing taker_gets")?;

    let taker_pays_amount = to_xrpl_amount(&taker_pays_raw.0, &taker_pays_raw.1)?;
    let taker_gets_amount = to_xrpl_amount(&taker_gets_raw.0, &taker_gets_raw.1)?;

    let pub_key_hex = hex::encode(wallet_obj.public_key.serialize()).to_uppercase();

    // The unsigned OfferCreate as JSON for `serialize_tx`; the codec
    // canonicalises field order. `Flags` is always present (0 when none).
    let mut tx_json_val = json!({
        "TransactionType": "OfferCreate",
        "Account": wallet_obj.address,
        "Fee": fee,
        "Sequence": sequence,
        "LastLedgerSequence": last_ledger_sequence,
        "Flags": offer_flags(cmd.flags.as_ref()),
        "TakerGets": taker_gets_amount,
        "TakerPays": taker_pays_amount,
    });

    // --- MANUAL SIGNING FLOW ---

    // 1. Inject SigningPubKey
    if let Some(obj) = tx_json_val.as_object_mut() {
        obj.insert("SigningPubKey".to_string(), serde_json::Value::String(pub_key_hex));
    }

    // 2. Hash the Binary (Unsigned)
    let unsigned_tx_json = serde_json::to_string(&tx_json_val).unwrap();
    let unsigned_hex = serialize_tx(unsigned_tx_json, false)
        .ok_or_else(|| "Binary encoding failed".to_string())?;
    let unsigned_bytes = hex::decode(&unsigned_hex).unwrap();

    // 3. Prefix + SHA-512/256 (STN\0)
    let mut payload = Vec::new();
    payload.extend_from_slice(&[0x53, 0x54, 0x58, 0x00]);
    payload.extend_from_slice(&unsigned_bytes);

    let mut hasher = Sha512::new();
    hasher.update(&payload);
    let hash = hasher.finalize();

    // 4. ECDSA Sign
    let secp = Secp256k1::new();
    let message = Message::from_digest_slice(&hash[0..32]).unwrap();
    let sig = secp.sign_ecdsa(&message, &wallet_obj.secret_key);
    let sig_hex = hex::encode(sig.serialize_der().as_ref()).to_uppercase();

    // 5. Inject TxnSignature
    if let Some(obj) = tx_json_val.as_object_mut() {
        obj.insert("TxnSignature".to_string(), serde_json::Value::String(sig_hex));
    }

    // 6. Final Blob
    let final_tx_json = serde_json::to_string(&tx_json_val).unwrap();
    let tx_blob = serialize_tx(final_tx_json, false)
        .ok_or_else(|| "Final encoding failed".to_string())?;

    Ok(tx_blob)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_fold_into_the_documented_bits() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(offer_flags(None), 0);
        assert_eq!(offer_flags(Some(&names(&[]))), 0);
        assert_eq!(offer_flags(Some(&names(&["tfFillOrKill"]))), 0x0004_0000);
        assert_eq!(offer_flags(Some(&names(&["tfImmediateOrCancel"]))), 0x0002_0000);
        assert_eq!(offer_flags(Some(&names(&["tfSell"]))), 0x0008_0000);
        assert_eq!(
            offer_flags(Some(&names(&["tfSell", "tfImmediateOrCancel", "bogus"]))),
            0x000A_0000
        );
    }

    #[test]
    fn xrp_for_token_offer_serializes() {
        // The codec is the arbiter of the JSON shape: a wrong key or type
        // makes serialize_tx return None.
        let gets = to_xrpl_amount("25", "XRP").unwrap();
        let pays = to_xrpl_amount("10.5", "RLUSD").unwrap();
        assert_eq!(gets, Value::String("25000000".into()));
        assert_eq!(pays["value"], "10.5");
        let mut v = json!({
            "TransactionType": "OfferCreate",
            "Account": "rLSn6Z3T8uCxbcd1oxwfGQN1Fdn5CyGujK",
            "Fee": "12",
            "Sequence": 1,
            "LastLedgerSequence": 99,
            "Flags": TF_SELL,
            "TakerGets": gets,
            "TakerPays": pays,
        });
        v.as_object_mut().unwrap().insert("SigningPubKey".into(), json!(""));
        let hex = serialize_tx(serde_json::to_string(&v).unwrap(), false)
            .expect("codec accepts the shape")
            .to_uppercase();
        // Flags is UInt32 field id 0x22; 0x00080000 = tfSell.
        assert!(hex.contains("2200080000"), "tfSell not encoded: {hex}");
    }
}
