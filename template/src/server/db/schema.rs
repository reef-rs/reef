//! Schema — row types for the database.
//!
//! Today these are plain Rust structs that mirror the SQL schema. The SQL
//! lives in `migrations/<timestamp>_<name>.sql`; this file is the typed
//! Rust view of those tables, used by `queries::*` and `actions::*`.
//!
//! ## v0.5 destination
//!
//! In v0.5, this file becomes the **single source of truth** for the schema.
//! The plan is a Drizzle-style flow:
//!
//! ```rust,ignore
//! #[reef::table]
//! pub struct Greeting {
//!     #[reef::column(primary_key)]
//!     pub id: i64,
//!     pub text: String,
//! }
//! ```
//!
//! Then `cargo reef db:push` will:
//! 1. Read this file, build an in-memory schema description
//! 2. Compare against the actual DB state
//! 3. Generate a new SQL migration that closes the diff
//! 4. Apply it (or write it to `migrations/` for review, depending on flag)
//!
//! Until then: keep this file in sync with `migrations/*.sql` by hand.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Greeting {
    pub id: i64,
    pub text: String,
}
