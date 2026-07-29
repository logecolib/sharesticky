//! Deciding what to do with the database file at startup, and doing it.
//!
//! Three real cases, chosen by (does a db file exist?) × (did a key already
//! exist before this run?):
//!
//! - no file: create a fresh encrypted db.
//! - file present, no prior key: the old plaintext db from the tauri-plugin-sql
//!   era — import it.
//! - file present, key already there: already encrypted, just open it.
//!
//! The decision is a pure function (tested exhaustively); the keyed-open and the
//! plaintext import are thin adapters around SQLCipher.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::database::{apply_migrations, SCHEMA_VERSION};
use super::vault::{pragma_passphrase, KEY_LEN};

#[derive(Debug)]
pub enum MigrateError {
    Db(String),
    Io(String),
}

impl std::fmt::Display for MigrateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(e) => write!(f, "database error: {e}"),
            Self::Io(e) => write!(f, "filesystem error: {e}"),
        }
    }
}
impl std::error::Error for MigrateError {}
impl From<rusqlite::Error> for MigrateError {
    fn from(e: rusqlite::Error) -> Self {
        MigrateError::Db(e.to_string())
    }
}
impl From<std::io::Error> for MigrateError {
    fn from(e: std::io::Error) -> Self {
        MigrateError::Io(e.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupAction {
    /// No database yet — create one, encrypted.
    CreateFresh,
    /// A plaintext database exists but we have no key — convert it.
    ImportPlaintext,
    /// Encrypted database and key both present — open it.
    OpenExisting,
}

/// Pure decision. `key_existed` must reflect whether a key was in the keystore
/// *before* this startup generated one.
pub fn decide(db_exists: bool, key_existed: bool) -> StartupAction {
    match (db_exists, key_existed) {
        (false, _) => StartupAction::CreateFresh,
        (true, false) => StartupAction::ImportPlaintext,
        (true, true) => StartupAction::OpenExisting,
    }
}

/// Open a keyed connection and bring its schema up to date.
pub fn open_keyed(path: &Path, key: &[u8; KEY_LEN]) -> Result<Connection, MigrateError> {
    let conn = Connection::open(path)?;
    // PRAGMA key must be the first thing on the connection.
    let pass = pragma_passphrase(key);
    conn.pragma_update(None, "key", pass.as_str())?;
    apply_migrations(&conn)?;
    Ok(conn)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "sharesticky.db".into());
    path.with_file_name(format!("{name}.{suffix}"))
}

/// Convert an existing plaintext database at `path` into an encrypted one, in
/// place. The original is kept as a `.plaintext.bak` sidecar rather than
/// deleted — losing a user's notes to a botched migration is unacceptable.
pub fn import_plaintext(path: &Path, key: &[u8; KEY_LEN]) -> Result<(), MigrateError> {
    let enc_path = sibling(path, "enc");
    let _ = fs::remove_file(&enc_path); // clear any stale attempt
    let pass = pragma_passphrase(key);

    {
        // Opened without a key: SQLCipher reads the plaintext db as-is.
        let plain = Connection::open(path)?;
        plain.execute(
            "ATTACH DATABASE ? AS enc KEY ?",
            rusqlite::params![enc_path.to_string_lossy(), pass.as_str()],
        )?;
        plain.query_row("SELECT sqlcipher_export('enc')", [], |_| Ok(()))?;
        // The source already has both migrations applied (old plugin), so stamp
        // the copy to the current schema version; apply_migrations then no-ops.
        plain.pragma_update(Some(rusqlite::DatabaseName::Attached("enc")), "user_version", SCHEMA_VERSION)?;
        plain.execute("DETACH DATABASE enc", [])?;
    } // plain dropped here, so the files are closed before we move them

    let backup = sibling(path, "plaintext.bak");
    let _ = fs::remove_file(&backup);
    fs::rename(path, &backup)?;
    fs::rename(&enc_path, path)?;
    Ok(())
}

/// Orchestrate startup: decide, perform, and return the live keyed connection.
pub fn run(path: &Path, key: &[u8; KEY_LEN], key_existed: bool) -> Result<Connection, MigrateError> {
    match decide(path.exists(), key_existed) {
        StartupAction::CreateFresh | StartupAction::OpenExisting => {}
        StartupAction::ImportPlaintext => import_plaintext(path, key)?,
    }
    open_keyed(path, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod decide {
        use super::*;

        #[test]
        fn no_file_is_always_create_fresh() {
            assert_eq!(decide(false, false), StartupAction::CreateFresh);
            assert_eq!(decide(false, true), StartupAction::CreateFresh);
        }

        #[test]
        fn file_without_a_prior_key_is_a_plaintext_import() {
            assert_eq!(decide(true, false), StartupAction::ImportPlaintext);
        }

        #[test]
        fn file_with_a_prior_key_just_opens() {
            assert_eq!(decide(true, true), StartupAction::OpenExisting);
        }
    }

    fn temp_db_path() -> PathBuf {
        use rand::Rng;
        let n: u64 = rand::thread_rng().gen();
        std::env::temp_dir().join(format!("sharesticky-test-{n}.db"))
    }

    fn key() -> [u8; KEY_LEN] {
        [7u8; KEY_LEN]
    }

    #[test]
    fn fresh_open_creates_a_keyed_db_with_schema() {
        let path = temp_db_path();
        let conn = open_keyed(&path, &key()).unwrap();
        // Table exists (migrations ran) and cipher is active.
        conn.execute("INSERT INTO stickies (id) VALUES ('a')", []).unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM stickies", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        drop(conn);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn imports_a_plaintext_db_and_the_result_needs_the_key() {
        let path = temp_db_path();

        // Build a plaintext db (no key) that looks like the old plugin's: v1+v2
        // schema, one row, user_version stamped by the plugin era = 0.
        {
            let plain = Connection::open(&path).unwrap();
            plain.execute_batch(super::super::database::MIGRATION_V1).unwrap();
            plain.execute_batch(super::super::database::MIGRATION_V2).unwrap();
            plain.execute("INSERT INTO stickies (id, content) VALUES ('a', 'secret')", []).unwrap();
        }

        import_plaintext(&path, &key()).unwrap();

        // The migrated file is unreadable without the key...
        {
            let no_key = Connection::open(&path).unwrap();
            assert!(
                no_key.query_row("SELECT count(*) FROM stickies", [], |r| r.get::<_, i64>(0)).is_err(),
                "encrypted db should not be readable without the key"
            );
        }
        // ...and the data survives with it.
        {
            let keyed = open_keyed(&path, &key()).unwrap();
            let content: String = keyed
                .query_row("SELECT content FROM stickies WHERE id='a'", [], |r| r.get(0))
                .unwrap();
            assert_eq!(content, "secret");
            let ver: i64 = keyed.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
            assert_eq!(ver, SCHEMA_VERSION);
        }

        // The plaintext backup was kept, not destroyed.
        assert!(sibling(&path, "plaintext.bak").exists());

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(sibling(&path, "plaintext.bak"));
    }
}
