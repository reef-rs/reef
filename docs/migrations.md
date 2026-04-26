# `cargo reef migrate *`

SQL migration management for Reef projects. Migrations live in `migrations/` at the project root; the runner lives in `cargo-reef`, not in user projects (per the architectural decision documented in `reef-template/CLAUDE.md`).

## Subcommands

```
cargo reef migrate run                # apply pending migrations
cargo reef migrate new <name>         # generate a timestamped .sql file
cargo reef migrate status             # show applied vs pending
cargo reef migrate revert             # roll back the last migration (when DOWN files exist)
```

## File layout

```
migrations/
├── 20260425_120000_init.sql              # always SQL — no Rust files
├── 20260425_120000_init.down.sql         # optional: rollback (paired with same timestamp)
├── 20260426_093000_add_users.sql
└── 20260426_093000_add_users.down.sql
```

**Naming convention:** `<timestamp>_<name>.sql` for forward, optional `<timestamp>_<name>.down.sql` for rollback. Timestamps over numeric sequences because parallel work doesn't conflict.

## Schema-tracking table

Auto-created on first `migrate run`:

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
    name TEXT PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    duration_ms INTEGER,
    checksum TEXT
);
```

`checksum` is sha256 of the .sql file contents — lets us detect if a migration was edited after being applied (warn loudly).

## Connection target

Reads from `.reef/config.toml`'s `[storage]`:

```toml
[storage]
backend = "libsql"
db_url_env = "DATABASE_URL"
db_path_default = "./data/app.db"
migrations_dir = "migrations"
```

Resolution: `DATABASE_URL` env var if set, else `db_path_default`. For deploy environments, `DATABASE_URL` is the canonical source.

## `cargo reef migrate run`

Algorithm:

```
1. Connect to the configured DB.
2. Ensure `schema_migrations` table exists.
3. Read `migrations/*.sql`, sorted lexicographically by filename.
4. For each file NOT in schema_migrations:
   a. Open transaction
   b. Compute checksum
   c. Execute the file's SQL
   d. INSERT INTO schema_migrations (name, duration_ms, checksum)
   e. Commit
5. For each file ALREADY in schema_migrations:
   a. If checksum mismatches → warn loudly (someone edited a committed migration)
6. Print a summary of applied migrations and total duration.
```

Errors abort the transaction and the migration. The next run picks up from where it left off.

## `cargo reef migrate new <name>`

Generates `migrations/<timestamp>_<name>.sql` with a comment header:

```sql
-- Migration: add_users_table
-- Generated: 2026-04-25T12:34:56Z
-- Forward — applied by `cargo reef migrate run`

-- Your CREATE/ALTER/DROP statements here.
```

If `--with-down` flag, also generates `<timestamp>_<name>.down.sql` with a corresponding rollback skeleton.

## `cargo reef migrate status`

```
$ cargo reef migrate status

Database: ./data/app.db
Schema:   schema_migrations (4 applied)

Applied:
  ✓ 20260425_120000_init                applied 2026-04-25T12:00:32Z (12ms)
  ✓ 20260426_093000_add_users           applied 2026-04-26T09:30:11Z (8ms)
  ✓ 20260427_140000_add_sessions        applied 2026-04-27T14:00:55Z (15ms)
  ✓ 20260428_100000_add_audit_log       applied 2026-04-28T10:00:09Z (22ms)

Pending:
  → 20260429_090000_add_indexes         (not yet applied)
```

## `cargo reef migrate revert`

Rolls back the last applied migration. Requires a `<timestamp>_<name>.down.sql` to exist for that migration. If missing, fails with "no DOWN script for <name>".

## Schema-as-code (`cargo reef db:push`) — shipped in v0.2

`src/server/db/schema.rs` is the single source of truth for the DB shape. `cargo reef db:push` parses it, diffs against the live DB, and applies the changes (Drizzle-style).

See **[`db-push.md`](./db-push.md)** for the full surface — schema syntax (`#[reef::table]`, `#[column]`, `#[index]`, etc.), the `--features` flag for cfg-aware multi-deployment schemas, the diff preview format, and safety guardrails (`--allow-drop`, NOT NULL backfill rules, libSQL ALTER COLUMN tightening).

**Coexistence with this command:** `migrate run` and `db:push` share the `schema_migrations` tracking table. Two common workflows:

1. **`migrate run` for bootstrap, `db:push` for iteration.** Template ships with an init migration that creates the starter schema; `db:push` handles every subsequent change. For production, use `db:push --write <name>` to capture each diff as a versioned migration file that CI applies via `migrate run`.
2. **All file-based.** Hand-author every migration with `migrate new <name>` and `migrate run`. Treat `schema.rs` as documentation that mirrors the SQL.

The schema is `Drizzle-like` — diff-driven from a Rust source-of-truth — but you can opt out per project.

## What this is NOT

- Not a SQL formatter — `.sql` files are user-authored
- Not a transactional sandbox — migrations apply against the real DB. Test in a separate environment.
- Not multi-DB-aware — one `.reef/config.toml` describes one DB. Multi-tenant migrations are the user's concern.

## Open questions

- **Idempotency** — should we lint user migrations for non-idempotent ops (e.g., `CREATE TABLE` without `IF NOT EXISTS`)? Probably warn, not block.
- **DOWN files mandatory or optional?** Probably optional. Most users don't write rollbacks; they roll forward with corrective migrations.
- **Concurrent runs?** Use a `BEGIN EXCLUSIVE` transaction or DB-level lock to prevent two migrators stepping on each other. libSQL supports this.
- **Cloud-hosted libSQL (Turso)?** The runner just speaks the libSQL wire protocol — should work without changes. Test on real Turso cloud during v0.5 dev.
