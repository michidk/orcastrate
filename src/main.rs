mod cli;
mod config;
mod diff;
mod error;
mod github;
mod sync;
mod template;

use clap::Parser;
use cli::{Cli, Command};
use config::Config;
use github::GitHubClient;
use octocrab::Octocrab;
use std::path::Path;
use template::TemplateRenderer;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let filter = match cli.verbose.log_level_filter() {
        log::LevelFilter::Off => "off",
        log::LevelFilter::Error => "error",
        log::LevelFilter::Warn => "warn",
        log::LevelFilter::Info => "info",
        log::LevelFilter::Debug => "debug",
        log::LevelFilter::Trace => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_target(false)
        .init();

    match &cli.command {
        Command::Sync { .. } => cmd_sync(&cli).await?,
        Command::Validate => cmd_validate(&cli)?,
        Command::ListRepos => cmd_list_repos(&cli).await?,
        Command::ListTemplates => cmd_list_templates(&cli)?,
        Command::Drift { .. } => cmd_drift(&cli).await?,
    }

    Ok(())
}

async fn cmd_sync(cli: &Cli) -> anyhow::Result<()> {
    let config = Config::load(&cli.config)?;
    let dry_run = cli.dry_run || config.orchestrator.dry_run;
    let filter_repo = match &cli.command {
        Command::Sync { repo } => repo.as_deref(),
        _ => None,
    };
    let templates_dir = Path::new(&config.orchestrator.templates_dir);
    let renderer = TemplateRenderer::new(templates_dir)?;
    let client = build_client(dry_run).await?;

    let report = sync::run(&config, &renderer, &client, filter_repo).await?;
    println!("{}", report.summary());

    let has_errors = report.results.iter().any(|r| !r.errors.is_empty());
    if has_errors {
        std::process::exit(1);
    }

    Ok(())
}

fn cmd_validate(cli: &Cli) -> anyhow::Result<()> {
    let config = Config::load(&cli.config)?;
    info!("config OK: {} repos configured", config.repos.len());

    let templates_dir = Path::new(&config.orchestrator.templates_dir);
    let renderer = TemplateRenderer::new(templates_dir)?;
    let templates = renderer.list_templates();
    info!("templates OK: {} templates loaded", templates.len());

    for t in &templates {
        info!("  - {t}");
    }

    println!("Validation passed.");
    Ok(())
}

async fn cmd_list_repos(cli: &Cli) -> anyhow::Result<()> {
    let config = Config::load(&cli.config)?;

    println!("Configured repos:");
    for repo in config.active_repos() {
        println!("  - {}", repo.name);
    }

    if let Some(ref discovery) = config.discovery {
        let client = build_client(true).await?;
        let discovered = client
            .discover_repos(&discovery.org, discovery.topic.as_deref())
            .await?;
        println!("\nDiscovered repos ({}):", discovered.len());
        for repo in &discovered {
            println!("  - {repo}");
        }
    }

    Ok(())
}

fn cmd_list_templates(cli: &Cli) -> anyhow::Result<()> {
    let config = Config::load(&cli.config)?;
    let templates_dir = Path::new(&config.orchestrator.templates_dir);
    let renderer = TemplateRenderer::new(templates_dir)?;

    println!("Available templates:");
    for t in renderer.list_templates() {
        println!("  - {t}");
    }

    Ok(())
}

async fn cmd_drift(cli: &Cli) -> anyhow::Result<()> {
    let config = Config::load(&cli.config)?;
    let filter_repo = match &cli.command {
        Command::Drift { repo } => repo.as_deref(),
        _ => None,
    };
    let templates_dir = Path::new(&config.orchestrator.templates_dir);
    let renderer = TemplateRenderer::new(templates_dir)?;
    let client = build_client(true).await?;

    let report = sync::run(&config, &renderer, &client, filter_repo).await?;
    println!("{}", report.summary());

    Ok(())
}

async fn build_client(dry_run: bool) -> anyhow::Result<GitHubClient> {
    let app_id = std::env::var("ORCASTRATE_APP_ID").ok();
    let private_key = std::env::var("ORCASTRATE_PRIVATE_KEY").ok();
    let installation_id = std::env::var("ORCASTRATE_INSTALLATION_ID").ok();

    let crab = if app_id.is_some() || private_key.is_some() || installation_id.is_some() {
        let (app_id, private_key, installation_id) = match (app_id, private_key, installation_id) {
            (Some(app_id), Some(private_key), Some(installation_id)) => {
                (app_id, private_key, installation_id)
            }
            _ => {
                anyhow::bail!(
                    "incomplete GitHub App auth configuration — set ORCASTRATE_APP_ID, ORCASTRATE_PRIVATE_KEY, and ORCASTRATE_INSTALLATION_ID"
                );
            }
        };

        let app_id: u64 = app_id
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid ORCASTRATE_APP_ID: {e}"))?;
        let installation_id: u64 = installation_id
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid ORCASTRATE_INSTALLATION_ID: {e}"))?;
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key.as_bytes())?;

        let app_crab = Octocrab::builder().app(app_id.into(), key).build()?;
        let (installation_crab, _token) = app_crab
            .installation_and_token(installation_id.into())
            .await?;
        installation_crab
    } else if let Ok(token) = std::env::var("ORCASTRATE_TOKEN") {
        Octocrab::builder().personal_token(token).build()?
    } else if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        Octocrab::builder().personal_token(token).build()?
    } else {
        anyhow::bail!(
            "no auth configured — set GITHUB_TOKEN, ORCASTRATE_TOKEN, or \
             ORCASTRATE_APP_ID + ORCASTRATE_PRIVATE_KEY + ORCASTRATE_INSTALLATION_ID"
        );
    };

    let pr_crab = std::env::var("GITHUB_TOKEN")
        .ok()
        .map(|t| Octocrab::builder().personal_token(t).build())
        .transpose()?;

    Ok(GitHubClient::new(crab, pr_crab, dry_run))
}
