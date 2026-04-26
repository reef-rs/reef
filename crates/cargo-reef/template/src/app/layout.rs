//! Root layout — `RootLayout` component.
//!
//! Equivalent to Next.js's `app/layout.tsx`. This wraps every route in the
//! app: holds document-wide elements (stylesheets, favicon, title) plus
//! `Outlet::<Route> {}`, which is where the matched page renders — same
//! role as `{children}` in a Next.js layout.
//!
//! Wired up via `#[layout(RootLayout)]` in `src/routes.rs`. Persistent
//! across navigation — only the `Outlet` content swaps when you change
//! routes.
//!
//! ## Adding a sub-layout
//!
//! For a layout that wraps a SUBSET of routes (e.g., a sidebar around the
//! dashboard but not the marketing pages — like Next's
//! `app/dashboard/layout.tsx`):
//!
//! 1. Create `src/app/<path>/layout.rs` with another component that has
//!    `Outlet::<Route> {}` inside.
//! 2. Add `#[layout(YourSubLayout)]` to the appropriate group in
//!    `src/routes.rs` — the inner layout wraps the routes nested under it.

use dioxus::prelude::*;

use crate::Route;

// `asset!()` gives a content-hashed URL — defeats Chrome's aggressive favicon
// cache, which otherwise serves a stale (or missing) icon for the lifetime of
// the tab. Direct `/favicon.png` works too but loses cache-busting.
const FAVICON: Asset = asset!("/assets/favicon.png");
const TAILWIND: Asset = asset!("/assets/tailwind.css");
const MAIN_CSS: Asset = asset!("/assets/main.css");

#[component]
pub fn RootLayout() -> Element {
    rsx! {
        // Document head — persistent across navigation
        document::Title { "Reef" }
        document::Link {
            rel: "icon",
            r#type: "image/png",
            sizes: "32x32",
            href: FAVICON,
        }
        document::Stylesheet { href: TAILWIND }
        document::Stylesheet { href: MAIN_CSS }

        // The matched page renders here (Next.js's `{children}` equivalent)
        Outlet::<Route> {}
    }
}
