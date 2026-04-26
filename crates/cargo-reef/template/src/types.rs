//! Shared wire types — referenced by both the WASM client and the native
//! server. Keep this minimal: pure data, derives, no logic, no framework
//! deps beyond serde.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub message: String,
    pub version: String,
    pub greeting: Option<String>,
}
