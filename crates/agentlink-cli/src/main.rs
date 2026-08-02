//! `agentlink` — one brain for every AI coding agent.

#![forbid(unsafe_code)]

mod app;
mod commands;
mod render;
mod ui;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::ui::{ColorChoice, Ui};

/// Give every AI coding agent in a repository the same rules and skills, without
/// copying a single file.
#[derive(Debug, Parser)]
#[command(
    name = "agentlink",
    version,
    about,
    long_about = None,
    after_help = "Learn more: https://github.com/fialhosoft/agentlink"
)]
struct Cli {
    /// Operate on this directory instead of the current one.
    #[arg(short = 'C', long = "dir", global = true, value_name = "PATH")]
    dir: Option<PathBuf>,

    /// When to colourise output.
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    color: ColorChoice,

    /// Suppress informational output.
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create the canonical layout and link every agent to it.
    Init,

    /// Materialise anything that is missing or out of date.
    #[command(alias = "sync")]
    Apply {
        /// Show what would happen without writing anything.
        #[arg(long)]
        dry_run: bool,

        /// Also move agent-owned content into the canonical layout.
        #[arg(long)]
        adopt: bool,
    },

    /// Show what `apply` would do.
    Status {
        /// Exit with code 2 if anything is pending or blocked. For CI.
        #[arg(long)]
        check: bool,
    },

    /// Move content from an agent's directory into the canonical layout, then
    /// link it back so that agent keeps reading its usual path.
    Adopt {
        /// Show what would move without writing anything.
        #[arg(long)]
        dry_run: bool,
    },

    /// Check the environment and report anything that needs attention.
    Doctor,

    /// List every known agent and how each one is served.
    Providers,

    /// Remove everything agentlink created, leaving the canonical layout intact.
    Clean {
        /// Show what would be removed without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let ui = Ui::new(cli.color, cli.quiet);

    let result = match cli.command {
        Command::Init => commands::init(ui, cli.dir),
        Command::Apply { dry_run, adopt } => commands::apply(ui, cli.dir, dry_run, adopt),
        Command::Status { check } => commands::status(ui, cli.dir, check),
        Command::Adopt { dry_run } => commands::adopt(ui, cli.dir, dry_run),
        Command::Doctor => commands::doctor(ui, cli.dir),
        Command::Providers => commands::providers(ui, cli.dir),
        Command::Clean { dry_run } => commands::clean(ui, cli.dir, dry_run),
    };

    match result {
        Ok(code) => std::process::ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(err) => {
            eprintln!("{} {err}", ui.red("error:"));
            for cause in err.chain().skip(1) {
                eprintln!("       {}", ui.dim(&cause.to_string()));
            }
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
