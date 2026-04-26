//! Actions — all writes (INSERT / UPDATE / DELETE) go through here.
//!
//! Reefer Rule: this is the canonical entry point for state changes. If you
//! find yourself writing INSERT / UPDATE / DELETE SQL elsewhere, ask if it
//! should be a function in this file.
//!
//! Functions take `&Db` (same pattern as `queries`) so tests can construct
//! a fresh DB without global state.

use anyhow::Result;

use crate::server::db::Db;

pub async fn upsert_greeting(db: &Db, text: &str) -> Result<()> {
    let conn = db.conn()?;
    conn.execute(
        "INSERT INTO greeting (id, text) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET text = excluded.text",
        [text],
    )
    .await?;
    Ok(())
}
