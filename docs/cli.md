# `cargo-reef` CLI Surface

Full set of subcommands, flags, and intended behavior. Stub for design alignment.

---

## Subcommands

```
cargo reef <subcommand> [flags]
```

| Subcommand | Status | Purpose |
|---|---|---|
| `new <name>` | **v0.2** | Scaffold a new Reef app from the embedded template |
| `dev` | **v0.2** | Sugar over `dx serve --web` with a Reef banner; passes through extra args |
| `migrate run` | **v0.2** | Apply pending SQL migrations from `migrations/` to the configured DB |
| `migrate new <name>` | **v0.2** | Generate a timestamped migration file in `migrations/` (optional `--with-down`) |
| `migrate status` | **v0.2** | Show applied vs pending migrations |
| `migrate revert` | **v0.2** | Roll back the last applied migration (when `*.down.sql` exists) |
| `db:push` | **v0.2** | Diff `src/server/db/schema.rs` (SSOT) against live DB, preview, apply or `--write` to a migration file. Supports `--features` for cfg-aware multi-deployment schemas. |
| `build` | TODO (v0.5) | Read `.reef/config.toml`, run `dx build` for each configured target/bin |
| `deploy` | TODO (v0.5) | Read `.reef/config.toml` `[deploy]`, ship the built artifact to the target |
| `db:reset` | TODO (v0.5+) | Drop all tables, reapply migrations (dev only) |
| `db:seed` | TODO | Run user-defined seed function (declared via attribute) |
| `doctor` | TODO | Diagnose common config issues (`Dioxus.toml`, `Cargo.toml`, env vars) |
| `upgrade` | TODO | Upgrade the project to a newer Reef version (codemod for known migrations) |
| `--version` / `--help` | **v0.2** | Standard |

**Hidden debug commands** (not in `--help`, but callable for parser/diff inspection):

| Subcommand | Purpose |
|---|---|
| `_debug-schema [path] [--features=...]` | Parse `schema.rs`, print the IR as JSON |
| `_debug-sql [path] [--features=...]` | Parse `schema.rs`, print emitted CREATE TABLE / INDEX SQL |
| `_debug-introspect [db]` | Read live DB via PRAGMA, print the IR as JSON (the inverse of `_debug-sql`) |
| `_debug-diff --schema=... --db=... [--features=...]` | Run the diff and render the human preview without applying |

---

## Universal flags

Apply to most subcommands:

| Flag | Default | Purpose |
|---|---|---|
| `--env <name>` | `dev` | Selects which `[env.<name>]` block in `.reef/config.toml` to merge over the base config (env vars, deploy target overrides, etc.). Common values: `dev`, `staging`, `prod`. |
| `--branch <name>` | (current git branch) | For environments where deploys are per-branch (e.g., Fly preview environments). |
| `--config <path>` | `.reef/config.toml` | Override the config file location. |
| `--verbose` / `-v` | off | Print full subprocess output (dx, tailwindcss, deploy CLIs). |
| `--no-cache` | off | Force fresh builds, skip dx's incremental cache. |

---

## `cargo reef new <name>` — scaffolder

Interactive prompts that branch the generated scaffold:

```
$ cargo reef new my-app

🦀  Welcome to the Reef.

? What kind of app are you building?
  ▸ Cloud-first web app          (Dioxus web + cloud backend, Vercel-style)
    Offline-first thick client   (Dioxus desktop + embedded libSQL, no backend)
    Hybrid: cloud + thick client (offline-capable, syncs when online)
    Edge-distributed             (cloud + self-managed edge devices)
    Mobile (iOS / Android)       (Dioxus mobile + cloud backend)

? Auth?
  ▸ None for now
    Local      (libSQL sessions + password / magic link)
    OIDC       (Google / GitHub / Auth0 / your IdP)
    Tailnet    (Headscale ACLs — for B2B / internal tools)

? Database?
  ▸ Embedded libSQL    (recommended)
    Postgres
    None — I'll add later

? Deploy target?
  ▸ Fly.io
    Cloudflare Workers
    Self-hosted (NixOS / systemd)
    Skip
```

Non-interactive mode for CI:

```
cargo reef new my-app --kind=hybrid --auth=oidc --db=libsql --deploy=fly --no-prompt
```

Each combination of answers produces a different scaffold (see `template/` repo for the source). Template is embedded in the cargo-reef binary at build time via `include_dir!` so `cargo reef new` works offline.

---

## Common workflows

```bash
# Day 1
cargo install cargo-reef
cargo reef new my-app --kind=cloud --auth=oidc --deploy=fly
cd my-app
cargo reef dev                                # alias for dx serve --web

# During dev
cargo reef migrate new add_users_table        # creates migrations/<ts>_add_users_table.sql
cargo reef migrate run                        # applies it

# Build
cargo reef build                              # builds every bin in .reef/config.toml [build].targets
cargo reef build --bin admin --env=staging    # one-off: just the admin bin, staging env

# Deploy
cargo reef deploy                             # deploys per .reef/config.toml [deploy], default env
cargo reef deploy --env=prod                  # deploys to prod
cargo reef deploy --bin admin --env=staging   # specific bin to staging

# Check health
cargo reef doctor                             # validates config, deps, dx version, etc.
```

See per-command docs for details:
- [`build.md`](./build.md)
- [`deploy.md`](./deploy.md)
- [`migrations.md`](./migrations.md)
- [`config-schema.md`](./config-schema.md)
