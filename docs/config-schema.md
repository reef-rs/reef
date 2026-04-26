# `.reef/config.toml` Schema

The full shape of the file every Reef project carries at the project root in `.reef/config.toml`. This is the durable answer to "what is this project?" — committed to git, edited by hand, consumed by every `cargo reef *` command.

## Layout philosophy

- Each top-level table describes ONE concern: project metadata, build, deploy, auth, storage, etc.
- Per-environment overrides live under `[env.<name>]` and are merged on top of the base.
- Secrets NEVER appear inline — only the env-var names that provide them.
- Empty tables / missing keys are fine; cargo-reef has reasonable defaults.

---

## Reference (canonical)

```toml
# ============================================================================
#  Project metadata — everything reads this
# ============================================================================
[project]
name = "my-app"
version = "0.1.0"
kind = "cloud"           # cloud | thick-client | hybrid | edge | mobile
description = ""

# ============================================================================
#  Auth
# ============================================================================
[auth]
provider = "none"        # none | local | oidc | tailnet | clerk

# Provider-specific config — only the relevant block is consulted
[auth.oidc]
issuer_env = "OIDC_ISSUER"
client_id_env = "OIDC_CLIENT_ID"
client_secret_env = "OIDC_CLIENT_SECRET"
scopes = ["openid", "profile", "email"]

[auth.local]
session_table = "axum_sessions"
password_hash = "argon2"

[auth.tailnet]
acl_file = "infra/headscale-acl.hujson"

# ============================================================================
#  Storage
# ============================================================================
[storage]
backend = "libsql"       # libsql | postgres | none
db_url_env = "DATABASE_URL"
db_path_default = "./data/app.db"
migrations_dir = "migrations"

# Used by `cargo reef db:push` (v0.5+ schema-as-code)
[storage.schema]
ssot_path = "src/server/db/schema.rs"
auto_migrate_on_startup = false

# ============================================================================
#  Build — see docs/build.md
# ============================================================================
[build]
targets = ["web", "server"]
optional_targets = ["desktop", "mobile"]

# Named binaries — `cargo reef build --bin <name>` selects one
[build.bins]
core   = { features = ["customer"],          platforms = ["web", "server"] }
admin  = { features = ["customer", "nexus"], platforms = ["web", "server"] }

[build.tailwind]
enabled = true
input = "tailwind.css"
output = "assets/tailwind.css"

[build.assets]
optimize_images = false
hash_for_cache_busting = true

# ============================================================================
#  Deploy — see docs/deploy.md
# ============================================================================
[deploy]
target = "fly"           # fly | cloudflare | nixos | docker | static | vercel | headscale | none
default_env = "staging"

[deploy.fly]             # only present when target = "fly"
app = "myapp"
config_file = "fly.toml"
strategy = "rolling"

[deploy.cloudflare]
account_id_env = "CF_ACCOUNT_ID"
worker_name = "myapp-worker"

[deploy.nixos]
hosts = ["edge-01.tail.ts.net"]
flake_target = ".#myapp"

[deploy.docker]
registry = "ghcr.io/me/myapp"
remote_compose = ["user@host:/srv/myapp/docker-compose.yml"]

# Secrets — declare which env-var names provide each value
[deploy.secrets]
DATABASE_URL = "DATABASE_URL"
OIDC_CLIENT_SECRET = "OIDC_SECRET"

# Optional pre/post deploy hooks (shell commands, run on the deploying machine)
[deploy.hooks]
pre  = ["cargo reef migrate run --env={env}"]
post = ["./scripts/notify-slack.sh {env} {sha}"]

# ============================================================================
#  Edge (only populated when kind includes "edge")
# ============================================================================
[edge]
headscale_url_env = "HEADSCALE_URL"
acl_file = "infra/headscale-acl.hujson"
default_modes = ["vms"]

# ============================================================================
#  Per-environment overrides — merged on top of the base config
# ============================================================================
[env.dev]
[env.dev.deploy.fly]
app = "myapp-dev"

[env.staging]
[env.staging.deploy.fly]
app = "myapp-staging"

[env.prod]
[env.prod.deploy.fly]
app = "myapp"
strategy = "bluegreen"
[env.prod.deploy.secrets]
SENTRY_DSN = "SENTRY_DSN_PROD"   # different env-var name in prod

# ============================================================================
#  Reef framework metadata
# ============================================================================
[reef]
version = "0.1.0"        # the reef-rs version this project was scaffolded with
template = "cloud"       # which template variant `cargo reef new` produced
```

---

## Resolution rules

1. Parse the base `.reef/config.toml`.
2. If `--env <name>` flag (or `default_env` from `[deploy]`) is set, deep-merge `[env.<name>]` onto the base config. Inner tables merge; arrays are replaced (not appended); scalars are replaced.
3. Resolve secrets: walk `[deploy.secrets]`, look up each env-var, fail if any required one is missing.
4. Apply CLI flag overrides (e.g., `--bin <name>`, `--target <platform>`).
5. Hand the resolved config to the relevant subcommand.

---

## What goes where: cargo.toml vs Dioxus.toml vs .reef/config.toml

| Concern | File |
|---|---|
| Rust dependencies, features, profiles | `Cargo.toml` |
| Cargo workspace / package metadata | `Cargo.toml` |
| `[[bin]]` declarations, `required-features` | `Cargo.toml` |
| Dioxus tooling (which platform dx targets, asset_dir, watch paths) | `Dioxus.toml` |
| Web bundling, prerender config | `Dioxus.toml` |
| Tailwind input/output, image optimization | `.reef/config.toml` (`[build]`) |
| Deploy targets, environments, hooks | `.reef/config.toml` (`[deploy]`) |
| Auth provider config, scopes | `.reef/config.toml` (`[auth]`) |
| Storage backend choice, migration paths | `.reef/config.toml` (`[storage]`) |
| Project shape (kind, default modes) | `.reef/config.toml` (`[project]`) |
| Per-environment overrides | `.reef/config.toml` (`[env.<name>]`) |

**Rule of thumb:** if a config affects compilation, it's `Cargo.toml` or `Dioxus.toml`. If it affects what cargo-reef does (build orchestration, deploy, migrations, scaffolder), it's `.reef/config.toml`.

---

## Validation

`cargo reef doctor` parses `.reef/config.toml` against this schema and reports:

- Unknown keys (typos)
- Missing required env vars referenced in `[deploy.secrets]`
- References to `[build.bins]` entries that don't exist as `[[bin]]` entries in `Cargo.toml`
- References to features that don't exist in `Cargo.toml [features]`
- Invalid adapter target (`target = "fly"` but `[deploy.fly]` block is missing)
- Inconsistent `[env.<name>]` overrides (e.g., overriding a target that doesn't exist in the base)

This catches most config errors before runtime.

---

## Versioning the schema

`[reef] version` lets us evolve the schema without breaking existing projects. When `cargo-reef` parses a config with an older schema version, it auto-migrates (additive changes) or warns (incompatible changes) and points at `cargo reef upgrade` for codemod-style migrations.
