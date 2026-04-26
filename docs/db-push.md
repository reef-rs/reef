# `cargo reef db:push`

Drizzle-style schema-as-code. Edit `src/server/db/schema.rs`, run `db:push`, and the live DB is brought into agreement.

```bash
cargo reef db:push                      # preview, prompt, apply
cargo reef db:push -y                   # apply without prompting
cargo reef db:push --dry-run            # preview only, no DB writes
cargo reef db:push --write add_users    # capture as migrations/<ts>_add_users.sql instead of applying
cargo reef db:push --allow-drop         # required when the diff drops a table or column
cargo reef db:push --features server,cloud    # evaluate cfg gates against this feature set
```

## What it does

1. Parses `src/server/db/schema.rs` with `syn`, looking for `#[reef::table]` structs.
2. Connects to the database resolved from `.reef/config.toml` `[storage]` (`db_url_env` env var or `db_path_default`).
3. Introspects the live DB via `PRAGMA table_info` / `index_list` / `foreign_key_list` and `sqlite_master`.
4. Diffs the two schemas and renders a human-readable preview.
5. Either prompts for confirmation and applies, or writes the SQL to a migration file.

## Coexistence with `migrate run`

The bundled template ships an init migration in `migrations/` that creates the initial `greeting` table. The flow on a fresh project:

```bash
cargo reef new my-app && cd my-app
cargo reef migrate run         # bootstrap from migrations/
cargo reef db:push             # "schema is up to date" — Greeting matches what migrate run created
```

After bootstrap, you can either:
- **Stay schema-as-code**: edit `schema.rs`, run `db:push` for each iteration. For production deploys, use `db:push --write <name>` to capture each diff as a versioned migration file that CI can apply with `migrate run`.
- **Stay file-based**: keep using `cargo reef migrate new <name>` for hand-authored migrations, treat `schema.rs` as documentation that mirrors the SQL.

The two systems coexist because they share the `schema_migrations` tracking table. `db:push` writes directly without recording in `schema_migrations`; `migrate run` records what it applied. If you `db:push --write`, the resulting file gets picked up by `migrate run` like any other migration.

## Schema-as-code surface

```rust
use reef::{Json, Jsonb};

#[reef::table(strict)]
#[index(name = "users_email_idx", columns = ["email"])]
pub struct User {
    #[column(primary_key, auto_increment)]
    pub id: i64,
    #[column(unique)]
    pub email: String,
    pub name: String,
    pub bio: Option<String>,                 // → TEXT (nullable from Option<T>)
    pub tags: Json<Vec<String>>,             // → TEXT (JSON-stored)
    pub metadata: Jsonb<UserMetadata>,       // → BLOB (JSONB-stored, SQLite 3.45+)
    #[column(default = "active",
             check = "status IN ('active','disabled')")]
    pub status: String,
}
```

**Table-level**: `name = "..."`, `strict`, `without_rowid`.
**Field-level `#[column(...)]`**: `primary_key`, `auto_increment`, `unique`, `default = ...`, `default_sql = "..."`, `check = ...`, `references = "table(col)"`, `on_delete = "..."`, `on_update = "..."`, `generated = "..."`, `generated_kind = "stored" | "virtual"`.

**Defaults: `default` vs `default_sql`** — easy to confuse:

| Form | What you write | What Reef emits |
|---|---|---|
| String literal | `default = "active"` | `DEFAULT 'active'` (Reef adds the SQL quotes) |
| Numeric / bool | `default = 0`, `default = true` | `DEFAULT 0`, `DEFAULT true` (raw) |
| **SQL expression** | `default_sql = "datetime('now')"` | `DEFAULT (datetime('now'))` (verbatim, paren-wrapped) |

Use `default_sql` whenever you need a SQL function call or expression — `datetime('now')`, `unixepoch()`, etc. The two keys are mutually exclusive on the same column.

**Common mistake:** writing `default = "'active'"` (Rust string containing quotes). Reef adds *another* layer of quoting → `DEFAULT '''active'''`. Drop the inner quotes — Reef handles them.
**Struct-level helpers**: `#[index(...)]`, `#[primary_key(columns = [...])]`, `#[foreign_key(...)]`, `#[check(name = ..., expr = ...)]`.

FK action values: `cascade`, `restrict`, `set_null`, `set_default`, `no_action`.

Naming: struct names snake_case to table names without pluralization. `User` → `user`, `PostLike` → `post_like`. Want plural? Either name the struct `Users` or override with `#[reef::table(name = "users")]`.

## Multi-deployment with `--features`

Schema gating via `#[cfg(feature = "...")]` lets one `schema.rs` describe multiple binaries' DB shapes (Reefer Rule 3 — one binary per role):

```rust
// schema.rs — single source of truth for every deployment

#[reef::table] pub struct User { ... }                         // always
#[reef::table] pub struct Post { ... }                         // always

#[cfg(feature = "cloud")]
#[reef::table] pub struct Tenant { ... }                       // SaaS build only

#[cfg(feature = "cloud")]
#[reef::table] pub struct Subscription { ... }                 // SaaS build only

#[cfg(feature = "desktop")]
#[reef::table] pub struct OfflinePref { ... }                  // desktop build only

#[cfg(all(feature = "server", not(feature = "cloud")))]
#[reef::table] pub struct AuditLog { ... }                     // any non-cloud server
```

Each binary's deployment runs `db:push` with the same features it was compiled with:

```bash
# SaaS / cloud build: includes Users, Post, Tenant, Subscription
cargo reef db:push --features server,cloud

# Desktop build: includes Users, Post, OfflinePref, AuditLog
cargo reef db:push --features server,desktop

# Self-hosted / on-prem build: includes Users, Post, AuditLog
cargo reef db:push --features server
```

Without `--features`, the parser is **unconstrained** — every `#[reef::table]` is included regardless of cfg. This is the back-compat default for projects that don't gate their schema.

The cfg evaluator handles `feature = "X"`, `not(...)`, `all(...)`, `any(...)`, and arbitrary nesting. Other cfg predicates (`target_os`, etc.) lenient-default to `true` to avoid hiding tables.

Cross-table FK validation runs **after** cfg filtering, so a FK that only exists in a feature-gated table doesn't error in builds where the gate is off.

## Diff preview format

```
Schema changes:
  + CREATE TABLE comment
  ~ post.author_id: ALTER (FK changed)
  + INDEX posts_title_idx ON post (title)
  ~ users.bio: ALTER (NULL → NOT NULL)
  + users.created_at: ADD INTEGER
  - users.name: DROP

Manual migration required (`cargo reef migrate new <name>`):
  ! users: STRICT changed (false -> true) — requires table rebuild

Warnings:
  ⚠ users.bio: tightening change — libSQL ALTER COLUMN applies to new writes
    only, existing rows are not revalidated. Backfill manually if needed.
```

- `+` add, `-` drop, `~` alter
- `!` manual migration required (the diff couldn't safely express it; write the SQL by hand)
- `⚠` advisory — the auto-applicable change has a behavioral caveat

## Safety guardrails

| Case | Behavior |
|---|---|
| Mixed auto + manual changes | Refuses silent partial application. Re-run with `--write <name>` to capture the auto changes as a migration file, then write the manual parts by hand. |
| Drops (DROP TABLE / DROP COLUMN) | Preview shows them, but `db:push` refuses to apply without `--allow-drop`. `--dry-run` and `--write` still show drops without requiring the flag. |
| Adding NOT NULL column without DEFAULT | Flagged as `NeedsRebuild` — SQLite can't backfill, so a rebuild or a manual migration is required. |
| Confirm prompt | Defaults to `no`. Accidental Enter doesn't apply. |

## libSQL `ALTER COLUMN` (and the tightening footgun)

Reef takes advantage of libSQL's `ALTER TABLE ALTER COLUMN ... TO ...` extension (over stock SQLite) for type / constraint / FK changes that would otherwise need a 12-step table rebuild.

**Footgun**: libSQL's `ALTER COLUMN` applies to **new writes only** — existing rows are NOT rewritten or revalidated. So if you tighten a constraint (NULL → NOT NULL, looser CHECK → stricter CHECK), existing rows that violate the new rule stay violating. The diff emits a tightening warning so you can decide whether to backfill manually before pushing.

## Known gaps (deferred)

- **CHECK constraints not introspected.** Live DB CHECKs only live in `sqlite_master.sql`; parsing them safely needs a real SQL parser. The diff trusts the schema source for CHECKs — we won't drift-warn on them yet. (~half day with `sqlparser-rs` to fix; deferred to v0.3.)
- **Composite-PK / STRICT / WITHOUT ROWID changes** flag `NeedsRebuild` rather than attempting the 12-step rebuild dance. (~1-2 weeks for production-grade; deferred to v0.3+.)
