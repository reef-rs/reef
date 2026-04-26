# Reef

A modern full-stack framework for Rust. One codebase, three deployment shapes: a thick desktop client, a 30 MB cloud binary, and an offline-capable edge node.

Reef sits on top of [Dioxus 0.7](https://dioxuslabs.com/) (rendering + routing + typed RPC), [libSQL](https://github.com/tursodatabase/libsql) (SQLite-compatible storage with embedded replicas), and [Tailscale](https://tailscale.com/) (identity at L3, no per-endpoint auth). It's opinionated about the things that don't matter (file layout, build orchestration, schema-as-code) so you can spend the time on what does.

```bash
cargo install cargo-reef
cargo reef new my-app
cd my-app
cargo reef migrate run        # bootstrap the database
cargo reef dev                # launch the dev loop
```

## What's in this repo

This is a **Cargo workspace** publishing three crates that ship in lockstep:

| Crate | Purpose | Where users see it |
|---|---|---|
| `cargo-reef` | CLI scaffolder + migration runner + `db:push` | `cargo install cargo-reef` |
| `reef` | Runtime: `#[reef::table]` attribute, `Json<T>` / `Jsonb<T>` wrappers, future `Db` helpers | `reef = "0.2"` in user `Cargo.toml` |
| `reef-macros` | Proc-macro impls for `reef` (transitive dep, never named directly) | — |

Plus a `template/` directory that's embedded into the `cargo-reef` binary at compile time and copied out by `cargo reef new`.

```
.
├── crates/
│   ├── cargo-reef/      ← the CLI
│   ├── reef/            ← user-facing runtime
│   └── reef-macros/     ← proc-macro impls
├── template/            ← what `cargo reef new` scaffolds
└── docs/                ← design docs (cli, build, deploy, migrations, db-push)
```

## Status

**v0.2 — schema-as-code released.** What works today:

- `cargo reef new <name>` — scaffold a Dioxus 0.7 fullstack app
- `cargo reef dev` — wraps `dx serve --web`
- `cargo reef migrate run | new | status | revert` — file-based SQL migrations with checksum drift detection
- `cargo reef db:push` — Drizzle-style schema-as-code: edit `src/server/db/schema.rs`, diff against the live DB, preview, apply
- `#[reef::table]` with the full SQLite/libSQL feature surface: composite PKs, composite FKs, FK actions, generated columns (stored + virtual), STRICT, WITHOUT ROWID, named CHECKs, expression indexes (`json_extract` etc.)
- `Json<T>` / `Jsonb<T>` newtype wrappers for typed JSON columns (TEXT and BLOB respectively)
- `--features X,Y` for cfg-aware multi-deployment schemas (one `schema.rs`, N binaries via Cargo features)
- Cross-table FK validation with "did you mean?" suggestions
- `--allow-drop` belt-and-suspenders for destructive diffs

See [`docs/db-push.md`](./docs/db-push.md) for the full schema-as-code surface and [`docs/cli.md`](./docs/cli.md) for the rest of the CLI.

## What's planned

- v0.3 — `cargo reef build` orchestrator, `cargo reef deploy` (Fly / Cloudflare / NixOS targets), CHECK-constraint introspection, the SQLite 12-step rebuild dance for currently-flagged-as-manual changes, `cargo reef doctor`
- v0.5+ — `cargo reef db:reset`, `cargo reef db:seed`, codemod-based `cargo reef upgrade`

See [`docs/`](./docs/) for the design notes that drive each piece.

## Reefer Ruleset

Framework principles, one line each. Long-form in scaffolded projects' `docs/ruleset.md`.

1. Code cleanliness is next to godliness — delete more than you write
2. Deployment is a hardware distinction, not software — same binary, different roles
3. One binary per role, not per environment
4. Offline is a first-class state
5. Trust the type system, not the README
6. Identity at L3, not L7
7. The wire format is the contract — typed RPC, no OpenAPI specs
8. Dependencies are debt
9. Vendors come and go; standards remain — SQLite over X, OIDC over home-grown auth
10. Async is the default; sync is the special case
11. Compile times are a tax — pay them at CI, not at every keystroke
12. `unsafe` and `Box<dyn Trait>` should make you stop and think
13. A crate marks a binary or target boundary; otherwise it's a module

## License

MIT OR Apache-2.0
