import * as Y from "yjs";
import * as syncProtocol from "y-protocols/sync";
import * as encoding from "lib0/encoding";
import * as decoding from "lib0/decoding";

/** Sends one framed sync message to the peer. The transport is opaque here. */
export type SendFrame = (frame: Uint8Array) => void;

/**
 * Drives the Yjs sync protocol for one document over one peer connection.
 *
 * Transport-agnostic by design: it only knows how to turn document state into
 * frames and apply frames back into the document. Whatever moves the bytes
 * between machines (an in-memory channel in tests, iroh in production) calls
 * {@link receive} for inbound frames and is handed outbound frames via the
 * `send` callback.
 *
 * Protocol (standard y-protocols): on {@link start} each side sends SyncStep1
 * (its state vector); the peer replies SyncStep2 (the updates the sender lacks).
 * Both sides doing this yields a full bidirectional catch-up. Thereafter every
 * local document change is broadcast as a live update.
 *
 * Applied remote updates carry this engine as their transaction origin, so the
 * update listener can tell them apart from local edits and never echoes a peer's
 * update back to it (which would ping-pong forever on a two-peer link).
 */
export class SyncEngine {
  private readonly doc: Y.Doc;
  private readonly send: SendFrame;
  private destroyed = false;
  private readonly onUpdate: (update: Uint8Array, origin: unknown) => void;

  constructor(doc: Y.Doc, send: SendFrame) {
    this.doc = doc;
    this.send = send;
    this.onUpdate = (update, origin) => {
      // Skip updates we just applied from a peer — they already have them.
      if (origin === this) return;
      const encoder = encoding.createEncoder();
      syncProtocol.writeUpdate(encoder, update);
      this.send(encoding.toUint8Array(encoder));
    };
    this.doc.on("update", this.onUpdate);
  }

  /** Begin the handshake: ask the peer for anything this doc is missing. */
  start(): void {
    if (this.destroyed) return;
    const encoder = encoding.createEncoder();
    syncProtocol.writeSyncStep1(encoder, this.doc);
    this.send(encoding.toUint8Array(encoder));
  }

  /** Apply one inbound frame, replying with a step-2 payload when required. */
  receive(frame: Uint8Array): void {
    if (this.destroyed) return;
    const decoder = decoding.createDecoder(frame);
    const encoder = encoding.createEncoder();
    // Applies the message to the doc with this engine as the origin; for a
    // SyncStep1 it also writes the matching SyncStep2 reply into `encoder`.
    syncProtocol.readSyncMessage(decoder, encoder, this.doc, this);
    if (encoding.length(encoder) > 0) {
      this.send(encoding.toUint8Array(encoder));
    }
  }

  /** Detach from the document; further edits are no longer broadcast. */
  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.doc.off("update", this.onUpdate);
  }
}
