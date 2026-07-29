//! Where the database key lives.
//!
//! Phase 1: a random 32-byte data key held in the OS keychain, so the app
//! unlocks silently and the encrypted file is useless if copied to another
//! machine or user. Phase 2 will wrap this key with an Argon2id password so it
//! can optionally be gated at boot — deferred here.
//!
//! Port-and-fake: a `KeyStore` trait (the keychain) with a fake, so the
//! "generate on first run, reuse after" logic is tested without touching the
//! real credential store.

use rand::RngCore;
use zeroize::Zeroizing;

pub const KEY_LEN: usize = 32;

#[derive(Debug)]
pub enum VaultError {
    Keychain(String),
    Corrupt(String),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Keychain(e) => write!(f, "keychain error: {e}"),
            Self::Corrupt(e) => write!(f, "stored key is unusable: {e}"),
        }
    }
}

impl std::error::Error for VaultError {}

/// A place that can hold one secret (the data key), keyed by nothing — there is
/// exactly one per app install.
pub trait KeyStore: Send + Sync {
    fn get(&self) -> Result<Option<Vec<u8>>, VaultError>;
    fn set(&self, secret: &[u8]) -> Result<(), VaultError>;
}

/// Return the data key, generating and persisting one on first run.
pub fn ensure_data_key(store: &dyn KeyStore) -> Result<Zeroizing<[u8; KEY_LEN]>, VaultError> {
    if let Some(existing) = store.get()? {
        if existing.len() != KEY_LEN {
            return Err(VaultError::Corrupt(format!(
                "expected {KEY_LEN} bytes, found {}",
                existing.len()
            )));
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&existing);
        return Ok(Zeroizing::new(key));
    }

    let mut key = [0u8; KEY_LEN];
    rand::thread_rng().fill_bytes(&mut key);
    store.set(&key)?;
    Ok(Zeroizing::new(key))
}

/// The key as a hex passphrase for SQLCipher's `PRAGMA key`.
///
/// Passed as a bound parameter (not a raw `x'..'` literal), SQLCipher treats it
/// as a passphrase and runs its own KDF over it — which is fine given the input
/// is already 32 bytes of entropy. Every place that keys a connection (open and
/// import) must use this exact form, or the database won't reopen.
pub fn pragma_passphrase(key: &[u8; KEY_LEN]) -> Zeroizing<String> {
    Zeroizing::new(hex::encode(key))
}

/// The real keychain adapter. Stores the key hex-encoded via the OS credential
/// store (Windows Credential Manager).
pub struct KeychainStore {
    service: String,
    user: String,
}

impl KeychainStore {
    pub fn new(service: &str, user: &str) -> Self {
        Self { service: service.into(), user: user.into() }
    }

    fn entry(&self) -> Result<keyring::Entry, VaultError> {
        keyring::Entry::new(&self.service, &self.user).map_err(|e| VaultError::Keychain(e.to_string()))
    }
}

impl KeyStore for KeychainStore {
    fn get(&self) -> Result<Option<Vec<u8>>, VaultError> {
        match self.entry()?.get_password() {
            Ok(hexed) => hex::decode(hexed.trim())
                .map(Some)
                .map_err(|e| VaultError::Corrupt(e.to_string())),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(VaultError::Keychain(e.to_string())),
        }
    }

    fn set(&self, secret: &[u8]) -> Result<(), VaultError> {
        self.entry()?
            .set_password(&hex::encode(secret))
            .map_err(|e| VaultError::Keychain(e.to_string()))
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::sync::Mutex;

    /// In-memory KeyStore for tests.
    #[derive(Default)]
    pub struct FakeKeyStore {
        secret: Mutex<Option<Vec<u8>>>,
        /// When set, every call fails, to exercise the error path.
        broken: Mutex<bool>,
    }

    impl FakeKeyStore {
        pub fn broken() -> Self {
            let s = Self::default();
            *s.broken.lock().unwrap() = true;
            s
        }
        /// Preload a specific stored value (e.g. a corrupt one).
        pub fn with_value(bytes: Vec<u8>) -> Self {
            let s = Self::default();
            *s.secret.lock().unwrap() = Some(bytes);
            s
        }
    }

    impl KeyStore for FakeKeyStore {
        fn get(&self) -> Result<Option<Vec<u8>>, VaultError> {
            if *self.broken.lock().unwrap() {
                return Err(VaultError::Keychain("fake is broken".into()));
            }
            Ok(self.secret.lock().unwrap().clone())
        }
        fn set(&self, secret: &[u8]) -> Result<(), VaultError> {
            if *self.broken.lock().unwrap() {
                return Err(VaultError::Keychain("fake is broken".into()));
            }
            *self.secret.lock().unwrap() = Some(secret.to_vec());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeKeyStore;
    use super::*;

    #[test]
    fn generates_a_key_on_first_run_and_persists_it() {
        let store = FakeKeyStore::default();
        let first = ensure_data_key(&store).unwrap();
        // Stored, and identical on the next call.
        let second = ensure_data_key(&store).unwrap();
        assert_eq!(*first, *second);
    }

    #[test]
    fn a_generated_key_is_not_all_zeros() {
        let store = FakeKeyStore::default();
        let key = ensure_data_key(&store).unwrap();
        assert_ne!(*key, [0u8; KEY_LEN]);
    }

    #[test]
    fn reports_a_corrupt_stored_key_rather_than_using_it() {
        let store = FakeKeyStore::with_value(vec![1, 2, 3]); // wrong length
        assert!(matches!(ensure_data_key(&store), Err(VaultError::Corrupt(_))));
    }

    #[test]
    fn surfaces_a_broken_keychain() {
        let store = FakeKeyStore::broken();
        assert!(matches!(ensure_data_key(&store), Err(VaultError::Keychain(_))));
    }

    #[test]
    fn pragma_passphrase_is_hex_of_the_key() {
        let key = [0xABu8; KEY_LEN];
        let pass = pragma_passphrase(&key);
        assert_eq!(pass.len(), KEY_LEN * 2);
        assert!(pass.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
