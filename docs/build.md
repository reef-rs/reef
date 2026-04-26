# `cargo reef build`

Single command to produce all the artifacts a Reef project ships. Reads `.reef/config.toml`, calls `dx build` per configured target/bin, runs the supporting build steps (tailwind, route generation), and reports.

---

## Mental model

```
.reef/config.toml [build] section
        │
        ▼
parse → list of (bin name, features, target platform) tuples
        │
        ├─→ run tailwindcss (if [build.tailwind].enabled)
        │
        ├─→ for each tuple:
        │       dx build --bin <name> --features <list> --<platform> --release
        │
        └─→ collect artifacts into out_dir, print summary
```

**`build.rs` runs as part of each `dx build`** invocation — Cargo guarantees it. So route regeneration from `src/app/` happens automatically; `cargo reef build` doesn't need to invoke it explicitly.

---

## Config schema (build-relevant parts)

```toml
# .reef/config.toml

[build]
# Default targets `cargo reef build` produces. Each entry resolves to a single
# `dx build` invocation.
targets = ["web", "server"]

# Optional targets users can opt into via `cargo reef build --target=desktop`.
optional_targets = ["desktop", "mobile"]

# Map of named binary configurations. `cargo reef build --bin <name>` picks
# one of these. If `bins` is empty, cargo-reef invokes `dx build` once with
# the project's default bin.
[build.bins]
core   = { features = ["public"],         platforms = ["web", "server"] }
admin  = { features = ["public", "cloud"], platforms = ["web", "server"] }
edge   = { features = ["public", "edge"],  platforms = ["server"] }

[build.tailwind]
enabled = true
input  = "tailwind.css"
output = "assets/tailwind.css"

[build.assets]
optimize_images = false   # opt-in: run image optimization on PNG/JPG before bundling
hash_for_cache_busting = true   # manganis already does this; this just toggles for non-manganis files
```

---

## Resolution rules

`cargo reef build` (no flags) does:

1. Read `.reef/config.toml`.
2. If `[build.bins]` is non-empty: build every entry in `[build.bins]` for every platform listed.
3. Else: invoke `dx build --release` for each target in `[build].targets`.
4. Run tailwindcss if enabled, before the dx invocations.
5. Stream output, fail fast on any error.

`cargo reef build --bin <name>` builds only that bin. `--bin <name> --target <platform>` builds only that bin for that platform.

`cargo reef build --features "x y" --no-default-features --bin <name>` overrides what's in config — useful for experiments. The override is one-shot; the config file is the durable answer.

---

## Equivalence to manual dx invocations

```bash
# `cargo reef build` with two bins × two platforms ≡ four dx invocations:
dx build --bin core  --features public        --release --web
dx build --bin core  --features public        --release --server
dx build --bin admin --features public,cloud  --release --web
dx build --bin admin --features public,cloud  --release --server
```

Plus tailwindcss prebuild + asset optimization, all serialized so failure short-circuits.

---

## Multi-target parallelism

For v0.5 minimum: serial. Run dx invocations in order, fail fast.

For v0.6+: optional `--parallel` flag that runs independent dx builds concurrently. Each dx invocation is single-threaded internally; running multiple in parallel uses more cores. Trade-off: noisier logs, OOM risk on small CI machines.

---

## Caching

dx already caches per-target compilation in `target/dx/`. cargo-reef does NOT add a second caching layer — that would be a footgun. Just relies on dx's incremental compilation.

`cargo reef build --no-cache` passes through to dx as `--force`.

---

## Failure modes & error reporting

- **dx not installed** → fail with explicit "install dx with `cargo install dioxus-cli`" message.
- **Cargo.toml missing a bin / feature listed in config** → fail before invoking dx, point at the `.reef/config.toml` line.
- **dx build fails for one target** → stop, print which target failed, skip the rest.
- **Tailwind missing when `[build.tailwind].enabled = true`** → install via `npx --no-install tailwindcss` or fail with install instructions.

---

## What `cargo reef build` is NOT

- Not a Cargo replacement — it just orchestrates dx
- Not a bundler — dx + tauri-bundler handle that
- Not a dev tool — `cargo reef dev` (or just `dx serve --web`) handles dev. `build` is for releases and CI.

---

## Open questions

- Should `cargo reef build` emit a manifest of produced artifacts (paths, sizes, hashes) for downstream `cargo reef deploy` to consume? Probably yes — write to `.reef/cache/last-build.json`.
- Should it auto-version artifacts (git SHA in filename)? Probably yes for `--release`; opt-out via flag.
- How do per-environment differences interact with build? Mostly env vars (`DATABASE_URL`, etc.) — the BUILD itself shouldn't change per env, only deploy config should. Keep it that way.
