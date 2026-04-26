//! `syn`-based schema.rs parser.
//!
//! Reads the file, walks top-level items, picks out structs marked with
//! `#[reef::table]` (or `#[table]` after `use reef::table`), and turns each
//! into a [`Table`] in the [`Schema`] IR.

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use quote::ToTokens;
use syn::{
    spanned::Spanned, Attribute, Expr, ExprArray, ExprLit, Field, Item, ItemStruct, Lit, LitStr,
    Meta,
};

use super::cfg::{item_is_active, FeatureSet};
use super::ir::{
    Column, ColumnFk, FkAction, Generated, GeneratedKind, Index, Schema, Table, TableCheck,
    TableForeignKey, TablePrimaryKey,
};
use super::types::map_field_type;

pub fn parse_file(path: &Path, features: &FeatureSet) -> Result<Schema> {
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let file = syn::parse_file(&src)
        .with_context(|| format!("parsing {} as Rust", path.display()))?;

    let mut tables = Vec::new();
    for item in &file.items {
        if let Item::Struct(s) = item {
            if !has_marker(&s.attrs, "table") {
                continue;
            }
            // Respect cfg-gating so multi-deployment projects (one schema.rs,
            // different binaries via features) get the right schema view.
            if !item_is_active(&s.attrs, features).with_context(|| {
                format!("evaluating cfg on struct `{}`", s.ident)
            })? {
                continue;
            }
            let table = parse_table(s).with_context(|| {
                format!("parsing struct `{}` as a #[reef::table]", s.ident)
            })?;
            tables.push(table);
        }
    }

    let schema = Schema { tables };
    validate_cross_refs(&schema)?;
    Ok(schema)
}

/// Cross-table validation pass — ensures every FK reference points at a
/// table and column(s) that actually exist in the schema. Catches typos
/// like leftover plural names after renaming a struct.
fn validate_cross_refs(schema: &Schema) -> Result<()> {
    use std::collections::HashMap;

    let table_cols: HashMap<&str, std::collections::HashSet<&str>> = schema
        .tables
        .iter()
        .map(|t| {
            (
                t.name.as_str(),
                t.columns.iter().map(|c| c.name.as_str()).collect(),
            )
        })
        .collect();

    let table_names: Vec<&str> = table_cols.keys().copied().collect();

    for t in &schema.tables {
        for col in &t.columns {
            if let Some(fk) = &col.references {
                check_fk_target(
                    &table_cols,
                    &table_names,
                    &fk.table,
                    std::slice::from_ref(&fk.column),
                    &format!("{}.{} `references = \"{}({})\"`", t.rust_name, col.name, fk.table, fk.column),
                )?;
            }
        }
        for fk in &t.foreign_keys {
            check_fk_target(
                &table_cols,
                &table_names,
                &fk.references_table,
                &fk.references_columns,
                &format!(
                    "{} `#[foreign_key(columns = [{}], references = \"{}({})\")]`",
                    t.rust_name,
                    fk.columns.join(", "),
                    fk.references_table,
                    fk.references_columns.join(", ")
                ),
            )?;
        }
    }
    Ok(())
}

fn check_fk_target(
    table_cols: &std::collections::HashMap<&str, std::collections::HashSet<&str>>,
    table_names: &[&str],
    target_table: &str,
    target_columns: &[String],
    context: &str,
) -> Result<()> {
    let Some(cols) = table_cols.get(target_table) else {
        let suggestion = closest_match(target_table, table_names)
            .map(|m| format!(" (did you mean `{m}`?)"))
            .unwrap_or_default();
        bail!(
            "{context}: target table `{target_table}` does not exist in this schema{suggestion}"
        );
    };
    for c in target_columns {
        if !cols.contains(c.as_str()) {
            let known: Vec<&str> = cols.iter().copied().collect();
            let suggestion = closest_match(c, &known)
                .map(|m| format!(" (did you mean `{m}`?)"))
                .unwrap_or_default();
            bail!(
                "{context}: column `{c}` does not exist on table `{target_table}`{suggestion}"
            );
        }
    }
    Ok(())
}

/// Tiny edit-distance suggester. Returns Some(name) if any candidate is
/// within 2 character edits — enough to catch the common rename misses
/// (`users` vs `user`, `id` vs `ids`) without false positives.
fn closest_match<'a>(target: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let mut best: Option<(usize, &'a str)> = None;
    for c in candidates {
        let d = edit_distance(target, c);
        if d <= 2 && best.map_or(true, |(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (cur[j] + 1)
                .min(prev[j + 1] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn parse_table(s: &ItemStruct) -> Result<Table> {
    let rust_name = s.ident.to_string();
    let mut name = snake_case(&rust_name);
    let mut strict = false;
    let mut without_rowid = false;

    // #[reef::table(name = "...", strict, without_rowid)] args
    for attr in s.attrs.iter().filter(|a| is_marker_attr(a, "table")) {
        if matches!(attr.meta, Meta::Path(_)) {
            continue; // bare `#[reef::table]` with no args
        }
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            match key.as_str() {
                "name" => {
                    let v: LitStr = meta.value()?.parse()?;
                    name = v.value();
                }
                "strict" => strict = true,
                "without_rowid" => without_rowid = true,
                other => return Err(meta.error(format!("unknown table arg `{other}`"))),
            }
            Ok(())
        })?;
    }

    // Field-level columns
    let mut columns = Vec::new();
    let syn::Fields::Named(named) = &s.fields else {
        bail!("`#[reef::table]` requires a struct with named fields");
    };
    for field in &named.named {
        columns.push(parse_column(field).with_context(|| {
            format!(
                "field `{}`",
                field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default()
            )
        })?);
    }
    if columns.is_empty() {
        bail!("`#[reef::table]` requires at least one field");
    }

    // Struct-level helper attrs
    let mut primary_key = None;
    let mut indexes = Vec::new();
    let mut foreign_keys = Vec::new();
    let mut checks = Vec::new();

    for attr in &s.attrs {
        if is_marker_attr(attr, "primary_key") {
            if primary_key.is_some() {
                bail!("multiple `#[primary_key(...)]` attributes — only one composite PK is allowed");
            }
            primary_key = Some(parse_primary_key(attr)?);
        } else if is_marker_attr(attr, "index") {
            indexes.push(parse_index(attr)?);
        } else if is_marker_attr(attr, "foreign_key") {
            foreign_keys.push(parse_foreign_key(attr)?);
        } else if is_marker_attr(attr, "check") {
            checks.push(parse_check(attr)?);
        }
    }

    Ok(Table {
        name,
        rust_name,
        strict,
        without_rowid,
        columns,
        primary_key,
        indexes,
        foreign_keys,
        checks,
    })
}

fn parse_column(field: &Field) -> Result<Column> {
    let name = field
        .ident
        .as_ref()
        .ok_or_else(|| anyhow!("tuple-struct fields are not supported"))?
        .to_string();

    let info = map_field_type(&field.ty)?;

    let mut col = Column {
        name,
        ty: info.column_type,
        nullable: info.nullable,
        primary_key: false,
        auto_increment: false,
        unique: false,
        default: None,
        check: None,
        references: None,
        generated: None,
    };

    let mut on_delete: Option<FkAction> = None;
    let mut on_update: Option<FkAction> = None;
    let mut generated_expr: Option<String> = None;
    let mut generated_kind: Option<GeneratedKind> = None;

    for attr in field.attrs.iter().filter(|a| is_marker_attr(a, "column")) {
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            match key.as_str() {
                "primary_key" => col.primary_key = true,
                "auto_increment" => col.auto_increment = true,
                "unique" => col.unique = true,
                "default" => {
                    if col.default.is_some() {
                        return Err(meta.error("`default` and `default_sql` are mutually exclusive"));
                    }
                    let v: Expr = meta.value()?.parse()?;
                    col.default = Some(expr_to_sql_literal(&v));
                }
                "default_sql" => {
                    if col.default.is_some() {
                        return Err(meta.error("`default` and `default_sql` are mutually exclusive"));
                    }
                    // Verbatim SQL — for function defaults like
                    // `datetime('now')` that aren't expressible as Rust
                    // literals. We wrap in parens so SQLite parses it as
                    // an expression rather than a literal.
                    let v: LitStr = meta.value()?.parse()?;
                    col.default = Some(format!("({})", v.value()));
                }
                "check" => {
                    let v: LitStr = meta.value()?.parse()?;
                    col.check = Some(v.value());
                }
                "references" => {
                    let v: LitStr = meta.value()?.parse()?;
                    let (table, column) = parse_single_fk_target(&v.value())
                        .map_err(|e| meta.error(e.to_string()))?;
                    col.references = Some(ColumnFk {
                        table,
                        column,
                        on_delete: None,
                        on_update: None,
                    });
                }
                "on_delete" => {
                    let v: LitStr = meta.value()?.parse()?;
                    on_delete = Some(
                        FkAction::parse(&v.value())
                            .ok_or_else(|| meta.error("invalid on_delete value"))?,
                    );
                }
                "on_update" => {
                    let v: LitStr = meta.value()?.parse()?;
                    on_update = Some(
                        FkAction::parse(&v.value())
                            .ok_or_else(|| meta.error("invalid on_update value"))?,
                    );
                }
                "generated" => {
                    let v: LitStr = meta.value()?.parse()?;
                    generated_expr = Some(v.value());
                }
                "generated_kind" => {
                    let v: LitStr = meta.value()?.parse()?;
                    generated_kind = Some(match v.value().as_str() {
                        "stored" => GeneratedKind::Stored,
                        "virtual" => GeneratedKind::Virtual,
                        _ => return Err(meta.error("generated_kind must be 'stored' or 'virtual'")),
                    });
                }
                other => return Err(meta.error(format!("unknown column key `{other}`"))),
            }
            Ok(())
        })?;
    }

    if let Some(fk) = col.references.as_mut() {
        fk.on_delete = on_delete;
        fk.on_update = on_update;
    } else if on_delete.is_some() || on_update.is_some() {
        bail!("on_delete/on_update set without `references`");
    }

    if let Some(expr) = generated_expr {
        col.generated = Some(Generated {
            expr,
            kind: generated_kind.unwrap_or(GeneratedKind::Virtual),
        });
    } else if generated_kind.is_some() {
        bail!("`generated_kind` set without `generated`");
    }

    Ok(col)
}

fn parse_primary_key(attr: &Attribute) -> Result<TablePrimaryKey> {
    let mut columns = Vec::new();
    attr.parse_nested_meta(|meta| {
        match ident_str(&meta.path).as_deref() {
            Some("columns") => {
                columns = parse_string_array(&meta.value()?.parse::<Expr>()?)?;
                Ok(())
            }
            _ => Err(meta.error("only `columns = [...]` is allowed")),
        }
    })?;
    if columns.is_empty() {
        bail!("`#[primary_key]` requires `columns = [...]`");
    }
    Ok(TablePrimaryKey { columns })
}

fn parse_index(attr: &Attribute) -> Result<Index> {
    let mut name = None;
    let mut columns = Vec::new();
    let mut unique = false;
    attr.parse_nested_meta(|meta| {
        match ident_str(&meta.path).as_deref() {
            Some("name") => {
                let v: LitStr = meta.value()?.parse()?;
                name = Some(v.value());
            }
            Some("columns") => {
                columns = parse_string_array(&meta.value()?.parse::<Expr>()?)?;
            }
            Some("unique") => unique = true,
            _ => return Err(meta.error("unknown index key")),
        }
        Ok(())
    })?;
    if columns.is_empty() {
        bail!("`#[index]` requires `columns = [...]`");
    }
    Ok(Index {
        name,
        columns,
        unique,
    })
}

fn parse_foreign_key(attr: &Attribute) -> Result<TableForeignKey> {
    let mut columns = Vec::new();
    let mut references = String::new();
    let mut on_delete = None;
    let mut on_update = None;
    attr.parse_nested_meta(|meta| {
        match ident_str(&meta.path).as_deref() {
            Some("columns") => {
                columns = parse_string_array(&meta.value()?.parse::<Expr>()?)?;
            }
            Some("references") => {
                let v: LitStr = meta.value()?.parse()?;
                references = v.value();
            }
            Some("on_delete") => {
                let v: LitStr = meta.value()?.parse()?;
                on_delete = Some(
                    FkAction::parse(&v.value())
                        .ok_or_else(|| meta.error("invalid on_delete"))?,
                );
            }
            Some("on_update") => {
                let v: LitStr = meta.value()?.parse()?;
                on_update = Some(
                    FkAction::parse(&v.value())
                        .ok_or_else(|| meta.error("invalid on_update"))?,
                );
            }
            _ => return Err(meta.error("unknown foreign_key key")),
        }
        Ok(())
    })?;
    if columns.is_empty() {
        bail!("`#[foreign_key]` requires `columns = [...]`");
    }
    if references.is_empty() {
        bail!("`#[foreign_key]` requires `references = \"table(c1, c2)\"`");
    }
    let (references_table, references_columns) = parse_composite_fk_target(&references)?;
    if references_columns.len() != columns.len() {
        bail!(
            "foreign_key column count mismatch: {} local, {} referenced",
            columns.len(),
            references_columns.len()
        );
    }
    Ok(TableForeignKey {
        columns,
        references_table,
        references_columns,
        on_delete,
        on_update,
    })
}

fn parse_check(attr: &Attribute) -> Result<TableCheck> {
    let mut name = String::new();
    let mut expr = String::new();
    attr.parse_nested_meta(|meta| {
        match ident_str(&meta.path).as_deref() {
            Some("name") => {
                let v: LitStr = meta.value()?.parse()?;
                name = v.value();
            }
            Some("expr") => {
                let v: LitStr = meta.value()?.parse()?;
                expr = v.value();
            }
            _ => return Err(meta.error("unknown check key")),
        }
        Ok(())
    })?;
    if name.is_empty() || expr.is_empty() {
        bail!("`#[check]` requires both `name` and `expr`");
    }
    Ok(TableCheck { name, expr })
}

// ============================================================================
//  small helpers
// ============================================================================

fn has_marker(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|a| is_marker_attr(a, name))
}

/// Match `#[<name>...]`, `#[reef::<name>...]`. Match by last path segment.
fn is_marker_attr(attr: &Attribute, name: &str) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|s| s.ident == name)
}

fn ident_str(p: &syn::Path) -> Option<String> {
    p.get_ident().map(|i| i.to_string())
}

fn parse_string_array(expr: &Expr) -> syn::Result<Vec<String>> {
    let Expr::Array(ExprArray { elems, .. }) = expr else {
        return Err(syn::Error::new(expr.span(), "expected `[...]` array"));
    };
    elems
        .iter()
        .map(|e| match e {
            Expr::Lit(ExprLit {
                lit: Lit::Str(s), ..
            }) => Ok(s.value()),
            _ => Err(syn::Error::new(e.span(), "expected a string literal")),
        })
        .collect()
}

/// Parse `"users(id)"` → `("users", "id")`.
fn parse_single_fk_target(s: &str) -> Result<(String, String)> {
    let (table, rest) = s
        .split_once('(')
        .ok_or_else(|| anyhow!("references must be `table(column)`, got `{s}`"))?;
    let column = rest
        .strip_suffix(')')
        .ok_or_else(|| anyhow!("references missing closing `)`"))?;
    let cols: Vec<&str> = column.split(',').map(str::trim).collect();
    if cols.len() != 1 {
        bail!("single-column FK on a column may only reference one column; use `#[foreign_key(...)]` for composite FKs");
    }
    Ok((table.trim().to_string(), cols[0].to_string()))
}

/// Parse `"users(id, tenant_id)"` → `("users", vec!["id", "tenant_id"])`.
fn parse_composite_fk_target(s: &str) -> Result<(String, Vec<String>)> {
    let (table, rest) = s
        .split_once('(')
        .ok_or_else(|| anyhow!("references must be `table(c1, c2)`, got `{s}`"))?;
    let cols = rest
        .strip_suffix(')')
        .ok_or_else(|| anyhow!("references missing closing `)`"))?;
    let columns: Vec<String> = cols.split(',').map(|c| c.trim().to_string()).collect();
    Ok((table.trim().to_string(), columns))
}

/// Render any expr as a SQL literal. String literals lose their Rust quotes
/// and become SQL `'...'` literals; numeric/bool/other exprs render as-is.
fn expr_to_sql_literal(e: &Expr) -> String {
    if let Expr::Lit(ExprLit {
        lit: Lit::Str(s), ..
    }) = e
    {
        format!("'{}'", s.value().replace('\'', "''"))
    } else {
        expr_string(e)
    }
}

fn expr_string(e: &Expr) -> String {
    e.to_token_stream().to_string()
}

/// Convert a Rust struct identifier to a SQL table name. PascalCase → snake_case.
/// Does NOT pluralize — `User` → `user`, `PostLike` → `post_like`. Users who
/// want plural names declare them explicitly via `#[reef::table(name = "users")]`.
/// Avoids the irregular-plural problem entirely (Box→boxes, Person→people, etc.)
/// without pulling in a pluralization dep.
fn snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
