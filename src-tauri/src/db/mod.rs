//! SQLite persistence: connection, migrations and repositories.
//!
//! Repositories are plain functions over `&Connection` so they can be unit
//! tested against an in-memory database and reused by services later.

pub mod activity;
pub mod migrations;
pub mod missions;
pub mod presets;
pub mod sessions;
pub mod settings;
pub mod workspaces;

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> AppResult<Self> {
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
        migrations::run(&mut conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::Internal("database lock poisoned".into()))?;
        f(&conn)
    }
}

/// A standalone connection (WAL allows concurrent readers/writer) for long
/// operations that must not hold the shared `Db` lock (network calls, jobs).
pub fn connect(path: &Path) -> AppResult<Connection> {
    let c = Connection::open(path)?;
    c.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
    Ok(c)
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
