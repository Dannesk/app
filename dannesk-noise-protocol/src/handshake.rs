//! Noise NK over X25519, AES-256-GCM and SHA-256, written from the Noise
//! Protocol Framework, revision 34 (noiseprotocol.org/noise.html). The names
//! are the specification's own — CipherState, SymmetricState, MixKey,
//! MixHash, Split — so the code can be read beside it, and the test at the
//! end drives both sides through the Noise community's published vector for
//! this suite, byte for byte.
//!
//! The pattern, with the proxy as the responder:
//!
//! ```text
//! NK:
//!   <- s          the proxy's public key, known to the app before it connects
//!   ...
//!   -> e, es      message 1: the app's ephemeral key, then its payload
//!   <- e, ee      message 2: the proxy's ephemeral key, then its payload
//! ```
//!
//! Each side is one type per step, consumed by that step, so a handshake
//! cannot be driven twice or out of order. Both end in a [`Session`]: one
//! cipher per direction, each with its own nonce, from which every transport
//! message comes.

use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, SharedSecret, StaticSecret};
use zeroize::Zeroizing;

/// The suite, as Noise names it. It seeds the handshake hash and the chaining
/// key, so two ends that disagree on it cannot complete a handshake.
pub const PROTOCOL_NAME: &[u8] = b"Noise_NK_25519_AESGCM_SHA256";
/// DHLEN, HASHLEN and the cipher's key length are all 32 in this suite.
pub const KEY_LEN: usize = 32;
/// The authentication tag AES-GCM appends to every encrypted message.
pub const TAG_LEN: usize = 16;
/// No Noise message, handshake or transport, is longer than this.
pub const MAX_MESSAGE_LEN: usize = 65_535;
/// The longest plaintext one transport message carries.
pub const MAX_TRANSPORT_PAYLOAD: usize = MAX_MESSAGE_LEN - TAG_LEN;
/// The longest payload a handshake message carries beside the ephemeral
/// public key and the tag.
pub const MAX_HANDSHAKE_PAYLOAD: usize = MAX_MESSAGE_LEN - KEY_LEN - TAG_LEN;

/// What can go wrong inside the protocol. Each one ends the connection; none
/// is recoverable in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The message did not authenticate: another key, altered bytes, or a
    /// message out of its order.
    Decrypt,
    /// The message is shorter or longer than the format allows.
    Format,
    /// The peer's public key is degenerate: the shared secret it gives is one
    /// anyone could compute.
    BadKey,
    /// The payload would make the message longer than [`MAX_MESSAGE_LEN`].
    TooLong,
    /// 2⁶⁴ − 1 messages went one way on this session. The nonce space is
    /// spent and the session must end; at any real rate, never.
    NonceExhausted,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Error::Decrypt => "message failed to authenticate",
            Error::Format => "message of an impossible length",
            Error::BadKey => "degenerate public key",
            Error::TooLong => "payload too long for one message",
            Error::NonceExhausted => "nonce space exhausted",
        })
    }
}

impl std::error::Error for Error {}

// ───────────────────────── primitives ─────────────────────────

fn hash(parts: &[&[u8]]) -> [u8; KEY_LEN] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

fn hmac(key: &[u8; KEY_LEN], parts: &[&[u8]]) -> Zeroizing<[u8; KEY_LEN]> {
    // HMAC takes a key of any length; the error is unreachable.
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts a key of any length");
    for p in parts {
        mac.update(p);
    }
    Zeroizing::new(mac.finalize().into_bytes().into())
}

/// HKDF as the specification defines it, for the two outputs every use here
/// needs: `temp = HMAC(ck, ikm)`, `out1 = HMAC(temp, 0x01)`,
/// `out2 = HMAC(temp, out1 ‖ 0x02)`.
fn hkdf(ck: &[u8; KEY_LEN], ikm: &[u8]) -> (Zeroizing<[u8; KEY_LEN]>, Zeroizing<[u8; KEY_LEN]>) {
    let temp = hmac(ck, &[ikm]);
    let out1 = hmac(&temp, &[&[1u8]]);
    let out2 = hmac(&temp, &[&out1[..], &[2u8]]);
    (out1, out2)
}

/// X25519, refusing the all-zero result a degenerate public key produces.
fn dh(secret: &StaticSecret, public: &PublicKey) -> Result<SharedSecret, Error> {
    let shared = secret.diffie_hellman(public);
    if shared.was_contributory() { Ok(shared) } else { Err(Error::BadKey) }
}

// ───────────────────────── CipherState ─────────────────────────

/// A key and the nonce of the next message. Before the first MixKey there is
/// no key and bytes pass through unchanged, as the specification has it; NK
/// never carries a payload in that state, but the rule is kept whole.
struct CipherState {
    cipher: Option<Aes256Gcm>,
    nonce: u64,
}

impl CipherState {
    fn empty() -> Self {
        CipherState { cipher: None, nonce: 0 }
    }

    fn keyed(key: &[u8; KEY_LEN]) -> Self {
        CipherState { cipher: Some(Aes256Gcm::new(key.into())), nonce: 0 }
    }

    /// The 96-bit nonce the specification prescribes for AES-GCM: four zero
    /// bytes, then the counter big-endian.
    fn nonce_bytes(n: u64) -> [u8; 12] {
        let mut bytes = [0u8; 12];
        bytes[4..].copy_from_slice(&n.to_be_bytes());
        bytes
    }

    fn encrypt(&mut self, ad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let Some(cipher) = &self.cipher else { return Ok(plaintext.to_vec()) };
        if self.nonce == u64::MAX {
            return Err(Error::NonceExhausted);
        }
        let out = cipher
            .encrypt(&Nonce::from(Self::nonce_bytes(self.nonce)), Payload { msg: plaintext, aad: ad })
            .map_err(|_| Error::TooLong)?;
        self.nonce += 1;
        Ok(out)
    }

    /// The nonce moves only on success: a message that fails to authenticate
    /// leaves the state as it was.
    fn decrypt(&mut self, ad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
        let Some(cipher) = &self.cipher else { return Ok(ciphertext.to_vec()) };
        if self.nonce == u64::MAX {
            return Err(Error::NonceExhausted);
        }
        let out = cipher
            .decrypt(&Nonce::from(Self::nonce_bytes(self.nonce)), Payload { msg: ciphertext, aad: ad })
            .map_err(|_| Error::Decrypt)?;
        self.nonce += 1;
        Ok(out)
    }
}

// ───────────────────────── SymmetricState ─────────────────────────

/// The chaining key `ck`, the handshake hash `h`, and the cipher the current
/// key feeds.
struct SymmetricState {
    cipher: CipherState,
    ck: Zeroizing<[u8; KEY_LEN]>,
    h: [u8; KEY_LEN],
}

impl SymmetricState {
    fn new(prologue: &[u8]) -> Self {
        // A protocol name no longer than the hash is the initial hash itself,
        // padded with zeros; a longer one is hashed.
        let h = if PROTOCOL_NAME.len() <= KEY_LEN {
            let mut h = [0u8; KEY_LEN];
            h[..PROTOCOL_NAME.len()].copy_from_slice(PROTOCOL_NAME);
            h
        } else {
            hash(&[PROTOCOL_NAME])
        };
        let mut state = SymmetricState { cipher: CipherState::empty(), ck: Zeroizing::new(h), h };
        state.mix_hash(prologue);
        state
    }

    fn mix_hash(&mut self, data: &[u8]) {
        self.h = hash(&[&self.h[..], data]);
    }

    fn mix_key(&mut self, ikm: &[u8]) {
        let (ck, key) = hkdf(&self.ck, ikm);
        self.ck = ck;
        self.cipher = CipherState::keyed(&key);
    }

    fn encrypt_and_hash(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let ciphertext = self.cipher.encrypt(&self.h, plaintext)?;
        self.mix_hash(&ciphertext);
        Ok(ciphertext)
    }

    fn decrypt_and_hash(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
        let plaintext = self.cipher.decrypt(&self.h, ciphertext)?;
        self.mix_hash(ciphertext);
        Ok(plaintext)
    }

    /// The two transport ciphers — the first for the initiator's messages,
    /// the second for the responder's — and the final handshake hash.
    fn split(self) -> (CipherState, CipherState, [u8; KEY_LEN]) {
        let (k1, k2) = hkdf(&self.ck, &[]);
        (CipherState::keyed(&k1), CipherState::keyed(&k2), self.h)
    }
}

// ───────────────────────── keys ─────────────────────────

/// The responder's long-term key pair: the proxy's. Its public half is what
/// the apps carry; its secret half never leaves the machine it was made on.
pub struct StaticKey {
    secret: StaticSecret,
    public: PublicKey,
}

impl StaticKey {
    /// A fresh key from the operating system's randomness.
    pub fn generate() -> Self {
        let secret = StaticSecret::random();
        let public = PublicKey::from(&secret);
        StaticKey { secret, public }
    }

    /// The key as a file holds it: its 32 secret bytes. The caller's own copy
    /// of them is the caller's to wipe.
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        let secret = StaticSecret::from(bytes);
        let public = PublicKey::from(&secret);
        StaticKey { secret, public }
    }

    /// The public half, as the apps embed it.
    pub fn public(&self) -> [u8; KEY_LEN] {
        self.public.to_bytes()
    }

    /// The secret half, for writing the key file once. Wiped on drop.
    pub fn secret_bytes(&self) -> Zeroizing<[u8; KEY_LEN]> {
        Zeroizing::new(self.secret.to_bytes())
    }
}

// ───────────────────────── the initiator ─────────────────────────

/// The app's side, before message 1.
pub struct Initiator {
    state: SymmetricState,
    e: StaticSecret,
    e_public: PublicKey,
    rs: PublicKey,
}

impl Initiator {
    /// Towards the responder whose public key is `server`, with the prologue
    /// both sides agree on beforehand.
    pub fn new(server: &[u8; KEY_LEN], prologue: &[u8]) -> Self {
        Self::with_ephemeral(server, prologue, StaticSecret::random())
    }

    /// [`new`](Self::new) draws the ephemeral; the vector test supplies one.
    fn with_ephemeral(server: &[u8; KEY_LEN], prologue: &[u8], e: StaticSecret) -> Self {
        let mut state = SymmetricState::new(prologue);
        let rs = PublicKey::from(*server);
        // The pre-message: `<- s`.
        state.mix_hash(rs.as_bytes());
        Initiator { state, e_public: PublicKey::from(&e), e, rs }
    }

    /// Message 1: `-> e, es`, then the payload. Returns the bytes to send.
    pub fn write_message(mut self, payload: &[u8]) -> Result<(InitiatorSent, Vec<u8>), Error> {
        if payload.len() > MAX_HANDSHAKE_PAYLOAD {
            return Err(Error::TooLong);
        }
        let mut message = Vec::with_capacity(KEY_LEN + payload.len() + TAG_LEN);
        // e
        message.extend_from_slice(self.e_public.as_bytes());
        self.state.mix_hash(self.e_public.as_bytes());
        // es
        let shared = dh(&self.e, &self.rs)?;
        self.state.mix_key(shared.as_bytes());
        message.extend_from_slice(&self.state.encrypt_and_hash(payload)?);
        Ok((InitiatorSent { state: self.state, e: self.e }, message))
    }
}

/// The app's side, after message 1 and before message 2.
pub struct InitiatorSent {
    state: SymmetricState,
    e: StaticSecret,
}

impl InitiatorSent {
    /// Message 2: `<- e, ee`, then the responder's payload. Returns the
    /// session and that payload.
    pub fn read_message(mut self, message: &[u8]) -> Result<(Session, Vec<u8>), Error> {
        if message.len() < KEY_LEN + TAG_LEN || message.len() > MAX_MESSAGE_LEN {
            return Err(Error::Format);
        }
        // e
        let re = PublicKey::from(<[u8; KEY_LEN]>::try_from(&message[..KEY_LEN]).map_err(|_| Error::Format)?);
        self.state.mix_hash(re.as_bytes());
        // ee
        let shared = dh(&self.e, &re)?;
        self.state.mix_key(shared.as_bytes());
        let payload = self.state.decrypt_and_hash(&message[KEY_LEN..])?;
        let (initiator_cipher, responder_cipher, handshake_hash) = self.state.split();
        let session = Session {
            send: Sender(initiator_cipher),
            recv: Receiver(responder_cipher),
            handshake_hash,
        };
        Ok((session, payload))
    }
}

// ───────────────────────── the responder ─────────────────────────

/// The proxy's side, before message 1.
pub struct Responder {
    state: SymmetricState,
    s: StaticSecret,
}

impl Responder {
    pub fn new(key: &StaticKey, prologue: &[u8]) -> Self {
        let mut state = SymmetricState::new(prologue);
        // The pre-message: `<- s`.
        state.mix_hash(key.public.as_bytes());
        Responder { state, s: key.secret.clone() }
    }

    /// Message 1: `-> e, es`, then the initiator's payload. Returns the next
    /// step and that payload.
    pub fn read_message(mut self, message: &[u8]) -> Result<(ResponderReplying, Vec<u8>), Error> {
        if message.len() < KEY_LEN + TAG_LEN || message.len() > MAX_MESSAGE_LEN {
            return Err(Error::Format);
        }
        // e
        let re = PublicKey::from(<[u8; KEY_LEN]>::try_from(&message[..KEY_LEN]).map_err(|_| Error::Format)?);
        self.state.mix_hash(re.as_bytes());
        // es
        let shared = dh(&self.s, &re)?;
        self.state.mix_key(shared.as_bytes());
        let payload = self.state.decrypt_and_hash(&message[KEY_LEN..])?;
        Ok((ResponderReplying { state: self.state, re }, payload))
    }
}

/// The proxy's side, after message 1 and before message 2.
pub struct ResponderReplying {
    state: SymmetricState,
    re: PublicKey,
}

impl ResponderReplying {
    /// Message 2: `<- e, ee`, then the payload. Returns the session and the
    /// bytes to send.
    pub fn write_message(self, payload: &[u8]) -> Result<(Session, Vec<u8>), Error> {
        self.write_with_ephemeral(payload, StaticSecret::random())
    }

    fn write_with_ephemeral(mut self, payload: &[u8], e: StaticSecret) -> Result<(Session, Vec<u8>), Error> {
        if payload.len() > MAX_HANDSHAKE_PAYLOAD {
            return Err(Error::TooLong);
        }
        let e_public = PublicKey::from(&e);
        let mut message = Vec::with_capacity(KEY_LEN + payload.len() + TAG_LEN);
        // e
        message.extend_from_slice(e_public.as_bytes());
        self.state.mix_hash(e_public.as_bytes());
        // ee
        let shared = dh(&e, &self.re)?;
        self.state.mix_key(shared.as_bytes());
        message.extend_from_slice(&self.state.encrypt_and_hash(payload)?);
        let (initiator_cipher, responder_cipher, handshake_hash) = self.state.split();
        let session = Session {
            send: Sender(responder_cipher),
            recv: Receiver(initiator_cipher),
            handshake_hash,
        };
        Ok((session, message))
    }
}

// ───────────────────────── the session ─────────────────────────

/// A finished handshake: one cipher for what this side sends and one for what
/// it receives, each counting its own messages. Transport messages carry no
/// associated data.
pub struct Session {
    send: Sender,
    recv: Receiver,
    handshake_hash: [u8; KEY_LEN],
}

impl Session {
    /// One transport message from a plaintext of up to [`MAX_TRANSPORT_PAYLOAD`] bytes.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        self.send.encrypt(plaintext)
    }

    /// The plaintext of one transport message.
    pub fn decrypt(&mut self, message: &[u8]) -> Result<Vec<u8>, Error> {
        self.recv.decrypt(message)
    }

    /// The handshake hash, the same on both sides: a digest of everything the
    /// handshake carried, for binding something to this very session.
    pub fn handshake_hash(&self) -> &[u8; KEY_LEN] {
        &self.handshake_hash
    }

    /// The two directions as separate values, for a reader and a writer that
    /// run apart.
    pub fn split(self) -> (Sender, Receiver) {
        (self.send, self.recv)
    }
}

/// The sending direction of a [`Session`].
pub struct Sender(CipherState);

impl Sender {
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        if plaintext.len() > MAX_TRANSPORT_PAYLOAD {
            return Err(Error::TooLong);
        }
        self.0.encrypt(&[], plaintext)
    }
}

/// The receiving direction of a [`Session`].
pub struct Receiver(CipherState);

impl Receiver {
    pub fn decrypt(&mut self, message: &[u8]) -> Result<Vec<u8>, Error> {
        if message.len() < TAG_LEN || message.len() > MAX_MESSAGE_LEN {
            return Err(Error::Format);
        }
        self.0.decrypt(&[], message)
    }
}

// ───────────────────────── tests ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// The Noise community's vector for this suite, generated by cacophony
    /// (the Haskell implementation) and kept in snow's repository as
    /// `tests/vectors/cacophony.txt`. Fixed keys on both sides, two handshake
    /// messages, then four transport messages alternating initiator and
    /// responder; every ciphertext must match, and so must the hash.
    const VECTOR: &str = include_str!("../tests/vectors/Noise_NK_25519_AESGCM_SHA256.json");

    fn bytes(v: &serde_json::Value, key: &str) -> Vec<u8> {
        hex::decode(v[key].as_str().unwrap_or_else(|| panic!("vector field {key}"))).unwrap()
    }

    fn bytes32(v: &serde_json::Value, key: &str) -> [u8; KEY_LEN] {
        bytes(v, key).try_into().unwrap()
    }

    #[test]
    fn cacophony_vector() {
        let v: serde_json::Value = serde_json::from_str(VECTOR).unwrap();
        assert_eq!(v["protocol_name"].as_str().unwrap().as_bytes(), PROTOCOL_NAME);
        let prologue = bytes(&v, "init_prologue");
        assert_eq!(prologue, bytes(&v, "resp_prologue"));
        let server = StaticKey::from_bytes(bytes32(&v, "resp_static"));
        assert_eq!(server.public(), bytes32(&v, "init_remote_static"), "public key derivation");
        let init_e = StaticSecret::from(bytes32(&v, "init_ephemeral"));
        let resp_e = StaticSecret::from(bytes32(&v, "resp_ephemeral"));
        let messages: Vec<(Vec<u8>, Vec<u8>)> = v["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| (bytes(m, "payload"), bytes(m, "ciphertext")))
            .collect();
        assert_eq!(messages.len(), 6);

        // Message 1, written by the initiator and read by the responder.
        let (initiator, m1) = Initiator::with_ephemeral(&server.public(), &prologue, init_e)
            .write_message(&messages[0].0)
            .unwrap();
        assert_eq!(m1, messages[0].1, "message 1");
        let (responder, p1) = Responder::new(&server, &prologue).read_message(&m1).unwrap();
        assert_eq!(p1, messages[0].0);

        // Message 2, the other way.
        let (mut responder_session, m2) = responder.write_with_ephemeral(&messages[1].0, resp_e).unwrap();
        assert_eq!(m2, messages[1].1, "message 2");
        let (mut initiator_session, p2) = initiator.read_message(&m2).unwrap();
        assert_eq!(p2, messages[1].0);

        let handshake_hash = bytes32(&v, "handshake_hash");
        assert_eq!(initiator_session.handshake_hash(), &handshake_hash);
        assert_eq!(responder_session.handshake_hash(), &handshake_hash);

        // Transport: the initiator sends the even messages, the responder the odd.
        for (i, (payload, ciphertext)) in messages[2..].iter().enumerate() {
            let (from, to) = if i % 2 == 0 {
                (&mut initiator_session, &mut responder_session)
            } else {
                (&mut responder_session, &mut initiator_session)
            };
            let message = from.encrypt(payload).unwrap();
            assert_eq!(&message, ciphertext, "transport message {}", i + 2);
            assert_eq!(&to.decrypt(&message).unwrap(), payload);
        }
    }

    /// Both sides with fresh keys, through the handshake, to two sessions.
    fn sessions(payload1: &[u8], payload2: &[u8]) -> (Session, Session) {
        let server = StaticKey::generate();
        let (initiator, m1) = Initiator::new(&server.public(), b"test").write_message(payload1).unwrap();
        let (responder, p1) = Responder::new(&server, b"test").read_message(&m1).unwrap();
        assert_eq!(p1, payload1);
        let (responder_session, m2) = responder.write_message(payload2).unwrap();
        let (initiator_session, p2) = initiator.read_message(&m2).unwrap();
        assert_eq!(p2, payload2);
        (initiator_session, responder_session)
    }

    #[test]
    fn round_trip_with_fresh_keys() {
        let (mut app, mut proxy) = sessions(b"hello", b"");
        assert_eq!(app.handshake_hash(), proxy.handshake_hash());
        for i in 0..5u8 {
            let out = app.encrypt(&[i; 100]).unwrap();
            assert_eq!(out.len(), 100 + TAG_LEN);
            assert_eq!(proxy.decrypt(&out).unwrap(), [i; 100]);
            let back = proxy.encrypt(&[i; 3]).unwrap();
            assert_eq!(app.decrypt(&back).unwrap(), [i; 3]);
        }
        // The two directions are independent keys: a message sent back as
        // received does not authenticate.
        let out = app.encrypt(b"one way").unwrap();
        assert_eq!(app.decrypt(&out), Err(Error::Decrypt));
    }

    #[test]
    fn altered_replayed_and_reordered_messages_fail() {
        let (mut app, mut proxy) = sessions(b"", b"");
        let first = app.encrypt(b"first").unwrap();
        let second = app.encrypt(b"second").unwrap();
        let mut altered = first.clone();
        altered[3] ^= 1;
        assert_eq!(proxy.decrypt(&altered), Err(Error::Decrypt));
        // A failure moves nothing: the genuine message still reads.
        assert_eq!(proxy.decrypt(&first).unwrap(), b"first");
        assert_eq!(proxy.decrypt(&first), Err(Error::Decrypt), "replay");
        assert_eq!(proxy.decrypt(&second).unwrap(), b"second");
        // Out of order: the third is read before the second arrives.
        let third = app.encrypt(b"third").unwrap();
        let fourth = app.encrypt(b"fourth").unwrap();
        assert_eq!(proxy.decrypt(&fourth), Err(Error::Decrypt));
        assert_eq!(proxy.decrypt(&third).unwrap(), b"third");
        assert_eq!(proxy.decrypt(&fourth).unwrap(), b"fourth");
    }

    #[test]
    fn wrong_server_key_fails_at_the_server() {
        let server = StaticKey::generate();
        let other = StaticKey::generate();
        let (_, m1) = Initiator::new(&other.public(), b"").write_message(b"hello").unwrap();
        assert!(matches!(Responder::new(&server, b"").read_message(&m1), Err(Error::Decrypt)));
    }

    #[test]
    fn different_prologues_fail() {
        let server = StaticKey::generate();
        let (_, m1) = Initiator::new(&server.public(), b"v1").write_message(b"").unwrap();
        assert!(matches!(Responder::new(&server, b"v2").read_message(&m1), Err(Error::Decrypt)));
    }

    #[test]
    fn lengths_are_enforced() {
        let server = StaticKey::generate();
        let too_long = vec![0u8; MAX_HANDSHAKE_PAYLOAD + 1];
        assert!(matches!(Initiator::new(&server.public(), b"").write_message(&too_long), Err(Error::TooLong)));
        let (initiator, m1) = Initiator::new(&server.public(), b"")
            .write_message(&vec![7u8; MAX_HANDSHAKE_PAYLOAD])
            .unwrap();
        assert_eq!(m1.len(), MAX_MESSAGE_LEN);
        let (responder, _) = Responder::new(&server, b"").read_message(&m1).unwrap();
        let (mut proxy, m2) = responder.write_message(b"").unwrap();
        let (mut app, _) = initiator.read_message(&m2).unwrap();
        assert!(matches!(app.encrypt(&vec![0u8; MAX_TRANSPORT_PAYLOAD + 1]), Err(Error::TooLong)));
        let full = app.encrypt(&vec![1u8; MAX_TRANSPORT_PAYLOAD]).unwrap();
        assert_eq!(full.len(), MAX_MESSAGE_LEN);
        assert_eq!(proxy.decrypt(&full).unwrap().len(), MAX_TRANSPORT_PAYLOAD);
        assert_eq!(proxy.decrypt(&[0u8; TAG_LEN - 1]), Err(Error::Format));
        assert!(matches!(Responder::new(&server, b"").read_message(&[0u8; KEY_LEN + TAG_LEN - 1]), Err(Error::Format)));
    }

    #[test]
    fn a_degenerate_ephemeral_is_refused() {
        let server = StaticKey::generate();
        // All-zero public key: X25519 maps it to an all-zero shared secret.
        let mut m1 = vec![0u8; KEY_LEN + TAG_LEN];
        m1[KEY_LEN..].fill(0xAA);
        assert!(matches!(Responder::new(&server, b"").read_message(&m1), Err(Error::BadKey)));
    }

    #[test]
    fn the_nonce_space_ends() {
        let (mut app, mut proxy) = sessions(b"", b"");
        app.send.0.nonce = u64::MAX - 1;
        proxy.recv.0.nonce = u64::MAX - 1;
        let last = app.encrypt(b"last").unwrap();
        assert_eq!(proxy.decrypt(&last).unwrap(), b"last");
        assert_eq!(app.encrypt(b"one more"), Err(Error::NonceExhausted));
        assert_eq!(proxy.decrypt(&last), Err(Error::NonceExhausted));
    }
}
