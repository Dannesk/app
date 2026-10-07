//! The XRPL binary format, for the transactions Dannesk signs: Payment,
//! OfferCreate, OfferCancel and TrustSet. Written by hand, like the address
//! codec beside it ([`xaddress`]): the layouts are fixed by the ledger, and
//! the five field types these transactions use fit in a screen of code.
//!
//! A transaction is its fields in canonical order, by type code and then by
//! field code, each written as a field id followed by its value. Both codes
//! come from rippled's definitions.json (`TYPES` and each field's `nth`). The
//! signing hash is SHA-512Half of `STX\0` ‖ every field but TxnSignature; the
//! blob submitted is all of them, and the ledger's id for it is SHA-512Half of
//! `TXN\0` ‖ blob.
//!
//! Values are typed, so a field cannot be misspelt into silence, and amounts are
//! checked when they are made: a value the format cannot hold exactly is
//! refused, never cut short or rounded to zero. The tests re-encode validated
//! mainnet transactions and must arrive at the ids the ledger gave them.

use sha2::{Digest, Sha512};

pub mod xaddress;

/// Prefix of the signing hash: `STX\0`.
const SIGNING_PREFIX: &[u8; 4] = b"STX\0";

/// All the XRP there is, in drops: 100 billion XRP.
const MAX_DROPS: u64 = 100_000_000_000_000_000;

// Type codes.
const UINT16: u8 = 1;
const UINT32: u8 = 2;
const AMOUNT: u8 = 6;
const BLOB: u8 = 7;
const ACCOUNT_ID: u8 = 8;

/// A field's place in the format, (type code, field code). Fields are written
/// in this order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Code(u8, u8);

/// TxnSignature, the one field here that is not signed.
const TXN_SIGNATURE: Code = Code(BLOB, 4);

/// The transaction types Dannesk signs (definitions.json `TRANSACTION_TYPES`).
#[derive(Clone, Copy, Debug)]
pub enum TransactionType {
    Payment = 0,
    OfferCreate = 7,
    OfferCancel = 8,
    TrustSet = 20,
}

/// An amount: XRP in drops, or a token value with its currency and issuer.
/// Made only by `xrp` and `issued`, which refuse what the format cannot hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Amount(AmountKind);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AmountKind {
    Xrp(u64),
    Issued { value: u64, currency: [u8; 20], issuer: [u8; 20] },
}

impl Amount {
    /// XRP in drops, at most the 100 billion XRP there are.
    pub fn xrp(drops: u64) -> Result<Amount, String> {
        if drops > MAX_DROPS {
            return Err("XRP amount is more than all the XRP there is.".to_string());
        }
        Ok(Amount(AmountKind::Xrp(drops)))
    }

    /// A token amount: a plain positive decimal, the currency as the 40 hex
    /// digits of its 160-bit code, and the issuer's r-address.
    pub fn issued(value: &str, currency: &str, issuer: &str) -> Result<Amount, String> {
        let value = issued_value(value)?;
        let currency = currency_code(currency)?;
        let issuer = xaddress::account_id(issuer)
            .ok_or_else(|| format!("Invalid issuer address: {issuer}"))?;
        Ok(Amount(AmountKind::Issued { value, currency, issuer }))
    }

    fn write(&self, out: &mut Vec<u8>) {
        match self.0 {
            // Bit 63 clear (XRP), bit 62 set (positive), then the drops.
            AmountKind::Xrp(drops) => {
                out.extend_from_slice(&(0x4000_0000_0000_0000 | drops).to_be_bytes())
            }
            // The value word, then 20 bytes of currency and 20 of issuer.
            AmountKind::Issued { value, currency, issuer } => {
                out.extend_from_slice(&value.to_be_bytes());
                out.extend_from_slice(&currency);
                out.extend_from_slice(&issuer);
            }
        }
    }
}

/// A token value as the format's 64-bit word: bit 63 set (not XRP), bit 62 set
/// (positive), eight bits of exponent + 97, and the mantissa scaled to 16
/// digits in the low 54 bits. Zero has a word of its own. Read from the decimal
/// string without floating point; a negative value, more than 16 significant
/// digits, or a value outside 1e-81 to 9.999999999999999e95 is refused.
fn issued_value(s: &str) -> Result<u64, String> {
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    let digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    if (int.is_empty() && frac.is_empty()) || !digits(int) || !digits(frac) {
        return Err(format!("Invalid token amount: {s}"));
    }
    let all = format!("{int}{frac}");
    let significant = all.trim_start_matches('0');
    if significant.is_empty() {
        return Ok(0x8000_0000_0000_0000);
    }
    let mantissa_digits = significant.trim_end_matches('0');
    if mantissa_digits.len() > 16 {
        return Err(format!("Token amount has more than 16 significant digits: {s}"));
    }
    // value = mantissa_digits × 10^exponent
    let mut exponent = (significant.len() - mantissa_digits.len()) as i64 - frac.len() as i64;
    let mut mantissa: u64 = mantissa_digits
        .parse()
        .map_err(|_| format!("Invalid token amount: {s}"))?;
    while mantissa < 1_000_000_000_000_000 {
        mantissa *= 10;
        exponent -= 1;
    }
    if !(-96..=80).contains(&exponent) {
        return Err(format!("Token amount out of range: {s}"));
    }
    Ok(0xC000_0000_0000_0000 | (((exponent + 97) as u64) << 54) | mantissa)
}

/// A currency code in its 160-bit form, from 40 hex digits. XRP's all-zero
/// code is refused: XRP is an `Amount::xrp`, never a token.
fn currency_code(hex40: &str) -> Result<[u8; 20], String> {
    let code: [u8; 20] = hex::decode(hex40)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| format!("Invalid currency code: {hex40}"))?;
    if code == [0u8; 20] {
        return Err("XRP is not a token currency.".to_string());
    }
    Ok(code)
}

/// One field of a transaction, made by the functions below: one per field,
/// each carrying its (type code, field code) from definitions.json.
#[derive(Clone, Debug)]
pub struct Field {
    code: Code,
    value: Value,
}

#[derive(Clone, Debug)]
enum Value {
    UInt16(u16),
    UInt32(u32),
    Amount(Amount),
    Blob(Vec<u8>),
    AccountId([u8; 20]),
}

/// TransactionType, UInt16 1/2.
pub fn transaction_type(t: TransactionType) -> Field {
    Field { code: Code(UINT16, 2), value: Value::UInt16(t as u16) }
}

fn uint32(field_code: u8, v: u32) -> Field {
    Field { code: Code(UINT32, field_code), value: Value::UInt32(v) }
}

/// Flags, UInt32 2/2.
pub fn flags(v: u32) -> Field {
    uint32(2, v)
}

/// Sequence, UInt32 2/4.
pub fn sequence(v: u32) -> Field {
    uint32(4, v)
}

/// DestinationTag, UInt32 2/14.
pub fn destination_tag(v: u32) -> Field {
    uint32(14, v)
}

/// OfferSequence, UInt32 2/25.
pub fn offer_sequence(v: u32) -> Field {
    uint32(25, v)
}

/// LastLedgerSequence, UInt32 2/27.
pub fn last_ledger_sequence(v: u32) -> Field {
    uint32(27, v)
}

fn amount_field(field_code: u8, a: Amount) -> Field {
    Field { code: Code(AMOUNT, field_code), value: Value::Amount(a) }
}

/// Amount, Amount 6/1.
pub fn amount(a: Amount) -> Field {
    amount_field(1, a)
}

/// LimitAmount, Amount 6/3.
pub fn limit_amount(a: Amount) -> Field {
    amount_field(3, a)
}

/// TakerPays, Amount 6/4.
pub fn taker_pays(a: Amount) -> Field {
    amount_field(4, a)
}

/// TakerGets, Amount 6/5.
pub fn taker_gets(a: Amount) -> Field {
    amount_field(5, a)
}

/// Fee, Amount 6/8, in XRP drops.
pub fn fee(drops: u64) -> Result<Field, String> {
    Ok(amount_field(8, Amount::xrp(drops)?))
}

/// SigningPubKey, Blob 7/3.
pub fn signing_pub_key(key: &[u8]) -> Field {
    Field { code: Code(BLOB, 3), value: Value::Blob(key.to_vec()) }
}

/// TxnSignature, Blob 7/4.
pub fn txn_signature(sig: &[u8]) -> Field {
    Field { code: TXN_SIGNATURE, value: Value::Blob(sig.to_vec()) }
}

fn account_field(field_code: u8, address: &str) -> Result<Field, String> {
    let id = xaddress::account_id(address)
        .ok_or_else(|| format!("Invalid account address: {address}"))?;
    Ok(Field { code: Code(ACCOUNT_ID, field_code), value: Value::AccountId(id) })
}

/// Account, AccountID 8/1, from an r-address.
pub fn account(address: &str) -> Result<Field, String> {
    account_field(1, address)
}

/// Destination, AccountID 8/3, from an r-address.
pub fn destination(address: &str) -> Result<Field, String> {
    account_field(3, address)
}

/// The blob: every field, in canonical order. Refuses a field given twice.
pub fn encode(fields: &[Field]) -> Result<Vec<u8>, String> {
    write(fields, |_| true)
}

/// What a single signer signs: SHA-512Half of `STX\0` ‖ every field but
/// TxnSignature.
pub fn signing_hash(fields: &[Field]) -> Result<[u8; 32], String> {
    let unsigned = write(fields, |code| code != TXN_SIGNATURE)?;
    Ok(sha512_half(SIGNING_PREFIX, &unsigned))
}

fn write(fields: &[Field], include: impl Fn(Code) -> bool) -> Result<Vec<u8>, String> {
    let mut sorted: Vec<&Field> = fields.iter().filter(|f| include(f.code)).collect();
    sorted.sort_by_key(|f| f.code);
    if sorted.windows(2).any(|w| w[0].code == w[1].code) {
        return Err("A transaction field was given twice.".to_string());
    }
    let mut out = Vec::with_capacity(256);
    for f in sorted {
        field_id(f.code, &mut out);
        match &f.value {
            Value::UInt16(v) => out.extend_from_slice(&v.to_be_bytes()),
            Value::UInt32(v) => out.extend_from_slice(&v.to_be_bytes()),
            Value::Amount(a) => a.write(&mut out),
            Value::Blob(b) => {
                length_prefix(b.len(), &mut out)?;
                out.extend_from_slice(b);
            }
            Value::AccountId(id) => {
                length_prefix(id.len(), &mut out)?;
                out.extend_from_slice(id);
            }
        }
    }
    Ok(out)
}

/// A field id: the type and field codes packed into one byte when both are
/// under 16, otherwise spread over two or three.
fn field_id(Code(type_code, field_code): Code, out: &mut Vec<u8>) {
    match (type_code < 16, field_code < 16) {
        (true, true) => out.push((type_code << 4) | field_code),
        (true, false) => out.extend_from_slice(&[type_code << 4, field_code]),
        (false, true) => out.extend_from_slice(&[field_code, type_code]),
        (false, false) => out.extend_from_slice(&[0, type_code, field_code]),
    }
}

/// The length in front of a variable-length field: one byte up to 192, two up
/// to 12,480, three up to 918,744.
fn length_prefix(len: usize, out: &mut Vec<u8>) -> Result<(), String> {
    match len {
        0..=192 => out.push(len as u8),
        193..=12_480 => {
            let l = len - 193;
            out.extend_from_slice(&[193 + (l >> 8) as u8, (l & 0xff) as u8]);
        }
        12_481..=918_744 => {
            let l = len - 12_481;
            out.extend_from_slice(&[241 + (l >> 16) as u8, ((l >> 8) & 0xff) as u8, (l & 0xff) as u8]);
        }
        _ => return Err("A transaction field is too long.".to_string()),
    }
    Ok(())
}

fn sha512_half(prefix: &[u8], data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha512::new();
    hasher.update(prefix);
    hasher.update(data);
    let full = hasher.finalize();
    let mut half = [0u8; 32];
    half.copy_from_slice(&full[..32]);
    half
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(s: &str) -> Vec<u8> {
        hex::decode(s).unwrap()
    }

    /// Re-encode a validated mainnet transaction from its published fields:
    /// the ledger's id for it, SHA-512Half of `TXN\0` ‖ blob, must come out.
    /// One wrong byte anywhere and it does not.
    fn assert_ledger_id(id: &str, fields: Vec<Field>) {
        let blob = encode(&fields).expect("encodes");
        assert_eq!(hex::encode_upper(sha512_half(b"TXN\0", &blob)), id);
    }

    // One validated mainnet transaction for every shape Dannesk signs, taken
    // from s1.ripple.com on 2026-10-04 and transcribed by script from the
    // ledger's JSON. Between them they carry every field above, both key types
    // (secp256k1 and ed25519) and both amount kinds.

    #[test]
    fn payment_xrp_with_destination_tag() {
        // An XRP payment with a destination tag. Ledger 107422022.
        assert_ledger_id(
            "788C1E84CDC708C713068AD3FB6E953EB0D639373BD26475F06477C85A2AC7B4",
            vec![
                transaction_type(TransactionType::Payment),
                account("rwpTh9DDa52XkM9nTKp2QrJuCGV5d1mQVP").unwrap(),
                destination("rD37r1cciGqmdBpqou2DneEiWA1iQsAphP").unwrap(),
                amount(Amount::xrp(15234749).unwrap()),
                destination_tag(370655),
                fee(20).unwrap(),
                sequence(2448257),
                last_ledger_sequence(107422045),
                flags(2147483648),
                signing_pub_key(&bytes("03D9EE314747B9CAD91A06500FEA50A114F27B3A1560A44D30C3FB008C2744802C")),
                txn_signature(&bytes("304402207556186BD8CEB2DD2F948AD42379C0F6975B1F0288D10E0DB5DCAD2FF689935202204BBA9B1E2F7757D5C7FA8589784A72C2D9676CA0029602031DB41E813C8ACD88")),
            ],
        );
    }

    #[test]
    fn payment_xrp() {
        // An XRP payment, Flags 0. Ledger 107422036.
        assert_ledger_id(
            "18F4AF49313B1AE7761E6059BC1E30FBE465707EF3CB76E7060D7DACF2081ED1",
            vec![
                transaction_type(TransactionType::Payment),
                account("rhfeww1qUmNtsiprY2s9Rpu3GkgvZXs2Th").unwrap(),
                destination("rLf68YmzupJpbq7HkKVqk8FuXbmQj4nmeQ").unwrap(),
                amount(Amount::xrp(96429523).unwrap()),
                fee(20).unwrap(),
                sequence(107405340),
                last_ledger_sequence(214827374),
                flags(0),
                signing_pub_key(&bytes("02079EDA60A20DEF64377E3960A39D2448CEB07B7DAF42EA858FE4366E32C0752B")),
                txn_signature(&bytes("3045022100DD9500EFE0CB133A081AA9E1412FD08B962D05256E19789611855A3E8CC6F26502203105650550B76EB014F2687C49061F598B6EA7328D15A8AFF4CFC874754DFAEA")),
            ],
        );
    }

    #[test]
    fn payment_usdc() {
        // A USDC payment, signed with an ed25519 key. Ledger 107421755.
        assert_ledger_id(
            "FE34622A34A0F77A93F8343308CE05D02D97679E47BC719676C9110F1029DA26",
            vec![
                transaction_type(TransactionType::Payment),
                account("rD5AL2MtdRUfDXJF8GPK5CpSF3ZNKKgp6z").unwrap(),
                destination("r4CAbSkDf4MgGZjao78Ufcks3jxr2UiPfD").unwrap(),
                amount(Amount::issued("3664.634418496", "5553444300000000000000000000000000000000", "rGm7WCVp9gb4jZHWTEtGUr4dd74z2XuWhE").unwrap()),
                fee(12).unwrap(),
                sequence(102951302),
                last_ledger_sequence(107421773),
                flags(0),
                signing_pub_key(&bytes("ED809DE539B4DA627346A68AB503617688E345EA0AF1F534C3E83E6B5EB3B007E4")),
                txn_signature(&bytes("3A09B2CD3B998DBC623C5D1365CC7ECE7EDE58A659965CCFBFBD56F5710EFA0FC2DEBBF3DE02076E0D3B9C85A23FDD04A8C152F18EB5E30D42C0CA6ADEA8F501")),
            ],
        );
    }

    #[test]
    fn payment_xsgd_sixteen_digits() {
        // An XSGD payment of 16 significant digits, no Flags field. Ledger 107207782.
        assert_ledger_id(
            "D86AF568B04E52FF8FE3E8C9372B1524763709A2F85F7E1F78D6ABF19BBC4728",
            vec![
                transaction_type(TransactionType::Payment),
                account("rKHB6QGLgEg72KesaShY8HPAFEUURvukkf").unwrap(),
                destination("rE4DzGu2c4S1uxXfbZ47fL4PttuSxRUYxK").unwrap(),
                amount(Amount::issued("3.174394990784714", "5853474400000000000000000000000000000000", "rK67JczCpaYXVtfw3qJVmqwpSfa1bYTptw").unwrap()),
                fee(12).unwrap(),
                sequence(101867721),
                last_ledger_sequence(107207800),
                signing_pub_key(&bytes("02F549CF7BC6D8288AFEC58DB75AEAF046058FC6F7F517008DAD5A2FFFF8F5AED7")),
                txn_signature(&bytes("3044022005708EEF2CE9F7E9BAF7DD73938E104E3C49826360D1EBD2D85656671984DF570220216539CED1D02ECD9657888E1CF1DD8D21B1F737A512A1D7220064C2CB4008D1")),
            ],
        );
    }

    #[test]
    fn offer_create_xrp_for_rlusd() {
        // An offer giving XRP for RLUSD. Ledger 107422071.
        assert_ledger_id(
            "35D47A227BAA86953A2B4234B13CA8D06E575FCDC2CD55D417A6908D8F1E03CE",
            vec![
                transaction_type(TransactionType::OfferCreate),
                account("rL589x8YeYMjS6spRFFwSkXSGrqrjtag51").unwrap(),
                taker_gets(Amount::xrp(23359900).unwrap()),
                taker_pays(Amount::issued("35.1847", "524C555344000000000000000000000000000000", "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De").unwrap()),
                fee(10).unwrap(),
                sequence(107273306),
                last_ledger_sequence(107422089),
                flags(0),
                signing_pub_key(&bytes("ED676B194273B9007CF423724A7AF4281F9CF69C7A3C238747331FA814E2F8D5F8")),
                txn_signature(&bytes("3F62ECC62F9FC66430C31539FEF5DE6ABCEBC71F0C3A0764EFC586C5A8869741A3738575034573E97098495ED11B748F06DA305A03FF3D5BD0B642866CF81506")),
            ],
        );
    }

    #[test]
    fn offer_create_usdt_for_xrp() {
        // An offer giving a token for XRP, replacing an older offer. Ledger 107422022.
        assert_ledger_id(
            "553D792DB2A7EEAA2D65EEFC47476D79F80EE565B0DA3E50E1824B3C58BE55B8",
            vec![
                transaction_type(TransactionType::OfferCreate),
                account("rBTwLga3i2gz3doX6Gva3MgEV8ZCD8jjah").unwrap(),
                taker_gets(Amount::issued("1482974.1", "5553445400000000000000000000000000000000", "rcvxE9PS9YBwxtGg1qNeewV6ZB3wGubZq").unwrap()),
                taker_pays(Amount::xrp(1000000000000).unwrap()),
                offer_sequence(283340644),
                fee(10).unwrap(),
                sequence(283340652),
                last_ledger_sequence(107422025),
                flags(0),
                signing_pub_key(&bytes("0253C1DFDCF898FE85F16B71CCE80A5739F7223D54CC9EBA4749616593470298C5")),
                txn_signature(&bytes("304402200303078834818DC7F9CA3D8B96E43A98CA61823B315953CE2B5385A7C2A40A4602203AFD9C822CDEFEBB794B11977A01A6E9AD423C82A6660A4A2F145103FECC300C")),
            ],
        );
    }

    #[test]
    fn offer_cancel() {
        // An offer cancel, no Flags field. Ledger 107422022.
        assert_ledger_id(
            "6BCED16CF88B2E46DCB2FCB3E04AACE12CC03B753353E59E7126782C9C63B5C4",
            vec![
                transaction_type(TransactionType::OfferCancel),
                account("rMJ1ejWzy5AEF2tzFrv8FQpeR83DMLUQYq").unwrap(),
                offer_sequence(103315103),
                fee(10).unwrap(),
                sequence(103315104),
                last_ledger_sequence(107422041),
                signing_pub_key(&bytes("ED0CA04C010C8DE240DF0EB82969553429281AF538934EA0A794C18E90B0783C9F")),
                txn_signature(&bytes("66135798FA4516F3CD5268F9AFE5D4E06B21FB04F24EE1CBDF9FB04A497EFFCC044616340EF48DDD06C3AB55F31D42BA3577C5E6443C055E5372B9146BE5DE02")),
            ],
        );
    }

    #[test]
    fn trust_set_no_ripple() {
        // A trust line with tfSetNoRipple and a 16-digit limit. Ledger 107422064.
        assert_ledger_id(
            "EF7846A8732364973A08FA361728C195CB8509491C6B4EAAA45E7B68A6F63643",
            vec![
                transaction_type(TransactionType::TrustSet),
                account("rfpG8NYHuMKdCWx8T275rvZBV1wFypudHV").unwrap(),
                limit_amount(Amount::issued("5393549159.697738", "4452474E00000000000000000000000000000000", "rCV6yeR2BUc5V29GNPbB5NDwh82UEczgD").unwrap()),
                fee(12).unwrap(),
                sequence(101770070),
                last_ledger_sequence(107422066),
                flags(131072),
                signing_pub_key(&bytes("ED25B9D9F4CA0F9830DC1013AAA086253DFCC947C061E16378ECCD534A2FAFA7D1")),
                txn_signature(&bytes("484144024A3A4EB8D2AD6D5E23C85CFC29EC852AFC44A0A7B68BF70D05502779DF6389971AEEA79561344D5ACDADFC80D5632F226BA75AC2C691099D2DBD9C0F")),
            ],
        );
    }


    #[test]
    fn token_values() {
        // The XRPL docs' reference word for 12.123.
        assert_eq!(issued_value("12.123"), Ok(0xD4C4_4E94_96DC_7800));
        assert_eq!(issued_value("0"), Ok(0x8000_0000_0000_0000));
        assert_eq!(issued_value("0.000"), Ok(0x8000_0000_0000_0000));
        // How a value is written does not change its word.
        assert_eq!(issued_value("10.50"), issued_value("10.5"));
        assert_eq!(issued_value("010.5"), issued_value("10.5"));
        assert_eq!(issued_value("1000000.000"), issued_value("1000000"));
        assert_eq!(issued_value(".5"), issued_value("0.5"));
    }

    #[test]
    fn token_values_the_format_cannot_hold_are_refused() {
        // 16 significant digits fit and 17 do not: nothing is cut off.
        assert!(issued_value("1234567890123456").is_ok());
        assert!(issued_value("12345678901234567").is_err());
        assert!(issued_value("0.12345678901234567").is_err());
        // The exponent runs -96..=80 on a 16-digit mantissa: 1e-81 up to just
        // under 1e96. Below that is refused, not rounded to zero.
        let smallest = format!("0.{}1", "0".repeat(80));
        let too_small = format!("0.{}1", "0".repeat(81));
        let largest = format!("9999999999999999{}", "0".repeat(80));
        let too_large = format!("1{}", "0".repeat(96));
        assert!(issued_value(&smallest).is_ok());
        assert!(issued_value(&too_small).is_err());
        assert!(issued_value(&largest).is_ok());
        assert!(issued_value(&too_large).is_err());
        // Only plain positive decimals.
        for bad in ["", ".", "-1", "+1", "1e5", "1.2.3", " 1", "1,5", "0x10"] {
            assert!(issued_value(bad).is_err(), "{bad:?} accepted");
        }
    }

    #[test]
    fn xrp_amounts() {
        assert!(Amount::xrp(MAX_DROPS).is_ok());
        assert!(Amount::xrp(MAX_DROPS + 1).is_err());
        // Amount 6/1 is field id 0x61; 1 XRP is 1,000,000 drops behind bit 62.
        let blob = encode(&[amount(Amount::xrp(1_000_000).unwrap())]).unwrap();
        assert_eq!(hex::encode_upper(blob), "6140000000000F4240");
    }

    #[test]
    fn token_currencies_and_issuers() {
        let rlusd = "524C555344000000000000000000000000000000";
        let issuer = "rMxCKbEDwqr76QuheSUMdEGf4B9xJ8m5De";
        assert!(Amount::issued("1", rlusd, issuer).is_ok());
        // XRP's all-zero code, a code that is not 40 hex digits, and an issuer
        // that is not an r-address are all refused.
        assert!(Amount::issued("1", &"0".repeat(40), issuer).is_err());
        assert!(Amount::issued("1", "USD", issuer).is_err());
        assert!(Amount::issued("1", &rlusd[2..], issuer).is_err());
        assert!(Amount::issued("1", rlusd, "rNotAnAddress").is_err());
    }

    #[test]
    fn field_ids_and_length_prefixes() {
        let id = |code| {
            let mut out = Vec::new();
            field_id(code, &mut out);
            out
        };
        assert_eq!(id(Code(1, 2)), [0x12]); // TransactionType
        assert_eq!(id(Code(2, 27)), [0x20, 0x1B]); // LastLedgerSequence
        assert_eq!(id(Code(16, 1)), [0x01, 0x10]);
        assert_eq!(id(Code(16, 16)), [0x00, 0x10, 0x10]);

        let prefix = |len| {
            let mut out = Vec::new();
            length_prefix(len, &mut out).map(|_| out)
        };
        assert_eq!(prefix(0), Ok(vec![0x00]));
        assert_eq!(prefix(192), Ok(vec![0xC0]));
        assert_eq!(prefix(193), Ok(vec![0xC1, 0x00]));
        assert_eq!(prefix(12_480), Ok(vec![0xF0, 0xFF]));
        assert_eq!(prefix(12_481), Ok(vec![0xF1, 0x00, 0x00]));
        assert_eq!(prefix(918_744), Ok(vec![0xFE, 0xD4, 0x17]));
        assert!(prefix(918_745).is_err());
    }

    #[test]
    fn field_order_is_the_formats_not_the_callers() {
        let mut fields = vec![
            account("rLSn6Z3T8uCxbcd1oxwfGQN1Fdn5CyGujK").unwrap(),
            signing_pub_key(&[0x02; 33]),
            fee(12).unwrap(),
            last_ledger_sequence(99),
            sequence(1),
            transaction_type(TransactionType::OfferCancel),
        ];
        let forward = encode(&fields).unwrap();
        fields.reverse();
        assert_eq!(encode(&fields).unwrap(), forward);
        // TransactionType (1/2) leads and Account (8/1) closes.
        assert_eq!(forward[0], 0x12);
        assert_eq!(forward[forward.len() - 22], 0x81);
    }

    #[test]
    fn a_field_given_twice_is_refused() {
        assert!(encode(&[flags(0), sequence(1), flags(0)]).is_err());
    }

    #[test]
    fn the_signing_hash_leaves_out_the_signature() {
        let unsigned = vec![
            transaction_type(TransactionType::OfferCancel),
            offer_sequence(7),
            signing_pub_key(&[0x02; 33]),
        ];
        let mut signed = unsigned.clone();
        signed.push(txn_signature(&[0x30; 70]));
        assert_eq!(signing_hash(&signed), signing_hash(&unsigned));
        assert_ne!(encode(&signed), encode(&unsigned));
    }
}
