# Reef Template — Claude Context

This project is the starter template `cargo reef new` scaffolds. It's a single-crate Dioxus 0.7 fullstack app organized for Next.js-style file conventions, with Reef-specific tooling overlaid via `.reef/`.

This file is the **source of truth for how things are supposed to be structured**. When in doubt, defer to what's written here over generic Rust/Dioxus instinct.

---

## Architecture overview

**One Cargo package. One binary crate. Two compile targets via Cargo features.**

- WASM client target — built when the `web` feature is on (default-features off + `--features web`)
- Native server target — built when the `server` feature is on (`default = ["server"]`)

The same source compiles to both. Server fns and components live in the same crate; the `#[server]` / `#[get]` / `#[post]` macros generate the WASM client stub and the native HTTP handler from one declaration.

**This is NOT a workspace.** No `[workspace]` section, no path-deps to sibling crates, no `frontend/`-`backend/`-tier crate split. We considered that pattern and rejected it — see Reefer Rule 13 (`docs/ruleset.md`).

**Multi-binary apps don't need multiple crates.** Cargo `[[bin]]` entries with `required-features`, plus `#[cfg(feature = "...")]` gates on the relevant code, produce N different binaries from ONE crate, each containing only the code its features enable. **All driven by dx flags** — never invoke cargo directly:

```bash
dx serve --bin core --web                # builds the "core" bin with its required-features
dx serve --bin cloud --web               # different bin, different features, different binary content
dx build --bin core --release --web      # production
dx build --bin cloud --release --web

# Manual feature override (rarely needed — required-features handles it)
dx serve --features "cloud extra-stuff" --no-default-features --web

# Fullstack with different bin per side (when client and server are separate bins)
dx serve @client --bin webapp @server --bin api --web
```

Reach for a separate crate (workspace) only when (a) publishing to crates.io, (b) compile parallelism on a huge codebase is causing real iteration pain, (c) "is X in this binary?" needs to be answerable from one Cargo.toml line for audit purposes, or (d) source must live in a separate (often private) repo. Otherwise: single crate with features + `[[bin]]`.

**Always use `dx`. Never `cargo build` / `cargo run` directly** — features are applied per target, bin selection respects `required-features`, and dx handles the WASM + server orchestration. Cargo by hand will get the features wrong (we hit this earlier).

---

## File layout

```
.
├── Cargo.toml                    # single-crate manifest, feature-gated deps
├── Dioxus.toml                   # tooling config: `default_platform = "fullstack"`
├── build.rs                      # stub today; v0.5 auto-generates routes.rs
├── tailwind.css                  # Tailwind v4 input file (root, where CLI looks)
├── rust-toolchain.toml
├── rustfmt.toml
├── clippy.toml
├── .editorconfig
├── .env.example
├── .gitignore
├── .cargo/config.toml
├── .config/nextest.toml
├── .github/workflows/ci.yml
├── .reef/config.toml             # Reef project METADATA — not compile config
├── migrations/                   # SQL migration files (cargo reef migrate run)
├── assets/                       # asset_dir per Dioxus.toml: copied to out_dir
│   ├── favicon.png               # referenced via asset!() — content-hashed URL
│   ├── logo.png                  # referenced via asset!() — gets hashed URL
│   ├── main.css                  # brand styles, manually written
│   └── tailwind.css              # Tailwind output (run npx tailwindcss to populate)
└── src/
    ├── main.rs                   # entry: cfg-gated launch/serve, plus launch_root()
    ├── routes.rs                 # Route enum (hand-written v0.1; auto-gen v0.5)
    ├── types.rs                  # shared wire types (Status, etc.)
    ├── middleware.rs             # route matchers (Next-style) + Tower middleware
    ├── api/
    │   └── mod.rs                # server fns: #[get]/#[post]/#[server] declarations
    ├── app/                      # UI layer (mirrors Next.js's app/ directory)
    │   ├── mod.rs                # the `/` page (Home component) + module decls
    │   ├── layout.rs             # RootLayout component (Next layout.tsx equivalent)
    │   └── components/
    │       ├── mod.rs            # re-exports
    │       └── splash.rs         # reusable component, takes typed props
    └── server/                   # cfg(feature = "server") only
        ├── mod.rs
        ├── db/
        │   ├── mod.rs            # Db struct (Arc<Database>), default_db() global
        │   └── schema.rs         # `#[reef::table]` row types — SSOT for the DB
        ├── queries.rs            # all reads
        └── actions.rs            # all writes
```

---

## Page / layout / route relationship (Next.js parity)

```
main::main()
  ├─ #[cfg(feature = "server")]    dioxus::serve(...launch_root...)
  └─ #[cfg(not(feature = "server"))] dioxus::launch(launch_root)

launch_root() -> Element
  └─ rsx! { Router::<Route> {} }                  ← Dioxus mounting point
                                                    (no Next.js equivalent)

Router consults `Route` enum:
  - matches URL → variant
  - applies #[layout(RootLayout)] from the enum

RootLayout()                                        ← src/app/layout.rs
  ├─ document::Stylesheet { ... }                   (Next: app/layout.tsx)
  └─ Outlet::<Route> {}                             (Next: {children})

Outlet renders the matched page:
  Home()                                            ← src/app/mod.rs
    └─ component body                               (Next: app/page.tsx)
```

| Concept | Next.js | Reef |
|---|---|---|
| Mount point (no equivalent) | — | `launch_root()` in `main.rs` |
| Root layout | `app/layout.tsx` | `RootLayout` in `src/app/layout.rs` |
| Home page | `app/page.tsx` | `Home` in `src/app/mod.rs` |
| `{children}` slot | `{children}` prop | `Outlet::<Route> {}` |
| Sub-layout | `app/dashboard/layout.tsx` | `DashboardLayout` in `src/app/dashboard/layout.rs`, referenced via additional `#[layout(...)]` in routes.rs |
| Sub-page | `app/dashboard/page.tsx` | `Dashboard` in `src/app/dashboard/mod.rs` |
| Middleware (auth gating) | `middleware.ts` | `src/middleware.rs` (route matchers) |
| API route | `app/api/foo/route.ts` | `#[get]/#[post]` in `src/api/mod.rs` |

**Convention:** `mod.rs` IS the page component for its folder's URL. There is no separate `page.rs`. Sub-routes nest by directory: `src/app/users/show/mod.rs` is the `/users/show` page.

---

## Routing (`src/routes.rs`)

**Hand-written for v0.1.** In v0.5, `build.rs` will auto-generate this from the filesystem (see `build.rs` for the planned conventions: `_id` for dynamic segments, `__slug` for catch-alls, `_group_` for route groups).

**Required syntax** (per Dioxus 0.7 docs):

```rust
#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(RootLayout)]                  // wraps EVERY route in RootLayout
        #[route("/")]
        Home {},                            // braces required, even for no params

        #[route("/users/:id")]
        UserShow { id: u64 },               // dynamic segment → struct field

        #[layout(DashboardLayout)]          // sub-layout for a subset
            #[route("/dashboard")]
            Dashboard {},
}
```

**Critical rules:**

- Variants MUST use struct-variant syntax: `Home {}` not `Home`. Required by `Routable` derive.
- Components referenced by variants must be in scope (use `pub use crate::app::Home;` etc.).
- Dynamic segments use `:id` syntax (NOT Next's `[id]`). Path params become struct fields with matching names.
- Catch-all: `:..segments` → `Vec<String>`.
- Query: `?:query&:sort`.
- `#[layout(Component)]` wraps a group of routes; the component must contain `Outlet::<Route> {}`.

---

## Server functions (`src/api/mod.rs`)

**Idiomatic Dioxus 0.7: explicit HTTP method + URL macros.**

```rust
#[get("/api/users/{user_id}")]
pub async fn get_user(user_id: Uuid) -> Result<User, ServerFnError> { ... }

#[post("/api/users")]
pub async fn create_user(body: Json<CreateUserRequest>) -> Result<User, ServerFnError> { ... }

#[get("/api/products/{product}?color&quantity")]
pub async fn get_product(
    product: String,
    color: String,
    quantity: Option<i32>,
) -> Result<Vec<Product>, ServerFnError> { ... }
```

Macros: `#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]`. Use `#[server]` (no method/URL) as a generic fallback when you don't care.

**Path params in `{name}` braces extract into matching function arguments** by name. Query params follow `?name1&name2`.

**Body runs server-side only** — the macro elides it on WASM builds. The body can `use crate::server::*` to delegate to `db`, `queries`, `actions`.

**Authentication** uses extractors declared inside the macro:

```rust
#[get("/api/me", auth: SessionAuth)]
pub async fn me(auth: SessionAuth) -> Result<User, ServerFnError> { ... }
```

Define extractors in `src/middleware.rs` (FromRequestParts impls).

**Per-server-fn middleware** uses the `#[middleware(...)]` attribute:

```rust
#[post("/api/timeout")]
#[middleware(TimeoutLayer::new(Duration::from_secs(1)))]
pub async fn timeout() -> Result<(), ServerFnError> { ... }
```

---

## Cargo.toml feature gating

**Server-only deps are `optional = true` and pulled in via the `server` feature using `dep:` syntax.** This is the official Dioxus pattern — without `dep:`, the optional dep wouldn't actually activate.

```toml
[dependencies]
dioxus = { version = "0.7.6", features = ["fullstack", "router"] }
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"
tracing = "0.1"

# Server-only — never compiled for WASM
tokio = { version = "1", features = ["full"], optional = true }
libsql = { version = "0.9", optional = true }
tracing-subscriber = { version = "0.3", optional = true }
# Reef runtime — `#[reef::table]` macro + Json/Jsonb wrappers. Crates.io
# package is `reef-rs` (the unhyphenated `reef` was taken by an unrelated
# crate); the `package = "reef-rs"` rename keeps the import path `reef::*`.
reef = { version = "0.2", package = "reef-rs", optional = true }

[features]
default = []                        # MUST be empty — see below
web = ["dioxus/web"]
server = [
    "dioxus/server",
    "dep:tokio",
    "dep:libsql",
    "dep:tracing-subscriber",
    "dep:reef",
]
```

**Critical: never pass `--features server` to `dx serve`.** Features are global — passing `server` manually applies it to the WASM build too, which re-enables `dioxus/server` and pulls tokio→mio into wasm32 (mio doesn't compile to WASM). Just run `dx serve` (no flags) — dx auto-applies features per target.

---

## Dioxus.toml — controls dx tooling, not compilation

**`default_platform = "web"` is required for fullstack apps.** This is non-obvious: there is NO `"fullstack"` value for `default_platform`. Setting it to `"fullstack"` is invalid and will be rejected/ignored. For a fullstack app:

- Set `default_platform = "web"` in `Dioxus.toml`
- Have `dioxus = { features = ["fullstack"] }` in `Cargo.toml`
- The `fullstack` feature on the dioxus crate is what enables server-side rendering and server fns
- When dx sees `web` platform + a `server` feature in Cargo.toml, it spawns the server alongside the WASM client

**Running:**
- `dx serve --web` — explicit, always works
- `dx serve` (no flags) — works only if `default_platform = "web"` is set in `Dioxus.toml`
- `dx serve --platform fullstack` — INVALID, this platform name doesn't exist

Other required keys (per docs):
- `out_dir` — where built artifacts land (`dist`)
- `asset_dir` — where dx looks for static assets (`assets`)
- `[web.app]` — at minimum `title`
- `[web.watcher]` — `reload_html`, `watch_path`, `index_on_404 = true` (the last is critical for SPA routing)
- `[web.resource]` and `[web.resource.dev]` — even if empty

**`[bundle]`** is for `dx bundle` (desktop/mobile distribution via tauri-bundler). Not used by `dx serve`.

**Cargo.toml `default = []` (empty), NOT `default = ["server"]`.** This is the OTHER non-obvious requirement. With `default = ["server"]`, dx classifies the project as server-only and never builds the WASM client. Empty defaults let dx independently apply `web` for the WASM build and `server` for the native build.

Trade-off: `cargo run` doesn't work without `--features server`. Use `dx serve` / `dx build` instead — they handle features per target automatically.

---

## Two layers, distinct jobs

| | Cargo.toml `[features]` | Dioxus.toml |
|---|---|---|
| Controls | What Rust code compiles for each target | What dx tooling builds and where |
| Read by | Cargo + the compiler | dx CLI |
| Example | `web = ["dioxus/web", "dep:web-sys"]` | `default_platform = "fullstack"` |

**Both must be configured correctly.** Cargo.toml without correct features = WASM build fails on native deps. Dioxus.toml without `default_platform = "fullstack"` = dx doesn't build both targets even if Cargo.toml is fine.

---

## Middleware

**Two flavors, both live in `src/middleware.rs`:**

### 1. Route matchers (Next.js-style; client-side gates)

Functions called by the root layout component to gate UI rendering:

```rust
pub fn is_public(route: &Route) -> bool {
    matches!(route, Route::Home {})
}
pub fn is_authenticated(route: &Route) -> bool {
    !is_public(route)
}
```

Used in `RootLayout` to redirect unauthenticated users before rendering protected pages.

### 2. Tower middleware (Dioxus-style; server-side HTTP gates)

Standard Tower layers, applied to the axum router in `main.rs`:

```rust
#[cfg(feature = "server")]
pub async fn log_request(
    req: dioxus::server::axum::extract::Request,
    next: dioxus::server::axum::middleware::Next,
) -> dioxus::server::axum::response::Response {
    // ...
}
```

Applied via `.layer(...)` on the router in `dioxus::serve()`'s closure:

```rust
dioxus::serve(|| async move {
    Ok(dioxus::server::router(launch_root)
        .layer(axum::middleware::from_fn(crate::middleware::log_request)))
});
```

Use `dioxus::server::axum::*` re-exports — don't add `axum` as a direct dep (avoids version mismatches with what Dioxus uses internally).

---

## Database / persistence (`src/server/`)

Native-only, gated by `cfg(feature = "server")`. Module layout:

- `db/mod.rs` — `Db` struct (Arc<libsql::Database>) with `Db::new()`, `Db::from_env()`, `default_db()` lazy global.
- `db/schema.rs` — `#[reef::table]` row types. **Single source of truth for the DB shape.** `cargo reef db:push` parses this file, diffs against the live DB, and applies migrations. See "Schema-as-code" below.
- `queries.rs` — all SELECTs. Functions take `&Db` for testability.
- `actions.rs` — all writes (INSERT/UPDATE/DELETE). Same pattern.

**Migrations** live in `migrations/` and are applied by `cargo reef migrate run`. The runner records what it applied in a `schema_migrations` table so re-runs are idempotent. `Db::new()` does NOT auto-run migrations — invoking the runner is a deployment concern, not an app concern.

```bash
cargo reef migrate run                  # apply pending migrations
cargo reef migrate new add_users_table  # generate a hand-authored migration file
cargo reef migrate status               # show applied vs pending
cargo reef migrate revert               # roll back last applied (requires *.down.sql)
```

---

## Schema-as-code (`#[reef::table]` + `cargo reef db:push`)

The `#[reef::table]` attribute makes a Rust struct simultaneously the row type AND the table declaration. There's no separate "tables enum" or migration DSL — the struct IS both.

```rust
use reef::{Json, Jsonb};
use serde::{Deserialize, Serialize};

#[reef::table(strict)]
#[index(name = "users_email_idx", columns = ["email"])]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    #[column(primary_key, auto_increment)]
    pub id: i64,
    #[column(unique)]
    pub email: String,
    pub name: String,
    pub bio: Option<String>,                          // → TEXT (nullable from Option<T>)
    pub tags: Json<Vec<String>>,                      // → TEXT (JSON-encoded)
    pub metadata: Jsonb<UserMetadata>,                // → BLOB (JSONB, SQLite 3.45+)
    #[column(default = "active",
             check = "status IN ('active','disabled')")]
    pub status: String,
}
```

The same `User` struct is used by `queries::*` (deserialized from row results) and by `db:push` (parsed by `cargo-reef` to know what `users` should look like).

### The two coexisting workflows

`cargo reef migrate run` and `cargo reef db:push` share the `schema_migrations` tracking table. Two ways to use them:

| Workflow | When to use |
|---|---|
| **`migrate run` for bootstrap, `db:push` for iteration** | Most projects. Edit `schema.rs`, `db:push`, repeat. For prod deploys, capture each diff with `db:push --write <name>` so CI can apply via `migrate run`. |
| **All file-based** (`migrate new` + `migrate run`) | Teams that want every migration hand-authored and code-reviewed. `schema.rs` becomes documentation. |

### `cargo reef db:push` essentials

```bash
cargo reef db:push                  # preview, prompt, apply
cargo reef db:push -y               # apply without prompting
cargo reef db:push --dry-run        # preview only
cargo reef db:push --write add_foo  # capture as migrations/<ts>_add_foo.sql
cargo reef db:push --allow-drop     # required to apply diffs that DROP a table or column
cargo reef db:push --features X,Y   # filter cfg-gated tables to this feature set
```

**Safety guardrails baked in:**
- Drops (`DROP TABLE` / `DROP COLUMN`) preview but require `--allow-drop` to apply.
- Mixed auto + manual migrations refuse silent partial application.
- Adding a NOT NULL column without DEFAULT is flagged as needing a manual migration (SQLite can't backfill).
- Tightening changes (NULL → NOT NULL) emit a warning — libSQL's `ALTER COLUMN` only applies to new writes; existing rows aren't revalidated.

### Attribute reference

**Table-level — `#[reef::table(...)]`:**
- `name = "..."` — override the SQL table name (default: snake_case of struct name, NO pluralization)
- `strict` — emit as a SQLite STRICT table (3.37+)
- `without_rowid` — emit as a WITHOUT ROWID table

**Field-level — `#[column(...)]`:**
- `primary_key`, `auto_increment`, `unique`
- `default = <expr>` — Rust literal. String literals get SQL-quoted automatically (`default = "active"` → `DEFAULT 'active'`); numerics/bools emit raw. **Don't add inner quotes** — `default = "'active'"` becomes `DEFAULT '''active'''` (a literal string containing the quote chars).
- `default_sql = "<sql>"` — verbatim SQL passthrough, paren-wrapped. Use for function calls / expressions: `default_sql = "datetime('now')"` → `DEFAULT (datetime('now'))`. Mutually exclusive with `default`.
- `check = "<sql_expr>"`, `references = "table(col)"`
- `on_delete` / `on_update` — `cascade`, `restrict`, `set_null`, `set_default`, `no_action`
- `generated = "<sql_expr>"`, `generated_kind = "stored"` | `"virtual"`

**Struct-level helpers (absorbed by `#[reef::table]`):**
- `#[index(name = "...", columns = [...], unique)]` — single or multi-column, supports expression indexes (`json_extract(meta, '$.path')`)
- `#[primary_key(columns = [...])]` — composite PK
- `#[foreign_key(columns = [...], references = "table(c1, c2)", on_delete = "...", on_update = "...")]` — composite FK
- `#[check(name = "...", expr = "...")]` — named table-level CHECK

**Type mapping (Rust → SQL):**
- `String` / `i64` / `f64` / `bool` / `Vec<u8>` → TEXT / INTEGER / REAL / INTEGER / BLOB (NOT NULL implicit)
- `Option<T>` → unwrap to T's SQL type, mark nullable
- `Json<T>` → TEXT (JSON-encoded; transparent serde wrapper)
- `Jsonb<T>` → BLOB (JSONB-encoded)
- Everything else (custom structs, `Vec<T>` for non-`u8`, `HashMap`) errors with a "wrap in `Json<>` / `Jsonb<>`" suggestion

### Multi-deployment via `#[cfg]` gates

When this app is built into multiple binaries (per Reefer Rule 3 — one binary per role), use `#[cfg(feature = "...")]` to gate which tables exist in which build:

```rust
#[reef::table] pub struct User { ... }                         // always
#[reef::table] pub struct Post { ... }                         // always

#[cfg(feature = "cloud")]
#[reef::table] pub struct Tenant { ... }                       // SaaS build only

#[cfg(feature = "desktop")]
#[reef::table] pub struct OfflinePref { ... }                  // desktop build only
```

Then `db:push --features <set>` parses `schema.rs` through that feature view, so each binary's deployment migrates only the tables it actually compiles. The cfg evaluator handles `feature`, `not`, `all`, `any`, and arbitrary nesting.

Cross-table FK validation runs **after** cfg filtering, so a FK that only exists in a feature-gated table doesn't break builds where the gate is off.

### What this replaces

If you've used Drizzle, this is the same `db:push` / `db:generate` model. If you've used Diesel or Active Record, the row type IS the schema declaration — no separate `schema.rs` macro file or `tables.rs` enum. There's nothing else to keep in sync.

**Don't create a `tables.rs` enum** to identify or dispatch on tables — the struct's type identity already does that. For role-based access enforcement (e.g., "this code can only write public tables, not admin tables"), use module structure + Cargo features, not a runtime enum. The compiler enforces the boundary at zero runtime cost.

---

## Asset handling

**Two paths, both legitimate:**

### A. Manganis `asset!()` macro (hashed, content-addressable URLs)

```rust
const LOGO: Asset = asset!("/assets/logo.png");
// → URL becomes /assets/logo-<hash>.png at runtime
```

Use this for: in-app references (logos, CSS, fonts referenced by Rust code). Cache-busting is automatic.

### B. Direct path via `asset_dir` (literal, predictable URLs)

Files placed in the `asset_dir` (per Dioxus.toml — we use `assets/`) are auto-copied to `out_dir` and served at literal paths.

```rust
document::Link { rel: "icon", href: "/some-fixed-path.png" }
// served from assets/some-fixed-path.png at /some-fixed-path.png
```

Use this for: things browsers fetch by **a hard-coded URL the page can't influence** — `robots.txt`, `sitemap.xml`. **Favicons are NOT in this category** — see below.

### Favicon: use `asset!()`, not direct path

Favicons go through `asset!()` for cache-busting:

```rust
const FAVICON: Asset = asset!("/assets/favicon.png");
document::Link {
    rel: "icon",
    r#type: "image/png",
    sizes: "32x32",
    href: FAVICON,
}
```

Reasons: (1) Chrome aggressively caches favicons by URL — content-hashed URLs defeat that, so a favicon swap takes effect on next page load, not "after the user manually clears cache or restarts the browser." (2) The `type` and `sizes` attributes matter for Chrome to render newer-format icons correctly.

**Don't use `[[web.resource.static]]`** — it's not in the canonical Dioxus.toml schema (despite some old examples showing it). Use `asset_dir` + direct paths.

---

## `.reef/config.toml`

**Project metadata, NOT compile control.** Read by `cargo reef *` commands to know what kind of project this is.

```toml
[project]
kind = "cloud"           # cloud | thick-client | hybrid | edge | mobile
[auth]
provider = "none"        # none | local | oidc | tailnet
[storage]
backend = "libsql"
[deploy]
target = "none"          # fly | cloudflare | nixos | none
[build]                  # what `cargo reef build` orchestrates (v0.5)
targets = ["web", "server"]
[build.tailwind]
enabled = true
input = "tailwind.css"
output = "assets/tailwind.css"
```

Reef CLI commands consult this file. Compile-time behavior is controlled by `Cargo.toml` and `Dioxus.toml` separately.

---

## `build.rs`

**Stub today (v0.1).** Documents the v0.5 plan: scan `src/app/` and auto-generate `src/routes.rs`. Conventions to recognize:

- `src/app/mod.rs` → `/`
- `src/app/<name>/mod.rs` → `/<name>`
- `src/app/<name>/layout.rs` → sub-layout for `/<name>/*`
- `src/app/_id/mod.rs` → `:id` dynamic segment (underscore prefix = Rust-friendly equivalent of Next's `[id]`)
- `src/app/__slug/mod.rs` → `*slug` catch-all
- `src/app/_group_/foo/mod.rs` → route group, `_group_` doesn't appear in URL

**Why build.rs and not a proc macro:** build scripts have first-class filesystem watching via `cargo:rerun-if-changed`. Proc macros don't reliably re-run when external files change. Same reason `prost`, `cxx`, etc. use build scripts for codegen.

---

## Common commands

**Always use dx.** Cargo features and bin selection are applied correctly per target only when invoked through dx; `cargo build`/`cargo run` directly will get features wrong and break the WASM build.

```bash
dx serve --web                    # dev loop — explicit form, always works
dx serve                          # works ONLY if default_platform = "web" in Dioxus.toml

dx serve --bin <name> --web       # pick a specific bin (auto-applies its required-features)
dx serve --features "x y" --web   # explicit feature overrides (rarely needed if [[bin]]s are set up)
dx serve --no-default-features --features web,server --web  # full manual control

dx build --web --release          # production build (WASM client + server)
dx build --bin <name> --release --web

# Type-check only (no build artifacts) — still goes through dx
dx check --web

# Tests — dx wraps cargo test/nextest with proper features
dx test

# Migrations (TODO: cargo-reef in development)
cargo reef migrate run
cargo reef migrate new <name>
```

**Why `--web` for fullstack?** dx's `--platform` flag selects the *frontend* target (web/desktop/mobile). The server is implicit when the `dioxus` crate has the `fullstack` feature AND a `server` feature exists in `[features]`. So `dx serve --web` actually means "build the WASM client + auto-spawn the native server it talks to." There is NO `--platform fullstack` option (that platform value doesn't exist).

**Multi-binary picking:** if `Cargo.toml` declares multiple `[[bin]]` entries with different `required-features`, `dx serve --bin <name>` builds whichever one you select with its features automatically applied. That's how you produce e.g. one binary with admin code included and one without — same crate, different bin selection at build time.

**For Tailwind:**

```bash
npx tailwindcss -i ./tailwind.css -o ./assets/tailwind.css --watch
```

Run alongside `dx serve` (separate terminal). Tailwind v4 syntax in the input file:

```css
@import "tailwindcss";
@source "./src/**/*.{rs,html,css}";
```

---

## Reefer Ruleset (project-level)

The framework's principles, in `docs/ruleset.md`. Most relevant for code generation in this template:

1. **Code cleanliness is next to godliness** — delete more than you write
2. **Deployment is a hardware distinction, not software** — same binary, different roles
3. **One binary per role, not per environment** — `--mode=...` flags
4. **Offline is a first-class state** — apps must work disconnected
5. **Trust the type system, not the README** — encode invariants in types
6. **Identity at L3, not L7** — Tailnet ACLs over per-endpoint authz
7. **The wire format is the contract** — typed RPC, no OpenAPI specs
8. **Dependencies are debt** — every dep is something you don't fully understand
9. **Vendors come and go; standards remain** — SQLite over X, OIDC over home-grown auth
10. **Async is the default; sync is the special case** — Tokio is non-negotiable
11. **Compile times are a tax** — pay them at CI, not at every keystroke
12. **`unsafe` and `Box<dyn Trait>` should make you stop and think**
13. **A crate marks a binary or target boundary; otherwise it's a module**

Don't violate these without explicit reason.

---

## Things that have specific correct shapes

| Thing | Correct shape |
|---|---|
| Cargo.toml `[features]` default | `default = []` (empty) — NOT `["server"]` |
| Dioxus.toml `default_platform` | `"web"` for fullstack — NOT `"fullstack"` (that value doesn't exist) |
| Run dev | `dx serve --web` (or `dx serve` if default_platform is set) |
| Variant in Route enum | `Home {}` (struct syntax even for no params) |
| Server fn declaration | `#[get("/path")]` or `#[post("/path")]` (idiomatic) — `#[server]` is fallback |
| Optional server-only dep | `tokio = { ..., optional = true }` + `server = ["dep:tokio"]` in features |
| App launch on server | `dioxus::serve(|| async move { Ok(dioxus::server::router(launch_root)) })` |
| App launch on WASM | `dioxus::launch(launch_root)` |
| Root layout component | Contains `document::Stylesheet { ... }` + `Outlet::<Route> {}` |
| Mounting point | `fn launch_root() -> Element { rsx! { Router::<Route> {} } }` |
| Home page component | `fn Home() -> Element` in `src/app/mod.rs` |
| Sub-layout component | New file `src/app/<path>/layout.rs`, referenced via `#[layout(...)]` in routes.rs |
| Stylesheet location | In `RootLayout` body via `document::Stylesheet { href: ... }` (NOT in pages — pages unmount on navigation) |
| Tailwind input file | `./tailwind.css` at project root |
| Tailwind output file | `./assets/tailwind.css` (referenced as `/tailwind.css` if via `[web.resource]`, or via `asset!()` for hashed URL) |
| Favicon | `./assets/favicon.png`, referenced via `asset!()` for cache-busting |
| Favicon link in head | `document::Link { rel: "icon", r#type: "image/png", sizes: "32x32", href: FAVICON }` |
| Migration files | `./migrations/<timestamp>_<name>.sql` |
| Reef config | `.reef/config.toml` (committed; metadata only) |

---

## When in doubt

1. Check this file first.
2. Reference `https://dioxuslabs.com/learn/0.7/` for Dioxus user-facing docs.
3. For Dioxus internals (when something behaves weirdly and the user-facing docs don't explain why), use the architecture references below — they were written explicitly for AI agents to understand the framework.
4. Don't make executive decisions about structural changes — confirm with Matt before refactoring.
5. Prefer pragmatic + idiomatic over clever.

---

## Reference: Dioxus internals

Dioxus ships its own AI-agent-targeted architecture docs at [`DioxusLabs/dioxus/notes/architecture/`](https://github.com/DioxusLabs/dioxus/tree/main/notes/architecture). Read these when:

- You hit unexpected behavior and the user-facing docs at `dioxuslabs.com/learn/0.7/` don't explain it
- You need to understand *why* something is the way it is (versus just *how* to use it)
- You're debugging something that smells like a Dioxus internals issue, not an app issue

**Most relevant for Reef work**, in priority order:

| Doc | When to read |
|---|---|
| [`05-FULLSTACK.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/05-FULLSTACK.md) | **Highest priority for us.** Server fns, SSR, hydration, fullstack request flow. Read when server fn behavior surprises you (registration, URL inference, `use_resource` + SSR). |
| [`09-ROUTER.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/09-ROUTER.md) | Routable enum internals, layout dispatch, Outlet resolution. Read when routes behave weird. |
| [`08-ASSETS.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/08-ASSETS.md) | manganis pipeline, `asset!()` macro internals, hashing/bundling. Read when asset paths or 404s confuse you. |
| [`02-CLI.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/02-CLI.md) | dx tooling internals — how it picks platforms, applies features per target, spawns servers. Read when `dx serve` does unexpected things. |
| [`04-SIGNALS.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/04-SIGNALS.md) | Reactivity model. Read when state updates don't trigger re-renders. |
| [`07-HOTRELOAD.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/07-HOTRELOAD.md) | Hot-reload mechanism. Read when `dx serve` doesn't pick up a change. |
| [`00-OVERVIEW.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/00-OVERVIEW.md) | High-level dependency graph and architectural patterns. Skim once for context. |
| [`01-CORE.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/01-CORE.md) | Virtual DOM, rendering pipeline, lifecycle. Read for deep debugging of render behavior. |
| [`03-RSX.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/03-RSX.md) | RSX macro internals. Read when rsx! errors are cryptic. |

**Less relevant for Reef-as-template** (still valuable for advanced use):

| Doc | Topic |
|---|---|
| [`06-RENDERERS.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/06-RENDERERS.md) | Web/desktop/mobile/liveview renderers. Useful when targeting non-web platforms. |
| [`10-WASM-SPLIT.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/10-WASM-SPLIT.md) | Code-splitting WASM bundles. Performance optimization. |
| [`11-NATIVE-PLUGIN-FFI.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/11-NATIVE-PLUGIN-FFI.md) | Native plugin / FFI for desktop/mobile. |
| [`12-MANIFEST-SYSTEM.md`](https://github.com/DioxusLabs/dioxus/blob/main/notes/architecture/12-MANIFEST-SYSTEM.md) | Permissions, Info.plist, AndroidManifest customization for native targets. |

**Also useful:**
- [`AGENTS.md`](https://github.com/DioxusLabs/dioxus/blob/main/AGENTS.md) — Dioxus's own onboarding doc for AI agents working on the framework. Crate-by-crate workspace map. Read once for orientation.

**How to fetch them in-context:**

```bash
# Direct fetch via WebFetch tool (preferred — fewer tokens than spawning a researcher)
WebFetch https://raw.githubusercontent.com/DioxusLabs/dioxus/main/notes/architecture/05-FULLSTACK.md

# Or via gh cli for the file content
gh api 'repos/DioxusLabs/dioxus/contents/notes/architecture/05-FULLSTACK.md' --jq '.content' | base64 -d
```

When investigating a Dioxus issue, fetch the most relevant doc directly rather than searching docs.dioxuslabs.com — the architecture docs are denser and more accurate about the *why*.
