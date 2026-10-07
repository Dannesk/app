//! The encrypted socket between a Dannesk app and the proxy.
//!
//! It is the Noise Protocol Framework's NK handshake, written from the
//! specification for one suite — X25519, AES-256-GCM and SHA-256 — on
//! primitives the app already relies on, rather than taken from a library.
//! NK is the shape of the problem: the proxy holds one long-term key whose
//! public half ships in the app, the app stays anonymous, and the app's first
//! bytes on the wire are already encrypted to that key. A connection costs
//! the TCP handshake and one exchange: no name lookup, no certificate, no
//! round trip spent on agreeing a key.
//!
//! Two layers. [`handshake`] is the protocol itself, pure functions over
//! bytes, checked against the Noise community's published test vector for
//! this suite. [`stream`], behind the `tokio` feature, is the socket both ends
//! speak: Noise messages as length-prefixed records on a byte stream, and the
//! app's frames as a stream of length-prefixed byte strings inside the
//! plaintext.
//!
//! One property to know before putting something in the first message: it is
//! encrypted to the proxy's long-term key and nothing else, so it can be
//! replayed, and it is readable later by anyone who recorded it and
//! afterwards obtained that key. Everything from the second message on has
//! forward secrecy. The opener names which of the proxy's keys the app used,
//! so the key can be rotated: the proxy answers to every key it still holds.

pub mod handshake;
#[cfg(feature = "tokio")]
pub mod stream;

pub use handshake::{
    Error, Initiator, InitiatorSent, Receiver, Responder, ResponderReplying, Sender, Session,
    StaticKey,
};
