//! # Reef
//!
//! Runtime crate for [Reef](https://reef.rs) apps. Today this provides:
//!
//! - The `#[reef::table]` schema-as-code attribute (re-exported from `reef-macros`)
//! - [`Json`] and [`Jsonb`] newtypes for typed JSON columns
//!
//! Future versions will add `Db` helpers and Dioxus convenience re-exports.
//!
//! ## Schema-as-code
//!
//! ```ignore
//! use reef::{Json, Jsonb};
//!
//! #[reef::table]
//! pub struct User {
//!     #[column(primary_key)]
//!     pub id: i64,
//!     #[column(unique)]
//!     pub email: String,
//!     pub tags: Json<Vec<String>>,
//!     pub metadata: Jsonb<UserMetadata>,
//! }
//! ```

pub use reef_macros::table;

mod json;
pub use json::{Json, Jsonb};
