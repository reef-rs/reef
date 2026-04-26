//! Schema IR — the in-memory representation of a parsed `schema.rs`.
//!
//! Faithful to what a SQLite/libSQL `CREATE TABLE` and friends can express,
//! organized so the diff engine can compare two `Schema` values cheaply.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Schema {
    pub tables: Vec<Table>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Table {
    /// SQL table name (snake_case from struct name unless overridden via
    /// `#[reef::table(name = "...")]`).
    pub name: String,
    /// Original Rust struct name. Used for error messages.
    pub rust_name: String,
    pub strict: bool,
    pub without_rowid: bool,
    pub columns: Vec<Column>,
    /// Composite primary key declared at the table level. Single-column
    /// PKs live on the [`Column`] itself, not here.
    pub primary_key: Option<TablePrimaryKey>,
    pub indexes: Vec<Index>,
    /// Composite foreign keys declared at the table level. Single-column
    /// FKs live on the [`Column`].
    pub foreign_keys: Vec<TableForeignKey>,
    pub checks: Vec<TableCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Column {
    pub name: String,
    pub ty: ColumnType,
    /// Derived from `Option<T>` in the Rust source.
    pub nullable: bool,
    pub primary_key: bool,
    pub auto_increment: bool,
    pub unique: bool,
    pub default: Option<String>,
    pub check: Option<String>,
    pub references: Option<ColumnFk>,
    pub generated: Option<Generated>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "inner", rename_all = "snake_case")]
pub enum ColumnType {
    Integer,
    Text,
    Real,
    Blob,
    /// `Json<T>` — stored as TEXT. `inner` is the stringified `T` (for docs).
    Json(String),
    /// `Jsonb<T>` — stored as BLOB (SQLite 3.45+ JSONB encoding).
    Jsonb(String),
}

impl ColumnType {
    /// SQL type token for `CREATE TABLE`.
    pub fn sql(&self) -> &'static str {
        match self {
            ColumnType::Integer => "INTEGER",
            ColumnType::Text | ColumnType::Json(_) => "TEXT",
            ColumnType::Real => "REAL",
            ColumnType::Blob | ColumnType::Jsonb(_) => "BLOB",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColumnFk {
    pub table: String,
    pub column: String,
    pub on_delete: Option<FkAction>,
    pub on_update: Option<FkAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FkAction {
    Cascade,
    Restrict,
    SetNull,
    SetDefault,
    NoAction,
}

impl FkAction {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "cascade" => Self::Cascade,
            "restrict" => Self::Restrict,
            "set_null" => Self::SetNull,
            "set_default" => Self::SetDefault,
            "no_action" => Self::NoAction,
            _ => return None,
        })
    }

    /// SQL keyword for `ON DELETE` / `ON UPDATE`.
    pub fn sql(self) -> &'static str {
        match self {
            Self::Cascade => "CASCADE",
            Self::Restrict => "RESTRICT",
            Self::SetNull => "SET NULL",
            Self::SetDefault => "SET DEFAULT",
            Self::NoAction => "NO ACTION",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Generated {
    pub expr: String,
    pub kind: GeneratedKind,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedKind {
    Stored,
    Virtual,
}

#[derive(Debug, Clone, Serialize)]
pub struct TablePrimaryKey {
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Index {
    /// User-supplied or derived (`<table>_<col1>_<col2>_idx` if omitted).
    pub name: Option<String>,
    /// Indexed expressions. Plain identifiers OR SQL expressions
    /// (e.g. `json_extract(meta, '$.country')`).
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableForeignKey {
    pub columns: Vec<String>,
    pub references_table: String,
    pub references_columns: Vec<String>,
    pub on_delete: Option<FkAction>,
    pub on_update: Option<FkAction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableCheck {
    pub name: String,
    pub expr: String,
}
