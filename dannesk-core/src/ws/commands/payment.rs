// ws/commands/payment.rs
// This module handles the blob creation for XRP, RLUSD, EURO, and SGD payments
use crate::channel::WSCommand;
use dannesk_xrpl_codec::{Amount, Field, TransactionType};
use crate::ws::commands::transaction_builder;
use crate::ws::commands::wallet_auth::Bip44Wallet; // Adjust path if needed

/// Converts an XRP amount string (e.g., "12.000001" or "12000") to an exact number of drops.
/// Handles up to 6 decimal places (XRPL precision), truncating excess. No floating-point used.
fn xrp_str_to_drops(xrp_str: &str) -> Result<u64, String> {
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

    // Parse to u128 for safety; checked arithmetic so an absurd amount is an
    // error, never a wrapped one. The codec caps it at all the XRP there is.
    let integer_drops: u128 = integer_str
        .parse()
        .map_err(|_| "Invalid integer part.".to_string())?;
    let fractional_drops: u128 = fractional_part
        .parse()
        .map_err(|_| "Invalid fractional part.".to_string())?;

    let total_drops = integer_drops
        .checked_mul(1_000_000)
        .and_then(|d| d.checked_add(fractional_drops))
        .and_then(|d| u64::try_from(d).ok())
        .ok_or("XRP amount is too large.")?;
    if negative < 0 && total_drops > 0 {
        return Err("Negative XRP amounts not supported.".to_string());
    }

    // Zero amounts are invalid for payments, but we'll check >0 below
    Ok(total_drops)
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

/// Issued-currency amount from the token registry: the value, the token's
/// 160-bit currency code and its issuer.
fn create_issued_amount(wallet_type: &str, amount_str: &str) -> Result<Amount, String> {
    let (currency_hex, issuer) = get_asset_config(wallet_type)
        .ok_or_else(|| format!("Unsupported issued currency: {}", wallet_type))?;

    let value = normalize_issued_value(amount_str)?;

    Amount::issued(&value, currency_hex, issuer)
}

/// The unsigned Payment's fields, without SigningPubKey and TxnSignature,
/// which `transaction_builder::sign` adds; the codec puts them in order.
/// `Flags` is always present (0 when none) and `DestinationTag` is only
/// present when there is one: a zero tag would be a different (and wrong)
/// transaction. Kept as its own fn so the tests below encode exactly what the
/// signer signs.
fn payment_fields(
    account: &str,
    fee: u64,
    sequence: u32,
    last_ledger_sequence: Option<u32>,
    amount: Amount,
    destination: &str,
    destination_tag: Option<u32>,
) -> Result<Vec<Field>, String> {
    let mut fields = vec![
        dannesk_xrpl_codec::transaction_type(TransactionType::Payment),
        dannesk_xrpl_codec::account(account)?,
        dannesk_xrpl_codec::fee(fee)?,
        dannesk_xrpl_codec::sequence(sequence),
        dannesk_xrpl_codec::flags(0),
        dannesk_xrpl_codec::amount(amount),
        dannesk_xrpl_codec::destination(destination)?,
    ];
    if let Some(lls) = last_ledger_sequence {
        fields.push(dannesk_xrpl_codec::last_ledger_sequence(lls));
    }
    if let Some(tag) = destination_tag {
        fields.push(dannesk_xrpl_codec::destination_tag(tag));
    }
    Ok(fields)
}

pub async fn construct_blob(
    wallet_obj: &Bip44Wallet,
    cmd: &WSCommand,
    sequence: u32,
    fee: u64,
    last_ledger_sequence: u32,
) -> Result<String, String> {
    let recipient = cmd.recipient.as_ref().ok_or("Missing recipient")?;
    let amount_str = cmd.amount.as_ref().ok_or("Missing amount")?;
    let wallet_type = cmd.wallet_type.as_ref().ok_or("Missing wallet_type")?;

    // Resolve the recipient once: an X-address decodes to a classic r-address
    // plus a baked-in destination tag (which overrides any manual tag); a plain
    // r-address keeps the manually-entered tag from the command.
    let resolved = dannesk_xrpl_codec::xaddress::resolve(recipient)
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
            if amount_drops == 0 {
                return Err("Amount must be greater than zero.".to_string());
            }
            Amount::xrp(amount_drops)?
        }
        _ => create_issued_amount(wallet_type.as_str(), amount_str)?,
    };

    let fields = payment_fields(
        &wallet_obj.address,
        fee,
        sequence,
        Some(last_ledger_sequence),
        amount,
        &destination,
        destination_tag,
    )?;

    transaction_builder::sign(wallet_obj, fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode the unsigned Payment exactly as `construct_blob` builds it (minus
    /// the key and signature) for a payment with the given tag, as uppercase hex.
    fn blob_with_tag(destination_tag: Option<u32>) -> String {
        let fields = payment_fields(
            "rLSn6Z3T8uCxbcd1oxwfGQN1Fdn5CyGujK",
            12,
            1,
            None,
            Amount::xrp(1_000_000).unwrap(),
            "rPEPPER7kfTD9w2To4CQk6UCfuHM9c6GDY",
            destination_tag,
        )
        .expect("payment fields");
        hex::encode_upper(dannesk_xrpl_codec::encode(&fields).expect("encode"))
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
    fn issued_amount_is_the_registry_token() {
        // RLUSD is in the registry: its currency code and issuer, and the
        // value as typed with the trailing zero dropped.
        let token = crate::utils::tokens::by_code("RLUSD").unwrap();
        assert_eq!(
            create_issued_amount("RLUSD", "10.50"),
            Amount::issued("10.5", token.currency_hex, token.issuer)
        );
        assert!(create_issued_amount("NOPE", "1").is_err());
    }

    #[test]
    fn issued_payment_encodes() {
        let amount = create_issued_amount("RLUSD", "1").unwrap();
        let fields = payment_fields(
            "rLSn6Z3T8uCxbcd1oxwfGQN1Fdn5CyGujK",
            12,
            1,
            Some(99),
            amount,
            "rPEPPER7kfTD9w2To4CQk6UCfuHM9c6GDY",
            None,
        )
        .unwrap();
        // Amount 6/1 is field id 0x61, followed by the token's 48 bytes.
        let hex = hex::encode_upper(dannesk_xrpl_codec::encode(&fields).unwrap());
        assert!(hex.contains("61D4838D7EA4C68000524C555344"), "RLUSD 1 not encoded: {hex}");
    }

    #[test]
    fn xrp_amounts_are_exact_and_bounded() {
        assert_eq!(xrp_str_to_drops("12.000001"), Ok(12_000_001));
        assert_eq!(xrp_str_to_drops("0.1234567"), Ok(123_456));
        // Drops that overflow u128 or u64 are an error, not a wrapped number.
        assert!(xrp_str_to_drops(&"9".repeat(33)).is_err());
        assert!(xrp_str_to_drops(&"9".repeat(20)).is_err());
    }
}
