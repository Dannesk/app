use crate::btc_script_type::BtcScriptType;
use crate::channel::{CHANNEL, WSCommand};
use crate::ws::commands::bitcoin_auth::BitcoinWallet;
use bitcoin::CompressedPublicKey;
use bitcoin::absolute::LockTime;
use bitcoin::address::Address;
use bitcoin::amount::Amount;
use bitcoin::blockdata::script::ScriptBuf;
use bitcoin::blockdata::transaction::{OutPoint, Transaction, TxIn, TxOut};
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::hashes::Hash;
use bitcoin::key::{Keypair, PrivateKey, TapTweak};
use bitcoin::script::PushBytesBuf;
use bitcoin::secp256k1::{Message, Secp256k1};
use bitcoin::sighash::{EcdsaSighashType, Prevouts, SighashCache, TapSighashType};
use bitcoin::transaction::Version;
use rand::RngExt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub struct Utxo {
    pub txid: String,
    pub vout: u32,
    pub amount: u64,
    /// The owning address — decides which key signs this input.
    pub address: String,
}

/// The script family of each coin, from its owner address — the same
/// judgement the signer makes per input. A coin whose address does not parse
/// (never, for a coin the relay pushed) is budgeted as the wallet's own type.
pub fn kinds_of(coins: &[Utxo]) -> Vec<BtcScriptType> {
    let own = crate::btc_script_type::stored();
    coins
        .iter()
        .map(|c| BtcScriptType::of_address(&c.address).unwrap_or(own))
        .collect()
}

/// Exact virtual size of the transaction `construct_transaction` will build:
/// one input per entry of `inputs`, each costed in its own script family,
/// and the given output scripts.
///
/// Per input: 41 fixed non-witness bytes (outpoint, scriptSig length,
/// sequence) plus the family's scriptSig, and the family's witness bytes.
/// Witness sizes are worst case — a 73-byte DER signature where one is used
/// — so the paid rate lands at or above the one picked, never a hair under.
/// A transaction with no witness input at all (a legacy wallet) has no
/// marker and flag bytes, and is costed without them.
///
/// This replaced the flat 140 vB guess (`ASSUMED_VBYTES`, gone): a wallet
/// whose send needed three inputs was underpaying its chosen rate by more
/// than half under that assumption, and one that emptied into a single
/// output was overpaying. The input count is real — the pushed UTXO set is on
/// hand — so the size is too.
pub fn vsize_for(inputs: &[BtcScriptType], out_spk_lens: &[usize]) -> u64 {
    fn varint(n: usize) -> u64 {
        if n < 0xfd { 1 } else { 3 }
    }
    let outs: u64 = out_spk_lens
        .iter()
        .map(|l| 8 + varint(*l) + *l as u64)
        .sum();
    let ins: u64 = inputs.iter().map(|k| 41 + k.script_sig_len()).sum();
    let witness: u64 = inputs.iter().map(|k| k.witness_len()).sum();
    let base = 4 + varint(inputs.len()) + ins + varint(out_spk_lens.len()) + outs + 4;
    if witness == 0 {
        return base;
    }
    (base * 4 + 2 + witness).div_ceil(4)
}

/// A rate priced against a concrete vsize, in **absolute satoshis**.
///
/// The rate is sat/vB and what we sign is a flat total, so the two are bridged
/// exactly once, here. Rounded up: rounding a fee down is how you land a hair
/// under the rate you picked.
///
/// **No clamping.** indexd already lifts every tier to the node's live
/// `mempoolminfee` before publishing (`node_stats::walk_mempool`), so these
/// numbers are floored by the network's own answer. A second floor of our own on
/// top of that was flattening four distinct prices — 14 / 52 / 54 / 61 sats at
/// the current rates — into one, and then had to explain why a tier called
/// `minimum` was outbidding `high`. There was nothing to explain: the
/// contradiction was ours.
pub fn tier_sats_at(rate: f32, vsize: u64) -> u64 {
    // Snapped before the ceiling, because the rate crosses the wire as `f32`
    // and comes back a hair off. `0.1` arrives as 0.100000001490116…, which
    // times 140 is 14.0000002 — and a bare `ceil` turns the node's exact
    // 14-satoshi floor into 15. Rounding to six places lands it back on the
    // number the node actually said, and is far finer than any real feerate.
    let raw = rate as f64 * vsize as f64;
    ((raw * 1e6).round() / 1e6).ceil() as u64
}

/// The coins this wallet may sign against, read from the pushed set — there is
/// no per-signing fetch any more (the relay pushes the whole set on every
/// event/subscribe/reconnect, see project_btc_utxo_in_redis).
///
/// Policy: every confirmed coin, plus our OWN unconfirmed change (a height-0
/// coin whose creating tx we sent — spendable immediately, and what lets
/// back-to-back sends work). A FOREIGN height-0 coin is someone else's
/// RBF-able payment and is excluded until it confirms.
/// A coin a mempool transaction already spends — ours or anyone's — is never
/// offered, whatever its height: the set marks it (the relay's push, or our
/// own dispatch moments earlier), and the node would refuse it anyway.
///
/// Deterministic order — confirmed oldest-first, own pending change last — so
/// the fee quoted in step 2 and the transaction signed here select the same
/// coins from the same set.
///
/// `Err` when the pushed set belongs to a different wallet (or never arrived):
/// signing against the wrong wallet's coins must be impossible, and an empty
/// answer would just fail later with a message about satoshis.
pub fn eligible_utxos(wallet: &str) -> Result<Vec<Utxo>, String> {
    let (set_wallet, set) = CHANNEL.btc_utxos_rx.borrow().clone();
    if set_wallet.as_deref() != Some(wallet) {
        return Err("Wallet data not loaded yet — try again shortly".to_string());
    }
    // HD: "we sent it" means ANY of our addresses appears among the inputs —
    // sender detection against the primary alone would misclassify change
    // returning to #0 from a spend that consumed a member address's coin.
    let ours: Vec<String> = crate::wallet::btc_address_records()
        .into_iter()
        .map(|r| r.address)
        .collect();
    let own_send = |txid: &str| -> bool {
        CHANNEL
            .btc_transactions_rx
            .borrow()
            .transactions
            .get(txid)
            .is_some_and(|tx| tx.sender_addresses.iter().any(|a| ours.contains(a)))
    };
    let mut coins: Vec<(u64, String, u32, u64, String)> = set
        .iter()
        .filter(|u| u.spent_by.is_none())
        .filter(|u| u.height > 0 || own_send(&u.txid))
        .map(|u| (u.height, u.txid.clone(), u.vout, u.sats, u.address.clone()))
        .collect();
    // height 0 sorts FIRST ascending — force unconfirmed change to the back so
    // confirmed coins are spent before a chain is built on pending ones.
    coins.sort_by(|a, b| {
        let key = |c: &(u64, String, u32, u64, String)| (c.0 == 0, c.0, c.1.clone(), c.2);
        key(a).cmp(&key(b))
    });
    Ok(coins
        .into_iter()
        .map(|(_, txid, vout, sats, address)| Utxo { txid, vout, amount: sats, address })
        .collect())
}

/// What [`eligible_utxos`] adds up to, in satoshis — the send flow's
/// "available" figure and the max-send base. Zero when the set isn't loaded:
/// for display there is nothing better to claim.
pub fn spendable_sats(wallet: &str) -> u64 {
    eligible_utxos(wallet)
        .map(|coins| coins.iter().map(|u| u.amount).sum())
        .unwrap_or(0)
}

pub fn calculate_unconfirmed_balance(utxos: &[Utxo]) -> u64 {
    let balance: u64 = utxos.iter().map(|utxo| utxo.amount).sum();
    balance
}

pub fn select_utxos(utxos: &[Utxo], amount: u64, fee: u64) -> Result<Vec<Utxo>, String> {
    let target = amount + fee;

    // Validate unconfirmed balance
    let unconfirmed_balance = calculate_unconfirmed_balance(utxos);
    if unconfirmed_balance < target {
        return Err(format!(
            "Insufficient unconfirmed balance: needed {} satoshis, available {} satoshis",
            target, unconfirmed_balance
        ));
    }

    // Select UTXOs in order (first-fit approach)
    let mut selected: Vec<Utxo> = Vec::new();
    let mut total: u64 = 0;
    for utxo in utxos.iter() {
        if total < target {
            selected.push(utxo.clone());
            total += utxo.amount;
        }
    }

    if total < target {
        return Err(format!(
            "Insufficient funds after UTXO selection: needed {} satoshis, got {} satoshis",
            target, total
        ));
    }

    Ok(selected)
}

pub async fn construct_transaction(
    wallet_obj: &BitcoinWallet,
    cmd: &WSCommand,
    _tx_type: &str,
    utxos: Vec<Utxo>,
    fee: String,
    // Where change returns: a fresh chain-1 address from the rotation walk,
    // or #0 for a legacy wallet without an account xpub. Always one of
    // btc.json's records — persisted and subscribed BEFORE this runs.
    change_address: &str,
) -> Result<String, String> {
    // Check recipient
    let recipient = cmd
        .recipient
        .as_ref()
        .ok_or("Missing recipient".to_string())?;

    // Check amount
    let amount_str = cmd.amount.as_ref().ok_or("Missing amount".to_string())?;

    // Parse and validate amount
    let amount_btc = amount_str
        .parse::<f64>()
        .map_err(|e| format!("Failed to parse amount as float: {}", e))?;
    if amount_btc <= 0.0 {
        return Err("Amount must be greater than zero.".to_string());
    }
    // Convert BTC to satoshis
    let amount = (amount_btc * 100_000_000.0).round() as u64;
    if amount == 0 {
        return Err("Amount must be greater than zero after conversion.".to_string());
    }

    // Parse fee
    let fee = fee
        .parse::<u64>()
        .map_err(|e| format!("Failed to parse fee: {}", e))?;

    // Validate unconfirmed balance
    let unconfirmed_balance = calculate_unconfirmed_balance(&utxos);
    let target = amount + fee;
    if unconfirmed_balance < target {
        return Err(format!(
            "Insufficient unconfirmed balance: needed {} satoshis, available {} satoshis",
            target, unconfirmed_balance
        ));
    }

    // Select UTXOs
    let selected_utxos = select_utxos(&utxos, amount, fee)?;
    let total_input: u64 = selected_utxos.iter().map(|utxo| utxo.amount).sum();

    // Parse recipient address
    let recipient_addr =
        Address::from_str(recipient).map_err(|e| format!("Invalid recipient address: {}", e))?;
    let recipient_addr = recipient_addr
        .require_network(bitcoin::Network::Bitcoin)
        .map_err(|e| format!("Invalid network for recipient: {}", e))?;

    // Create transaction inputs
    let inputs: Vec<TxIn> = selected_utxos
        .iter()
        .map(|utxo| {
            Ok(TxIn {
                previous_output: OutPoint {
                    txid: bitcoin::Txid::from_str(&utxo.txid)
                        .map_err(|e| format!("Invalid txid {}: {}", utxo.txid, e))?,
                    vout: utxo.vout,
                },
                script_sig: ScriptBuf::new(),
                // BIP125-replaceable (0xfffffffd), not MAX. Our own node runs
                // full-RBF so it would accept a replacement either way, but
                // propagation would lean on every peer doing the same. Signalling
                // it costs nothing here — `lock_time` is already ZERO, which is
                // exactly the case this constant is named for.
                //
                // Landed ahead of the bump-fee flow on purpose: it applies to new
                // sends only, so any transaction broadcast before this point is
                // one that cannot later be cheaply replaced.
                sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: bitcoin::Witness::new(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    // Create transaction
    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: vec![TxOut {
            value: Amount::from_sat(amount),
            script_pubkey: recipient_addr.script_pubkey(),
        }],
    };

    // Add change output if necessary
    let change = total_input - amount - fee;
    if change > 0 {
        let change_addr = Address::from_str(change_address)
            .map_err(|e| format!("Invalid change address: {}", e))?;
        let change_addr = change_addr
            .require_network(bitcoin::Network::Bitcoin)
            .map_err(|e| format!("Invalid network for wallet: {}", e))?;
        tx.output.push(TxOut {
            value: Amount::from_sat(change),
            script_pubkey: change_addr.script_pubkey(),
        });
    }

    sign_inputs(&mut tx, &selected_utxos, wallet_obj)?;

    // Serialize transaction
    let tx_hex = serialize_hex(&tx);
    Ok(tx_hex)
}


/// Sign every input of `tx` with ITS owner's key, in the script family the
/// owner address IS. Inputs are positional 1:1 with `coins`, so the owner of
/// input i is `coins[i].address`. Keys parse once per distinct owner; a coin
/// whose owner has no key in the authenticated wallet fails the WHOLE build —
/// a partially-signed transaction is not a smaller transaction, it is an
/// invalid one. Shared by the send and the fee bump, which differ only in how
/// the inputs and outputs were chosen.
///
/// The family comes from the address string, parsed by the crate — never from
/// btc.json's `script_type`. The address decides what script the network will
/// check, so it is the only honest source; the stored type only ever decided
/// how the address was derived. One wallet holds one family in practice, but
/// nothing here assumes it: each input is judged alone.
///
/// - P2WPKH (`bc1q`): BIP-143 sighash, ECDSA, witness = [sig, key].
/// - P2SH-P2WPKH (`3…`): identical witness and sighash (the script code is
///   the inner P2WPKH's), plus a scriptSig of one push — the 22-byte redeem
///   script — so the P2SH layer can be satisfied.
/// - P2PKH (`1…`): the legacy sighash over the output script, ECDSA,
///   scriptSig = [sig, key], empty witness.
/// - P2TR (`bc1p`): BIP-341 key-path spend — the key tweaked with no script
///   tree, the sighash committing to EVERY prevout, one 64-byte Schnorr
///   signature as the whole witness, default sighash type implied.
fn sign_inputs(
    tx: &mut Transaction,
    coins: &[Utxo],
    wallet_obj: &BitcoinWallet,
) -> Result<(), String> {
    if tx.input.len() != coins.len() {
        return Err("input/coin count mismatch".to_string());
    }
    let secp = Secp256k1::new();
    struct Owner {
        secret: bitcoin::secp256k1::SecretKey,
        pk: CompressedPublicKey,
        kind: BtcScriptType,
        spk: ScriptBuf,
    }
    let mut owners: std::collections::HashMap<String, Owner> = std::collections::HashMap::new();
    for coin in coins {
        if owners.contains_key(&coin.address) {
            continue;
        }
        let wif = wallet_obj
            .wif_for(&coin.address)
            .ok_or_else(|| format!("No signing key for address {}", coin.address))?;
        let private_key =
            PrivateKey::from_wif(wif).map_err(|e| format!("Invalid private key: {}", e))?;
        let pk = CompressedPublicKey(private_key.public_key(&secp).inner);
        let kind = BtcScriptType::of_address(&coin.address)
            .ok_or_else(|| format!("Cannot sign for address {}", coin.address))?;
        // The script the coin is locked to, rebuilt from OUR key in the
        // address's own family, and checked against the address itself: a
        // key that does not reproduce its address (a btc.json record from a
        // different seed) is refused here, not discovered as a rejected
        // broadcast.
        let spk = kind.address(&secp, &pk).script_pubkey();
        let locked = Address::from_str(&coin.address)
            .ok()
            .and_then(|a| a.require_network(bitcoin::Network::Bitcoin).ok())
            .map(|a| a.script_pubkey());
        if locked.as_ref() != Some(&spk) {
            return Err(format!("Signing key does not match address {}", coin.address));
        }
        owners.insert(coin.address.clone(), Owner { secret: private_key.inner, pk, kind, spk });
    }

    // Taproot commits to every input's prevout, so the full list is built
    // once even when only some inputs are taproot.
    let prevouts: Vec<TxOut> = coins
        .iter()
        .map(|c| TxOut { value: Amount::from_sat(c.amount), script_pubkey: owners[&c.address].spk.clone() })
        .collect();

    let mut signed: Vec<(ScriptBuf, bitcoin::Witness)> = Vec::with_capacity(coins.len());
    for i in 0..tx.input.len() {
        let owner = &owners[&coins[i].address];
        let mut cache = SighashCache::new(&*tx);
        match owner.kind {
            BtcScriptType::NativeSegwit | BtcScriptType::NestedSegwit => {
                // The BIP-143 script code is the inner P2WPKH's for both.
                let inner = Address::p2wpkh(&owner.pk, bitcoin::Network::Bitcoin).script_pubkey();
                let sighash = cache
                    .p2wpkh_signature_hash(i, &inner, Amount::from_sat(coins[i].amount), EcdsaSighashType::All)
                    .map_err(|e| format!("Failed to compute sighash for input {}: {}", i, e))?;
                let signature = secp.sign_ecdsa(&Message::from(sighash), &owner.secret);
                let mut sig = signature.serialize_der().to_vec();
                sig.push(EcdsaSighashType::All as u8);
                let witness = bitcoin::Witness::from_slice(&[&sig[..], &owner.pk.to_bytes()[..]]);
                let script_sig = match owner.kind {
                    BtcScriptType::NestedSegwit => {
                        let redeem = PushBytesBuf::try_from(inner.into_bytes())
                            .map_err(|_| "redeem script too long to push".to_string())?;
                        ScriptBuf::builder().push_slice(redeem).into_script()
                    }
                    _ => ScriptBuf::new(),
                };
                signed.push((script_sig, witness));
            }
            BtcScriptType::Legacy => {
                let sighash = cache
                    .legacy_signature_hash(i, &owner.spk, EcdsaSighashType::All.to_u32())
                    .map_err(|e| format!("Failed to compute sighash for input {}: {}", i, e))?;
                let signature = secp.sign_ecdsa(&Message::from(sighash), &owner.secret);
                let mut sig = signature.serialize_der().to_vec();
                sig.push(EcdsaSighashType::All as u8);
                let sig = PushBytesBuf::try_from(sig).map_err(|_| "signature too long to push".to_string())?;
                let key = PushBytesBuf::try_from(owner.pk.to_bytes().to_vec())
                    .map_err(|_| "key too long to push".to_string())?;
                let script_sig = ScriptBuf::builder().push_slice(sig).push_slice(key).into_script();
                signed.push((script_sig, bitcoin::Witness::new()));
            }
            BtcScriptType::Taproot => {
                let sighash = cache
                    .taproot_key_spend_signature_hash(i, &Prevouts::All(&prevouts), TapSighashType::Default)
                    .map_err(|e| format!("Failed to compute sighash for input {}: {}", i, e))?;
                let keypair = Keypair::from_secret_key(&secp, &owner.secret);
                let tweaked = keypair.tap_tweak(&secp, None);
                // BIP-340 auxiliary randomness from the app's own RNG (the
                // same draw `encrypt.rs` uses for salts) — a fresh 32 bytes
                // per signature, so a fault or a repeated nonce can never
                // pair two signatures against one key.
                let aux: [u8; 32] = rand::rng().random();
                let signature = secp.sign_schnorr_with_aux_rand(
                    &Message::from_digest(sighash.to_byte_array()),
                    &tweaked.to_keypair(),
                    &aux,
                );
                let witness = bitcoin::Witness::p2tr_key_spend(&bitcoin::taproot::Signature {
                    signature,
                    sighash_type: TapSighashType::Default,
                });
                signed.push((ScriptBuf::new(), witness));
            }
        }
    }
    for (input, (script_sig, witness)) in tx.input.iter_mut().zip(signed.into_iter()) {
        input.script_sig = script_sig;
        input.witness = witness;
    }
    Ok(())
}

// ── Fee bump (BIP125 replace-by-fee) ────────────────────────────────────────

/// Below this a change output is not worth creating: Core's dust limit for a
/// P2PKH output, deliberately the more conservative of the two script sizes
/// this wallet could meet, so a replacement is never refused for dust by a
/// peer with older policy.
pub const DUST_SATS: u64 = 546;

/// The replacement, decided. The view prices tiers with it and the signer
/// builds from it, so the number on the stack is the number that gets signed.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplacementPlan {
    /// Every input of the original — BIP125 rule 2, the replacement conflicts
    /// with nothing but the original — plus at most one confirmed coin added
    /// when the original's change cannot fund the higher fee.
    pub inputs: Vec<Utxo>,
    /// Outputs re-created byte for byte: everything that is not our change.
    pub keep: Vec<(ScriptBuf, u64)>,
    /// The change output, if one survives: its amount, and its script when
    /// the original already had one (`None` = a fresh change address is
    /// needed, which only the signer may mint).
    pub change_sats: Option<u64>,
    pub change_spk: Option<ScriptBuf>,
    /// What the replacement actually pays — the asked fee, or more when a
    /// sub-dust change remainder was absorbed into it.
    pub fee: u64,
    pub vsize: u64,
    pub added_input: bool,
}

impl ReplacementPlan {
    pub fn rate_sat_vb(&self) -> f64 {
        self.fee as f64 / self.vsize.max(1) as f64
    }
}

/// The least a replacement of `info` may pay at `vsize` vbytes, in satoshis:
/// the original's fee plus `incrementalrelayfee` × the replacement's size
/// (BIP125 rule 4), never below the node's relay minimum for that size, and
/// strictly above the original's RATE (rule 6 as Core enforces it).
pub fn min_replacement_fee(info: &crate::channel::BtcRbfInfo, vsize: u64) -> u64 {
    let ceil_rate = |rate: f32| ((rate as f64 * vsize as f64 * 1e6).round() / 1e6).ceil() as u64;
    let rule4 = info.fee_sats.saturating_add(ceil_rate(info.incremental_sat_vb));
    let relay_min = ceil_rate(info.min_sat_vb);
    // Same rate would be `fee_sats * vsize / info.vsize`; one satoshi past it.
    let same_rate = (info.fee_sats as u128 * vsize as u128).div_ceil(info.vsize.max(1) as u128) as u64;
    rule4.max(relay_min).max(same_rate + 1)
}

/// Decide the replacement of `info` at an absolute `fee`.
///
/// Inputs are the original's, all of which must be ours. Outputs are kept
/// verbatim except our change — the LAST output paying one of `ours` — which
/// shrinks to fund the difference. When it cannot, `spare` (one CONFIRMED
/// coin of ours) is added and the change grows to absorb it; when the
/// remainder falls under dust the change output is dropped and the fee takes
/// it. Every BIP125 rule the node enforces is checked here so the stack can
/// grey a tier out instead of sending a broadcast that will be refused.
pub fn plan_replacement(
    info: &crate::channel::BtcRbfInfo,
    ours: &[String],
    spare: Option<&Utxo>,
    fee: u64,
) -> Result<ReplacementPlan, String> {
    if info.descendants > 0 {
        return Err("a later payment already spends this one's change".to_string());
    }
    let mut inputs: Vec<Utxo> = Vec::with_capacity(info.inputs.len() + 1);
    for i in &info.inputs {
        let Some(address) = i.address.as_deref().filter(|a| ours.iter().any(|o| o == a)) else {
            return Err("not every input is this wallet's to re-sign".to_string());
        };
        inputs.push(Utxo { txid: i.txid.clone(), vout: i.vout, amount: i.sats, address: address.to_string() });
    }
    let is_ours = |o: &crate::channel::BtcRbfOutput| o.address.as_deref().is_some_and(|a| ours.iter().any(|x| x == a));
    let change_idx = info.outputs.iter().rposition(is_ours);
    let mut keep: Vec<(ScriptBuf, u64)> = Vec::new();
    let mut change_spk: Option<ScriptBuf> = None;
    for (idx, o) in info.outputs.iter().enumerate() {
        let spk = match (&o.spk, &o.address) {
            (Some(hex), _) => ScriptBuf::from_hex(hex).map_err(|e| format!("bad output script: {e}"))?,
            (None, Some(addr)) => Address::from_str(addr)
                .ok()
                .and_then(|a| a.require_network(bitcoin::Network::Bitcoin).ok())
                .map(|a| a.script_pubkey())
                .ok_or_else(|| "an output's address can't be rebuilt".to_string())?,
            (None, None) => return Err("an output has no script to rebuild".to_string()),
        };
        if Some(idx) == change_idx {
            change_spk = Some(spk);
        } else {
            keep.push((spk, o.sats));
        }
    }
    let keep_sum: u64 = keep.iter().map(|(_, v)| *v).sum();
    let mut in_sum: u64 = inputs.iter().map(|u| u.amount).sum();
    let mut room = in_sum as i128 - keep_sum as i128 - fee as i128;
    let mut added_input = false;
    if room < 0 {
        let Some(extra) = spare else {
            return Err("this wallet can't cover the higher fee".to_string());
        };
        inputs.push(extra.clone());
        in_sum = in_sum.saturating_add(extra.amount);
        room += extra.amount as i128;
        added_input = true;
        if room < 0 {
            return Err("this wallet can't cover the higher fee".to_string());
        }
    }
    let change_sats = (room as u64 >= DUST_SATS).then_some(room as u64);
    let effective_fee = in_sum - keep_sum - change_sats.unwrap_or(0);

    let mut out_lens: Vec<usize> = keep.iter().map(|(spk, _)| spk.len()).collect();
    if change_sats.is_some() {
        out_lens.push(change_spk.as_ref().map_or(crate::btc_script_type::stored().spk_len(), |s| s.len()));
    }
    let vsize = vsize_for(&kinds_of(&inputs), &out_lens);

    let floor = min_replacement_fee(info, vsize);
    if effective_fee < floor {
        return Err(format!("at least {floor} sats to replace it"));
    }
    Ok(ReplacementPlan {
        inputs,
        keep,
        change_sats,
        change_spk,
        fee: effective_fee,
        vsize,
        added_input,
    })
}

/// The transaction a bump is rebuilt from — the row's own record plus the
/// node frame, nothing fetched. The record carries the body since 2026-09-06
/// (indexd's mempool frame → the relay's record → `btc_transactions_rx`, or
/// the signed transaction itself for a send this client just made); the node
/// frame carries the two policy numbers. Descendants are counted from the same
/// records: any pending row of ours spending one of this transaction's
/// outputs. `None` = a row from before the record carried its body, or a node
/// frame that has not reported yet — the stack says so and stays disarmed.
pub fn rbf_info_for(txid: &str) -> Option<crate::channel::BtcRbfInfo> {
    let txs = CHANNEL.btc_transactions_rx.borrow();
    let rec = txs.transactions.get(txid)?;
    if rec.inputs.is_empty() || rec.outputs.is_empty() {
        return None;
    }
    let vsize = rec.vsize.filter(|v| *v > 0)?;
    let fee_sats: u64 = rec.fees.parse().ok().filter(|f| *f > 0)?;
    let node = CHANNEL.btc_node_rx.borrow().clone();
    let incremental_sat_vb = node.incremental_sat_vb?;
    let min_sat_vb = node.tiers?[0];
    let descendants = txs
        .transactions
        .values()
        .filter(|t| matches!(t.status, crate::channel::BitcoinTransactionStatus::Pending))
        .filter(|t| t.inputs.iter().any(|i| i.txid == txid))
        .count() as u32;
    Some(crate::channel::BtcRbfInfo {
        txid: txid.to_string(),
        vsize,
        fee_sats,
        inputs: rec.inputs.clone(),
        outputs: rec.outputs.clone(),
        incremental_sat_vb,
        min_sat_vb,
        descendants,
    })
}

/// Price a tier: the rate against the replacement's real size, converging in
/// a step when the plan changes shape (an added input, a dropped change).
pub fn quote_replacement(
    info: &crate::channel::BtcRbfInfo,
    ours: &[String],
    spare: Option<&Utxo>,
    rate: f32,
) -> Result<ReplacementPlan, String> {
    let mut fee = tier_sats_at(rate, info.vsize);
    let mut plan = plan_replacement(info, ours, spare, fee)?;
    for _ in 0..3 {
        let next = tier_sats_at(rate, plan.vsize);
        if next == fee || next <= plan.fee && plan.change_sats.is_none() {
            break;
        }
        fee = next;
        plan = plan_replacement(info, ours, spare, fee)?;
    }
    Ok(plan)
}

/// One CONFIRMED coin of this wallet that the original does not already
/// spend, in the same deterministic order signing selects from — the view
/// and the signer must name the same coin. Unconfirmed coins are never
/// offered: the original's own change vanishes with it, and a foreign
/// pending coin is not ours to spend yet.
pub fn spare_coin(wallet: &str, info: &crate::channel::BtcRbfInfo) -> Option<Utxo> {
    let (set_wallet, set) = CHANNEL.btc_utxos_rx.borrow().clone();
    if set_wallet.as_deref() != Some(wallet) {
        return None;
    }
    let mut coins: Vec<&crate::channel::BtcUtxo> = set
        .iter()
        .filter(|u| u.height > 0 && u.txid != info.txid)
        .filter(|u| !info.inputs.iter().any(|i| i.txid == u.txid && i.vout == u.vout))
        .collect();
    coins.sort_by(|a, b| (a.height, &a.txid, a.vout).cmp(&(b.height, &b.txid, b.vout)));
    coins
        .first()
        .map(|u| Utxo { txid: u.txid.clone(), vout: u.vout, amount: u.sats, address: u.address.clone() })
}

/// Build and sign the replacement a [`ReplacementPlan`] describes.
/// `change_address` is used only when the plan needs a change output the
/// original did not have (a spare coin was added to a change-less send).
pub fn construct_replacement(
    wallet_obj: &BitcoinWallet,
    plan: &ReplacementPlan,
    change_address: &str,
) -> Result<String, String> {
    let inputs: Vec<TxIn> = plan
        .inputs
        .iter()
        .map(|utxo| {
            Ok(TxIn {
                previous_output: OutPoint {
                    txid: bitcoin::Txid::from_str(&utxo.txid)
                        .map_err(|e| format!("Invalid txid {}: {}", utxo.txid, e))?,
                    vout: utxo.vout,
                },
                script_sig: ScriptBuf::new(),
                sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: bitcoin::Witness::new(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut output: Vec<TxOut> = plan
        .keep
        .iter()
        .map(|(spk, sats)| TxOut { value: Amount::from_sat(*sats), script_pubkey: spk.clone() })
        .collect();
    if let Some(sats) = plan.change_sats {
        let spk = match &plan.change_spk {
            Some(spk) => spk.clone(),
            None => Address::from_str(change_address)
                .map_err(|e| format!("Invalid change address: {}", e))?
                .require_network(bitcoin::Network::Bitcoin)
                .map_err(|e| format!("Invalid network for wallet: {}", e))?
                .script_pubkey(),
        };
        output.push(TxOut { value: Amount::from_sat(sats), script_pubkey: spk });
    }
    let mut tx = Transaction { version: Version::TWO, lock_time: LockTime::ZERO, input: inputs, output };
    sign_inputs(&mut tx, &plan.inputs, wallet_obj)?;
    Ok(serialize_hex(&tx))
}

#[cfg(test)]
mod sign_tests {
    use super::*;
    use crate::btc_script_type::BtcScriptType;
    use bitcoin::key::XOnlyPublicKey;
    use bitcoin::secp256k1::SecretKey;
    use bitcoin::secp256k1::ecdsa::Signature as EcdsaSignature;

    /// One key per family, its address in that family, and one coin on it.
    fn wallet_of(types: &[BtcScriptType]) -> (BitcoinWallet, Vec<Utxo>) {
        let secp = Secp256k1::new();
        let mut keys = Vec::new();
        let mut coins = Vec::new();
        for (i, t) in types.iter().enumerate() {
            let sk = SecretKey::from_slice(&[i as u8 + 1; 32]).unwrap();
            let pk = PrivateKey { compressed: true, network: bitcoin::NetworkKind::Main, inner: sk };
            let address = t.address(&secp, &CompressedPublicKey(pk.public_key(&secp).inner)).to_string();
            keys.push((address.clone(), pk.to_wif()));
            coins.push(Utxo {
                txid: format!("{:02x}", 0x11 + i).repeat(32),
                vout: i as u32,
                amount: 50_000 + i as u64,
                address,
            });
        }
        (BitcoinWallet::for_test(coins[0].address.clone(), keys), coins)
    }

    fn unsigned(coins: &[Utxo]) -> Transaction {
        Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: coins
                .iter()
                .map(|c| TxIn {
                    previous_output: OutPoint { txid: bitcoin::Txid::from_str(&c.txid).unwrap(), vout: c.vout },
                    script_sig: ScriptBuf::new(),
                    sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
                    witness: bitcoin::Witness::new(),
                })
                .collect(),
            output: vec![TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: ScriptBuf::from_hex("0014aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
            }],
        }
    }

    fn prevouts(coins: &[Utxo]) -> Vec<TxOut> {
        coins
            .iter()
            .map(|c| TxOut {
                value: Amount::from_sat(c.amount),
                script_pubkey: Address::from_str(&c.address).unwrap().assume_checked().script_pubkey(),
            })
            .collect()
    }

    fn pushes(script: &ScriptBuf) -> Vec<Vec<u8>> {
        script
            .instructions()
            .map(|i| i.unwrap().push_bytes().unwrap().as_bytes().to_vec())
            .collect()
    }

    /// Each family's input, signed in one transaction with the others, is
    /// what its script demands: the right sighash algorithm over the right
    /// script code, a signature that verifies under the coin's own key —
    /// for taproot, under the TWEAKED output key the address encodes — and
    /// the scriptSig / witness shape the network will evaluate.
    #[test]
    fn every_family_signs_and_verifies() {
        let types = BtcScriptType::IMPORT;
        let (wallet, coins) = wallet_of(&types);
        let mut tx = unsigned(&coins);
        sign_inputs(&mut tx, &coins, &wallet).unwrap();
        let secp = Secp256k1::new();
        let prevouts = prevouts(&coins);
        for (i, t) in types.iter().enumerate() {
            let input = &tx.input[i];
            let mut cache = SighashCache::new(&tx);
            match t {
                BtcScriptType::Taproot => {
                    assert!(input.script_sig.is_empty());
                    assert_eq!(input.witness.len(), 1);
                    let sig = bitcoin::taproot::Signature::from_slice(&input.witness[0]).unwrap();
                    assert_eq!(sig.sighash_type, TapSighashType::Default);
                    let sighash = cache
                        .taproot_key_spend_signature_hash(i, &Prevouts::All(&prevouts), TapSighashType::Default)
                        .unwrap();
                    let output_key = XOnlyPublicKey::from_slice(&prevouts[i].script_pubkey.as_bytes()[2..]).unwrap();
                    secp.verify_schnorr(&sig.signature, &Message::from_digest(sighash.to_byte_array()), &output_key)
                        .unwrap();
                }
                BtcScriptType::Legacy => {
                    assert!(input.witness.is_empty());
                    let items = pushes(&input.script_sig);
                    assert_eq!(items.len(), 2);
                    let (der, ty) = items[0].split_at(items[0].len() - 1);
                    assert_eq!(ty, [EcdsaSighashType::All as u8]);
                    let pk = CompressedPublicKey::from_slice(&items[1]).unwrap();
                    assert_eq!(Address::p2pkh(pk, bitcoin::Network::Bitcoin).to_string(), coins[i].address);
                    let sighash = cache
                        .legacy_signature_hash(i, &prevouts[i].script_pubkey, EcdsaSighashType::All.to_u32())
                        .unwrap();
                    secp.verify_ecdsa(&Message::from(sighash), &EcdsaSignature::from_der(der).unwrap(), &pk.0)
                        .unwrap();
                }
                BtcScriptType::NativeSegwit | BtcScriptType::NestedSegwit => {
                    assert_eq!(input.witness.len(), 2);
                    let (der, ty) = input.witness[0].split_at(input.witness[0].len() - 1);
                    assert_eq!(ty, [EcdsaSighashType::All as u8]);
                    let pk = CompressedPublicKey::from_slice(&input.witness[1]).unwrap();
                    assert_eq!(t.address(&secp, &pk).to_string(), coins[i].address);
                    let inner = Address::p2wpkh(&pk, bitcoin::Network::Bitcoin).script_pubkey();
                    let sighash = cache
                        .p2wpkh_signature_hash(i, &inner, Amount::from_sat(coins[i].amount), EcdsaSighashType::All)
                        .unwrap();
                    secp.verify_ecdsa(&Message::from(sighash), &EcdsaSignature::from_der(der).unwrap(), &pk.0)
                        .unwrap();
                    match t {
                        BtcScriptType::NestedSegwit => {
                            assert_eq!(pushes(&input.script_sig), vec![inner.into_bytes()]);
                        }
                        _ => assert!(input.script_sig.is_empty()),
                    }
                }
            }
        }
    }

    /// A native-only transaction signs exactly as it did before the other
    /// families existed: two witness items, empty scriptSig.
    #[test]
    fn native_alone_is_unchanged() {
        let (wallet, coins) = wallet_of(&[BtcScriptType::NativeSegwit, BtcScriptType::NativeSegwit]);
        let mut tx = unsigned(&coins);
        sign_inputs(&mut tx, &coins, &wallet).unwrap();
        for input in &tx.input {
            assert!(input.script_sig.is_empty());
            assert_eq!(input.witness.len(), 2);
        }
    }

    #[test]
    fn a_coin_without_a_key_fails_the_whole_build() {
        let (wallet, mut coins) = wallet_of(&[BtcScriptType::Taproot]);
        coins.push(Utxo { txid: "22".repeat(32), vout: 0, amount: 1, address: "bc1qyfxmsjaaaaaaaaaaaaaaaaaaaaaaaaaaamh06f67".into() });
        let mut tx = unsigned(&coins);
        assert!(sign_inputs(&mut tx, &coins, &wallet).is_err());
    }

    /// A key stored under an address it does not derive — the one way a
    /// btc.json record from another seed could reach the signer — is refused
    /// before anything is signed.
    #[test]
    fn a_key_that_does_not_reproduce_its_address_is_refused() {
        let (_, coins) = wallet_of(&[BtcScriptType::Legacy]);
        let other = PrivateKey { compressed: true, network: bitcoin::NetworkKind::Main, inner: SecretKey::from_slice(&[9u8; 32]).unwrap() };
        let wallet = BitcoinWallet::for_test(coins[0].address.clone(), vec![(coins[0].address.clone(), other.to_wif())]);
        let mut tx = unsigned(&coins);
        let err = sign_inputs(&mut tx, &coins, &wallet).unwrap_err();
        assert!(err.contains("does not match"), "{err}");
    }
}

#[cfg(test)]
mod rbf_tests {
    use super::*;
    use crate::channel::{BtcRbfInfo, BtcRbfInput, BtcRbfOutput};

    const OWN: &str = "bc1qyfxmsjaaaaaaaaaaaaaaaaaaaaaaaaaaamh06f67";
    const THEM_SPK: &str = "0014aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OWN_SPK: &str = "0014bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    /// 1 in (100k) → 60k to them + 39,859 change, fee 141 (1 sat/vB at 141 vB).
    fn info() -> BtcRbfInfo {
        BtcRbfInfo {
            txid: "aa".repeat(32),
            vsize: 141,
            fee_sats: 141,
            inputs: vec![BtcRbfInput { txid: "11".repeat(32), vout: 0, sats: 100_000, address: Some(OWN.into()) }],
            outputs: vec![
                BtcRbfOutput { vout: 0, sats: 60_000, address: Some("them".into()), spk: Some(THEM_SPK.into()) },
                BtcRbfOutput { vout: 1, sats: 39_859, address: Some(OWN.into()), spk: Some(OWN_SPK.into()) },
            ],
            incremental_sat_vb: 1.0,
            min_sat_vb: 1.0,
            descendants: 0,
        }
    }
    fn ours() -> Vec<String> { vec![OWN.to_string()] }

    #[test]
    fn the_change_shrinks_and_the_recipient_is_untouched() {
        let plan = plan_replacement(&info(), &ours(), None, 1_000).unwrap();
        assert_eq!(plan.fee, 1_000);
        assert_eq!(plan.change_sats, Some(39_000));
        assert_eq!(plan.keep, vec![(ScriptBuf::from_hex(THEM_SPK).unwrap(), 60_000)]);
        assert_eq!(plan.vsize, 141);
        assert!(!plan.added_input);
    }

    #[test]
    fn bip125_floor_is_fee_plus_incremental_and_a_higher_rate() {
        // 141 + 1×141 = 282 is the least; 281 is refused, 282 accepted.
        assert!(plan_replacement(&info(), &ours(), None, 281).is_err());
        assert_eq!(plan_replacement(&info(), &ours(), None, 282).unwrap().fee, 282);
        assert_eq!(min_replacement_fee(&info(), 141), 282);
        // A fatter relay minimum wins when it is higher.
        let mut i = info(); i.min_sat_vb = 5.0;
        assert_eq!(min_replacement_fee(&i, 141), 705);
    }

    #[test]
    fn a_sub_dust_remainder_is_absorbed_into_the_fee() {
        // Leaves 39,859 − 39_500 = 359 < 546 of change: no change output,
        // fee becomes 100k − 60k = 40,000 at the change-less size.
        let plan = plan_replacement(&info(), &ours(), None, 39_500).unwrap();
        assert_eq!(plan.change_sats, None);
        assert_eq!(plan.fee, 40_000);
        assert_eq!(plan.vsize, 110);
    }

    #[test]
    fn a_spare_coin_funds_what_the_change_cannot() {
        let spare = Utxo { txid: "22".repeat(32), vout: 1, amount: 50_000, address: OWN.into() };
        assert!(plan_replacement(&info(), &ours(), None, 45_000).is_err());
        let plan = plan_replacement(&info(), &ours(), Some(&spare), 45_000).unwrap();
        assert!(plan.added_input);
        assert_eq!(plan.inputs.len(), 2);
        assert_eq!(plan.change_sats, Some(150_000 - 60_000 - 45_000));
        assert_eq!(plan.vsize, 209);
    }

    #[test]
    fn descendants_and_foreign_inputs_refuse() {
        let mut i = info(); i.descendants = 1;
        assert!(plan_replacement(&i, &ours(), None, 1_000).is_err());
        let mut i = info(); i.inputs[0].address = Some("bc1qsomeoneelse".into());
        assert!(plan_replacement(&i, &ours(), None, 1_000).is_err());
    }

    /// The relay's record carries no script; the planner rebuilds it from the
    /// address and lands on the bytes the original was paid with.
    #[test]
    fn an_output_script_is_rebuilt_from_its_address() {
        let mut i = info();
        i.outputs[0] = BtcRbfOutput {
            vout: 0,
            sats: 60_000,
            address: Some("bc1qu5g2twq0udg5g09h2u03u54x2ve8zkzcum7szm".into()),
            spk: None,
        };
        let plan = plan_replacement(&i, &ours(), None, 1_000).unwrap();
        let want = Address::from_str("bc1qu5g2twq0udg5g09h2u03u54x2ve8zkzcum7szm")
            .unwrap()
            .require_network(bitcoin::Network::Bitcoin)
            .unwrap()
            .script_pubkey();
        assert_eq!(plan.keep, vec![(want, 60_000)]);
    }

    #[test]
    fn a_tier_is_priced_at_the_replacement_size() {
        // 10 sat/vB at 141 vB = 1,410; change survives so the size holds.
        let plan = quote_replacement(&info(), &ours(), None, 10.0).unwrap();
        assert_eq!(plan.fee, 1_410);
        // 1 sat/vB cannot beat the original's 1 sat/vB: not offered.
        assert!(quote_replacement(&info(), &ours(), None, 1.0).is_err());
    }
}

#[cfg(test)]
mod size_tests {
    use super::*;
    use crate::btc_script_type::BtcScriptType;

    /// A tier costs its rate times the size, rounded up so the signed fee is
    /// never a hair under the rate that was picked — and nothing else. Any
    /// flooring has already happened at the node. Pinned at 140 vB here purely
    /// so the historical expectations still read; the size is a real
    /// measurement everywhere outside tests now.
    #[test]
    fn a_tier_costs_its_rate_and_nothing_else() {
        assert_eq!(tier_sats_at(1.0, 140), 140);
        assert_eq!(tier_sats_at(0.1, 140), 14);
        assert_eq!(tier_sats_at(3.0, 140), 420);
        assert_eq!(tier_sats_at(2.5, 140), 350);
        assert_eq!(tier_sats_at(1.001, 140), 141);
    }

    /// The exact sizes of the transactions this wallet actually builds. The
    /// 1-in/2-out case lands on 141 — one vB over the old flat guess — and
    /// every added input costs 68–69 vB, which is exactly the underquote the
    /// constant used to hide.
    #[test]
    fn vsize_is_exact_for_the_shapes_we_build() {
        use BtcScriptType::*;
        // 1 P2WPKH input, recipient + change both P2WPKH.
        assert_eq!(vsize_for(&[NativeSegwit], &[22, 22]), 141);
        // Each further input: 41 base bytes ×4 + 109 witness = 273 weight.
        assert_eq!(vsize_for(&[NativeSegwit; 2], &[22, 22]), 209);
        assert_eq!(vsize_for(&[NativeSegwit; 3], &[22, 22]), 278);
        // The change-less max-send shape is a whole output smaller.
        assert_eq!(vsize_for(&[NativeSegwit], &[22]), 110);
        // Paying legacy (25-byte spk) and taproot (34-byte spk) costs more.
        assert_eq!(vsize_for(&[NativeSegwit], &[25, 22]), 144);
        assert_eq!(vsize_for(&[NativeSegwit], &[34, 22]), 153);
    }

    /// The other three families, each spending one coin to a bc1q recipient
    /// with change in its own script. Taproot is the cheapest input on the
    /// network, legacy the dearest, and a legacy-only transaction carries no
    /// segwit marker at all.
    #[test]
    fn vsize_per_input_family() {
        use BtcScriptType::*;
        // base 4+1+41+1+(31+43)+4 = 125; weight 500+2+66 = 568 → 142.
        assert_eq!(vsize_for(&[Taproot], &[22, 34]), 142);
        // base 4+1+(41+23)+1+(31+32)+4 = 137; weight 548+2+109 = 659 → 165.
        assert_eq!(vsize_for(&[NestedSegwit], &[22, 23]), 165);
        // base 4+1+(41+108)+1+(31+34)+4 = 224, no witness → 224.
        assert_eq!(vsize_for(&[Legacy], &[22, 25]), 224);
        // Mixed inputs are summed per family, not by the wallet's default:
        // base 4+1+82+1+74+4 = 166; weight 664+2+(66+109) = 841 → 211.
        assert_eq!(vsize_for(&[Taproot, NativeSegwit], &[22, 34]), 211);
    }

    /// The live reading that exposed the old bug: 0.1 / 0.37 / 0.38 / 0.43
    /// sat/vB. These are four DISTINCT prices. A client-side 140-sat floor
    /// flattened them into one and then had to explain why `minimum` outbid
    /// `high`; nothing here may reintroduce that.
    #[test]
    fn the_live_rates_are_four_distinct_prices() {
        let live = [0.1f32, 0.37, 0.38, 0.43].map(|r| tier_sats_at(r, 140));
        assert_eq!(live, [14, 52, 54, 61]);
        // The cheapest tier is the cheapest. This is the invariant the old
        // floor broke, and it holds for free once nothing clamps.
        assert!(live[0] <= live[3], "the minimum is outbidding the top tier");
    }
}

#[cfg(test)]
mod offer_tests {
    use super::*;
    use crate::channel::BtcUtxo;

    /// What the signer may pick from is the set minus every coin a mempool
    /// transaction already spends — the mark the relay pushes, or the one our
    /// own dispatch sets moments earlier. Whatever a balance pane shows, a
    /// marked coin is never offered, so no transaction is ever built on one.
    #[test]
    fn a_marked_coin_is_never_offered_to_the_signer() {
        let wallet = "bc1qoffer-test";
        let coin = |txid: &str, sats: u64, height: u64, spent_by: Option<&str>| BtcUtxo {
            txid: txid.to_string(),
            vout: 0,
            sats,
            height,
            address: wallet.to_string(),
            spent_by: spent_by.map(String::from),
        };
        CHANNEL.btc_utxos_tx.send_replace((
            Some(wallet.to_string()),
            vec![
                coin("free", 5_923, 965_711, None),
                coin("consumed", 5_802, 969_974, Some("33cf")),
                coin("foreign", 100, 0, None),
            ],
        ));
        let offered: Vec<String> = eligible_utxos(wallet).unwrap().into_iter().map(|u| u.txid).collect();
        assert_eq!(offered, vec!["free".to_string()]);
    }
}
