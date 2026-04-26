//! Reef build script.
//!
//! Cargo automatically runs this before compiling the crate. Today it does
//! nothing — `routes.rs` is hand-written for v0.1.
//!
//! ## v0.5 plan: filesystem-driven routes
//!
//! In v0.5 of Reef, this build script will scan `src/app/` and auto-generate
//! `src/routes.rs` on every build. The user will never edit `routes.rs` by
//! hand — they'll just create files under `src/app/` and the route enum
//! gets regenerated automatically.
//!
//! The shape will be roughly:
//!
//! ```rust,ignore
//! fn main() {
//!     // Re-run this script whenever anything under src/app/ changes.
//!     // Cargo picks this up natively — no proc-macro foot-guns.
//!     println!("cargo:rerun-if-changed=src/app");
//!
//!     // Delegate the actual scanning + codegen to the cargo-reef crate
//!     // so the logic ships via dependency updates, not user-maintained code.
//!     cargo_reef::generate_routes("src/app", "src/routes.rs")
//!         .expect("failed to generate routes from src/app/");
//! }
//! ```
//!
//! The conventions `cargo-reef` will recognize when scanning `src/app/`:
//!
//! - **`src/app/mod.rs`** → the `/` page (Home component inside)
//! - **`src/app/<name>/mod.rs`** → `/<name>` page
//! - **`src/app/<name>/<sub>/mod.rs`** → `/<name>/<sub>` page
//! - **`src/app/<name>/layout.rs`** → sub-layout wrapping `<name>/*` routes
//! - **`src/app/_id/mod.rs`** → dynamic segment, becomes `/:id` (underscore
//!   prefix is the Rust-friendly equivalent of Next.js's `[id]` since
//!   bracket directory names aren't valid Rust module identifiers)
//! - **`src/app/__slug/mod.rs`** → catch-all, becomes `/*slug` (double
//!   underscore = catch-all)
//! - **`src/app/_group_/foo/mod.rs`** → route group, `_group_` is dropped
//!   from the URL (just `/foo`) — used for layout grouping without affecting URLs
//!
//! ## Why a build.rs and not a proc macro
//!
//! Build scripts have first-class support for filesystem watching via
//! `cargo:rerun-if-changed`. Proc macros don't — they can read the
//! filesystem at compile time, but Cargo doesn't track those reads, so
//! adding a new file in `src/app/` wouldn't trigger the macro to re-run.
//!
//! This is the same reason `prost`, `cxx`, and similar crates use build
//! scripts for codegen instead of macros.

fn main() {
    // Placeholder. v0.5 will populate this with the route generator.
}
