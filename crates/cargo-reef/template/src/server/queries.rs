//! Queries — all reads (SELECTs) go through here.
//!
//! Functions take `&Db` so tests can pass a fresh in-memory database without
//! any global state. App code typically calls `storage::default_db().await`
//! once and reuses the result.

use anyhow::Result;

use crate::server::db::Db;

pub async fn fetch_greeting(db: &Db) -> Result<Option<String>> {
    let conn = db.conn()?;
    let mut rows = conn
        .query("SELECT text FROM greeting WHERE id = 1", ())
        .await?;
    let Some(row) = rows.next().await? else {
        return Ok(None);
    };
    Ok(Some(row.get::<String>(0)?))
}
