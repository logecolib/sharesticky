//! Tauri commands over the sticky repository. These replace the raw SQL the
//! frontend bridge used to run through tauri-plugin-sql; the bridge is now a
//! thin `invoke()` layer with the same signatures.

use rand::Rng;
use tauri::State;
use uuid::Uuid;

use crate::storage::now_millis;
use crate::storage::repo::{Sticky, StickyPatch, StickyRepo, SqliteRepo};

#[tauri::command]
pub fn list_stickies(repo: State<'_, SqliteRepo>) -> Result<Vec<Sticky>, String> {
    repo.list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_sticky(repo: State<'_, SqliteRepo>, color: Option<String>) -> Result<Sticky, String> {
    let now = now_millis();
    let mut rng = rand::thread_rng();
    // A newly created sticky opens immediately, so is_open starts at 1.
    let sticky = Sticky {
        id: Uuid::new_v4().to_string(),
        doc_id: Uuid::new_v4().to_string(),
        content: "{}".into(),
        color: color.unwrap_or_else(|| "#fff9c4".into()),
        desktop_id: String::new(),
        position_x: 100.0 + rng.gen::<f64>() * 200.0,
        position_y: 100.0 + rng.gen::<f64>() * 200.0,
        width: 250.0,
        height: 200.0,
        pinned: 0,
        is_open: 1,
        sharing_tier: 0,
        share_key: String::new(),
        created_at: now,
        updated_at: now,
    };
    repo.create(&sticky).map_err(|e| e.to_string())?;
    Ok(sticky)
}

#[tauri::command]
pub fn update_sticky(
    repo: State<'_, SqliteRepo>,
    id: String,
    patch: StickyPatch,
) -> Result<(), String> {
    // A content/colour/desktop edit: stamps updated_at.
    repo.update(&id, &patch, true).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_sticky_window_state(
    repo: State<'_, SqliteRepo>,
    id: String,
    patch: StickyPatch,
) -> Result<(), String> {
    // Position/size/open state: must NOT stamp updated_at (the manager sorts by
    // it, so this would reorder the list on open/drag).
    repo.update(&id, &patch, false).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_sticky(repo: State<'_, SqliteRepo>, id: String) -> Result<(), String> {
    repo.delete(&id).map_err(|e| e.to_string())
}

/// The note's Yjs document bytes, or an empty array if it has none yet. Empty
/// means "seed me from `content`" to the frontend (the CRDT migration).
#[tauri::command]
pub fn get_sticky_doc(repo: State<'_, SqliteRepo>, id: String) -> Result<Vec<u8>, String> {
    repo.get_doc(&id)
        .map(|opt| opt.unwrap_or_default())
        .map_err(|e| e.to_string())
}

/// Persist a note's Yjs document plus its derived `content` projection (for the
/// manager preview), stamping `updated_at`.
#[tauri::command]
pub fn save_sticky_doc(
    repo: State<'_, SqliteRepo>,
    id: String,
    bytes: Vec<u8>,
    content: String,
) -> Result<(), String> {
    repo.save_doc(&id, &bytes, &content).map_err(|e| e.to_string())
}
