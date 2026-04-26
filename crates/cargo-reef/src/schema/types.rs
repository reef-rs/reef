//! Rust type → SQL `ColumnType` mapping.
//!
//! Type-shape based, no semantic resolution:
//!
//! - Primitive numerics and `String`/`Vec<u8>` map directly.
//! - `Option<T>` unwraps to `T` and marks the column nullable.
//! - `Json<T>` / `Jsonb<T>` map to TEXT/BLOB respectively, preserving the
//!   inner type as a string for documentation / debug.
//! - Anything else errors with a "wrap in `Json<>` or `Jsonb<>`" suggestion.

use anyhow::{anyhow, bail, Result};
use quote::ToTokens;
use syn::{GenericArgument, PathArguments, Type, TypePath};

use super::ir::ColumnType;

/// Result of inspecting a field's Rust type.
pub struct TypeInfo {
    pub column_type: ColumnType,
    pub nullable: bool,
}

pub fn map_field_type(ty: &Type) -> Result<TypeInfo> {
    // Strip Option<T> first to set nullable, then map the inner T.
    if let Some(inner) = peel_generic(ty, "Option") {
        let column_type = map_inner(&inner)?;
        return Ok(TypeInfo {
            column_type,
            nullable: true,
        });
    }
    Ok(TypeInfo {
        column_type: map_inner(ty)?,
        nullable: false,
    })
}

fn map_inner(ty: &Type) -> Result<ColumnType> {
    // Vec<u8> is BLOB — special-case BEFORE generic Vec falls through.
    if let Some(inner) = peel_generic(ty, "Vec") {
        if type_is_named(&inner, "u8") {
            return Ok(ColumnType::Blob);
        }
        // Other Vec<T> requires explicit Json<>/Jsonb<> wrapper.
        return Err(unwrapped_collection_error(ty, "Vec"));
    }

    if let Some(inner) = peel_generic(ty, "Json") {
        return Ok(ColumnType::Json(type_to_string(&inner)));
    }
    if let Some(inner) = peel_generic(ty, "Jsonb") {
        return Ok(ColumnType::Jsonb(type_to_string(&inner)));
    }

    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(seg) = path.segments.last() {
            let name = seg.ident.to_string();
            if let Some(t) = primitive_for(&name) {
                return Ok(t);
            }
            // HashMap / BTreeMap / HashSet — collection types that should be JSON-wrapped.
            if matches!(
                name.as_str(),
                "HashMap" | "BTreeMap" | "HashSet" | "BTreeSet"
            ) {
                return Err(unwrapped_collection_error(ty, &name));
            }
        }
    }

    Err(anyhow!(
        "unsupported field type `{}`. Wrap in `reef::Json<>` (TEXT) or \
         `reef::Jsonb<>` (BLOB) to store as JSON, or use a recognized \
         primitive (i64, i32, u64, u32, f64, f32, bool, String, Vec<u8>).",
        type_to_string(ty)
    ))
}

fn primitive_for(name: &str) -> Option<ColumnType> {
    Some(match name {
        // Integer family — SQLite stores all in INTEGER affinity.
        "i64" | "i32" | "i16" | "i8" | "u64" | "u32" | "u16" | "u8" | "isize" | "usize"
        | "bool" => ColumnType::Integer,
        "f64" | "f32" => ColumnType::Real,
        "String" | "str" => ColumnType::Text,
        _ => return None,
    })
}

/// Match `Wrapper<Inner>` and return `Inner`.
///
/// Path matches the LAST segment by name — so `reef::Json<T>`, `Json<T>`,
/// and `crate::Json<T>` all match `peel_generic(ty, "Json")`.
fn peel_generic(ty: &Type, wrapper_name: &str) -> Option<Type> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return None;
    };
    let seg = path.segments.last()?;
    if seg.ident != wrapper_name {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    })
}

fn type_is_named(ty: &Type, name: &str) -> bool {
    matches!(
        ty,
        Type::Path(TypePath { path, .. })
            if path.segments.last().is_some_and(|s| s.ident == name)
    )
}

fn type_to_string(ty: &Type) -> String {
    ty.to_token_stream()
        .to_string()
        .replace(" < ", "<")
        .replace(" >", ">")
        .replace(" , ", ", ")
}

fn unwrapped_collection_error(ty: &Type, kind: &str) -> anyhow::Error {
    anyhow!(
        "field type `{}` — `{kind}<T>` cannot be stored directly. Wrap in \
         `reef::Json<>` (TEXT) or `reef::Jsonb<>` (BLOB), e.g. `Json<{}>`.",
        type_to_string(ty),
        type_to_string(ty)
    )
}

// Convenience: bail!-style helper isn't used, re-export anyhow's Result.
#[allow(dead_code)]
fn _ensure_imports() {
    let _: fn(_) -> Result<()> = |s: &str| bail!("{s}");
}
