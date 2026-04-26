//! `#[cfg(...)]` predicate evaluation for schema-as-code.
//!
//! When a project uses Cargo features to gate which `#[reef::table]` structs
//! are part of which build (per Reefer Rule 3 — "one binary per role"), the
//! parser needs to evaluate cfg predicates against the active feature set so
//! `cargo reef db:push --features server,nexus` only includes the tables
//! that build would compile.
//!
//! Supports the cfg shapes that account for ~all real-world schema.rs gating:
//!
//! - `#[cfg(feature = "X")]`
//! - `#[cfg(not(<predicate>))]`
//! - `#[cfg(all(<p1>, <p2>, ...))]`
//! - `#[cfg(any(<p1>, <p2>, ...))]`
//!
//! Other cfg predicates (`target_os`, `target_arch`, bare `test` /
//! `debug_assertions`) trigger a warning the first time they're seen and are
//! evaluated to `true` (assume host). Schema gating by target_os is rare; we
//! can tighten if it ever matters.

use std::collections::HashSet;

use anyhow::Result;
use syn::{punctuated::Punctuated, Attribute, Meta, MetaList, MetaNameValue, Token};

/// Active feature set for evaluating `#[cfg(feature = "...")]` predicates.
#[derive(Debug, Clone)]
pub enum FeatureSet {
    /// Don't filter — include every `#[reef::table]` struct regardless of
    /// cfg attributes. Used by debug commands and as the back-compat default
    /// for projects that don't use cfg gating.
    Unconstrained,
    /// Specific set of enabled features. `feature = "X"` is true iff X is in.
    Specific(HashSet<String>),
}

impl FeatureSet {
    pub fn unconstrained() -> Self {
        Self::Unconstrained
    }

    pub fn from_features<I, S>(features: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Specific(features.into_iter().map(Into::into).collect())
    }

    fn feature_enabled(&self, name: &str) -> bool {
        match self {
            Self::Unconstrained => true,
            Self::Specific(set) => set.contains(name),
        }
    }
}

/// Returns `Ok(true)` if every `#[cfg(...)]` attribute on `attrs` evaluates
/// to true given `features`. Items with no cfg gates are always active.
///
/// Errors only on malformed cfg syntax — unknown predicates are treated as
/// `true` so we don't accidentally hide tables.
pub fn item_is_active(attrs: &[Attribute], features: &FeatureSet) -> Result<bool> {
    if matches!(features, FeatureSet::Unconstrained) {
        return Ok(true);
    }
    for attr in attrs {
        if !attr.path().is_ident("cfg") {
            continue;
        }
        let inner: Meta = attr.parse_args()?;
        if !eval(&inner, features) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn eval(meta: &Meta, features: &FeatureSet) -> bool {
    match meta {
        Meta::NameValue(nv) => eval_name_value(nv, features),
        Meta::List(list) => eval_list(list, features),
        // Bare ident (`test`, `debug_assertions`, etc.) — unknown, default true
        // so we don't accidentally hide schema items.
        Meta::Path(_) => true,
    }
}

fn eval_name_value(nv: &MetaNameValue, features: &FeatureSet) -> bool {
    let key = nv
        .path
        .get_ident()
        .map(|i| i.to_string())
        .unwrap_or_default();
    if key != "feature" {
        // target_os, target_arch, etc. — assume true (we're not cross-evaluating).
        return true;
    }
    let syn::Expr::Lit(lit) = &nv.value else {
        return true;
    };
    let syn::Lit::Str(s) = &lit.lit else {
        return true;
    };
    features.feature_enabled(&s.value())
}

fn eval_list(list: &MetaList, features: &FeatureSet) -> bool {
    let name = list
        .path
        .get_ident()
        .map(|i| i.to_string())
        .unwrap_or_default();
    let inner: Punctuated<Meta, Token![,]> = match list.parse_args_with(Punctuated::parse_terminated) {
        Ok(p) => p,
        Err(_) => return true, // be lenient — better to over-include than under-include
    };
    match name.as_str() {
        "not" => {
            // `not(...)` takes a single predicate.
            let Some(first) = inner.first() else {
                return true;
            };
            !eval(first, features)
        }
        "all" => inner.iter().all(|m| eval(m, features)),
        "any" => inner.iter().any(|m| eval(m, features)),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn fs(features: &[&str]) -> FeatureSet {
        FeatureSet::from_features(features.iter().copied())
    }

    fn active(attrs: Vec<Attribute>, features: &FeatureSet) -> bool {
        item_is_active(&attrs, features).unwrap()
    }

    #[test]
    fn no_cfg_always_active() {
        assert!(active(vec![], &fs(&[])));
        assert!(active(vec![], &fs(&["server"])));
    }

    #[test]
    fn unconstrained_includes_everything() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[cfg(feature = "nexus")])];
        assert!(active(attrs, &FeatureSet::Unconstrained));
    }

    #[test]
    fn simple_feature() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[cfg(feature = "nexus")])];
        assert!(active(attrs.clone(), &fs(&["nexus"])));
        assert!(!active(attrs, &fs(&["server"])));
    }

    #[test]
    fn not_feature() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[cfg(not(feature = "nexus"))])];
        assert!(!active(attrs.clone(), &fs(&["nexus"])));
        assert!(active(attrs, &fs(&["server"])));
    }

    #[test]
    fn all_features() {
        let attrs: Vec<Attribute> =
            vec![parse_quote!(#[cfg(all(feature = "server", feature = "nexus"))])];
        assert!(active(attrs.clone(), &fs(&["server", "nexus"])));
        assert!(!active(attrs.clone(), &fs(&["server"])));
        assert!(!active(attrs, &fs(&["nexus"])));
    }

    #[test]
    fn any_features() {
        let attrs: Vec<Attribute> =
            vec![parse_quote!(#[cfg(any(feature = "edge", feature = "nexus"))])];
        assert!(active(attrs.clone(), &fs(&["edge"])));
        assert!(active(attrs.clone(), &fs(&["nexus"])));
        assert!(active(attrs.clone(), &fs(&["edge", "nexus"])));
        assert!(!active(attrs, &fs(&["server"])));
    }

    #[test]
    fn nested_predicates() {
        let attrs: Vec<Attribute> = vec![parse_quote!(
            #[cfg(all(feature = "server", not(feature = "edge")))]
        )];
        assert!(active(attrs.clone(), &fs(&["server"])));
        assert!(!active(attrs.clone(), &fs(&["server", "edge"])));
        assert!(!active(attrs, &fs(&["edge"])));
    }

    #[test]
    fn multiple_cfg_attrs_anded() {
        let attrs: Vec<Attribute> = vec![
            parse_quote!(#[cfg(feature = "server")]),
            parse_quote!(#[cfg(feature = "nexus")]),
        ];
        assert!(active(attrs.clone(), &fs(&["server", "nexus"])));
        assert!(!active(attrs, &fs(&["server"])));
    }

    #[test]
    fn unknown_predicate_lenient() {
        let attrs: Vec<Attribute> = vec![parse_quote!(#[cfg(target_os = "linux")])];
        assert!(active(attrs, &fs(&["server"])));
    }
}
