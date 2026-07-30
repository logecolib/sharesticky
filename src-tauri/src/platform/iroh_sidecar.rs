//! App-side adapter for the `net-sidecar` process - the real implementation of
//! the [`Transport`](super::transport::Transport) port.
//!
//! iroh cannot be linked into this binary on Windows (its wmi dependency clashes
//! with tauri's windows crate; see memory: project_iroh_sidecar), so all QUIC
//! networking lives in a separate process. This adapter spawns it, feeds it the
//! identity seed, and translates the port's calls into the sidecar's NDJSON
//! protocol - dial/send out over stdin, inbound frames in over stdout to the
//! [`InboundSink`]. No decisions live here; it is pure plumbing.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use base64::prelude::*;
use serde::{Deserialize, Serialize};

use super::transport::{InboundSink, PeerId, Transport, TransportError};

/// How long to wait for the sidecar to bind its endpoint and report `ready`.
const READY_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Serialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum SideCommand {
    Init { seed: String },
    Dial { peer: String },
    Send { peer: String, frame: String },
}

#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum SideEvent {
    Ready { id: String },
    Frame { peer: String, frame: String },
    Log { msg: String },
    Error { msg: String },
}

pub struct IrohSidecarTransport {
    stdin: Mutex<ChildStdin>,
    my_id: String,
    /// Kept alive for the transport's lifetime; dropping it kills the sidecar.
    _child: Child,
}

impl IrohSidecarTransport {
    /// Spawn the sidecar at `exe`, hand it `seed`, and block until it is ready
    /// (or times out). Inbound frames are delivered to `sink` on a reader thread.
    pub fn spawn(exe: &Path, seed: &[u8; 32], sink: InboundSink) -> Result<Self, TransportError> {
        let mut child = Command::new(exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| TransportError::Unavailable(format!("spawning sidecar: {e}")))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| TransportError::Unavailable("sidecar stdin missing".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| TransportError::Unavailable("sidecar stdout missing".into()))?;

        // Ask it to bind under our identity.
        write_line(&mut stdin, &SideCommand::Init { seed: hex::encode(seed) })?;

        // Reader thread: signal readiness once, then pump inbound frames forever.
        let (ready_tx, ready_rx) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            let mut ready_tx = Some(ready_tx);
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                match serde_json::from_str::<SideEvent>(&line) {
                    Ok(SideEvent::Ready { id }) => {
                        if let Some(tx) = ready_tx.take() {
                            let _ = tx.send(id);
                        }
                    }
                    Ok(SideEvent::Frame { peer, frame }) => {
                        if let Ok(bytes) = BASE64_STANDARD.decode(frame.as_bytes()) {
                            sink((PeerId(peer), bytes));
                        }
                    }
                    Ok(SideEvent::Log { msg }) => log::info!("[net] {msg}"),
                    Ok(SideEvent::Error { msg }) => log::warn!("[net] {msg}"),
                    Err(e) => log::warn!("[net] unparseable line: {e}"),
                }
            }
        });

        let my_id = ready_rx
            .recv_timeout(READY_TIMEOUT)
            .map_err(|_| TransportError::Unavailable("sidecar did not become ready".into()))?;

        Ok(Self {
            stdin: Mutex::new(stdin),
            my_id,
            _child: child,
        })
    }

    fn send_command(&self, cmd: &SideCommand) -> Result<(), TransportError> {
        let mut stdin = self.stdin.lock().unwrap();
        write_line(&mut stdin, cmd)
    }
}

fn write_line(stdin: &mut ChildStdin, cmd: &SideCommand) -> Result<(), TransportError> {
    let mut line =
        serde_json::to_string(cmd).map_err(|e| TransportError::Unavailable(e.to_string()))?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .and_then(|_| stdin.flush())
        .map_err(|e| TransportError::NotConnected(e.to_string()))
}

impl Transport for IrohSidecarTransport {
    fn endpoint_id(&self) -> String {
        self.my_id.clone()
    }

    fn dial(&self, peer: &PeerId) -> Result<(), TransportError> {
        self.send_command(&SideCommand::Dial {
            peer: peer.0.clone(),
        })
    }

    fn send(&self, peer: &PeerId, frame: &[u8]) -> Result<(), TransportError> {
        self.send_command(&SideCommand::Send {
            peer: peer.0.clone(),
            frame: BASE64_STANDARD.encode(frame),
        })
    }
}
