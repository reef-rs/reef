//! Connection management.
//!
//! `Db` wraps an embedded libSQL `Database`. Cheap to clone (it's a thin
//! handle); call `.conn()` to check out a connection for a single query.
//!
//! Migrations are NOT run by this module. Apply them via the framework CLI:
//!
//! ```bash
//! cargo reef migrate run
//! ```
//!
//! The runner lives in `cargo-reef`, not in your project. This means:
//! - Your project never has to maintain migration runner code
//! - Migration bug-fixes ship via cargo-reef updates, not your codebase
//! - Migrations are an explicit operational step (like `rails db:migrate`)
//!
//! For dev convenience until `cargo reef migrate run` ships, you can apply
//! the SQL files directly:
//!
//! ```bash
//! for f in migrations/*.sql; do sqlite3 ./data/reef.db < "$f"; done
//! ```

pub mod schema;

use std::sync::Arc;

use anyhow::Result;
use libsql::{Builder, Connection, Database};
use tokio::sync::OnceCell;

#[derive(Clone)]
pub struct Db {
    inner: Arc<Database>,
}

impl Db {
    /// Open the embedded libSQL database at `path`. Creates parent dirs as needed.
    pub async fn new(path: &str) -> Result<Self> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        let inner = Arc::new(Builder::new_local(path).build().await?);
        Ok(Self { inner })
    }

    /// Open the database from `DATABASE_URL`, defaulting to `./data/reef.db`.
    pub async fn from_env() -> Result<Self> {
        let path = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "./data/reef.db".to_string());
        Self::new(&path).await
    }

    /// Check out a connection for a single query / transaction.
    pub fn conn(&self) -> Result<Connection> {
        Ok(self.inner.connect()?)
    }
}

// ---- Default global Db ----
//
// Lazy async-init singleton. This is the canonical "Option 2" pattern from
// Dioxus's [examples/07-fullstack/server_state.rs] — equivalent in shape to
// `dioxus::fullstack::Lazy<T>` but using std/tokio primitives directly so we
// don't pull in a Dioxus-specific wrapper for one global.
//
// Tests should construct their own `Db::new(":memory:")` instead of relying
// on this — that's the whole point of having `Db` be an explicit struct.
//
// If you want explicit per-fn dependency declarations (the "Option 4" pattern),
// declare an `AppState` struct, register it via `axum::Extension` in the
// dioxus::serve closure, and pull it into server fns with the macro's `state:`
// parameter:
//
//   #[post("/api/foo", db: State<Db>)]
//   pub async fn foo() -> Result<()> { db.conn()?... }

static DEFAULT_DB: OnceCell<Db> = OnceCell::const_new();

/// Lazily-initialized default `Db` (uses `DATABASE_URL` or `./data/reef.db`).
pub async fn default_db() -> Result<&'static Db> {
    DEFAULT_DB
        .get_or_try_init(|| async { Db::from_env().await })
        .await
}
