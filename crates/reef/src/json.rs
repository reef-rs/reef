//! Typed JSON column wrappers.
//!
//! - [`Json<T>`] stores as TEXT (human-readable JSON)
//! - [`Jsonb<T>`] stores as BLOB (SQLite 3.45+ binary JSONB encoding)
//!
//! Both newtypes are `#[serde(transparent)]`, so when used in RPC return
//! types they serialize as the inner `T` directly — no extra wrapping in
//! the wire format.
//!
//! `cargo reef db:push` recognizes these wrappers in schema field types and
//! emits the corresponding SQL column type (`TEXT` or `BLOB`).
//!
//! SQL marshaling helpers (libsql `FromValue` / `IntoValue`) are not yet
//! implemented — write your queries with `serde_json::to_string` / `from_str`
//! against the inner `T` for now.

use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

/// Wrap a serde-serializable value to mark a column as JSON-stored TEXT.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Json<T>(pub T);

/// Wrap a serde-serializable value to mark a column as JSONB-stored BLOB.
///
/// Requires SQLite 3.45+ (libSQL inherits this). Faster to parse than
/// [`Json<T>`] at the cost of opacity in the `sqlite3` shell.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Jsonb<T>(pub T);

macro_rules! impl_wrapper {
    ($name:ident) => {
        impl<T> $name<T> {
            pub fn new(inner: T) -> Self {
                Self(inner)
            }
            pub fn into_inner(self) -> T {
                self.0
            }
        }
        impl<T> Deref for $name<T> {
            type Target = T;
            fn deref(&self) -> &T {
                &self.0
            }
        }
        impl<T> DerefMut for $name<T> {
            fn deref_mut(&mut self) -> &mut T {
                &mut self.0
            }
        }
        impl<T> From<T> for $name<T> {
            fn from(inner: T) -> Self {
                Self(inner)
            }
        }
    };
}

impl_wrapper!(Json);
impl_wrapper!(Jsonb);
