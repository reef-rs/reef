//! UI layer.
//!
//! Convention:
//!   - `mod.rs` = the page component for that folder's URL.
//!     - `src/app/mod.rs` is the `/` page (Home component below).
//!     - `src/app/users/mod.rs` would be the `/users` page.
//!     - `src/app/users/show/mod.rs` would be the `/users/show` page.
//!   - `layout.rs` — root layout (the `App` component, persistent across
//!     navigation, where document::Stylesheet etc. go).
//!   - `components/` — reusable UI primitives (buttons, cards, etc.).
//!
//! File path mirrors URL by convention, but Dioxus routing is driven by the
//! `Route` enum — moving a file doesn't change its URL.

pub mod components;
pub mod layout;

use dioxus::prelude::*;

use crate::api;
use crate::app::components::Splash;

const LOGO: Asset = asset!("/assets/logo.png");

/// `/` — home page.
#[component]
pub fn Home() -> Element {
    let status = use_resource(api::get_status);

    rsx! {
        Splash {
            logo: LOGO,
            status: status.read_unchecked().clone().map(|r| r.map_err(|e| e.to_string())),
        }
    }
}
