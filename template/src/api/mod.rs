//! API layer — the wire surface. Both server fns (one-shot RPC) and WS
//! endpoints live here.
//!
//! ## The macros
//!
//! - `#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]` — explicit HTTP
//!   method + URL path. The idiomatic Dioxus 0.7 way.
//! - `#[server]` — generic fallback when you don't care about method/URL.
//!
//! Path parameters in `{name}` braces get extracted into matching function
//! arguments by name. Query params follow `?name1&name2` syntax.
//!
//! ## Why explicit URLs
//!
//! The macro generates BOTH halves from one source:
//!   - WASM: a typed client stub the UI calls like a normal async fn
//!   - Native: an HTTP handler registered at the declared path
//!
//! Both halves see the same path, the same param types, and the same return
//! type. Refactor-safe end-to-end.
//!
//! ## Where the body runs
//!
//! Function bodies run **server-side only**. Bodies can `use crate::server::*`
//! to delegate to DB and business logic. The macro auto-elides bodies on
//! WASM builds — your client never sees server-only code.

use dioxus::prelude::*;

use crate::types::Status;

#[get("/api/status")]
pub async fn get_status() -> Result<Status, ServerFnError> {
    let db = crate::server::db::default_db()
        .await
        .map_err(|e| ServerFnError::new(format!("db init: {e}")))?;

    let greeting = crate::server::queries::fetch_greeting(db).await.ok().flatten();

    Ok(Status {
        message: "Reef is running".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        greeting,
    })
}
