/// SQL migration v1: Create the stickies table.
///
/// This migration is applied automatically by tauri-plugin-sql
/// when the database is first opened.
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
