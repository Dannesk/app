// ws/commands/payment.rs
// This module handles the blob creation for XRP, RLUSD, EURO, and SGD payments
use crate::channel::WSCommand;
use crate::ws::commands::wallet_auth::Bip44Wallet; // Adjust path if needed
use rippled_binary_codec::serialize::serialize_tx;
use serde_json::{json, Value};
//imports for manually signing
use bitcoin::secp256k1::{Message, Secp256k1};
use sha2::{Digest, Sha512};

/// Converts an XRP amount string (e.g., "12.000001" or "12000") to an exact integer drops string.
/// Handles up to 6 decimal places (XRPL precision), truncating excess. No floating-point used.
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
        // Simple truncate (add rounding logic here if needed: e.g., if 7th digit >= '5', increment last digit)
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

    // Zero amounts are invalid for payments, but we'll check >0 below
    Ok(total_drops.to_string())
}

fn get_asset_config(wallet_type: &str) -> Option<(&'static str, &'static str)> {
    crate::utils::tokens::by_code(wallet_type).map(|t| (t.currency_hex, t.issuer))
}

/// Validate and normalize a positive decimal amount string for an XRPL issued
/// currency, without round-tripping through f64.
///
/// Going through `f64` (e.g. `format!("{:.15}", parsed)`) injects representation
/// noise into the value string — "1000000.005" becomes "1000000.005000000004657"
/// — producing an over-precise mantissa that XRPL rejects or silently re-rounds.
/// XRPL issued currencies are 15-significant-digit decimals, so we keep the
/// user's digits exactly and reject anything that exceeds that precision.
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

/// Issued-currency amount in the JSON shape the binary codec expects:
/// `{ currency, issuer, value }` (XRP is a bare drops string instead).
fn create_issued_amount(wallet_type: &str, amount_str: &str) -> Result<Value, String> {
    let (currency_hex, issuer) = get_asset_config(wallet_type)
        .ok_or_else(|| format!("Unsupported issued currency: {}", wallet_type))?;

    let value = normalize_issued_value(amount_str)?;

    Ok(json!({
        "currency": currency_hex,
        "issuer": issuer,
        "value": value,
    }))
}

/// The unsigned Payment as JSON for `serialize_tx`. Field names are the
/// ledger's own; ordering is irrelevant because the codec canonicalises.
/// `Flags` is always present (0 when none) and `DestinationTag` is only
/// present when there is one — the codec would otherwise zero-encode it,
/// which is a different (and wrong) transaction. Kept as its own fn so the
/// blob test below builds exactly what the signer signs.
fn payment_json(
    account: &str,
    fee: &str,
    sequence: u32,
    last_ledger_sequence: Option<u32>,
    amount: Value,
    destination: &str,
    destination_tag: Option<u32>,
) -> Value {
    let mut v = json!({
        "TransactionType": "Payment",
        "Account": account,
        "Fee": fee,
        "Sequence": sequence,
        "Flags": 0,
        "Amount": amount,
        "Destination": destination,
    });
    let obj = v.as_object_mut().unwrap();
    if let Some(lls) = last_ledger_sequence {
        obj.insert("LastLedgerSequence".into(), json!(lls));
    }
    if let Some(tag) = destination_tag {
        obj.insert("DestinationTag".into(), json!(tag));
    }
    v
}

pub async fn construct_blob(
    wallet_obj: &Bip44Wallet,
    cmd: &WSCommand,
    sequence: u32,
    fee: String,
    last_ledger_sequence: u32,
) -> Result<String, String> {
    let recipient = cmd.recipient.as_ref().ok_or("Missing recipient")?;
    let amount_str = cmd.amount.as_ref().ok_or("Missing amount")?;
    let wallet_type = cmd.wallet_type.as_ref().ok_or("Missing wallet_type")?;

    // Resolve the recipient once: an X-address decodes to a classic r-address
    // plus a baked-in destination tag (which overrides any manual tag); a plain
    // r-address keeps the manually-entered tag from the command.
    let resolved = crate::utils::xaddress::resolve(recipient)
        .ok_or("Invalid recipient address")?;
    let destination = resolved.classic;
    let destination_tag = if resolved.from_xaddress {
        resolved.tag
    } else {
        cmd.destination_tag
    };

    let amount = match wallet_type.as_str() {
        "XRP" => {
            let amount_drops = xrp_str_to_drops(amount_str)?;
            if amount_drops == "0" {
                return Err("Amount must be greater than zero.".to_string());
            }
            Value::String(amount_drops)
        }
        _ => create_issued_amount(wallet_type.as_str(), amount_str)?,
    };

    // 1. Serialize Public Key (Compressed 33-bytes)
    let pub_key_hex = hex::encode(wallet_obj.public_key.serialize()).to_uppercase();

    // 2. Build the unsigned Payment JSON
    let mut tx_json_val = payment_json(
        &wallet_obj.address,
        &fee,
        sequence,
        Some(last_ledger_sequence),
        amount,
        &destination,
        destination_tag,
    );

    // 3. Inject SigningPubKey into the JSON before the first serialization
    if let Some(obj) = tx_json_val.as_object_mut() {
        obj.insert("SigningPubKey".to_string(), serde_json::Value::String(pub_key_hex));
    }

    // 4. Create the Unsigned Binary Blob
    let unsigned_tx_json = serde_json::to_string(&tx_json_val).unwrap();
    let unsigned_hex = serialize_tx(unsigned_tx_json, false)
        .ok_or_else(|| "Failed to encode unsigned hex".to_string())?;

    let unsigned_bytes = hex::decode(&unsigned_hex)
        .map_err(|_| "Hex Decode Error".to_string())?;

    // 5. Construct the Signing Payload
    // CRITICAL: Must be 0x53545800 (STX\0) for Transactions. 
    // You had 0x53544E00 (STN\0) which is for Node manifests.
    let mut payload = Vec::new();
    payload.extend_from_slice(&[0x53, 0x54, 0x58, 0x00]); 
    payload.extend_from_slice(&unsigned_bytes);

    // 6. SHA-512 Half Hash
    let mut hasher = Sha512::new();
    hasher.update(&payload);
    let full_hash = hasher.finalize();
    
    let mut message_hash = [0u8; 32];
    message_hash.copy_from_slice(&full_hash[0..32]);

    // 7. Sign with secp256k1
    let secp = Secp256k1::new();
    let message = Message::from_digest_slice(&message_hash)
        .map_err(|_| "Invalid Digest".to_string())?;
    
    // sign_ecdsa ensures canonical (low-S) signatures by default
    let sig = secp.sign_ecdsa(&message, &wallet_obj.secret_key);
    let der_sig = sig.serialize_der();
    let sig_hex = hex::encode(der_sig.as_ref()).to_uppercase();

    // 8. Inject the TxnSignature into the final JSON
    if let Some(obj) = tx_json_val.as_object_mut() {
        obj.insert("TxnSignature".to_string(), serde_json::Value::String(sig_hex));
    }

    // 9. Final Serialization for Broadcast
    let final_tx_json = serde_json::to_string(&tx_json_val).unwrap();
    let tx_blob = serialize_tx(final_tx_json, false)
        .ok_or_else(|| "Final Encoding Failed".to_string())?;

    Ok(tx_blob)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the unsigned blob exactly as `construct_blob` does (minus signing)
    /// for a payment with the given tag, and return the uppercase hex.
    fn blob_with_tag(destination_tag: Option<u32>) -> String {
        let mut v = payment_json(
            "rLSn6Z3T8uCxbcd1oxwfGQN1Fdn5CyGujK",
            "12",
            1,
            None,
            Value::String("1000000".into()),
            "rPEPPER7kfTD9w2To4CQk6UCfuHM9c6GDY",
            destination_tag,
        );
        v.as_object_mut().unwrap().insert(
            "SigningPubKey".to_string(),
            serde_json::Value::String(String::new()),
        );
        let json = serde_json::to_string(&v).unwrap();
        serialize_tx(json, false).expect("serialize").to_uppercase()
    }

    #[test]
    fn destination_tag_lands_in_blob() {
        // DestinationTag is the UInt32 field with field id 0x2E; 12345 = 0x00003039.
        let with = blob_with_tag(Some(12345));
        let without = blob_with_tag(None);
        assert!(with.contains("2E00003039"), "tag not encoded: {with}");
        assert!(!without.contains("2E00003039"), "stray tag in no-tag blob: {without}");
        // A UInt32 field is 1-byte id + 4-byte value = 5 bytes = 10 hex chars.
        // Equal-minus-10 proves None omits the field rather than zero-encoding it.
        assert_eq!(
            without.len() + 10,
            with.len(),
            "None should omit DestinationTag, not zero-encode it",
        );
    }

    #[test]
    fn issued_amount_is_currency_issuer_value_object() {
        // RLUSD is in the registry; the codec needs exactly these three keys.
        let v = create_issued_amount("RLUSD", "10.50").expect("registry token");
        let o = v.as_object().expect("object");
        assert_eq!(o.len(), 3);
        assert_eq!(o["value"], "10.5");
        assert!(o["currency"].as_str().unwrap().len() == 40, "160-bit currency hex");
        assert!(o["issuer"].as_str().unwrap().starts_with('r'));
    }

    #[test]
    fn issued_payment_serializes() {
        // The whole point of the JSON shape: the codec accepts it. A bad key
        // or a non-string value makes serialize_tx return None.
        let amount = create_issued_amount("RLUSD", "1").unwrap();
        let mut v = payment_json(
            "rLSn6Z3T8uCxbcd1oxwfGQN1Fdn5CyGujK",
            "12",
            1,
            Some(99),
            amount,
            "rPEPPER7kfTD9w2To4CQk6UCfuHM9c6GDY",
            None,
        );
        v.as_object_mut().unwrap().insert("SigningPubKey".into(), json!(""));
        assert!(serialize_tx(serde_json::to_string(&v).unwrap(), false).is_some());
    }
}