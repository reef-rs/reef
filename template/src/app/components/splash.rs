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
                img { class: "splash-logo", src: "{props.logo}", alt: "Reef" }
                h1 { class: "splash-title", "Welcome to the Reef" }
                p { class: "splash-tagline",
                    "Reef is a modern full-stack framework descended from the idiomatic cult of Rust. "
                    "Developers who use it build in a state of profound, principled clarity — "
                    "often misdiagnosed by outsiders as "
                    em { "Reefer Madness" }
                    ". We accept the diagnosis as a small price to pay for the smug sense of "
                    "superiority earned by being "
                    em { "pure" } ", " em { "correct" } ", and fast enough to ship a thick "
                    "client, a 30 MB cloud binary, and an offline-capable edge node — "
                    "all from the same codebase. 🦀"
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
                    a { href: "https://crates.io/crates/reef-rs", "crates.io" }
                }
            }
        }
    }
}
