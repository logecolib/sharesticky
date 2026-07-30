//! Sharing (Phase 3b): live peer-to-peer sync of individual notes.
//!
//! The heavy lifting is elsewhere: the Yjs sync protocol runs in the webview,
//! and QUIC networking runs in the `net-sidecar` process behind the
//! [`Transport`](crate::platform::transport::Transport) port. These commands are
//! the seam between them - hand the webview our address, dial a peer, fan a
//! note's sync frames out to connected peers, and materialise a received note.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use tauri::State;
use uuid::Uuid;

use crate::platform::transport::{encode_envelope, PeerId, Transport, TransportError};
use crate::storage::now_millis;
use crate::storage::repo::{SqliteRepo, Sticky, StickyRepo};

/// Networking state for sharing: the transport plus the set of peers we sync
/// with. A note edit fans out to every peer in the set.
///
/// Peers join two ways: we dial one (outgoing share), or one dials us and we
/// learn its id from its first inbound frame (incoming share). The latter is
/// inserted by the inbound sink in `lib.rs`, which holds this same `peers` set -
/// that is what lets the receiver's edits flow back to the sharer.
pub struct Sharing {
    transport: Arc<dyn Transport>,
    peers: Arc<Mutex<HashSet<PeerId>>>,
}

impl Sharing {
    pub fn new(transport: Arc<dyn Transport>, peers: Arc<Mutex<HashSet<PeerId>>>) -> Self {
        Self { transport, peers }
    }
}

/// Our dialable endpoint id, to hand to a peer so they can connect back.
#[tauri::command]
pub fn sharing_endpoint_id(state: State<'_, Sharing>) -> String {
    state.transport.endpoint_id()
}

/// Start connecting to a peer and remember it as a sync target.
#[tauri::command]
pub fn sharing_dial(state: State<'_, Sharing>, peer_id: String) -> Result<(), String> {
    let peer = PeerId(peer_id);
    state.peers.lock().unwrap().insert(peer.clone());
    state.transport.dial(&peer).map_err(|e| e.to_string())
}

/// Fan a note's Yjs sync frame out to every connected peer.
///
/// Best-effort per peer: a send to a dropped peer does not fail the whole call
/// (the sync protocol re-converges when the peer reconnects).
#[tauri::command]
pub fn send_sync_frame(
    state: State<'_, Sharing>,
    note_id: String,
    frame: Vec<u8>,
) -> Result<(), String> {
    let envelope = encode_envelope(&note_id, &frame);
    let peers: Vec<PeerId> = state.peers.lock().unwrap().iter().cloned().collect();
    for peer in peers {
        let _ = state.transport.send(&peer, &envelope);
    }
    Ok(())
}

/// Accept a shared note by id: create a local row with that exact id - so both
/// sides address the same Yjs document - if one does not already exist, marked
/// shared. Returns the sticky to open. Idempotent.
#[tauri::command]
pub fn accept_shared_sticky(repo: State<'_, SqliteRepo>, id: String) -> Result<Sticky, String> {
    if let Some(existing) = repo.get(&id).map_err(|e| e.to_string())? {
        return Ok(existing);
    }
    let now = now_millis();
    let sticky = Sticky {
        id,
        doc_id: Uuid::new_v4().to_string(),
        content: "{}".into(),
        color: "#c8e6c9".into(), // a received note gets a distinct default tint
        desktop_id: String::new(),
        position_x: 150.0,
        position_y: 150.0,
        width: 250.0,
        height: 200.0,
        pinned: 0,
        is_open: 1,
        sharing_tier: 1,
        share_key: String::new(),
        created_at: now,
        updated_at: now,
    };
    repo.create(&sticky).map_err(|e| e.to_string())?;
    Ok(sticky)
}

/// A transport that is present but does nothing, so the sharing commands degrade
/// to clear errors (never a panic on unmanaged state) when the sidecar could not
/// start - e.g. the binary is missing, or the machine is offline.
pub struct NoopTransport;

impl Transport for NoopTransport {
    fn endpoint_id(&self) -> String {
        String::new()
    }
    fn dial(&self, _peer: &PeerId) -> Result<(), TransportError> {
        Err(TransportError::Unavailable("sharing is offline".into()))
    }
    fn send(&self, _peer: &PeerId, _frame: &[u8]) -> Result<(), TransportError> {
        Err(TransportError::Unavailable("sharing is offline".into()))
    }
}
