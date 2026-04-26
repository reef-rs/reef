//! Diff two [`Schema`] values into a sequence of migration [`Action`]s.
//!
//! The diff is conservative: anything we can't express safely with one of
//! libSQL's supported ALTER forms produces a [`Action::NeedsRebuild`] entry
//! with a human-readable reason instead of guessing at a 12-step rebuild.
//!
//! ## What we use
//!
//! - libSQL's `ALTER TABLE ALTER COLUMN ... TO ...` for type/constraint/FK
//!   changes (one of libSQL's extensions over stock SQLite — see
//!   `notes/architecture.md`).
//! - Stock `ALTER TABLE ADD COLUMN` for added columns.
//! - Stock `ALTER TABLE DROP COLUMN` for removed columns, *only when* the
//!   column isn't PK/UNIQUE/indexed/FK'd (SQLite restriction).
//! - `CREATE/DROP INDEX` for index changes.
//!
//! ## Tightening warnings
//!
//! When ALTER COLUMN would tighten a constraint (NULL → NOT NULL, weaker
//! CHECK → stricter CHECK), libSQL applies the new rule to *new* writes
//! only — existing rows are NOT revalidated. The diff emits a warning so
//! the user can decide whether to backfill manually before pushing.

use std::collections::{BTreeMap, BTreeSet};

use super::ir::{Column, ColumnFk, ColumnType, Index, Schema, Table, TableForeignKey};

#[derive(Debug, Clone)]
pub enum Action {
    CreateTable(Table),
    DropTable(String),
    AddColumn {
        table: String,
        column: Column,
    },
    DropColumn {
        table: String,
        column: String,
    },
    AlterColumn {
        table: String,
        before: Column,
        after: Column,
    },
    CreateIndex {
        table: String,
        index: Index,
    },
    DropIndex {
        name: String,
    },
    /// libSQL can't safely express this change in-place. Caller must
    /// `cargo reef migrate new <name>` and write a manual migration.
    NeedsRebuild {
        table: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Diff {
    pub actions: Vec<Action>,
    /// Non-fatal advisories — typically "tightening" changes that libSQL
    /// applies to new writes only.
    pub warnings: Vec<String>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

pub fn diff(desired: &Schema, actual: &Schema) -> Diff {
    let mut out = Diff::default();

    let desired_by_name: BTreeMap<&str, &Table> =
        desired.tables.iter().map(|t| (t.name.as_str(), t)).collect();
    let actual_by_name: BTreeMap<&str, &Table> =
        actual.tables.iter().map(|t| (t.name.as_str(), t)).collect();

    let all_names: BTreeSet<&str> = desired_by_name
        .keys()
        .chain(actual_by_name.keys())
        .copied()
        .collect();

    for name in all_names {
        match (desired_by_name.get(name), actual_by_name.get(name)) {
            (Some(d), None) => {
                out.actions.push(Action::CreateTable((*d).clone()));
            }
            (None, Some(_)) => {
                out.actions.push(Action::DropTable(name.to_string()));
            }
            (Some(d), Some(a)) => {
                diff_table(d, a, &mut out);
            }
            (None, None) => unreachable!(),
        }
    }

    out
}

fn diff_table(d: &Table, a: &Table, out: &mut Diff) {
    let table = &d.name;

    // STRICT / WITHOUT ROWID changes can't be ALTERed — both require rebuild.
    if d.strict != a.strict {
        out.actions.push(Action::NeedsRebuild {
            table: table.clone(),
            reason: format!(
                "STRICT changed ({} -> {}) — requires table rebuild",
                a.strict, d.strict
            ),
        });
        return;
    }
    if d.without_rowid != a.without_rowid {
        out.actions.push(Action::NeedsRebuild {
            table: table.clone(),
            reason: format!(
                "WITHOUT ROWID changed ({} -> {}) — requires table rebuild",
                a.without_rowid, d.without_rowid
            ),
        });
        return;
    }

    // Composite PK changes also force a rebuild.
    if d.primary_key.as_ref().map(|p| &p.columns) != a.primary_key.as_ref().map(|p| &p.columns) {
        out.actions.push(Action::NeedsRebuild {
            table: table.clone(),
            reason: "composite PRIMARY KEY changed — requires table rebuild".to_string(),
        });
        return;
    }

    // Normalize columns so equivalent shapes compare equal.
    let d_cols: Vec<Column> = d
        .columns
        .iter()
        .map(|c| normalize_column(c, &d.foreign_keys))
        .collect();
    let a_cols: Vec<Column> = a
        .columns
        .iter()
        .map(|c| normalize_column(c, &a.foreign_keys))
        .collect();

    let d_by_name: BTreeMap<&str, &Column> =
        d_cols.iter().map(|c| (c.name.as_str(), c)).collect();
    let a_by_name: BTreeMap<&str, &Column> =
        a_cols.iter().map(|c| (c.name.as_str(), c)).collect();

    // Column add / drop / alter
    let all_col_names: BTreeSet<&str> = d_by_name
        .keys()
        .chain(a_by_name.keys())
        .copied()
        .collect();

    for col in all_col_names {
        match (d_by_name.get(col), a_by_name.get(col)) {
            (Some(d_c), None) => {
                // Adding a NOT NULL column without DEFAULT is illegal in SQLite
                // (can't backfill). Steer the user to add a default or rebuild.
                if !d_c.nullable && d_c.default.is_none() && d_c.generated.is_none() {
                    out.actions.push(Action::NeedsRebuild {
                        table: table.clone(),
                        reason: format!(
                            "adding NOT NULL column `{}` without a DEFAULT — \
                             SQLite requires a default for backfill, or a manual migration",
                            d_c.name
                        ),
                    });
                } else {
                    out.actions.push(Action::AddColumn {
                        table: table.clone(),
                        column: (*d_c).clone(),
                    });
                }
            }
            (None, Some(a_c)) => {
                if column_drop_requires_rebuild(a_c, a) {
                    out.actions.push(Action::NeedsRebuild {
                        table: table.clone(),
                        reason: format!(
                            "dropping column `{}` requires a rebuild — column is \
                             PRIMARY KEY, UNIQUE, indexed, or referenced by a FK",
                            a_c.name
                        ),
                    });
                } else {
                    out.actions.push(Action::DropColumn {
                        table: table.clone(),
                        column: a_c.name.clone(),
                    });
                }
            }
            (Some(d_c), Some(a_c)) => {
                if columns_equal(d_c, a_c) {
                    continue;
                }
                if column_change_requires_rebuild(a_c, d_c) {
                    out.actions.push(Action::NeedsRebuild {
                        table: table.clone(),
                        reason: format!(
                            "column `{}` change requires rebuild (PK / generated / \
                             auto_increment edits aren't expressible via ALTER COLUMN)",
                            d_c.name
                        ),
                    });
                } else {
                    if is_tightening(a_c, d_c) {
                        out.warnings.push(format!(
                            "{}.{}: tightening change — libSQL ALTER COLUMN \
                             applies to new writes only, existing rows are \
                             not revalidated. Backfill manually if needed.",
                            table, d_c.name
                        ));
                    }
                    out.actions.push(Action::AlterColumn {
                        table: table.clone(),
                        before: (*a_c).clone(),
                        after: (*d_c).clone(),
                    });
                }
            }
            (None, None) => unreachable!(),
        }
    }

    diff_indexes(table, &d.indexes, &a.indexes, out);
}

fn diff_indexes(table: &str, desired: &[Index], actual: &[Index], out: &mut Diff) {
    let key = |i: &Index| {
        i.name
            .clone()
            .unwrap_or_else(|| format!("__anon_{:?}", i.columns))
    };
    let d_by: BTreeMap<String, &Index> = desired.iter().map(|i| (key(i), i)).collect();
    let a_by: BTreeMap<String, &Index> = actual.iter().map(|i| (key(i), i)).collect();

    let all: BTreeSet<&String> = d_by.keys().chain(a_by.keys()).collect();

    for k in all {
        match (d_by.get(k), a_by.get(k)) {
            (Some(d), None) => out.actions.push(Action::CreateIndex {
                table: table.to_string(),
                index: (*d).clone(),
            }),
            (None, Some(a)) => out.actions.push(Action::DropIndex {
                name: a.name.clone().unwrap_or_else(|| k.clone()),
            }),
            (Some(d), Some(a)) => {
                if indexes_equal(d, a) {
                    continue;
                }
                // Indexes are immutable — recreate.
                out.actions.push(Action::DropIndex {
                    name: a.name.clone().unwrap_or_else(|| k.clone()),
                });
                out.actions.push(Action::CreateIndex {
                    table: table.to_string(),
                    index: (*d).clone(),
                });
            }
            (None, None) => unreachable!(),
        }
    }
}

// ============================================================================
//  normalization
// ============================================================================

/// Collapse single-column table-level FKs onto their column so the parsed
/// IR (which keeps them in `foreign_keys`) compares equal to the introspected
/// IR (which puts them on the column).
fn normalize_column(c: &Column, table_fks: &[TableForeignKey]) -> Column {
    let mut out = c.clone();
    if out.references.is_none() {
        for fk in table_fks {
            if fk.columns.len() == 1 && fk.columns[0] == c.name {
                out.references = Some(ColumnFk {
                    table: fk.references_table.clone(),
                    column: fk
                        .references_columns
                        .first()
                        .cloned()
                        .unwrap_or_default(),
                    on_delete: fk.on_delete,
                    on_update: fk.on_update,
                });
                break;
            }
        }
    }
    out
}

// ============================================================================
//  equality checks (with documented loosening for known introspection gaps)
// ============================================================================

fn columns_equal(d: &Column, a: &Column) -> bool {
    d.name == a.name
        && column_types_equal(&d.ty, &a.ty)
        && d.nullable == a.nullable
        && d.primary_key == a.primary_key
        && d.auto_increment == a.auto_increment
        && d.unique == a.unique
        && defaults_equal(d.default.as_deref(), a.default.as_deref())
        // CHECK is not introspected — trust the schema source. If desired
        // declares a CHECK and actual doesn't, we'd loop forever trying to
        // add it. Punt: ignore in equality.
        && fks_equal(d.references.as_ref(), a.references.as_ref())
        && d.generated.as_ref().map(|g| (&g.expr, g.kind as u8))
            == a.generated.as_ref().map(|g| (&g.expr, g.kind as u8))
}

fn column_types_equal(d: &ColumnType, a: &ColumnType) -> bool {
    // Json<T> and TEXT are the same on disk. Same for Jsonb<T> and BLOB.
    matches!(
        (d, a),
        (ColumnType::Integer, ColumnType::Integer)
            | (ColumnType::Real, ColumnType::Real)
            | (ColumnType::Text, ColumnType::Text)
            | (ColumnType::Json(_), ColumnType::Json(_))
            | (ColumnType::Json(_), ColumnType::Text)
            | (ColumnType::Text, ColumnType::Json(_))
            | (ColumnType::Blob, ColumnType::Blob)
            | (ColumnType::Jsonb(_), ColumnType::Jsonb(_))
            | (ColumnType::Jsonb(_), ColumnType::Blob)
            | (ColumnType::Blob, ColumnType::Jsonb(_))
    )
}

fn fks_equal(d: Option<&ColumnFk>, a: Option<&ColumnFk>) -> bool {
    match (d, a) {
        (None, None) => true,
        (Some(d), Some(a)) => {
            d.table == a.table
                && d.column == a.column
                && d.on_delete == a.on_delete
                && d.on_update == a.on_update
        }
        _ => false,
    }
}

fn defaults_equal(d: Option<&str>, a: Option<&str>) -> bool {
    // SQLite normalizes defaults somewhat (e.g. wraps strings in '...').
    // Our parser already wraps Rust string literals as SQL literals, so
    // they should round-trip. Compare trimmed.
    match (d, a) {
        (None, None) => true,
        (Some(x), Some(y)) => x.trim() == y.trim(),
        _ => false,
    }
}

fn indexes_equal(d: &Index, a: &Index) -> bool {
    // Both names must match (we keyed on it), so just compare contents.
    // Empty actual.columns means we hit the expression-index introspection
    // gap — pessimistically treat as different so the diff drops + recreates.
    if a.columns.is_empty() && !d.columns.is_empty() {
        return false;
    }
    d.unique == a.unique && d.columns == a.columns
}

// ============================================================================
//  rebuild predicates
// ============================================================================

fn column_drop_requires_rebuild(c: &Column, t: &Table) -> bool {
    if c.primary_key || c.unique {
        return true;
    }
    if t.indexes.iter().any(|i| i.columns.contains(&c.name)) {
        return true;
    }
    if t.foreign_keys.iter().any(|fk| fk.columns.contains(&c.name)) {
        return true;
    }
    if t.primary_key
        .as_ref()
        .is_some_and(|pk| pk.columns.contains(&c.name))
    {
        return true;
    }
    // Other tables FK-ing INTO this column would also block the drop, but
    // we don't have a cross-table view here; the SQLite engine rejects the
    // drop at apply time, surfacing a clear error.
    false
}

fn column_change_requires_rebuild(before: &Column, after: &Column) -> bool {
    // Adding/removing PK or AUTOINCREMENT can't be expressed in libSQL's
    // ALTER COLUMN form; same for generated columns (the generation can't
    // change on the fly).
    if before.primary_key != after.primary_key
        || before.auto_increment != after.auto_increment
        || before.generated.is_some() != after.generated.is_some()
    {
        return true;
    }
    if let (Some(b), Some(a)) = (&before.generated, &after.generated) {
        if b.expr != a.expr || b.kind as u8 != a.kind as u8 {
            return true;
        }
    }
    false
}

fn is_tightening(before: &Column, after: &Column) -> bool {
    // NULL -> NOT NULL: tightening.
    if before.nullable && !after.nullable {
        return true;
    }
    // Adding a CHECK where there wasn't one. (We don't introspect CHECKs,
    // so this fires whenever the source declares one — but that's harmless
    // noise; the warning is informational.)
    if before.check.is_none() && after.check.is_some() {
        return true;
    }
    false
}
