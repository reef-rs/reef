# `cargo reef deploy`

Deploys the build artifacts produced by `cargo reef build` to the target configured in `.reef/config.toml`. One command, all environments — flags select the env / branch / specific bin.

---

## Mental model

```
.reef/config.toml [deploy] section + [env.<name>] override
        │
        ▼
parse → (target adapter, target-specific config, secrets resolution)
        │
        ▼
adapter trait dispatches:
    Fly        → flyctl deploy --config fly.toml --image-label ...
    Cloudflare → wrangler deploy --env ...
    NixOS      → nixos-rebuild switch --target-host ... --build-host ...
    Docker     → docker push <registry>/<image>:<tag>; ssh deploy script
    Static     → rsync to S3/R2/etc., invalidate CDN cache
```

cargo-reef holds adapter implementations as first-class code — adding a new target is one new module + a Cargo.toml feature.

---

## Config schema (deploy-relevant parts)

```toml
# .reef/config.toml

[deploy]
target = "fly"                # adapter selector — see "Adapters" below
default_env = "staging"        # which [env.<name>] block applies when no --env flag

[deploy.fly]                   # target-specific config
app = "myapp"
config_file = "fly.toml"
strategy = "rolling"           # rolling | bluegreen | canary | immediate

[deploy.cloudflare]            # only present when target = "cloudflare"
account_id_env = "CF_ACCOUNT_ID"
worker_name = "myapp-worker"

[deploy.nixos]
hosts = ["edge-01.tail123abc.ts.net", "edge-02.tail123abc.ts.net"]
flake_target = ".#myapp"

# Per-environment overrides — merged on top of the base config
[env.dev]
[env.dev.fly]
app = "myapp-dev"

[env.staging]
[env.staging.fly]
app = "myapp-staging"

[env.prod]
[env.prod.fly]
app = "myapp"
strategy = "bluegreen"

# Secrets — referenced by env var name, NEVER stored inline
[deploy.secrets]
DATABASE_URL = "DATABASE_URL"        # resolved from process env at deploy time
OIDC_CLIENT_SECRET = "OIDC_SECRET"
```

---

## Adapter trait

```rust
#[async_trait]
pub trait DeployAdapter {
    /// Identifier used in `[deploy].target`.
    const NAME: &'static str;

    /// Validate the config block + environment overrides at parse time.
    /// Fail before invoking any external tool.
    fn validate(config: &DeployConfig, env: &EnvConfig) -> Result<()>;

    /// Pre-deploy hook: confirm CLI tools are installed, secrets are present, etc.
    async fn preflight(&self, ctx: &DeployContext) -> Result<()>;

    /// Push the built artifacts. Streams output back to the user.
    async fn deploy(&self, ctx: &DeployContext) -> Result<DeployResult>;

    /// Optional rollback. Not all adapters support this.
    async fn rollback(&self, ctx: &DeployContext, to: RollbackTarget) -> Result<()> {
        Err(anyhow!("rollback not supported by {}", Self::NAME))
    }
}
```

Adapters live in `src/deploy/<name>.rs`. Adding one is:
1. New file with the adapter struct
2. Implement the trait
3. Register in `src/deploy/registry.rs`
4. Document the `[deploy.<name>]` schema in this file

---

## Adapters (planned)

| Name | Status | What it wraps | Notes |
|---|---|---|---|
| `fly` | TODO | `flyctl deploy` | The default for fullstack web apps |
| `cloudflare` | TODO | `wrangler deploy` | For Cloudflare Workers — limited Rust support, requires WASM-only build |
| `nixos` | TODO | `nixos-rebuild switch --target-host` | For self-hosted servers / edge devices |
| `docker` | TODO | `docker build` + `docker push` + remote `docker compose pull && up` | For self-hosted via container orchestration |
| `static` | TODO | rsync / aws s3 sync / cloudflare r2 cli | For pre-rendered SSG output (no server) |
| `vercel` | Maybe | `vercel deploy` | If we add Vercel-as-target support |
| `headscale` | TODO | Push tailnet ACL changes via Headscale API | For deploying ACL changes alongside code |

Adapters should be opt-in via Cargo features so a minimal `cargo-reef` install doesn't drag in every cloud SDK:

```toml
[features]
default = []
fly = ["dep:flyctl-helpers"]
cloudflare = ["dep:wrangler-runtime"]
nixos = []   # shells out to nixos-rebuild, no Rust deps
all-adapters = ["fly", "cloudflare", "nixos", "docker", "static"]
```

Users add `cargo install cargo-reef --features fly` to get just the adapter they need.

---

## Common workflows

```bash
# Deploy to default env (whatever default_env is in config)
cargo reef deploy

# Deploy to a specific env (overrides default_env)
cargo reef deploy --env=prod

# Deploy a specific bin (e.g., admin only)
cargo reef deploy --bin=admin --env=staging

# Deploy a specific git branch as a Fly preview environment
cargo reef deploy --env=preview --branch=feat/new-thing
# → adapter receives env="preview" + branch="feat/new-thing", does the right thing
#   (e.g., for Fly: deploys to app-name-feat-new-thing.fly.dev)

# Rollback (adapter must support it)
cargo reef deploy rollback --env=prod --to=previous   # or --to=<sha>
```

---

## Build → deploy interaction

`cargo reef deploy` reads `.reef/cache/last-build.json` (written by `cargo reef build`) to find the artifacts. If no recent build matches the requested env/bin, it runs `cargo reef build` first, then deploys.

```bash
cargo reef deploy --env=prod
# 1. Check .reef/cache/last-build.json — has prod-compatible artifacts?
# 2. If no: run `cargo reef build --env=prod`
# 3. Run preflight: confirm flyctl is installed, FLY_API_TOKEN is set, etc.
# 4. Run adapter.deploy() — streams output
# 5. On success: write to .reef/cache/last-deploy.json (target, env, sha, timestamp)
```

`--no-build` skips step 1-2 and uses the existing artifacts. `--force-build` always rebuilds.

---

## Secrets resolution

Secrets NEVER live in `.reef/config.toml` (it's committed). Schema only declares which env-var names provide each secret:

```toml
[deploy.secrets]
DATABASE_URL = "DATABASE_URL"            # the secret value comes from process env
OIDC_CLIENT_SECRET = "OIDC_SECRET"
SENTRY_DSN = "SENTRY_DSN_STAGING"        # different env-var name OK
```

At deploy time, cargo-reef reads the values from process env (or `.env` if dotenv is enabled) and passes them to the adapter, which forwards to the target (`flyctl secrets set`, `wrangler secret put`, NixOS sops-nix, etc.).

Missing required secrets fail at preflight, not mid-deploy.

---

## What `cargo reef deploy` is NOT

- Not a CI orchestrator — runs once, exits. Use GitHub Actions / Fly's auto-deploy / etc. for the loop.
- Not infrastructure-as-code — doesn't create databases, DNS records, or LB configs. Adapter assumes infra exists.
- Not multi-region-aware — one adapter, one target per invocation. Multi-region happens at the adapter level (e.g., Fly's regional deploys) or by running `cargo reef deploy` multiple times.
- Not a secret manager — secrets pass through, never get stored.

---

## Open questions

- **Where does `cargo reef deploy` get artifact info?** Proposal: `.reef/cache/last-build.json`. Alternative: re-derive from `dist/` contents. Probably both — manifest is canonical, dir scan is the fallback for hand-built artifacts.
- **Pre-deploy hooks?** A user-defined script that runs before deploy (e.g., apply migrations to the target DB). Probably yes — `[deploy.hooks]` with shell-script paths. Be careful not to reinvent Make.
- **Deploy diffing?** "What would change?" preview. Adapter-dependent — Fly has `flyctl status` + `--dry-run`; NixOS has `nixos-rebuild dry-build`. Optional flag, not all adapters can support it.
- **Rollback semantics?** Per-adapter. Some support point-in-time (Fly), some support previous-release (NixOS profile rollback), some don't support it (static rsync). Document per-adapter what's available.
