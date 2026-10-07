// ws/commands/offer_cancel.rs
use crate::channel::WSCommand;
use dannesk_xrpl_codec::TransactionType;
use crate::ws::commands::transaction_builder;
use crate::ws::commands::wallet_auth::Bip44Wallet;

pub async fn construct_blob(
    wallet_obj: &Bip44Wallet,
    cmd: &WSCommand,
    sequence: u32,
    fee: u64,
    last_ledger_sequence: u32,
) -> Result<String, String> {
    let offer_sequence = cmd.offer_sequence.ok_or("Missing offer_sequence")?;

    // The unsigned OfferCancel; `transaction_builder::sign` adds the key and
    // the signature, and the codec puts the fields in order.
    let fields = vec![
        dannesk_xrpl_codec::transaction_type(TransactionType::OfferCancel),
        dannesk_xrpl_codec::account(&wallet_obj.address)?,
        dannesk_xrpl_codec::fee(fee)?,
        dannesk_xrpl_codec::sequence(sequence),
        dannesk_xrpl_codec::last_ledger_sequence(last_ledger_sequence),
        dannesk_xrpl_codec::offer_sequence(offer_sequence),
        dannesk_xrpl_codec::flags(0),
    ];

    transaction_builder::sign(wallet_obj, fields)
}
