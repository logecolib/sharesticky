// Phase 4: AES-256-GCM encryption for shared sticky content
//
// Planned:
// - generate_key() -> [u8; 32]
// - encrypt(plaintext, key) -> (ciphertext, nonce)
// - decrypt(ciphertext, nonce, key) -> plaintext
// - derive_share_key(sticky_id, master_key) -> share-specific key
//
// Will use the `aes-gcm` crate for authenticated encryption.
// Each shared sticky gets its own derived key so revoking a share
// only invalidates that sticky's key.
