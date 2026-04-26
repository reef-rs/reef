//! Live SQLite/libSQL introspection — the inverse of [`super::emit`].
//!
//! Queries `PRAGMA` + `sqlite_master` to build a [`Schema`] that mirrors what
//! the parser produces from `schema.rs`, so the diff engine can compare them
//! cell-for-cell.
//!
//! ## Reconstruction gaps (acknowledged)
//!
//! Some details aren't recoverable from the live DB and have to round-trip
//! through string-matching `sqlite_master.sql`:
//!
//! - `STRICT`, `WITHOUT ROWID`, `AUTOINCREMENT` — substring-matched
//! - Generated-column expression text — substring-matched (the **kind**
//!   stored vs virtual is deterministic from `PRAGMA table_xinfo.hidden`)
//! - **CHECK constraints are NOT introspected in v0.2.** They live only in
//!   `sqlite_master.sql` and parsing them safely needs a real SQL parser.
//!   The diff engine treats CHECKs as "trust the schema source" — if you
//!   add or remove one, regenerate; we won't drift-warn on them yet.
//! - `Json<T>` / `Jsonb<T>` round-trip as plain `Text` / `Blob` (the
//!   wrapper info is a Rust-side concept; SQL type is identical). The
//!   diff engine treats them as equivalent.

use anyhow::{Context, Result};
use libsql::Connection;

use super::ir::{
    Column, ColumnFk, ColumnType, FkAction, Generated, GeneratedKind, Index, Schema, Table,
    TableForeignKey, TablePrimaryKey,
};

pub async fn introspect_db(conn: &Connection) -> Result<Schema> {
    let table_rows = collect_user_tables(conn).await?;
    let mut tables = Vec::with_capacity(table_rows.len());
    for (name, sql) in &table_rows {
        let table = introspect_table(conn, name, sql)
            .await
            .with_context(|| format!("introspecting table `{name}`"))?;
        tables.push(table);
    }
    Ok(Schema { tables })
}

/// `(name, sql_create_text)` pairs for every user table. Hides the
/// `sqlite_*` internal tables and our own `schema_migrations`.
async fn collect_user_tables(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut rows = conn
        .query(
            "SELECT name, sql FROM sqlite_master \
             WHERE type = 'table' \
               AND name NOT LIKE 'sqlite_%' \
               AND name != 'schema_migrations' \
             ORDER BY name",
            (),
        )
        .await
        .context("listing user tables")?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        let name: String = row.get(0)?;
        let sql: Option<String> = row.get(1).ok();
        out.push((name, sql.unwrap_or_default()));
    }
    Ok(out)
}

async fn introspect_table(conn: &Connection, name: &str, create_sql: &str) -> Result<Table> {
    let (cols, primary_key) = introspect_columns(conn, name, create_sql).await?;
    let indexes = introspect_indexes(conn, name).await?;
    let (single_fks, composite_fks) = introspect_foreign_keys(conn, name).await?;

    let cols = merge_column_fks(cols, single_fks);

    let strict = sql_has_keyword_after_body(create_sql, "STRICT");
    let without_rowid = sql_has_keyword_after_body(create_sql, "WITHOUT ROWID");

    Ok(Table {
        name: name.to_string(),
        rust_name: name.to_string(), // unrecoverable from DB; mirror the SQL name
        strict,
        without_rowid,
        columns: cols,
        primary_key,
        indexes,
        foreign_keys: composite_fks,
        checks: Vec::new(), // see module docs — not introspected in v0.2
    })
}

// ============================================================================
//  columns — PRAGMA table_xinfo + sqlite_master.sql sniffing
// ============================================================================

async fn introspect_columns(
    conn: &Connection,
    table: &str,
    create_sql: &str,
) -> Result<(Vec<Column>, Option<TablePrimaryKey>)> {
    let mut rows = conn
        .query(&format!("PRAGMA table_xinfo({})", quote_pragma(table)), ())
        .await
        .with_context(|| format!("PRAGMA table_xinfo({table})"))?;

    // PRAGMA `pk` field counts the PK columns: 0 = not in PK, 1.. = position.
    // We collect (col, pk_position) so we can later distinguish single vs
    // composite primary keys.
    let mut cols: Vec<(i64, Column)> = Vec::new();
    while let Some(row) = rows.next().await? {
        let _cid: i64 = row.get(0)?;
        let name: String = row.get(1)?;
        let decl_type: String = row.get(2)?;
        let notnull: i64 = row.get(3)?;
        let dflt: Option<String> = row.get(4).ok();
        let pk_pos: i64 = row.get(5)?;
        let hidden: i64 = row.get(6).unwrap_or(0);

        let ty = decl_type_to_column_type(&decl_type);
        let single_pk = pk_pos == 1;
        // PRAGMA `notnull` reports 0 for `INTEGER PRIMARY KEY` columns (SQLite
        // historical quirk — that form is technically nullable as "use next
        // ROWID"). Semantically a PK column is not nullable; align with what
        // the user would have written in schema.rs (`id: i64`, not `Option<i64>`).
        let nullable = notnull == 0 && pk_pos == 0;
        let auto_increment = single_pk && create_sql_has_autoincrement(create_sql, &name);
        let generated = match hidden {
            2 => Some(Generated {
                expr: extract_generated_expr(create_sql, &name).unwrap_or_default(),
                kind: GeneratedKind::Stored,
            }),
            3 => Some(Generated {
                expr: extract_generated_expr(create_sql, &name).unwrap_or_default(),
                kind: GeneratedKind::Virtual,
            }),
            _ => None,
        };

        cols.push((
            pk_pos,
            Column {
                name,
                ty,
                nullable,
                primary_key: single_pk,
                auto_increment,
                unique: false, // populated from PRAGMA index_list below
                default: dflt,
                check: None,   // see module docs
                references: None, // populated from foreign-key pass
                generated,
            },
        ));
    }

    // Build the composite-PK list (if any) BEFORE demoting per-column flags.
    let mut pk_members: Vec<(i64, String)> = cols
        .iter()
        .filter(|(p, _)| *p > 0)
        .map(|(p, c)| (*p, c.name.clone()))
        .collect();
    pk_members.sort_by_key(|(p, _)| *p);
    let composite_pk = if pk_members.len() > 1 {
        // Demote per-column flags — composite PK lives at the table level.
        for (_, c) in cols.iter_mut() {
            c.primary_key = false;
            c.auto_increment = false;
        }
        Some(TablePrimaryKey {
            columns: pk_members.into_iter().map(|(_, n)| n).collect(),
        })
    } else {
        None
    };

    // Surface column-level UNIQUE constraints from PRAGMA index_list. SQLite
    // emits an auto-named `sqlite_autoindex_*` row per single-column UNIQUE
    // declared inline. Multi-column UNIQUEs become regular indexes (handled
    // in introspect_indexes).
    apply_unique_flags(conn, table, &mut cols).await?;

    Ok((cols.into_iter().map(|(_, c)| c).collect(), composite_pk))
}

async fn apply_unique_flags(
    conn: &Connection,
    table: &str,
    cols: &mut [(i64, Column)],
) -> Result<()> {
    let mut idx_rows = conn
        .query(&format!("PRAGMA index_list({})", quote_pragma(table)), ())
        .await?;
    while let Some(row) = idx_rows.next().await? {
        let idx_name: String = row.get(1)?;
        let unique: i64 = row.get(2)?;
        let origin: String = row.get(3).unwrap_or_default();
        if unique != 1 || origin == "pk" {
            continue;
        }
        // Only single-column UNIQUE declared inline (origin == 'u' on the
        // column itself). Origin 'u' from the table level may also be
        // single-col; we check by reading index_info.
        let cols_in_idx = read_index_columns(conn, &idx_name).await?;
        if cols_in_idx.len() == 1 {
            if let Some(target) = cols_in_idx.first() {
                if let Some((_, c)) = cols.iter_mut().find(|(_, c)| c.name == *target) {
                    c.unique = true;
                }
            }
        }
    }
    Ok(())
}

async fn read_index_columns(conn: &Connection, index: &str) -> Result<Vec<String>> {
    let mut rows = conn
        .query(&format!("PRAGMA index_info({})", quote_pragma(index)), ())
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        let name: Option<String> = row.get(2).ok();
        if let Some(n) = name {
            out.push(n);
        }
    }
    Ok(out)
}

// ============================================================================
//  foreign keys — PRAGMA foreign_key_list
// ============================================================================

/// Returns (single-column FKs keyed by local column name, composite FKs).
async fn introspect_foreign_keys(
    conn: &Connection,
    table: &str,
) -> Result<(Vec<(String, ColumnFk)>, Vec<TableForeignKey>)> {
    let mut rows = conn
        .query(
            &format!("PRAGMA foreign_key_list({})", quote_pragma(table)),
            (),
        )
        .await?;

    // Group rows by `id`; each group is one FK constraint (composite if >1 row).
    let mut groups: std::collections::BTreeMap<i64, Vec<FkRow>> =
        std::collections::BTreeMap::new();
    while let Some(row) = rows.next().await? {
        let id: i64 = row.get(0)?;
        let _seq: i64 = row.get(1)?;
        let target_table: String = row.get(2)?;
        let from: String = row.get(3)?;
        let to: Option<String> = row.get(4).ok();
        let on_update: String = row.get(5).unwrap_or_default();
        let on_delete: String = row.get(6).unwrap_or_default();
        groups.entry(id).or_default().push(FkRow {
            target_table,
            from,
            to,
            on_update: parse_pragma_action(&on_update),
            on_delete: parse_pragma_action(&on_delete),
        });
    }

    let mut singles = Vec::new();
    let mut composites = Vec::new();
    for (_, grp) in groups {
        if grp.len() == 1 {
            let r = &grp[0];
            singles.push((
                r.from.clone(),
                ColumnFk {
                    table: r.target_table.clone(),
                    column: r.to.clone().unwrap_or_default(),
                    on_delete: r.on_delete,
                    on_update: r.on_update,
                },
            ));
        } else {
            let target_table = grp[0].target_table.clone();
            let on_delete = grp[0].on_delete;
            let on_update = grp[0].on_update;
            composites.push(TableForeignKey {
                columns: grp.iter().map(|r| r.from.clone()).collect(),
                references_table: target_table,
                references_columns: grp
                    .iter()
                    .map(|r| r.to.clone().unwrap_or_default())
                    .collect(),
                on_delete,
                on_update,
            });
        }
    }

    Ok((singles, composites))
}

struct FkRow {
    target_table: String,
    from: String,
    to: Option<String>,
    on_update: Option<FkAction>,
    on_delete: Option<FkAction>,
}

fn parse_pragma_action(s: &str) -> Option<FkAction> {
    match s {
        "CASCADE" => Some(FkAction::Cascade),
        "RESTRICT" => Some(FkAction::Restrict),
        "SET NULL" => Some(FkAction::SetNull),
        "SET DEFAULT" => Some(FkAction::SetDefault),
        "NO ACTION" | "" => None, // NO ACTION is the SQLite default; treat as "unspecified"
        _ => None,
    }
}

fn merge_column_fks(mut cols: Vec<Column>, singles: Vec<(String, ColumnFk)>) -> Vec<Column> {
    for (col_name, fk) in singles {
        if let Some(c) = cols.iter_mut().find(|c| c.name == col_name) {
            c.references = Some(fk);
        }
    }
    cols
}

// ============================================================================
//  indexes — PRAGMA index_list
// ============================================================================

async fn introspect_indexes(conn: &Connection, table: &str) -> Result<Vec<Index>> {
    let mut idx_rows = conn
        .query(&format!("PRAGMA index_list({})", quote_pragma(table)), ())
        .await?;
    let mut out = Vec::new();
    while let Some(row) = idx_rows.next().await? {
        let idx_name: String = row.get(1)?;
        let unique: i64 = row.get(2)?;
        let origin: String = row.get(3).unwrap_or_default();
        // Skip auto-created PK and column-level UNIQUE indexes — those are
        // surfaced through column attributes, not as standalone indexes.
        if origin == "pk" || idx_name.starts_with("sqlite_autoindex_") {
            continue;
        }
        // PRAGMA index_info returns NULL for expression positions in an
        // expression index (e.g. `json_extract(meta, '$.country')`). For those,
        // fall back to parsing the index's CREATE INDEX text from sqlite_master,
        // which always carries the original expression list.
        let mut columns = read_index_columns(conn, &idx_name).await?;
        let needs_expr_recovery = columns.is_empty()
            || columns.len() < count_index_arity(conn, &idx_name).await.unwrap_or(0);
        if needs_expr_recovery {
            if let Some(expr_cols) = recover_index_columns_from_master(conn, &idx_name).await? {
                columns = expr_cols;
            }
        }
        out.push(Index {
            name: Some(idx_name),
            columns,
            unique: unique == 1,
        });
    }
    Ok(out)
}

/// PRAGMA index_xinfo gives one row per indexed position (vs index_info which
/// only emits rows with non-NULL column names). Counting it tells us the
/// "true" arity so we can detect when index_info underreported due to
/// expression positions.
async fn count_index_arity(conn: &Connection, index: &str) -> Result<usize> {
    let mut rows = conn
        .query(&format!("PRAGMA index_xinfo({})", quote_pragma(index)), ())
        .await?;
    let mut count = 0;
    while let Some(row) = rows.next().await? {
        // `key` field (column 5 in xinfo) is 1 for "key" columns, 0 for
        // included/aux. Count only key columns.
        let key: i64 = row.get(5).unwrap_or(1);
        if key == 1 {
            count += 1;
        }
    }
    Ok(count)
}

/// Read `sqlite_master.sql` for the index, find the `(...)` after `ON tablename`,
/// and split the contents on top-level commas (respecting parens + quoted
/// strings). Returns None if the row is missing or malformed.
async fn recover_index_columns_from_master(
    conn: &Connection,
    index: &str,
) -> Result<Option<Vec<String>>> {
    let mut rows = conn
        .query(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
            libsql::params![index.to_string()],
        )
        .await?;
    let Some(row) = rows.next().await? else {
        return Ok(None);
    };
    let Some(sql): Option<String> = row.get(0).ok() else {
        return Ok(None);
    };
    Ok(parse_index_columns_from_sql(&sql))
}

/// Extract the column list from `CREATE [UNIQUE] INDEX name ON tbl (a, b, expr(...))`.
/// Top-level comma split with paren and quote awareness — handles expression
/// indexes like `json_extract(meta, '$.country')` correctly.
fn parse_index_columns_from_sql(sql: &str) -> Option<Vec<String>> {
    // Find the parenthesized column list. Locate `ON <ident> (` and start
    // there. We walk by index since SQL identifiers are ASCII-friendly here.
    let lower = sql.to_ascii_lowercase();
    let on_pos = lower.find(" on ")?;
    let after_on = &sql[on_pos + 4..];
    let open_rel = after_on.find('(')?;
    let body_start = on_pos + 4 + open_rel + 1;

    // Walk to the matching close paren.
    let bytes = sql.as_bytes();
    let mut depth = 1i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut end = None;
    let mut splits: Vec<usize> = Vec::new();
    for (i, &b) in bytes.iter().enumerate().skip(body_start) {
        let c = b as char;
        if in_single {
            if c == '\'' {
                // Handle SQL escape '' (doubled quote).
                if bytes.get(i + 1).copied() == Some(b'\'') {
                    continue;
                }
                in_single = false;
            }
            continue;
        }
        if in_double {
            if c == '"' {
                in_double = false;
            }
            continue;
        }
        match c {
            '\'' => in_single = true,
            '"' => in_double = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            ',' if depth == 1 => splits.push(i),
            _ => {}
        }
    }
    let end = end?;

    let mut parts = Vec::new();
    let mut cursor = body_start;
    for s in splits.iter().chain(std::iter::once(&end)) {
        let frag = sql[cursor..*s].trim();
        if !frag.is_empty() {
            // Strip trailing ASC/DESC and COLLATE clauses if present —
            // they don't affect "what column expression is being indexed."
            parts.push(strip_index_modifiers(frag));
        }
        cursor = *s + 1;
    }
    Some(parts)
}

fn strip_index_modifiers(s: &str) -> String {
    // Walk from right; strip ASC/DESC and COLLATE <name> tokens.
    let mut out = s.trim().to_string();
    loop {
        let lower = out.to_ascii_lowercase();
        let trimmed = lower.trim_end();
        let stripped = trimmed
            .strip_suffix(" asc")
            .or_else(|| trimmed.strip_suffix(" desc"));
        if let Some(new_lower) = stripped {
            out.truncate(new_lower.len());
            out = out.trim_end().to_string();
            continue;
        }
        // COLLATE <name> — find " collate " in lowercase, truncate at its position
        if let Some(pos) = lower.rfind(" collate ") {
            // Only strip if there's nothing else after the collate clause
            // (i.e., it's the trailing modifier).
            let tail = &lower[pos + " collate ".len()..];
            if tail.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.truncate(pos);
                out = out.trim_end().to_string();
                continue;
            }
        }
        break;
    }
    out
}

// ============================================================================
//  helpers
// ============================================================================

fn decl_type_to_column_type(decl: &str) -> ColumnType {
    // SQLite's "type affinity" rules — simplified. We don't preserve the
    // user's exact declared text (e.g. VARCHAR(50)) because the diff engine
    // compares by ColumnType, not by raw declaration text.
    let upper = decl.to_ascii_uppercase();
    if upper.contains("INT") {
        ColumnType::Integer
    } else if upper.contains("REAL") || upper.contains("FLOA") || upper.contains("DOUB") {
        ColumnType::Real
    } else if upper.contains("BLOB") || upper.is_empty() {
        ColumnType::Blob
    } else if upper.contains("CHAR") || upper.contains("TEXT") || upper.contains("CLOB") {
        ColumnType::Text
    } else {
        // Numeric affinity falls through here. SQLite stores as appropriate
        // depending on data; map to Integer as the closest discrete bucket.
        ColumnType::Integer
    }
}

/// Quote a table/index identifier for use inside a `PRAGMA` argument. PRAGMA
/// args don't accept normal `?` parameter binding, so we have to inline.
fn quote_pragma(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Returns true if `keyword` (e.g. "STRICT", "WITHOUT ROWID") appears after
/// the table body's closing `)`. Substring match — fine for the deterministic
/// shapes SQLite emits in `sqlite_master.sql`.
fn sql_has_keyword_after_body(create_sql: &str, keyword: &str) -> bool {
    let Some(close) = create_sql.rfind(')') else {
        return false;
    };
    let tail = &create_sql[close..].to_ascii_uppercase();
    tail.contains(&keyword.to_ascii_uppercase())
}

/// Cheap detector: does this column's declaration in `create_sql` carry
/// AUTOINCREMENT? Substring-match against the column-name prefix.
fn create_sql_has_autoincrement(create_sql: &str, column: &str) -> bool {
    // Find the column name as a token, then look ahead a short window for
    // AUTOINCREMENT before the next comma or close paren.
    let upper = create_sql.to_ascii_uppercase();
    let needle = column.to_ascii_uppercase();
    let Some(pos) = upper.find(&needle) else {
        return false;
    };
    let after = &upper[pos..];
    let end = after
        .find(",\n")
        .or_else(|| after.find(",\r"))
        .or_else(|| after.find(','))
        .or_else(|| after.find('\n'))
        .unwrap_or(after.len());
    after[..end].contains("AUTOINCREMENT")
}

/// Extract the parenthesized expression after `GENERATED ALWAYS AS` for the
/// given column. Returns None if not found — caller substitutes empty string.
fn extract_generated_expr(create_sql: &str, column: &str) -> Option<String> {
    let upper = create_sql.to_ascii_uppercase();
    let needle = column.to_ascii_uppercase();
    let mut search_from = 0usize;
    while let Some(rel) = upper[search_from..].find(&needle) {
        let pos = search_from + rel;
        let after = &create_sql[pos..];
        let after_upper = &upper[pos..];
        // Look for GENERATED within the column's declaration window.
        let window_end = after.find(",\n").unwrap_or(after.len());
        let win = &after[..window_end];
        let win_upper = &after_upper[..window_end];
        if let Some(g) = win_upper.find("GENERATED ALWAYS AS") {
            let rest = &win[g + "GENERATED ALWAYS AS".len()..];
            if let Some(open) = rest.find('(') {
                if let Some(close) = matched_paren_end(&rest[open..]) {
                    let expr = &rest[open + 1..open + close];
                    return Some(expr.trim().to_string());
                }
            }
        }
        search_from = pos + needle.len();
    }
    None
}

/// Given a string starting at `(`, return the index of the matching `)`.
fn matched_paren_end(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}
