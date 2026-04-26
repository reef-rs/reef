//! Schema — single source of truth for the database shape.
//!
//! Each `#[reef::table]` struct declares a SQL table. The struct fields are
//! the columns; the Rust types map directly to SQL types (`String` → TEXT
//! NOT NULL, `Option<i64>` → INTEGER nullable, etc.). The same struct is
//! also the row type used by `queries::*` and `actions::*`.
//!
//! ## Workflow
//!
//! - **First-time setup**: `cargo reef migrate run` applies the bundled
//!   SQL bootstrap migration in `migrations/`. This creates the initial
//!   schema you see here.
//! - **Ongoing changes**: edit this file, then run `cargo reef db:push` to
//!   diff against the live DB and apply the changes (Drizzle-style).
//!   Use `cargo reef db:push --write <name>` to capture the diff as a
//!   migration file in `migrations/` instead of applying directly — useful
//!   for production deploys where you want migrations in version control.
//!
//! ## Attribute reference (abbreviated — full list in the cargo-reef repo)
//!
//! Table-level: `#[reef::table(name = "...", strict, without_rowid)]`
//! Field-level: `#[column(primary_key, auto_increment, unique, default = ...,
//!                        check = ..., references = "table(col)",
//!                        on_delete = "cascade", generated = ...)]`
//! Struct helpers: `#[index(...)]`, `#[primary_key(columns = [...])]`,
//!                 `#[foreign_key(...)]`, `#[check(...)]`
//!
//! For typed JSON columns: `pub field: reef::Json<MyType>` (TEXT) or
//!                         `pub field: reef::Jsonb<MyType>` (BLOB, SQLite 3.45+).

use serde::{Deserialize, Serialize};

#[reef::table]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Greeting {
    #[column(primary_key)]
    pub id: i64,
    pub text: String,
}
