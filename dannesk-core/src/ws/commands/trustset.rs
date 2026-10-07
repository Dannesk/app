// ws/commands/trustset.rs
use crate::channel::WSCommand;
use dannesk_xrpl_codec::{Amount, TransactionType};
use crate::ws::commands::transaction_builder;
use crate::ws::commands::wallet_auth::Bip44Wallet; // Import our custom wallet

/// TrustSet `Flags` bit: set the NoRipple flag on this side of the line.
/// https://xrpl.org/trustset.html#trustset-flags
const TF_SET_NO_RIPPLE: u32 = 0x0002_0000;

pub async fn construct_blob(
    wallet_obj: &Bip44Wallet, // Updated type
    cmd: &WSCommand,
    sequence: u32,
    fee: u64,
    last_ledger_sequence: u32,
) -> Result<String, String> {
    // Default to a high limit if not provided
    let trustline_limit_value = cmd
        .trustline_limit
        .clone()
        .unwrap_or_else(|| "1000000".to_string());

    let asset_type = cmd
        .wallet_type
        .as_deref()
        .ok_or("Missing asset type for trustset")?;

    let (currency_hex, issuer_address) = crate::utils::tokens::by_code(asset_type)
        .map(|t| (t.currency_hex, t.issuer))
        .ok_or_else(|| format!("Unsupported asset type for trustset: {}", asset_type))?;

    // LimitAmount is the issued-currency amount: the limit, in the token's
    // currency, with its issuer.
    let limit = Amount::issued(&trustline_limit_value, currency_hex, issuer_address)?;

    // The unsigned TrustSet; `transaction_builder::sign` adds the key and the
    // signature, and the codec puts the fields in order.
    let fields = vec![
        dannesk_xrpl_codec::transaction_type(TransactionType::TrustSet),
        dannesk_xrpl_codec::account(&wallet_obj.address)?,
        dannesk_xrpl_codec::fee(fee)?,
        dannesk_xrpl_codec::sequence(sequence),
        dannesk_xrpl_codec::last_ledger_sequence(last_ledger_sequence),
        dannesk_xrpl_codec::flags(TF_SET_NO_RIPPLE),
        dannesk_xrpl_codec::limit_amount(limit),
    ];

    transaction_builder::sign(wallet_obj, fields)
}
