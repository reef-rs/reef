//! Reef app entry. Single-binary fullstack — compiles to two targets:
//!   - WASM client when built without the `server` feature
//!   - native server when built with `--features server` (the default)
//!
//! Run with `dx serve` (no flags). dx auto-swaps features per target.

use dioxus::prelude::*;

mod api;
mod app;
mod middleware;
mod routes;
mod types;

#[cfg(feature = "server")]
mod server;

pub use routes::Route;

fn main() {
    // ---- Native server build ----
    //
    // Use `dioxus::serve()` (instead of `dioxus::launch()`) so we can attach
    // axum routes, Tower middleware layers, and custom state to the router.
    // `dioxus::server::router(launch_root)` returns an axum Router pre-configured
    // with static asset serving, SSR, and server function registration —
    // we then chain `.layer()` / `.route()` on it like any axum router.
    //
    // Manually registered `.route()` calls take priority over Dioxus SSR
    // (which runs as the fallback handler).
    #[cfg(feature = "server")]
    {
        tracing_subscriber::fmt::init();
        tracing::info!("Reef server starting…");

        dioxus::serve(|| async move {
            let router = dioxus::server::router(launch_root);

            // ---- Apply global Tower middleware here ----
            // Examples:
            //   .layer(axum::middleware::from_fn(crate::middleware::log_request))
            //   .layer(tower_http::cors::CorsLayer::permissive())
            //   .layer(tower_http::compression::CompressionLayer::new())
            //
            // Per-server-fn middleware uses `#[middleware(...)]` on the
            // server fn declaration in `src/api/mod.rs` instead.

            Ok(router)
        });
    }

    // ---- WASM client build ----
    #[cfg(not(feature = "server"))]
    dioxus::launch(launch_root);
}

/// The Dioxus mounting point. Renders `Router::<Route> {}`, which consults the
/// `Route` enum (in `src/routes.rs`) to dispatch the current URL to the matching
/// page component, optionally wrapped in any `#[layout(...)]` declared in the
/// enum.
///
/// This function has no Next.js equivalent — Next implicitly mounts its router
/// for you. Dioxus makes the mount point explicit so it can be customized
/// (passed different state, wrapped in providers, etc.).
fn launch_root() -> Element {
    rsx! { Router::<Route> {} }
}
