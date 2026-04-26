//! Middleware — request gates of two flavors.
//!
//! Dioxus and Next.js both use the word "middleware" but for different things.
//! Both apply, both live here:
//!
//! ## 1. Route matchers (Next.js-style; client-side gates)
//!
//! Functions like `is_public` and `is_authenticated` are called by the root
//! `App` component (in `src/app/layout.rs`) every render, BEFORE the matched
//! page resolves. Used to redirect unauthenticated users, enforce subscription
//! tiers, etc.
//!
//! ```rust,ignore
//! #[component]
//! pub fn App() -> Element {
//!     let route = use_route::<Route>();
//!
//!     if !middleware::is_public(&route) {
//!         let user = use_auth();
//!         if user.is_none() {
//!             return rsx! { Redirect { to: Route::SignIn } };
//!         }
//!     }
//!
//!     rsx! { document::Stylesheet { ... } Outlet::<Route> {} }
//! }
//! ```
//!
//! ## 2. Tower middleware (Dioxus-style; server-side HTTP gates)
//!
//! Functions decorated for use as `axum::middleware::from_fn(...)` layers
//! applied in `main.rs`'s `dioxus::serve()` closure. Standard Tower
//! middleware — runs on every HTTP request, server-side only.
//!
//! Per-server-fn Tower layers (e.g. `TimeoutLayer` on a single endpoint)
//! use the `#[middleware(...)]` attribute on the server fn directly,
//! NOT this file. This file is for layers applied globally.

#![allow(dead_code)]

use crate::Route;

// ============================================================================
//  Route matchers (client-side gates)
// ============================================================================

/// Routes anyone can access without authentication.
///
/// Add public-facing pages here (landing, pricing, terms, sign-in, etc.).
pub fn is_public(route: &Route) -> bool {
    matches!(route, Route::Home {})
}

/// Routes that require an authenticated user.
///
/// Sign-in flow should send users back to the original URL after auth.
pub fn is_authenticated(route: &Route) -> bool {
    !is_public(route)
}

// ============================================================================
//  Tower middleware (server-side gates)
// ============================================================================

/// Log every HTTP request the server receives (method + path + status).
///
/// Wired up in `main.rs` via:
///
/// ```rust,ignore
/// .layer(axum::middleware::from_fn(crate::middleware::log_request))
/// ```
#[cfg(feature = "server")]
pub async fn log_request(
    req: dioxus::server::axum::extract::Request,
    next: dioxus::server::axum::middleware::Next,
) -> dioxus::server::axum::response::Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let response = next.run(req).await;
    tracing::info!(%method, path = %uri.path(), status = %response.status(), "request");
    response
}
