//! Human-readable rendering of a [`Diff`] for the CLI preview.
//!
//! Used by `_debug-diff` today and `cargo reef db:push` once it lands. The
//! goal is a glance-able summary: what's changing, what needs manual work,
//! what to watch out for.

use console::style;

use super::diff::{Action, Diff};

pub fn render_diff(d: &Diff) -> String {
    if d.is_empty() && d.warnings.is_empty() {
        return format!("{} schema is up to date", style("✓").green().bold());
    }

    let mut out = String::new();
    let mut auto: Vec<String> = Vec::new();
    let mut manual: Vec<String> = Vec::new();

    for action in &d.actions {
        match action {
            Action::CreateTable(t) => auto.push(format!(
                "{} {} {}",
                style("+").green().bold(),
                style("CREATE TABLE").bold(),
                style(&t.name).cyan()
            )),
            Action::DropTable(name) => auto.push(format!(
                "{} {} {}",
                style("-").red().bold(),
                style("DROP TABLE").bold(),
                style(name).cyan()
            )),
            Action::AddColumn { table, column } => auto.push(format!(
                "{} {}.{}: ADD {}",
                style("+").green().bold(),
                style(table).cyan(),
                style(&column.name).bold(),
                column.ty.sql()
            )),
            Action::DropColumn { table, column } => auto.push(format!(
                "{} {}.{}: DROP",
                style("-").red().bold(),
                style(table).cyan(),
                style(column).bold(),
            )),
            Action::AlterColumn { table, before, after } => auto.push(format!(
                "{} {}.{}: ALTER ({})",
                style("~").yellow().bold(),
                style(table).cyan(),
                style(&after.name).bold(),
                summarize_column_change(before, after)
            )),
            Action::CreateIndex { table, index } => auto.push(format!(
                "{} {} {} ON {} ({})",
                style("+").green().bold(),
                if index.unique { "UNIQUE INDEX" } else { "INDEX" },
                style(index.name.as_deref().unwrap_or("<auto>")).cyan(),
                style(table).cyan(),
                index.columns.join(", ")
            )),
            Action::DropIndex { name } => auto.push(format!(
                "{} {} {}",
                style("-").red().bold(),
                style("DROP INDEX").bold(),
                style(name).cyan()
            )),
            Action::NeedsRebuild { table, reason } => manual.push(format!(
                "{} {}: {}",
                style("!").red().bold(),
                style(table).cyan(),
                reason
            )),
        }
    }

    if !auto.is_empty() {
        out.push_str(&format!("{}\n", style("Schema changes:").bold()));
        for line in &auto {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }

    if !manual.is_empty() {
        if !auto.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!(
            "{}\n",
            style("Manual migration required (`cargo reef migrate new <name>`):")
                .red()
                .bold()
        ));
        for line in &manual {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }

    if !d.warnings.is_empty() {
        out.push('\n');
        out.push_str(&format!("{}\n", style("Warnings:").yellow().bold()));
        for w in &d.warnings {
            out.push_str(&format!("  {} {}\n", style("⚠").yellow(), w));
        }
    }

    out
}

fn summarize_column_change(before: &super::ir::Column, after: &super::ir::Column) -> String {
    let mut parts = Vec::new();
    if before.ty.sql() != after.ty.sql() {
        parts.push(format!("{} → {}", before.ty.sql(), after.ty.sql()));
    }
    if before.nullable != after.nullable {
        parts.push(format!(
            "{} → {}",
            if before.nullable { "NULL" } else { "NOT NULL" },
            if after.nullable { "NULL" } else { "NOT NULL" },
        ));
    }
    if before.unique != after.unique {
        parts.push(format!(
            "unique {} → {}",
            before.unique, after.unique
        ));
    }
    if before.default != after.default {
        parts.push(format!(
            "default {:?} → {:?}",
            before.default, after.default
        ));
    }
    if before.references.is_some() != after.references.is_some() {
        parts.push(if after.references.is_some() {
            "FK added".to_string()
        } else {
            "FK removed".to_string()
        });
    } else if before.references != after.references {
        parts.push("FK changed".to_string());
    }
    if parts.is_empty() {
        "minor change".to_string()
    } else {
        parts.join(", ")
    }
}
