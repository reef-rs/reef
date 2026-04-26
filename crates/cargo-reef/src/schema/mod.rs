//! Schema-as-code parser.
//!
//! Reads `src/server/db/schema.rs` (or any path the user gives us) with `syn`
//! and builds a [`Schema`] IR that downstream code (`db:push`) diffs against
//! the live database.
//!
//! The parser only sees the AST — it does NOT do type resolution. Field types
//! are matched by syntactic shape (last path segment + generics), not by
//! semantic identity. Anything outside the recognized primitive table or the
//! `Json<T>` / `Jsonb<T>` wrappers errors with a "wrap in Json<>/Jsonb<>"
//! suggestion.

mod diff;
mod emit;
mod introspect;
mod ir;
mod parse;
mod render;
mod types;

pub use diff::{diff, Action};
pub use emit::{emit_action, emit_schema};
pub use introspect::introspect_db;
pub use ir::Schema;
pub use parse::parse_file;
pub use render::render_diff;
