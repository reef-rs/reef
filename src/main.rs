//! `cargo-reef` — CLI scaffolder for Reef apps.
//!
//! v0.1 ships ONE subcommand: `cargo reef new <name>`.
//! Copies the embedded reef-template into a new directory, substitutes the
//! project name, runs `git init`. No conditional branches yet — users delete
//! what they don't need.
//!
//! Future subcommands (build / deploy / migrate / db:push / etc.) are designed
//! in docs/ but not yet implemented.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use console::style;
use include_dir::{include_dir, Dir, DirEntry};

/// The reef-template, vendored by build.rs at compile time.
static TEMPLATE: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/template");

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
        /// Name of the app (also the directory created).
        name: String,
    },
}

fn main() -> Result<()> {
    let CargoCli::Reef(args) = CargoCli::parse();

    match args.cmd {
        ReefCommand::New { name } => scaffold_new(&name),
    }
}

// ============================================================================
//  cargo reef new <name>
// ============================================================================

fn scaffold_new(name: &str) -> Result<()> {
    validate_name(name)?;

    let target = PathBuf::from(name);
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

    print_next_steps(name, count);
    Ok(())
}

/// Recursively writes the embedded template into `target`, substituting the
/// project name into text files as it goes.
fn walk_template(dir: &Dir, target: &Path, project_name: &str, count: &mut usize) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(d) => {
                let dst = target.join(d.path());
                std::fs::create_dir_all(&dst)
                    .with_context(|| format!("mkdir {}", dst.display()))?;
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

                std::fs::write(&dst, bytes)
                    .with_context(|| format!("write {}", dst.display()))?;
                *count += 1;
            }
        }
    }
    Ok(())
}

/// Replace placeholder strings in text files with the user's project name.
///
/// `reef-template` is the literal name we use everywhere in the template
/// (Cargo.toml `name = "reef-template"`, .reef/config.toml, README, etc.).
/// Substituting it gives the user their named project.
fn substitute(text: &str, project_name: &str) -> String {
    text.replace("reef-template", project_name)
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("project name cannot be empty");
    }
    // Cargo allows fairly liberal package names; we mirror its rules:
    // letters, digits, _, -. First char must be a letter.
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() {
        bail!("project name must start with a letter, got `{name}`");
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            bail!("project name `{name}` contains invalid character `{c}` (use letters, digits, _, -)");
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
//  Output styling
// ============================================================================

fn print_banner() {
    println!();
    println!("{}", style("🦀  Welcome to the Reef.").bold().cyan());
    println!();
}

fn print_next_steps(name: &str, count: usize) {
    println!();
    println!(
        "{} Generated {} files in {}",
        style("✓").green().bold(),
        count,
        style(format!("./{name}")).bold()
    );
    println!();
    println!("Next steps:");
    println!();
    println!("  {} {}", style("$").dim(), style(format!("cd {name}")).bold());
    println!("  {} {}", style("$").dim(), style("dx serve --web").bold());
    println!();
    println!(
        "Learn more at {}",
        style("https://reef.rs").underlined().cyan()
    );
    println!();
}
