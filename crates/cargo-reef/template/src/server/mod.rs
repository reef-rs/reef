//! Server-side internals. Compiled only with `--features server`.
//!
//! Module layout:
//!   - `db`       — connection management (`Db` struct, `default_db()` global)
//!   - `db::schema` — `#[reef::table]` row types; SSOT for the DB shape (`cargo reef db:push`)
//!   - `queries`  — read-side functions (SELECTs)
//!   - `actions`  — write-side functions (INSERT / UPDATE / DELETE)
//!
//! Code here is NOT part of the wire surface. It's called by `crate::api::*`
//! server fns. Browsers / edge devices never reach into this module — they
//! call the typed RPC defined in `api/`, which delegates here.

pub mod actions;
pub mod db;
pub mod queries;
