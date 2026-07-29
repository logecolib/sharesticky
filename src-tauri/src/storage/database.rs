/// SQL migration v1: Create the stickies table.
///
/// Applied by `apply_migrations` (below) when the database is opened.
pub const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS stickies (
  id TEXT PRIMARY KEY,
  doc_id TEXT UNIQUE,
  yjs_state BLOB,
  content TEXT DEFAULT '{}',
  color TEXT DEFAULT '#fff9c4',
  desktop_id TEXT DEFAULT '',
  position_x REAL DEFAULT 100.0,
  position_y REAL DEFAULT 100.0,
  width REAL DEFAULT 250.0,
  height REAL DEFAULT 200.0,
  pinned INTEGER DEFAULT 0,
  sharing_tier INTEGER DEFAULT 0,
  share_key TEXT DEFAULT '',
  created_at INTEGER,
  updated_at INTEGER
);
"#;

/// SQL migration v2: remember which stickies were on screen.
///
/// Without this, every restart loses the user's working set - the notes they
/// had open are still in the table, but nothing records that they were
/// *showing*, so the app comes back empty.
pub const MIGRATION_V2: &str = r#"
ALTER TABLE stickies ADD COLUMN is_open INTEGER DEFAULT 0;
"#;

/// Latest schema version. `apply_migrations` brings a connection up to this.
pub const SCHEMA_VERSION: i64 = 2;

/// Run any migrations the connection is behind on, tracked via
/// `PRAGMA user_version`. Idempotent: safe to call on every startup.
///
/// Note the ALTER in v2 is not idempotent on its own, which is why gating on
/// `user_version` matters — a DB imported from the old `tauri-plugin-sql` setup
/// already has both migrations applied and is stamped to `SCHEMA_VERSION` at
/// import time, so nothing re-runs here.
pub fn apply_migrations(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(MIGRATION_V1)?;
    }
    if version < 2 {
        conn.execute_batch(MIGRATION_V2)?;
    }
    if version < SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}
