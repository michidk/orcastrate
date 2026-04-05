use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "orcastrate",
    about = "GitHub Actions orchestrator — template, sync, and manage workflows at org scale",
    author,
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(short, long, default_value = "orchestrator.toml", global = true)]
    pub config: PathBuf,

    #[arg(short, long, alias = "dry", global = true)]
    pub dry_run: bool,

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity<clap_verbosity_flag::InfoLevel>,
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
