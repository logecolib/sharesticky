pub mod database;
pub mod encryption;
pub mod migrate;
pub mod repo;
pub mod vault;

/// Milliseconds since the Unix epoch — the timestamp format the frontend uses
/// (matches JS `Date.now()`), stored in `created_at`/`updated_at`.
pub fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
