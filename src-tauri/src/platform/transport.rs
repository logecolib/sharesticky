//! The port between our sharing logic and the peer-to-peer network.
//!
//! Everything above this trait is ordinary testable Rust. Everything below it
//! is a thin adapter holding the iroh QUIC calls and no decisions - see
//! `platform::iroh_transport` (added with the real adapter).
//!
//! Deliberately no iroh types appear in these signatures. If an `EndpointId` or
//! `Connection` leaked into the trait, every caller would transitively depend on
//! the network crate and we would have moved the problem rather than solved it.

use std::fmt;
use std::sync::Arc;

/// A peer's dialable identity: the hex form of its iroh `EndpointId` (an Ed25519
/// public key). The same string is our own address, shared out of band so a
/// peer can connect back.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeerId(pub String);

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a transport operation did not succeed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// No live session to this peer - dial first, or it has dropped.
    NotConnected(String),
    /// The peer id was not a valid address.
    BadPeer(String),
    /// A received frame was not a well-formed envelope.
    MalformedFrame(String),
    /// The transport itself is unusable (e.g. it failed to bind a socket).
    Unavailable(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected(s) => write!(f, "no live session to peer: {s}"),
            Self::BadPeer(s) => write!(f, "bad peer address: {s}"),
            Self::MalformedFrame(s) => write!(f, "malformed frame: {s}"),
            Self::Unavailable(s) => write!(f, "transport unavailable: {s}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// One frame inbound from a peer: who sent it, and its opaque bytes. What the
/// bytes mean (which note, which sync message) is decoded a layer up.
pub type Inbound = (PeerId, Vec<u8>);

/// Where a transport delivers inbound frames. Production forwards them to the
/// webview as a Tauri event; tests collect them in a vec. Keeping this a
/// callback (not a channel) frees the trait from any runtime/channel type.
pub type InboundSink = Arc<dyn Fn(Inbound) + Send + Sync>;

/// Operations we need from the peer-to-peer network.
///
/// `dial` and `send` are **non-blocking**: they hand work to background tasks
/// and return at once, so command handlers never await the network. Delivery
/// failures and inbound frames both surface asynchronously (the latter via the
/// [`InboundSink`] given when the transport was built).
pub trait Transport: Send + Sync {
    /// Our own dialable id, to share with a peer so they can connect back.
    fn endpoint_id(&self) -> String;

    /// Begin connecting to a peer. Idempotent: a live session is reused.
    fn dial(&self, peer: &PeerId) -> Result<(), TransportError>;

    /// Enqueue a frame for delivery to an (already dialed) peer.
    fn send(&self, peer: &PeerId, frame: &[u8]) -> Result<(), TransportError>;
}

/// The wire envelope carried by every frame: which note the sync payload belongs
/// to, then the payload itself.
///
/// Layout: `[u16 big-endian note-id length][note-id utf-8][payload]`. Note ids
/// are short (uuid-like), far under `u16::MAX`. Routing lives here, not in the
/// transport, so one connection can carry many notes.
pub fn encode_envelope(note_id: &str, payload: &[u8]) -> Vec<u8> {
    let id = note_id.as_bytes();
    let mut out = Vec::with_capacity(2 + id.len() + payload.len());
    out.extend_from_slice(&(id.len() as u16).to_be_bytes());
    out.extend_from_slice(id);
    out.extend_from_slice(payload);
    out
}

/// Inverse of [`encode_envelope`]. Returns the note id and the payload bytes.
pub fn decode_envelope(bytes: &[u8]) -> Result<(String, Vec<u8>), TransportError> {
    if bytes.len() < 2 {
        return Err(TransportError::MalformedFrame("shorter than header".into()));
    }
    let id_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    let rest = &bytes[2..];
    if rest.len() < id_len {
        return Err(TransportError::MalformedFrame("note id truncated".into()));
    }
    let (id_bytes, payload) = rest.split_at(id_len);
    let note_id = std::str::from_utf8(id_bytes)
        .map_err(|_| TransportError::MalformedFrame("note id not utf-8".into()))?
        .to_string();
    Ok((note_id, payload.to_vec()))
}

/// An in-process stand-in for the network, for tests and local wiring.
///
/// Endpoints register an inbound sink under their id; a `send` routes straight
/// into the target's sink, tagged with the sender's id. No sockets, no runtime.
#[cfg(test)]
pub mod fake {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Clone, Default)]
    pub struct FakeNetwork {
        sinks: Arc<Mutex<HashMap<PeerId, InboundSink>>>,
    }

    impl FakeNetwork {
        /// Register an endpoint `id` whose inbound frames go to `on_inbound`.
        pub fn endpoint(&self, id: &str, on_inbound: InboundSink) -> FakeTransport {
            self.sinks
                .lock()
                .unwrap()
                .insert(PeerId(id.to_string()), on_inbound);
            FakeTransport {
                id: PeerId(id.to_string()),
                net: self.clone(),
                dialed: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    pub struct FakeTransport {
        id: PeerId,
        net: FakeNetwork,
        dialed: Arc<Mutex<Vec<PeerId>>>,
    }

    impl FakeTransport {
        /// Peers this transport was asked to dial, in order.
        pub fn dialed(&self) -> Vec<PeerId> {
            self.dialed.lock().unwrap().clone()
        }
    }

    impl Transport for FakeTransport {
        fn endpoint_id(&self) -> String {
            self.id.0.clone()
        }

        fn dial(&self, peer: &PeerId) -> Result<(), TransportError> {
            self.dialed.lock().unwrap().push(peer.clone());
            Ok(())
        }

        fn send(&self, peer: &PeerId, frame: &[u8]) -> Result<(), TransportError> {
            let sinks = self.net.sinks.lock().unwrap();
            let sink = sinks
                .get(peer)
                .ok_or_else(|| TransportError::NotConnected(peer.0.clone()))?;
            sink((self.id.clone(), frame.to_vec()));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeNetwork;
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn envelope_round_trips_note_id_and_payload() {
        let frame = encode_envelope("note-42", b"\x00\x01\x02sync-bytes");
        let (id, payload) = decode_envelope(&frame).unwrap();
        assert_eq!(id, "note-42");
        assert_eq!(payload, b"\x00\x01\x02sync-bytes");
    }

    #[test]
    fn envelope_round_trips_empty_payload() {
        let frame = encode_envelope("n", b"");
        let (id, payload) = decode_envelope(&frame).unwrap();
        assert_eq!(id, "n");
        assert!(payload.is_empty());
    }

    #[test]
    fn decode_rejects_a_frame_shorter_than_its_header() {
        assert!(matches!(
            decode_envelope(&[0x00]),
            Err(TransportError::MalformedFrame(_))
        ));
    }

    #[test]
    fn decode_rejects_a_truncated_note_id() {
        // Header claims a 5-byte id but only 2 bytes follow.
        let bytes = [0x00, 0x05, b'a', b'b'];
        assert!(matches!(
            decode_envelope(&bytes),
            Err(TransportError::MalformedFrame(_))
        ));
    }

    #[test]
    fn decode_rejects_a_non_utf8_note_id() {
        let bytes = [0x00, 0x01, 0xFF];
        assert!(matches!(
            decode_envelope(&bytes),
            Err(TransportError::MalformedFrame(_))
        ));
    }

    fn collector() -> (InboundSink, Arc<Mutex<Vec<Inbound>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink_seen = seen.clone();
        let sink: InboundSink = Arc::new(move |inbound| sink_seen.lock().unwrap().push(inbound));
        (sink, seen)
    }

    #[test]
    fn fake_delivers_a_frame_to_the_target_tagged_with_the_sender() {
        let net = FakeNetwork::default();
        let (sink_b, seen_b) = collector();
        let a = net.endpoint("A", collector().0);
        let _b = net.endpoint("B", sink_b);

        a.send(&PeerId("B".into()), b"hello").unwrap();

        let seen = seen_b.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, PeerId("A".into()));
        assert_eq!(seen[0].1, b"hello");
    }

    #[test]
    fn fake_send_to_an_unregistered_peer_is_not_connected() {
        let net = FakeNetwork::default();
        let a = net.endpoint("A", collector().0);
        assert_eq!(
            a.send(&PeerId("ghost".into()), b"x"),
            Err(TransportError::NotConnected("ghost".into()))
        );
    }

    #[test]
    fn fake_records_dialed_peers() {
        let net = FakeNetwork::default();
        let a = net.endpoint("A", collector().0);
        a.dial(&PeerId("B".into())).unwrap();
        a.dial(&PeerId("C".into())).unwrap();
        assert_eq!(a.dialed(), vec![PeerId("B".into()), PeerId("C".into())]);
    }
}
