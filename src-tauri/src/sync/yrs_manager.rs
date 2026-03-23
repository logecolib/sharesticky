// Phase 3: Yrs (Yjs Rust port) document management
//
// Each sticky note will have an associated Yrs Doc for CRDT-based
// collaborative editing. This module will manage:
// - Creating new Yrs docs for stickies
// - Applying updates from local edits
// - Merging remote updates from peers
// - Persisting Yrs state to the yjs_state BLOB column
// - Encoding/decoding state vectors for sync
