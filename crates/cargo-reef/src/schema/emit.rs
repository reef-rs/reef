//! Emit `CREATE TABLE` / `CREATE INDEX` SQL from a [`Schema`].
//!
//! Pure transform — no I/O. Used both for "first migration from an empty DB"
//! and for the codegen side of the diff engine (which renders a sequence of
//! [`crate::schema::ir::Table`] additions as SQL).
//!
//! Output style: pretty-printed with 4-space indent and one column per line,
//! trailing semicolons, no terminating newline. Statements come back as a
//! `Vec<String>` so callers can join with `\n\n` or apply individually.

use super::diff::Action;
use super::ir::{
    Column, ColumnFk, Generated, GeneratedKind, Index, Schema, Table, TableForeignKey,
};

/// Render one [`Action`] as a SQL statement. Returns `None` for
/// [`Action::NeedsRebuild`] — those require a hand-written migration.
pub fn emit_action(action: &Action) -> Option<String> {
    Some(match action {
        Action::CreateTable(t) => emit_table(t),
        Action::DropTable(name) => format!("DROP TABLE {};", quote_ident(name)),
        Action::AddColumn { table, column } => {
            format!(
                "ALTER TABLE {} ADD COLUMN {};",
                quote_ident(table),
                emit_column(column)
            )
        }
        Action::DropColumn { table, column } => {
            format!(
                "ALTER TABLE {} DROP COLUMN {};",
                quote_ident(table),
                quote_ident(column)
            )
        }
        // libSQL extension: ALTER COLUMN <name> TO <new column definition>.
        // The "TO" form replaces the column's declaration in-place.
        Action::AlterColumn { table, after, .. } => {
            format!(
                "ALTER TABLE {} ALTER COLUMN {} TO {};",
                quote_ident(table),
                quote_ident(&after.name),
                emit_column(after)
            )
        }
        Action::CreateIndex { table, index } => emit_index(table, index),
        Action::DropIndex { name } => format!("DROP INDEX {};", quote_ident(name)),
        Action::NeedsRebuild { .. } => return None,
    })
}

/// Emit one statement per table + one per index, in dependency-stable order
/// (tables first by source order, then indexes table-by-table).
pub fn emit_schema(schema: &Schema) -> Vec<String> {
    let mut out = Vec::with_capacity(schema.tables.len() * 2);
    for t in &schema.tables {
        out.push(emit_table(t));
    }
    for t in &schema.tables {
        for idx in &t.indexes {
            out.push(emit_index(&t.name, idx));
        }
    }
    out
}

pub fn emit_table(t: &Table) -> String {
    let mut s = String::new();
    s.push_str("CREATE TABLE ");
    s.push_str(&quote_ident(&t.name));
    s.push_str(" (\n");

    let mut parts: Vec<String> = Vec::new();

    for col in &t.columns {
        parts.push(format!("    {}", emit_column(col)));
    }

    if let Some(pk) = &t.primary_key {
        parts.push(format!(
            "    PRIMARY KEY ({})",
            pk.columns
                .iter()
                .map(|c| quote_ident(c))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    for fk in &t.foreign_keys {
        parts.push(format!("    {}", emit_table_fk(fk)));
    }

    for chk in &t.checks {
        parts.push(format!(
            "    CONSTRAINT {} CHECK ({})",
            quote_ident(&chk.name),
            chk.expr
        ));
    }

    s.push_str(&parts.join(",\n"));
    s.push_str("\n)");

    let mut suffixes: Vec<&str> = Vec::new();
    if t.without_rowid {
        suffixes.push("WITHOUT ROWID");
    }
    if t.strict {
        suffixes.push("STRICT");
    }
    if !suffixes.is_empty() {
        s.push(' ');
        s.push_str(&suffixes.join(", "));
    }

    s.push(';');
    s
}

fn emit_column(col: &Column) -> String {
    let mut s = String::new();
    s.push_str(&quote_ident(&col.name));
    s.push(' ');
    s.push_str(col.ty.sql());

    if col.primary_key {
        s.push_str(" PRIMARY KEY");
        if col.auto_increment {
            // SQLite requires AUTOINCREMENT on INTEGER PRIMARY KEY only; the
            // schema validator catches the mismatch separately. Here we just
            // emit what was declared.
            s.push_str(" AUTOINCREMENT");
        }
    }

    if !col.nullable && !col.primary_key {
        // PRIMARY KEY columns are NOT NULL by definition (with the historical
        // INTEGER PRIMARY KEY caveat); avoid the redundant clause.
        s.push_str(" NOT NULL");
    }

    if col.unique && !col.primary_key {
        s.push_str(" UNIQUE");
    }

    if let Some(d) = &col.default {
        s.push_str(" DEFAULT ");
        s.push_str(d);
    }

    if let Some(c) = &col.check {
        s.push_str(" CHECK (");
        s.push_str(c);
        s.push(')');
    }

    if let Some(g) = &col.generated {
        emit_generated(&mut s, g);
    }

    if let Some(fk) = &col.references {
        emit_column_fk(&mut s, fk);
    }

    s
}

fn emit_generated(s: &mut String, g: &Generated) {
    s.push_str(" GENERATED ALWAYS AS (");
    s.push_str(&g.expr);
    s.push_str(") ");
    s.push_str(match g.kind {
        GeneratedKind::Stored => "STORED",
        GeneratedKind::Virtual => "VIRTUAL",
    });
}

fn emit_column_fk(s: &mut String, fk: &ColumnFk) {
    s.push_str(" REFERENCES ");
    s.push_str(&quote_ident(&fk.table));
    s.push_str(" (");
    s.push_str(&quote_ident(&fk.column));
    s.push(')');
    if let Some(a) = fk.on_delete {
        s.push_str(" ON DELETE ");
        s.push_str(a.sql());
    }
    if let Some(a) = fk.on_update {
        s.push_str(" ON UPDATE ");
        s.push_str(a.sql());
    }
}

fn emit_table_fk(fk: &TableForeignKey) -> String {
    let mut s = String::new();
    s.push_str("FOREIGN KEY (");
    s.push_str(
        &fk.columns
            .iter()
            .map(|c| quote_ident(c))
            .collect::<Vec<_>>()
            .join(", "),
    );
    s.push_str(") REFERENCES ");
    s.push_str(&quote_ident(&fk.references_table));
    s.push_str(" (");
    s.push_str(
        &fk.references_columns
            .iter()
            .map(|c| quote_ident(c))
            .collect::<Vec<_>>()
            .join(", "),
    );
    s.push(')');
    if let Some(a) = fk.on_delete {
        s.push_str(" ON DELETE ");
        s.push_str(a.sql());
    }
    if let Some(a) = fk.on_update {
        s.push_str(" ON UPDATE ");
        s.push_str(a.sql());
    }
    s
}

pub fn emit_index(table_name: &str, idx: &Index) -> String {
    let mut s = String::new();
    s.push_str("CREATE ");
    if idx.unique {
        s.push_str("UNIQUE ");
    }
    s.push_str("INDEX ");
    let derived;
    let name: &str = if let Some(n) = &idx.name {
        n.as_str()
    } else {
        // Fallback name: <table>_<col1>_<col2>_idx, sanitized.
        derived = format!(
            "{}_{}_idx",
            table_name,
            idx.columns
                .iter()
                .map(|c| sanitize_for_ident(c))
                .collect::<Vec<_>>()
                .join("_")
        );
        derived.as_str()
    };
    s.push_str(&quote_ident(name));
    s.push_str(" ON ");
    s.push_str(&quote_ident(table_name));
    s.push_str(" (");
    // Each entry can be a bare ident OR a SQL expression (e.g. json_extract(...)).
    // We don't try to disambiguate — emit verbatim. Quote-only-if-bare-ident
    // would be safer but adds parsing; defer.
    s.push_str(&idx.columns.join(", "));
    s.push_str(");");
    s
}

// ============================================================================
//  identifier handling
// ============================================================================

/// Quote an identifier with double-quotes if it's a SQLite reserved word OR
/// contains anything other than `[A-Za-z0-9_]`. Otherwise emit bare for
/// readability.
fn quote_ident(name: &str) -> String {
    if needs_quoting(name) {
        format!("\"{}\"", name.replace('"', "\"\""))
    } else {
        name.to_string()
    }
}

fn needs_quoting(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    let bad_char = !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_');
    let starts_digit = name.chars().next().is_some_and(|c| c.is_ascii_digit());
    bad_char || starts_digit || is_reserved(name)
}

/// Conservative subset of SQLite reserved words. Not exhaustive — we quote
/// when in doubt rather than silently emit invalid SQL.
fn is_reserved(name: &str) -> bool {
    const RESERVED: &[&str] = &[
        "abort", "action", "add", "after", "all", "alter", "analyze", "and", "as", "asc",
        "attach", "autoincrement", "before", "begin", "between", "by", "cascade", "case",
        "cast", "check", "collate", "column", "commit", "conflict", "constraint", "create",
        "cross", "current", "database", "default", "deferrable", "deferred", "delete", "desc",
        "detach", "distinct", "drop", "each", "else", "end", "escape", "except", "exclusive",
        "exists", "explain", "fail", "for", "foreign", "from", "full", "glob", "group",
        "having", "if", "ignore", "immediate", "in", "index", "indexed", "initially",
        "inner", "insert", "instead", "intersect", "into", "is", "isnull", "join", "key",
        "left", "like", "limit", "match", "natural", "no", "not", "notnull", "null", "of",
        "offset", "on", "or", "order", "outer", "plan", "pragma", "primary", "query",
        "raise", "references", "regexp", "reindex", "release", "rename", "replace",
        "restrict", "right", "rollback", "row", "savepoint", "select", "set", "table",
        "temp", "temporary", "then", "to", "transaction", "trigger", "union", "unique",
        "update", "using", "vacuum", "values", "view", "virtual", "when", "where",
        "with", "without",
    ];
    RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name))
}

/// Reduce a SQL expression like `json_extract(meta, '$.x')` to a token suitable
/// for use inside an index name. Used only for fallback-name derivation.
fn sanitize_for_ident(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_underscore = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_underscore = false;
        } else if !last_underscore && !out.is_empty() {
            out.push('_');
            last_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}
