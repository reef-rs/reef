//! `cargo-reef` — CLI scaffolder + tooling for Reef apps.
//!
//! Subcommands:
//!   - `new <name>`               Scaffold a new app from the embedded template
//!   - `dev`                      Start the dev loop (`dx serve --web` + banner)
//!   - `migrate run`              Apply pending SQL migrations
//!   - `migrate new <name>`       Generate a timestamped migration file
//!   - `migrate status`           Show applied vs pending migrations
//!   - `migrate revert`           Roll back the last migration (requires *.down.sql)
//!
//! Designed in `docs/` — see cli.md, build.md, deploy.md, migrations.md.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use console::style;
use include_dir::{include_dir, Dir, DirEntry};
use serde::Deserialize;

mod schema;

/// The Reef template — committed at the workspace root in `template/`,
/// embedded into the binary at compile time. Single source of truth.
static TEMPLATE: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../template");

// ============================================================================
//  CLI
// ============================================================================

/// Cargo invokes us as `cargo-reef reef <args>` (cargo prepends the subcommand
/// name). This wrapper consumes the leading "reef" arg.
#[derive(Parser)]
#[command(bin_name = "cargo")]
enum CargoCli {
    Reef(ReefArgs),
}

#[derive(Parser)]
#[command(
    name = "reef",
    about = "Scaffold and manage Reef apps",
    version,
    disable_help_subcommand = true
)]
struct ReefArgs {
    #[command(subcommand)]
    cmd: ReefCommand,
}

#[derive(Subcommand)]
enum ReefCommand {
    /// Scaffold a new Reef app from the embedded template.
    New {
        /// Name of the app, or a path (e.g. `my-app`, `../my-app`, `/tmp/test`).
        name: String,
    },

    /// Start the dev loop (sugar for `dx serve --web` with a Reef banner).
    Dev {
        /// Additional args to pass through to `dx serve`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },

    /// Database migration commands.
    Migrate {
        #[command(subcommand)]
        cmd: MigrateCommand,
    },

    /// Parse a `schema.rs` file and print the IR as JSON. Hidden — used to
    /// debug the schema-as-code parser before `db:push` lands.
    #[command(name = "_debug-schema", hide = true)]
    DebugSchema {
        /// Path to a `schema.rs` file (defaults to `src/server/db/schema.rs`).
        #[arg(default_value = "src/server/db/schema.rs")]
        path: std::path::PathBuf,
    },

    /// Parse a `schema.rs` file and print the emitted SQL. Hidden — used to
    /// eyeball the SQL emitter before `db:push` lands.
    #[command(name = "_debug-sql", hide = true)]
    DebugSql {
        #[arg(default_value = "src/server/db/schema.rs")]
        path: std::path::PathBuf,
    },

    /// Introspect a live SQLite/libSQL database and print the IR as JSON.
    /// Hidden — used to eyeball the introspector before `db:push` lands.
    #[command(name = "_debug-introspect", hide = true)]
    DebugIntrospect {
        /// Path to the SQLite database file.
        #[arg(default_value = "./data/reef.db")]
        db: std::path::PathBuf,
    },

    /// Diff a schema.rs against a live database and preview the changes.
    /// Hidden — preview of what `db:push` will do once it lands.
    #[command(name = "_debug-diff", hide = true)]
    DebugDiff {
        #[arg(long, default_value = "src/server/db/schema.rs")]
        schema: std::path::PathBuf,
        #[arg(long, default_value = "./data/reef.db")]
        db: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
enum MigrateCommand {
    /// Apply all pending migrations.
    Run,
    /// Generate a new migration file in `migrations/`.
    New {
        /// Short snake_case name describing the migration (e.g. `add_users_table`).
        name: String,
        /// Also generate a paired `<ts>_<name>.down.sql` rollback skeleton.
        #[arg(long)]
        with_down: bool,
    },
    /// Show applied vs pending migrations.
    Status,
    /// Roll back the last applied migration (requires `*.down.sql`).
    Revert,
}

fn main() -> ExitCode {
    let CargoCli::Reef(args) = CargoCli::parse();

    let result = match args.cmd {
        ReefCommand::New { name } => scaffold_new(&name),
        ReefCommand::Dev { extra } => run_dev(&extra),
        ReefCommand::Migrate { cmd } => run_migrate(cmd),
        ReefCommand::DebugSchema { path } => debug_schema(&path),
        ReefCommand::DebugSql { path } => debug_sql(&path),
        ReefCommand::DebugIntrospect { db } => block_on(debug_introspect(&db)),
        ReefCommand::DebugDiff { schema, db } => block_on(debug_diff(&schema, &db)),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} {:#}", style("error:").red().bold(), e);
            ExitCode::FAILURE
        }
    }
}

// ============================================================================
//  cargo reef new
// ============================================================================

fn scaffold_new(input: &str) -> Result<()> {
    let target = PathBuf::from(input);
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("could not derive a project name from `{input}`"))?;

    validate_name(name)?;

    if target.exists() {
        bail!(
            "directory `{}` already exists — pick a different name or remove it",
            target.display()
        );
    }

    print_banner();

    std::fs::create_dir_all(&target)
        .with_context(|| format!("creating directory `{}`", target.display()))?;

    let mut count = 0;
    walk_template(&TEMPLATE, &target, name, &mut count)?;

    init_git(&target);

    println!();
    println!(
        "{} Generated {} files in {} ({})",
        style("✓").green().bold(),
        count,
        style(input).bold(),
        style(format!("package: {name}")).dim()
    );
    println!();
    println!("Next steps:");
    println!();
    println!("  {} {}", style("$").dim(), style(format!("cd {input}")).bold());
    println!("  {} {}", style("$").dim(), style("cargo reef migrate run").bold());
    println!("  {} {}", style("$").dim(), style("cargo reef dev").bold());
    println!();
    println!("Learn more at {}", style("https://reef.rs").underlined().cyan());
    println!();

    Ok(())
}

fn walk_template(dir: &Dir, target: &Path, project_name: &str, count: &mut usize) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(d) => {
                let dst = target.join(d.path());
                std::fs::create_dir_all(&dst).with_context(|| format!("mkdir {}", dst.display()))?;
                walk_template(d, target, project_name, count)?;
            }
            DirEntry::File(f) => {
                let dst = target.join(f.path());
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("mkdir {}", parent.display()))?;
                }
                let bytes = if let Some(text) = f.contents_utf8() {
                    substitute(text, project_name).into_bytes()
                } else {
                    f.contents().to_vec()
                };
                std::fs::write(&dst, bytes).with_context(|| format!("write {}", dst.display()))?;
                *count += 1;
            }
        }
    }
    Ok(())
}

fn substitute(text: &str, project_name: &str) -> String {
    text.replace("reef-template", project_name)
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("project name cannot be empty");
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() {
        bail!("project name must start with a letter, got `{name}`");
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            bail!(
                "project name `{name}` contains invalid character `{c}` (use letters, digits, _, -)"
            );
        }
    }
    Ok(())
}

fn init_git(target: &Path) {
    let _ = std::process::Command::new("git")
        .arg("init")
        .arg("--quiet")
        .current_dir(target)
        .status();
}

// ============================================================================
//  cargo reef dev
// ============================================================================

fn run_dev(extra: &[String]) -> Result<()> {
    print_banner();
    println!("{}", style("Starting dev loop (dx serve --web)…").dim());
    println!();

    // Verify dx is installed before we exec — friendlier error than a "not found" trap
    let dx_check = std::process::Command::new("dx").arg("--version").output();
    if dx_check.is_err() {
        bail!(
            "dx (Dioxus CLI) not found in PATH. Install with:\n\n  \
             cargo install dioxus-cli\n\n\
             then retry `cargo reef dev`."
        );
    }

    let status = std::process::Command::new("dx")
        .arg("serve")
        .arg("--web")
        .args(extra)
        .status()
        .context("launching dx serve")?;

    if !status.success() {
        bail!("dx serve exited with status {status}");
    }
    Ok(())
}

// ============================================================================
//  cargo reef migrate
// ============================================================================

#[derive(Debug, Deserialize)]
struct ReefConfig {
    storage: StorageConfig,
}

#[derive(Debug, Deserialize)]
struct StorageConfig {
    #[serde(default = "default_db_url_env")]
    db_url_env: String,
    #[serde(default = "default_db_path")]
    db_path_default: String,
    #[serde(default = "default_migrations_dir")]
    migrations_dir: String,
}

fn default_db_url_env() -> String {
    "DATABASE_URL".to_string()
}
fn default_db_path() -> String {
    "./data/reef.db".to_string()
}
fn default_migrations_dir() -> String {
    "migrations".to_string()
}

fn read_config() -> Result<ReefConfig> {
    let path = Path::new(".reef/config.toml");
    if !path.exists() {
        bail!(
            "no .reef/config.toml found in {}. \
             Run this from a Reef project root (or scaffold one with `cargo reef new`).",
            std::env::current_dir().unwrap_or_default().display()
        );
    }
    let text = std::fs::read_to_string(path).context("reading .reef/config.toml")?;
    let cfg: ReefConfig = toml::from_str(&text).context("parsing .reef/config.toml")?;
    Ok(cfg)
}

fn resolve_db_path(cfg: &StorageConfig) -> String {
    std::env::var(&cfg.db_url_env).unwrap_or_else(|_| cfg.db_path_default.clone())
}

fn run_migrate(cmd: MigrateCommand) -> Result<()> {
    let cfg = read_config()?.storage;

    match cmd {
        MigrateCommand::Run => block_on(migrate_run(&cfg)),
        MigrateCommand::New { name, with_down } => migrate_new(&cfg, &name, with_down),
        MigrateCommand::Status => block_on(migrate_status(&cfg)),
        MigrateCommand::Revert => block_on(migrate_revert(&cfg)),
    }
}

fn block_on<F: std::future::Future<Output = Result<()>>>(fut: F) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("building tokio runtime")?
        .block_on(fut)
}

/// Discover all `*.sql` files in `migrations/` (excluding `*.down.sql`),
/// sorted lexicographically by filename. Each entry is `(name, path)` where
/// `name` is the file stem (e.g. `20260425_120000_init`).
fn discover_forward_migrations(migrations_dir: &str) -> Result<Vec<(String, PathBuf)>> {
    let dir = Path::new(migrations_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for e in std::fs::read_dir(dir).with_context(|| format!("reading {migrations_dir}"))? {
        let e = e?;
        let path = e.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if !name.ends_with(".sql") {
            continue;
        }
        // Skip down-only files
        if name.ends_with(".down.sql") {
            continue;
        }
        let stem = name.trim_end_matches(".sql").to_string();
        entries.push((stem, path));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

async fn open_db(path: &str) -> Result<libsql::Connection> {
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
    }
    let db = libsql::Builder::new_local(path)
        .build()
        .await
        .context("opening libSQL database")?;
    let conn = db.connect().context("connecting to libSQL database")?;

    // Bootstrap the migration tracking table on first use.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            duration_ms INTEGER,
            checksum TEXT
        )",
        (),
    )
    .await
    .context("creating schema_migrations table")?;

    Ok(conn)
}

async fn applied_migrations(conn: &libsql::Connection) -> Result<std::collections::HashMap<String, Option<String>>> {
    let mut rows = conn
        .query("SELECT name, checksum FROM schema_migrations", ())
        .await
        .context("querying schema_migrations")?;
    let mut map = std::collections::HashMap::new();
    while let Some(row) = rows.next().await? {
        let name: String = row.get(0)?;
        let checksum: Option<String> = row.get(1).ok();
        map.insert(name, checksum);
    }
    Ok(map)
}

fn checksum(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    // Cheap, dependency-free hash. Not cryptographic — purely for "did this
    // file change after being applied" detection.
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

async fn migrate_run(cfg: &StorageConfig) -> Result<()> {
    let db_path = resolve_db_path(cfg);
    println!("{} {}", style("Database:       ").dim(), style(&db_path).bold());
    println!(
        "{} {}",
        style("Migrations dir: ").dim(),
        style(&cfg.migrations_dir).bold()
    );
    println!();

    let conn = open_db(&db_path).await?;
    let applied = applied_migrations(&conn).await?;
    let files = discover_forward_migrations(&cfg.migrations_dir)?;

    let mut applied_count = 0;
    let mut warned = false;
    for (name, path) in &files {
        let sql = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("reading {}", path.display()))?;
        let sum = checksum(sql.as_bytes());

        match applied.get(name) {
            Some(prev) => {
                // Already applied — warn if the file has been edited since
                if let Some(p) = prev {
                    if p != &sum {
                        eprintln!(
                            "  {} {} (checksum mismatch — file was edited after being applied)",
                            style("⚠").yellow().bold(),
                            style(name).bold()
                        );
                        warned = true;
                    }
                }
                continue;
            }
            None => {
                let started = std::time::Instant::now();
                conn.execute_batch(&sql)
                    .await
                    .with_context(|| format!("applying {name}"))?;
                let duration_ms = started.elapsed().as_millis() as i64;
                conn.execute(
                    "INSERT INTO schema_migrations (name, duration_ms, checksum) VALUES (?1, ?2, ?3)",
                    libsql::params![name.clone(), duration_ms, sum],
                )
                .await
                .with_context(|| format!("recording {name}"))?;

                println!(
                    "  {} {} {}",
                    style("✓").green().bold(),
                    style(name).bold(),
                    style(format!("({}ms)", duration_ms)).dim()
                );
                applied_count += 1;
            }
        }
    }

    println!();
    if applied_count == 0 {
        println!("{} (already up to date)", style("Nothing to do").dim());
    } else {
        println!(
            "{} Applied {} migration{}",
            style("✓").green().bold(),
            applied_count,
            if applied_count == 1 { "" } else { "s" }
        );
    }
    if warned {
        println!();
        println!(
            "{} See checksum warnings above. Edited migrations after being applied is risky — \
             prefer rolling forward with a new migration.",
            style("Note:").yellow().bold()
        );
    }
    Ok(())
}

fn migrate_new(cfg: &StorageConfig, name: &str, with_down: bool) -> Result<()> {
    validate_migration_name(name)?;

    let dir = Path::new(&cfg.migrations_dir);
    std::fs::create_dir_all(dir).with_context(|| format!("mkdir {}", dir.display()))?;

    let now = chrono::Utc::now();
    let timestamp = now.format("%Y%m%d_%H%M%S").to_string();
    let stem = format!("{timestamp}_{name}");

    let forward_path = dir.join(format!("{stem}.sql"));
    let forward_template = format!(
        "-- Migration: {name}\n\
         -- Generated: {generated}\n\
         -- Forward — applied by `cargo reef migrate run`\n\n\
         -- Your CREATE/ALTER/DROP statements here.\n",
        name = name,
        generated = now.format("%Y-%m-%dT%H:%M:%SZ")
    );
    std::fs::write(&forward_path, forward_template)
        .with_context(|| format!("writing {}", forward_path.display()))?;

    println!(
        "{} {}",
        style("✓").green().bold(),
        style(forward_path.display().to_string()).bold()
    );

    if with_down {
        let down_path = dir.join(format!("{stem}.down.sql"));
        let down_template = format!(
            "-- Rollback for: {name}\n\
             -- Generated: {generated}\n\
             -- Applied by `cargo reef migrate revert`\n\n\
             -- Statements that undo the forward migration.\n",
            name = name,
            generated = now.format("%Y-%m-%dT%H:%M:%SZ")
        );
        std::fs::write(&down_path, down_template)
            .with_context(|| format!("writing {}", down_path.display()))?;
        println!(
            "{} {}",
            style("✓").green().bold(),
            style(down_path.display().to_string()).bold()
        );
    }

    Ok(())
}

fn validate_migration_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("migration name cannot be empty");
    }
    for c in name.chars() {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            bail!(
                "migration name `{name}` contains invalid character `{c}` \
                 (use letters, digits, underscores)"
            );
        }
    }
    Ok(())
}

async fn migrate_status(cfg: &StorageConfig) -> Result<()> {
    let db_path = resolve_db_path(cfg);
    let conn = open_db(&db_path).await?;
    let applied = applied_migrations(&conn).await?;
    let files = discover_forward_migrations(&cfg.migrations_dir)?;

    println!();
    println!(
        "{} {} ({} migration{} known)",
        style("Database:").dim(),
        style(&db_path).bold(),
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
    println!();

    if files.is_empty() {
        println!("{} no migrations in `{}`", style("ℹ").cyan(), &cfg.migrations_dir);
        return Ok(());
    }

    let pending: Vec<&(String, PathBuf)> =
        files.iter().filter(|(n, _)| !applied.contains_key(n)).collect();
    let applied_files: Vec<&(String, PathBuf)> =
        files.iter().filter(|(n, _)| applied.contains_key(n)).collect();

    if !applied_files.is_empty() {
        println!("{}", style("Applied:").bold());
        for (name, _) in applied_files {
            println!("  {} {}", style("✓").green(), name);
        }
        println!();
    }

    if !pending.is_empty() {
        println!("{}", style("Pending:").bold());
        for (name, _) in pending {
            println!("  {} {}", style("→").yellow(), name);
        }
        println!();
        println!(
            "Run {} to apply.",
            style("cargo reef migrate run").bold()
        );
    } else {
        println!("{} all caught up", style("✓").green().bold());
    }

    Ok(())
}

async fn migrate_revert(cfg: &StorageConfig) -> Result<()> {
    let db_path = resolve_db_path(cfg);
    let conn = open_db(&db_path).await?;

    // Find the most recent applied migration. Drop the row stream before
    // doing anything else — libsql holds a read lock on schema_migrations
    // until the stream is dropped, which would deadlock the DELETE below.
    let last = {
        let mut rows = conn
            .query(
                "SELECT name FROM schema_migrations ORDER BY applied_at DESC, name DESC LIMIT 1",
                (),
            )
            .await?;
        match rows.next().await? {
            Some(row) => row.get::<String>(0)?,
            None => {
                println!("{} nothing to revert (no applied migrations)", style("ℹ").cyan());
                return Ok(());
            }
        }
    };

    let down_path = Path::new(&cfg.migrations_dir).join(format!("{last}.down.sql"));
    if !down_path.exists() {
        bail!(
            "no rollback file `{}` for `{}`. \
             Roll forward with a new corrective migration instead, \
             or write a `.down.sql` and re-run.",
            down_path.display(),
            last
        );
    }

    let sql = tokio::fs::read_to_string(&down_path)
        .await
        .with_context(|| format!("reading {}", down_path.display()))?;

    println!("{} {}", style("Reverting:").bold(), style(&last).bold());
    let started = std::time::Instant::now();
    conn.execute_batch(&sql)
        .await
        .with_context(|| format!("applying rollback for {last}"))?;
    conn.execute(
        "DELETE FROM schema_migrations WHERE name = ?1",
        libsql::params![last.clone()],
    )
    .await
    .context("removing migration record")?;

    println!(
        "  {} done {}",
        style("✓").green().bold(),
        style(format!("({}ms)", started.elapsed().as_millis())).dim()
    );
    Ok(())
}

// ============================================================================
//  Output styling
// ============================================================================

fn print_banner() {
    println!();
    println!("{}", style("🦀  Welcome to the Reef.").bold().cyan());
    println!();
}

// ============================================================================
//  cargo reef _debug-schema
// ============================================================================

fn debug_schema(path: &std::path::Path) -> Result<()> {
    let schema = schema::parse_file(path)
        .with_context(|| format!("parsing {}", path.display()))?;
    let json = serde_json::to_string_pretty(&schema).context("rendering schema as JSON")?;
    println!("{json}");
    Ok(())
}

fn debug_sql(path: &std::path::Path) -> Result<()> {
    let schema = schema::parse_file(path)
        .with_context(|| format!("parsing {}", path.display()))?;
    let stmts = schema::emit_schema(&schema);
    println!("{}", stmts.join("\n\n"));
    Ok(())
}

async fn debug_introspect(db_path: &std::path::Path) -> Result<()> {
    if !db_path.exists() {
        bail!("database file not found: {}", db_path.display());
    }
    let db = libsql::Builder::new_local(db_path)
        .build()
        .await
        .context("opening libSQL database")?;
    let conn = db.connect().context("connecting to libSQL database")?;
    let schema = schema::introspect_db(&conn).await?;
    let json = serde_json::to_string_pretty(&schema).context("rendering schema as JSON")?;
    println!("{json}");
    Ok(())
}

async fn debug_diff(schema_path: &std::path::Path, db_path: &std::path::Path) -> Result<()> {
    let desired = schema::parse_file(schema_path)
        .with_context(|| format!("parsing {}", schema_path.display()))?;

    let actual = if db_path.exists() {
        let db = libsql::Builder::new_local(db_path)
            .build()
            .await
            .context("opening libSQL database")?;
        let conn = db.connect().context("connecting to libSQL database")?;
        schema::introspect_db(&conn).await?
    } else {
        // No DB yet — diff against an empty schema, so everything reads as
        // CREATE TABLE actions.
        schema::Schema { tables: Vec::new() }
    };

    let diff = schema::diff(&desired, &actual);
    println!("{}", schema::render_diff(&diff));
    Ok(())
}
