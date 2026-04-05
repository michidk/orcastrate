use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "orcastrate",
    about = "GitHub Actions orchestrator — template, sync, and manage workflows at org scale",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(short, long, default_value = "orchestrator.toml")]
    pub config: PathBuf,

    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Sync workflows across all configured repos
    Sync {
        #[arg(long)]
        repo: Option<String>,
    },

    /// Validate config and templates without syncing
    Validate,

    /// List all configured/discovered repos
    ListRepos,

    /// List available templates
    ListTemplates,

    /// Check drift status without creating PRs
    Drift {
        #[arg(long)]
        repo: Option<String>,
    },
}
