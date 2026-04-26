# cargo-reef Design Docs

Working notes for the `cargo-reef` CLI. Not user-facing yet — these capture intent so we don't lose context between sessions.

## Status

| Component | Status |
|---|---|
| `cargo reef new` (scaffolder) | Not built. Will copy `reef-rs/template` (embedded at build time via `include_dir!`). |
| `cargo reef build` | Not built. Designed below — reads `.reef/config.toml`, drives `dx build` per configured bin. |
| `cargo reef deploy` | Not built. Designed below — reads `.reef/config.toml` `[deploy]` section, dispatches to target adapter (Fly / Cloudflare / NixOS / etc.). |
| `cargo reef migrate run/new/status` | Not built. Hand-rolled SQL migration runner against libSQL. |
| `cargo reef db:push` | Not built. v0.5+ schema-as-code diff generator (Drizzle-style). |
| Build script integration (`build.rs` route gen) | Not built. Library function in `cargo-reef` that user projects' `build.rs` calls. |

## Documents

- [`cli.md`](./cli.md) — Overall CLI surface: every subcommand, flag, intended behavior
- [`build.md`](./build.md) — `cargo reef build` design: how it reads config, drives dx, handles multi-bin
- [`deploy.md`](./deploy.md) — `cargo reef deploy` design: target adapters, env/branch handling, secrets
- [`config-schema.md`](./config-schema.md) — `.reef/config.toml` full schema reference
- [`routes-generation.md`](./routes-generation.md) — Filesystem→`routes.rs` auto-generation (called from user's `build.rs`)
- [`migrations.md`](./migrations.md) — `cargo reef migrate *` design + libSQL specifics

## Design principles for cargo-reef

1. **Wrap dx, never replace it.** `cargo reef build` invokes `dx build`; doesn't reimplement. dx remains the source of truth for compilation.
2. **Configuration lives in `.reef/config.toml`.** Programmatic project description that drives every Reef command. Single source of truth for "what is this project?" — no flag duplication across commands.
3. **One command per intent.** `cargo reef build` produces every artifact. `cargo reef deploy` ships to every configured target. Users don't memorize per-environment incantations.
4. **Flags override config.** `cargo reef build --bin admin --env=staging` is allowed for one-off runs; the durable answer lives in `.reef/config.toml`.
5. **Embed reef-rs/template at build time.** `cargo reef new` is offline, fast, and version-locked to the cargo-reef binary. No runtime clone, no auth needed.
6. **Adapter pattern for deploy targets.** Each target (fly, cloudflare, nixos, vercel, etc.) is a module implementing a small trait. Adding a new target is a single file.

## Non-goals (for v0.5)

- Replacing `dx serve` or `dx build` — those stay the canonical dev/build commands. cargo-reef just orchestrates them.
- A full TUI / interactive build dashboard — text logs are enough.
- Arbitrary plugin system — adapters are first-class code in cargo-reef, not third-party plugins.
- Cross-cloud orchestration (Terraform-like) — cargo-reef deploys ONE app to ONE target per invocation; multi-region/multi-cloud is the user's job.
