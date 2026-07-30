//! ShareSticky P2P transport sidecar.
//!
//! A standalone process (see crate-level note in Cargo.toml for why it is not
//! linked into the app) that owns all iroh/QUIC networking. It speaks
//! newline-delimited JSON over stdio:
//!
//! app -> sidecar (stdin):
//!   {"t":"init","seed":"<64 hex>"}      bind the endpoint under this identity
//!   {"t":"dial","peer":"<hex id>"}      begin connecting to a peer
//!   {"t":"send","peer":"<hex id>","frame":"<base64>"}   enqueue a frame
//!
//! sidecar -> app (stdout):
//!   {"t":"ready","id":"<hex id>"}       bound; here is our dialable id
//!   {"t":"frame","peer":"<hex id>","frame":"<base64>"}  inbound frame
//!   {"t":"log","msg":"..."}  /  {"t":"error","msg":"..."}
//!
//! Frames are opaque here; their meaning (which note) is an envelope the app
//! encodes/decodes. This process makes no product decisions - it is the IO
//! adapter below the app's `Transport` port.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use base64::prelude::*;
use iroh::{endpoint::presets, endpoint::Connection, Endpoint, EndpointId, SecretKey};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

/// Application protocol id. Bumped if the on-stream framing ever changes.
const ALPN: &[u8] = b"sharesticky/sync/0";

/// Reject an absurd length prefix rather than allocating on a corrupt/hostile
/// peer. Notes are tiny; 64 MiB is far above any real Yjs state.
const MAX_FRAME: usize = 64 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Command {
    Init { seed: String },
    Dial { peer: String },
    Send { peer: String, frame: String },
}

#[derive(Serialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Event {
    Ready { id: String },
    Frame { peer: String, frame: String },
    Log { msg: String },
    Error { msg: String },
}

/// The single serialized writer to stdout. Every task emits events through here.
type EventTx = mpsc::UnboundedSender<Event>;

struct Net {
    endpoint: Endpoint,
    events: EventTx,
    /// Per-peer channel into that session's writer task. Present iff connected.
    sessions: Mutex<HashMap<String, mpsc::UnboundedSender<Vec<u8>>>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // One writer task owns stdout so interleaved events never corrupt a line.
    let (events, mut event_rx) = mpsc::unbounded_channel::<Event>();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(ev) = event_rx.recv().await {
            if let Ok(mut line) = serde_json::to_string(&ev) {
                line.push('\n');
                if stdout.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                let _ = stdout.flush().await;
            }
        }
    });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut net: Option<Arc<Net>> = None;

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let cmd: Command = match serde_json::from_str(line) {
            Ok(c) => c,
            Err(e) => {
                let _ = events.send(Event::Error {
                    msg: format!("bad command: {e}"),
                });
                continue;
            }
        };

        match cmd {
            Command::Init { seed } => {
                if net.is_some() {
                    continue; // already bound; ignore a duplicate init
                }
                match bind(&seed, events.clone()).await {
                    Ok(bound) => {
                        let _ = events.send(Event::Ready {
                            id: bound.endpoint.id().to_string(),
                        });
                        spawn_accept_loop(bound.clone());
                        net = Some(bound);
                    }
                    Err(e) => {
                        let _ = events.send(Event::Error {
                            msg: format!("init failed: {e}"),
                        });
                    }
                }
            }
            Command::Dial { peer } => {
                if let Some(net) = &net {
                    net.clone().dial(peer);
                } else {
                    let _ = events.send(Event::Error {
                        msg: "dial before init".into(),
                    });
                }
            }
            Command::Send { peer, frame } => {
                if let Some(net) = &net {
                    match BASE64_STANDARD.decode(frame.as_bytes()) {
                        Ok(bytes) => net.send(&peer, bytes),
                        Err(e) => {
                            let _ = events.send(Event::Error {
                                msg: format!("bad base64 frame: {e}"),
                            });
                        }
                    }
                }
            }
        }
    }

    drop(events);
    let _ = writer.await;
    Ok(())
}

async fn bind(seed_hex: &str, events: EventTx) -> Result<Arc<Net>> {
    let seed = decode_seed(seed_hex).context("decoding identity seed")?;
    let secret = SecretKey::from_bytes(&seed);
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .context("binding endpoint")?;
    Ok(Arc::new(Net {
        endpoint,
        events,
        sessions: Mutex::new(HashMap::new()),
    }))
}

fn decode_seed(hex: &str) -> Result<[u8; 32]> {
    let mut out = [0u8; 32];
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<_, _>>()
        .context("seed is not valid hex")?;
    anyhow::ensure!(bytes.len() == 32, "seed must be 32 bytes, got {}", bytes.len());
    out.copy_from_slice(&bytes);
    Ok(out)
}

impl Net {
    fn dial(self: Arc<Self>, peer: String) {
        if self.sessions.lock().unwrap().contains_key(&peer) {
            return; // reuse a live session
        }
        let id: EndpointId = match peer.parse() {
            Ok(id) => id,
            Err(e) => {
                let _ = self.events.send(Event::Error {
                    msg: format!("bad peer {peer}: {e}"),
                });
                return;
            }
        };
        let net = self.clone();
        tokio::spawn(async move {
            match net.endpoint.connect(id, ALPN).await {
                Ok(conn) => run_session(net, peer, conn, true).await,
                Err(e) => {
                    let _ = net.events.send(Event::Log {
                        msg: format!("dial {peer} failed: {e}"),
                    });
                }
            }
        });
    }

    fn send(&self, peer: &str, frame: Vec<u8>) {
        let sessions = self.sessions.lock().unwrap();
        if let Some(tx) = sessions.get(peer) {
            let _ = tx.send(frame);
        } else {
            let _ = self.events.send(Event::Error {
                msg: format!("send to unconnected peer {peer}"),
            });
        }
    }
}

fn spawn_accept_loop(net: Arc<Net>) {
    let endpoint = net.endpoint.clone();
    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let net = net.clone();
            tokio::spawn(async move {
                match incoming.await {
                    Ok(conn) => {
                        let peer = conn.remote_id().to_string();
                        run_session(net, peer, conn, false).await;
                    }
                    Err(e) => {
                        let _ = net.events.send(Event::Log {
                            msg: format!("accept failed: {e}"),
                        });
                    }
                }
            });
        }
    });
}

/// Drive one peer connection: a single bidirectional stream carrying
/// length-prefixed frames both ways. The initiator opens the stream; the
/// accepter waits for it. Cleans up the session entry when the connection ends.
async fn run_session(net: Arc<Net>, peer: String, conn: Connection, initiator: bool) {
    let streams = if initiator {
        conn.open_bi().await
    } else {
        conn.accept_bi().await
    };
    let (mut send, mut recv) = match streams {
        Ok(pair) => pair,
        Err(e) => {
            let _ = net.events.send(Event::Log {
                msg: format!("stream with {peer} failed: {e}"),
            });
            return;
        }
    };

    // Writer task: drain the per-peer queue, framing each with a u32 length.
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    net.sessions.lock().unwrap().insert(peer.clone(), tx);
    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let len = (frame.len() as u32).to_be_bytes();
            if send.write_all(&len).await.is_err() || send.write_all(&frame).await.is_err() {
                break;
            }
        }
        let _ = send.finish();
    });

    // Reader loop: read a length prefix, then that many bytes, then emit.
    loop {
        let mut len_buf = [0u8; 4];
        if recv.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > MAX_FRAME {
            let _ = net.events.send(Event::Log {
                msg: format!("frame from {peer} too large ({len}); dropping"),
            });
            break;
        }
        let mut buf = vec![0u8; len];
        if recv.read_exact(&mut buf).await.is_err() {
            break;
        }
        let _ = net.events.send(Event::Frame {
            peer: peer.clone(),
            frame: BASE64_STANDARD.encode(&buf),
        });
    }

    net.sessions.lock().unwrap().remove(&peer);
    writer.abort();
}
