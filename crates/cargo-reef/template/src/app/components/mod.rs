//! Reusable UI components — pure renderers that take props.
//!
//! Components don't fetch their own data. The page component (`page.rs`)
//! orchestrates: calls server fns, passes results down as props.

mod splash;

pub use splash::Splash;
