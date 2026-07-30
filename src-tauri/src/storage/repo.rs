//! The sticky repository — the only place that reads or writes the `stickies`
//! table. Mirrors the six operations the frontend bridge used to issue as raw
//! SQL, now owned by Rust so the database can be SQLCipher-encrypted.
//!
//! Follows the platform-layer port-and-fake shape: a `StickyRepo` trait, a
//! `rusqlite` adapter, behaviour-named tests. The tests run against a real keyed
//! in-memory connection, which also proves SQLCipher is linked and keying.

use std::sync::Mutex;

use rusqlite::{Connection, ToSql};
use serde::{Deserialize, Serialize};

/// A note row, exactly the columns the frontend `Sticky` interface expects.
///
/// `yjs_state` is intentionally omitted — it is reserved for the future CRDT
/// work and never surfaced to the frontend, so we select explicit columns
/// rather than `SELECT *`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sticky {
    pub id: String,
    pub doc_id: String,
    pub content: String,
    pub color: String,
    pub desktop_id: String,
    pub position_x: f64,
    pub position_y: f64,
    pub width: f64,
    pub height: f64,
    pub pinned: i64,
    pub is_open: i64,
    pub sharing_tier: i64,
    pub share_key: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A partial update. Every field is optional; only the `Some` ones are written.
/// The struct fields *are* the column whitelist — a JSON patch from the webview
/// can never name an arbitrary column.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct StickyPatch {
    pub content: Option<String>,
    pub color: Option<String>,
    pub desktop_id: Option<String>,
    pub position_x: Option<f64>,
    pub position_y: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub pinned: Option<i64>,
    pub is_open: Option<i64>,
    pub sharing_tier: Option<i64>,
    pub share_key: Option<String>,
    pub doc_id: Option<String>,
}

#[derive(Debug)]
pub enum RepoError {
    Db(String),
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for RepoError {}

impl From<rusqlite::Error> for RepoError {
    fn from(e: rusqlite::Error) -> Self {
        RepoError::Db(e.to_string())
    }
}

const COLUMNS: &str = "id, doc_id, content, color, desktop_id, position_x, \
    position_y, width, height, pinned, is_open, sharing_tier, share_key, \
    created_at, updated_at";

/// Storage of sticky notes.
pub trait StickyRepo: Send + Sync {
    fn list(&self) -> Result<Vec<Sticky>, RepoError>;
    fn create(&self, sticky: &Sticky) -> Result<(), RepoError>;
    fn update(&self, id: &str, patch: &StickyPatch, stamp_updated_at: bool) -> Result<(), RepoError>;
    fn delete(&self, id: &str) -> Result<(), RepoError>;

    /// The note's Yjs document bytes (`yjs_state`), or `None` if it has none yet
    /// (a note that predates the CRDT migration, or was never edited).
    fn get_doc(&self, id: &str) -> Result<Option<Vec<u8>>, RepoError>;

    /// Persist the Yjs document and its derived `content` projection together,
    /// stamping `updated_at` (so the manager re-sorts). One write, kept atomic.
    fn save_doc(&self, id: &str, bytes: &[u8], content: &str) -> Result<(), RepoError>;
}

/// Build the `SET` clause and params for an update from the `Some` fields of a
/// patch. Returns `None` when the patch is empty (nothing to write). Kept
/// separate so the column selection can be unit tested without a database.
/// Returns `None` for an empty patch. The returned params cover only the patch
/// fields; the caller appends `now` (when `stamp_updated_at`) and the id, since
/// those are owned by the caller's scope — which keeps the borrows valid without
/// leaking. When stamping, the clause ends with `, updated_at = ?`, so the
/// caller must push `now` before the id.
fn build_update(
    patch: &StickyPatch,
    stamp_updated_at: bool,
) -> Option<(String, Vec<&dyn ToSql>)> {
    let mut cols: Vec<&str> = Vec::new();
    let mut params: Vec<&dyn ToSql> = Vec::new();

    macro_rules! maybe {
        ($field:ident, $col:literal) => {
            if let Some(v) = &patch.$field {
                cols.push(concat!($col, " = ?"));
                params.push(v as &dyn ToSql);
            }
        };
    }
    maybe!(content, "content");
    maybe!(color, "color");
    maybe!(desktop_id, "desktop_id");
    maybe!(position_x, "position_x");
    maybe!(position_y, "position_y");
    maybe!(width, "width");
    maybe!(height, "height");
    maybe!(pinned, "pinned");
    maybe!(is_open, "is_open");
    maybe!(sharing_tier, "sharing_tier");
    maybe!(share_key, "share_key");
    maybe!(doc_id, "doc_id");

    if cols.is_empty() {
        return None;
    }
    // The manager sorts by updated_at, so window-state writes deliberately do
    // not stamp it (see the frontend store's updateWindowState).
    let mut clause = cols.join(", ");
    if stamp_updated_at {
        clause.push_str(", updated_at = ?");
    }
    Some((clause, params))
}

fn row_to_sticky(row: &rusqlite::Row<'_>) -> rusqlite::Result<Sticky> {
    Ok(Sticky {
        id: row.get(0)?,
        doc_id: row.get(1)?,
        content: row.get(2)?,
        color: row.get(3)?,
        desktop_id: row.get(4)?,
        position_x: row.get(5)?,
        position_y: row.get(6)?,
        width: row.get(7)?,
        height: row.get(8)?,
        pinned: row.get(9)?,
        is_open: row.get(10)?,
        sharing_tier: row.get(11)?,
        share_key: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

/// The real adapter over a keyed SQLCipher connection.
pub struct SqliteRepo {
    conn: Mutex<Connection>,
}

impl SqliteRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn: Mutex::new(conn) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("sticky connection mutex poisoned")
    }
}

impl StickyRepo for SqliteRepo {
    fn list(&self) -> Result<Vec<Sticky>, RepoError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM stickies ORDER BY updated_at DESC"
        ))?;
        let rows = stmt.query_map([], row_to_sticky)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    fn create(&self, s: &Sticky) -> Result<(), RepoError> {
        let conn = self.lock();
        conn.execute(
            &format!(
                "INSERT INTO stickies ({COLUMNS}) VALUES \
                 (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
            ),
            rusqlite::params![
                s.id, s.doc_id, s.content, s.color, s.desktop_id, s.position_x,
                s.position_y, s.width, s.height, s.pinned, s.is_open,
                s.sharing_tier, s.share_key, s.created_at, s.updated_at
            ],
        )?;
        Ok(())
    }

    fn update(&self, id: &str, patch: &StickyPatch, stamp_updated_at: bool) -> Result<(), RepoError> {
        let Some((set, mut params)) = build_update(patch, stamp_updated_at) else {
            return Ok(()); // nothing to write
        };
        // now and id are owned here so the borrows in `params` stay valid.
        let now = crate::storage::now_millis();
        if stamp_updated_at {
            params.push(&now);
        }
        params.push(&id);
        let conn = self.lock();
        conn.execute(&format!("UPDATE stickies SET {set} WHERE id = ?"), &params[..])?;
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<(), RepoError> {
        let conn = self.lock();
        conn.execute("DELETE FROM stickies WHERE id = ?", [id])?;
        Ok(())
    }

    fn get_doc(&self, id: &str) -> Result<Option<Vec<u8>>, RepoError> {
        let conn = self.lock();
        let row = conn.query_row(
            "SELECT yjs_state FROM stickies WHERE id = ?",
            [id],
            |r| r.get::<_, Option<Vec<u8>>>(0),
        );
        match row {
            Ok(bytes) => Ok(bytes),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn save_doc(&self, id: &str, bytes: &[u8], content: &str) -> Result<(), RepoError> {
        let now = crate::storage::now_millis();
        let conn = self.lock();
        conn.execute(
            "UPDATE stickies SET yjs_state = ?, content = ?, updated_at = ? WHERE id = ?",
            rusqlite::params![bytes, content, now, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sticky(id: &str) -> Sticky {
        Sticky {
            id: id.into(),
            doc_id: format!("doc-{id}"),
            content: "{}".into(),
            color: "#fff9c4".into(),
            desktop_id: String::new(),
            position_x: 10.0,
            position_y: 20.0,
            width: 250.0,
            height: 200.0,
            pinned: 0,
            is_open: 1,
            sharing_tier: 0,
            share_key: String::new(),
            created_at: 1000,
            updated_at: 1000,
        }
    }

    /// A keyed in-memory DB — also proves SQLCipher is linked and keying.
    fn keyed_repo() -> SqliteRepo {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "key", "test-key").unwrap();
        conn.execute_batch(crate::storage::database::MIGRATION_V1).unwrap();
        conn.execute_batch(crate::storage::database::MIGRATION_V2).unwrap();
        SqliteRepo::new(conn)
    }

    #[test]
    fn sqlcipher_is_actually_linked() {
        let conn = Connection::open_in_memory().unwrap();
        let ver: Option<String> = conn
            .query_row("PRAGMA cipher_version", [], |r| r.get(0))
            .unwrap_or(None);
        assert!(ver.is_some(), "expected SQLCipher, got vanilla SQLite");
    }

    #[test]
    fn creates_and_lists_a_sticky() {
        let repo = keyed_repo();
        repo.create(&sticky("a")).unwrap();
        let all = repo.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "a");
    }

    #[test]
    fn lists_newest_first() {
        let repo = keyed_repo();
        let mut older = sticky("old");
        older.updated_at = 100;
        let mut newer = sticky("new");
        newer.updated_at = 200;
        repo.create(&older).unwrap();
        repo.create(&newer).unwrap();
        let ids: Vec<String> = repo.list().unwrap().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["new", "old"]);
    }

    #[test]
    fn update_stamps_updated_at() {
        let repo = keyed_repo();
        repo.create(&sticky("a")).unwrap();
        let patch = StickyPatch { content: Some("edited".into()), ..Default::default() };
        repo.update("a", &patch, true).unwrap();
        let got = &repo.list().unwrap()[0];
        assert_eq!(got.content, "edited");
        assert!(got.updated_at > 1000, "updated_at should advance");
    }

    #[test]
    fn window_state_update_leaves_updated_at_alone() {
        let repo = keyed_repo();
        repo.create(&sticky("a")).unwrap();
        let patch = StickyPatch { position_x: Some(999.0), ..Default::default() };
        repo.update("a", &patch, false).unwrap();
        let got = &repo.list().unwrap()[0];
        assert_eq!(got.position_x, 999.0);
        assert_eq!(got.updated_at, 1000, "window state must not reorder the list");
    }

    #[test]
    fn deletes_only_the_named_sticky() {
        let repo = keyed_repo();
        repo.create(&sticky("a")).unwrap();
        repo.create(&sticky("b")).unwrap();
        repo.delete("a").unwrap();
        let ids: Vec<String> = repo.list().unwrap().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["b"]);
    }

    #[test]
    fn save_doc_round_trips_bytes_and_projection() {
        let repo = keyed_repo();
        repo.create(&sticky("a")).unwrap();

        let bytes = vec![1u8, 2, 3, 4, 5];
        repo.save_doc("a", &bytes, "{\"type\":\"doc\"}").unwrap();

        assert_eq!(repo.get_doc("a").unwrap(), Some(bytes));
        let got = &repo.list().unwrap()[0];
        assert_eq!(got.content, "{\"type\":\"doc\"}");
        assert!(got.updated_at > 1000, "save_doc should stamp updated_at");
    }

    #[test]
    fn get_doc_is_none_for_a_note_without_one() {
        let repo = keyed_repo();
        repo.create(&sticky("a")).unwrap(); // yjs_state left NULL
        assert_eq!(repo.get_doc("a").unwrap(), None);
    }

    #[test]
    fn get_doc_is_none_for_a_missing_sticky() {
        let repo = keyed_repo();
        assert_eq!(repo.get_doc("ghost").unwrap(), None);
    }

    #[test]
    fn build_update_selects_only_present_fields() {
        let patch = StickyPatch { color: Some("#000".into()), pinned: Some(1), ..Default::default() };
        let (set, params) = build_update(&patch, false).unwrap();
        assert_eq!(set, "color = ?, pinned = ?");
        assert_eq!(params.len(), 2); // patch fields only; id appended by caller
    }

    #[test]
    fn build_update_appends_updated_at_clause_when_stamping() {
        let patch = StickyPatch { color: Some("#000".into()), ..Default::default() };
        let (set, params) = build_update(&patch, true).unwrap();
        assert_eq!(set, "color = ?, updated_at = ?");
        assert_eq!(params.len(), 1); // now is pushed by the caller, not here
    }

    #[test]
    fn build_update_is_none_for_an_empty_patch() {
        assert!(build_update(&StickyPatch::default(), true).is_none());
    }
}
