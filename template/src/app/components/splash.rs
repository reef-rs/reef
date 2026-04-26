use dioxus::prelude::*;

use crate::types::Status;

#[derive(Props, Clone, PartialEq)]
pub struct SplashProps {
    pub logo: Asset,
    pub status: Option<Result<Status, String>>,
}

#[component]
pub fn Splash(props: SplashProps) -> Element {
    rsx! {
        main { class: "splash",
            div { class: "splash-card",
                div { class: "splash-logo-wrap",
                    // Decorative bubbles drifting up behind the logo. Pure CSS,
                    // no JS — keyframes in main.css.
                    span { class: "bubble bubble-1" }
                    span { class: "bubble bubble-2" }
                    span { class: "bubble bubble-3" }
                    span { class: "bubble bubble-4" }
                    img { class: "splash-logo", src: "{props.logo}", alt: "Reef" }
                }
                h1 { class: "splash-title", "Welcome to the Reef" }
                p { class: "splash-tagline",
                    "A modern full-stack framework for Rust. "
                    em { "Pure" } ", " em { "clean" } ", and " em { "fast" }
                    " — ship a thick client, a 30 MB cloud binary, and an "
                    "offline-capable edge node from one codebase. 🪸"
                }

                div { class: "splash-status",
                    match &props.status {
                        Some(Ok(s)) => rsx! {
                            span { class: "status-dot status-ok" }
                            span { "Server: {s.message} (v{s.version})" }
                        },
                        Some(Err(e)) => rsx! {
                            span { class: "status-dot status-err" }
                            span { "Server unreachable: {e}" }
                        },
                        None => rsx! {
                            span { class: "status-dot" }
                            span { "Connecting…" }
                        },
                    }
                }

                div { class: "splash-links",
                    a { href: "https://reef.rs", "Docs" }
                    a { href: "https://github.com/reef-rs", "GitHub" }
                    a { href: "https://crates.io/crates/reef", "crates.io" }
                }
            }
        }
    }
}
