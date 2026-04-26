//! Proc-macro impls for the `reef` crate.
//!
//! `#[reef::table]` is an outer attribute macro applied to a struct. It does
//! two jobs:
//!
//! 1. **Strips our marker sub-attributes** before re-emitting the struct, so
//!    the compiler doesn't choke on attributes it doesn't know about. The
//!    sub-attributes are: `#[column(...)]` on fields, and `#[index(...)]`,
//!    `#[primary_key(...)]`, `#[foreign_key(...)]`, `#[check(...)]` on the
//!    struct.
//! 2. **Validates** that those sub-attributes (and the macro's own args) use
//!    known keys, failing fast with a clear error pointing at the bad span.
//!
//! It does NOT generate any code today. The struct is re-emitted essentially
//! unchanged. `cargo reef db:push` parses the original `schema.rs` source
//! file with `syn` separately and reads the (still-present) attributes there.
//!
//! Users don't depend on this crate directly — it's re-exported by `reef`.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, spanned::Spanned, Attribute, Fields, ItemStruct};

/// Mark a struct as a SQL table.
///
/// # Example
///
/// ```ignore
/// #[reef::table(strict)]
/// #[index(name = "users_email_idx", columns = ["email"])]
/// pub struct User {
///     #[column(primary_key, auto_increment)]
///     pub id: i64,
///     #[column(unique)]
///     pub email: String,
///     pub name: String,
/// }
///
/// #[reef::table]
/// #[primary_key(columns = ["user_id", "post_id"])]
/// pub struct PostLike {
///     #[column(references = "users(id)", on_delete = "cascade")]
///     pub user_id: i64,
///     #[column(references = "posts(id)", on_delete = "cascade")]
///     pub post_id: i64,
/// }
/// ```
///
/// # Macro arguments — `#[reef::table(...)]`
///
/// - `name = "<sql_table_name>"` — override the default snake_case derivation
/// - `strict` — emit as a SQLite STRICT table (3.37+)
/// - `without_rowid` — emit as a WITHOUT ROWID table (perf optimization for
///   tables with non-INTEGER primary keys)
///
/// # Field-level — `#[column(...)]`
///
/// - `primary_key` — single-column PK (use `#[primary_key(columns = [...])]`
///   at the struct level for composite PKs)
/// - `auto_increment` — emit `AUTOINCREMENT` (only valid with INTEGER PK)
/// - `unique`
/// - `default = <expr>` — Rust literal. String literals get SQL-quoted
///   (`default = "active"` → `DEFAULT 'active'`); numerics/bools emit raw.
/// - `default_sql = "<sql>"` — verbatim SQL passthrough for function calls
///   like `default_sql = "datetime('now')"` (use this when you need
///   `DEFAULT (datetime('now'))` rather than a quoted string literal).
/// - `check = <expr>` — SQL CHECK constraint scoped to this column
/// - `references = "<table>(<column>)"` — single-column FK target
/// - `on_delete = "cascade" | "restrict" | "set_null" | "set_default" | "no_action"`
/// - `on_update = "..."` — same options as `on_delete`
/// - `generated = "<sql_expr>"` — `GENERATED ALWAYS AS (<expr>)`
/// - `generated_kind = "stored" | "virtual"` — defaults to "virtual"
///
/// # Struct-level — helper attributes
///
/// - `#[index(name = "...", columns = [...], unique)]` — single or multi-column index
/// - `#[primary_key(columns = [...])]` — composite primary key
/// - `#[foreign_key(columns = [...], references = "<table>(<col>, <col>, ...)", on_delete = "...", on_update = "...")]`
///   — composite foreign key
/// - `#[check(name = "...", expr = "...")]` — named table-level CHECK
#[proc_macro_attribute]
pub fn table(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Validate macro args (#[reef::table(strict, name = "...")]).
    let attr2: proc_macro2::TokenStream = attr.into();
    if !attr2.is_empty() {
        if let Err(e) = parse_table_args(attr2) {
            return e.to_compile_error().into();
        }
    }

    let mut item_struct = parse_macro_input!(item as ItemStruct);

    // Validate + strip struct-level helper attrs.
    for (marker, allowed) in STRUCT_HELPERS {
        if let Err(e) = validate_attrs(&item_struct.attrs, marker, allowed) {
            return e.to_compile_error().into();
        }
    }
    item_struct
        .attrs
        .retain(|a| !STRUCT_HELPERS.iter().any(|(m, _)| is_marker(a, m)));

    // Validate + strip field-level #[column(...)].
    if let Fields::Named(fields) = &mut item_struct.fields {
        for field in &mut fields.named {
            if let Err(e) = validate_attrs(&field.attrs, "column", COLUMN_KEYS) {
                return e.to_compile_error().into();
            }
            field.attrs.retain(|a| !is_marker(a, "column"));
        }
    }

    quote! { #item_struct }.into()
}

const COLUMN_KEYS: &[&str] = &[
    "primary_key",
    "auto_increment",
    "unique",
    "default",
    "default_sql",
    "check",
    "references",
    "on_delete",
    "on_update",
    "generated",
    "generated_kind",
];

const INDEX_KEYS: &[&str] = &["name", "columns", "unique"];
const PRIMARY_KEY_KEYS: &[&str] = &["columns"];
const FOREIGN_KEY_KEYS: &[&str] = &["columns", "references", "on_delete", "on_update"];
const CHECK_KEYS: &[&str] = &["name", "expr"];

const STRUCT_HELPERS: &[(&str, &[&str])] = &[
    ("index", INDEX_KEYS),
    ("primary_key", PRIMARY_KEY_KEYS),
    ("foreign_key", FOREIGN_KEY_KEYS),
    ("check", CHECK_KEYS),
];

const TABLE_ARG_KEYS: &[&str] = &["name", "strict", "without_rowid"];

fn is_marker(attr: &Attribute, name: &str) -> bool {
    attr.path().is_ident(name)
}

/// Walk attrs named `marker_name`, parse their meta items, and ensure each
/// key is in `allowed`. Errors point at the offending span.
fn validate_attrs(attrs: &[Attribute], marker_name: &str, allowed: &[&str]) -> syn::Result<()> {
    for attr in attrs.iter().filter(|a| is_marker(a, marker_name)) {
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            if !allowed.contains(&key.as_str()) {
                return Err(syn::Error::new(
                    meta.path.span(),
                    format!(
                        "unknown `#[{marker_name}]` key `{key}`. Allowed: {}",
                        allowed.join(", ")
                    ),
                ));
            }
            // Consume associated value (`= expr` or list) if present.
            if meta.input.peek(syn::Token![=]) {
                let _: syn::Expr = meta.value()?.parse()?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// Validate `#[reef::table(strict, name = "users")]` style args.
fn parse_table_args(tokens: proc_macro2::TokenStream) -> syn::Result<()> {
    let parser = syn::meta::parser(|meta| {
        let key = meta
            .path
            .get_ident()
            .map(|i| i.to_string())
            .unwrap_or_default();
        if !TABLE_ARG_KEYS.contains(&key.as_str()) {
            return Err(syn::Error::new(
                meta.path.span(),
                format!(
                    "unknown `#[reef::table]` arg `{key}`. Allowed: {}",
                    TABLE_ARG_KEYS.join(", ")
                ),
            ));
        }
        if meta.input.peek(syn::Token![=]) {
            let _: syn::Expr = meta.value()?.parse()?;
        }
        Ok(())
    });
    syn::parse::Parser::parse2(parser, tokens)
}
