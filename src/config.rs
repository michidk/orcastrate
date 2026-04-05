use serde::Deserialize;
use std::path::Path;

use crate::error::{Error, Result};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub orchestrator: OrchestratorConfig,
    #[serde(default)]
    pub repos: Vec<RepoEntry>,
    #[serde(default)]
    pub discovery: Option<DiscoveryConfig>,
}

#[derive(Debug, Deserialize)]
pub struct OrchestratorConfig {
    pub templates_dir: String,
    #[serde(default = "default_branch_prefix")]
    pub branch_prefix: String,
    #[serde(default = "default_pr_label")]
    pub pr_label: String,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub struct RepoEntry {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct DiscoveryConfig {
    pub org: String,
    #[serde(default)]
    pub topic: Option<String>,
}

fn default_branch_prefix() -> String {
    "orcastrate/sync".to_string()
}

fn default_pr_label() -> String {
    "orcastrate".to_string()
}

fn default_enabled() -> bool {
    true
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("failed to read {}: {e}", path.display())))?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.repos.is_empty() && self.discovery.is_none() {
            return Err(Error::Config(
                "at least one of [[repos]] or [discovery] must be configured".into(),
            ));
        }
        for repo in &self.repos {
            if !(repo.name.split('/').count() == 2
                && repo.name.split('/').all(|part| !part.is_empty()))
            {
                return Err(Error::Config(format!(
                    "repo '{}' must be in 'owner/name' format",
                    repo.name
                )));
            }
        }
        Ok(())
    }

    pub fn active_repos(&self) -> Vec<&RepoEntry> {
        self.repos.iter().filter(|r| r.enabled).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_config(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn parse_minimal_config() {
        let f = write_temp_config(
            r#"
[orchestrator]
templates_dir = "templates"

[[repos]]
name = "myorg/myrepo"
"#,
        );
        let config = Config::load(f.path()).unwrap();
        assert_eq!(config.repos.len(), 1);
        assert_eq!(config.repos[0].name, "myorg/myrepo");
        assert_eq!(config.orchestrator.branch_prefix, "orcastrate/sync");
    }

    #[test]
    fn reject_missing_repos_and_discovery() {
        let f = write_temp_config(
            r#"
[orchestrator]
templates_dir = "templates"
"#,
        );
        assert!(Config::load(f.path()).is_err());
    }

    #[test]
    fn reject_bad_repo_format() {
        let f = write_temp_config(
            r#"
[orchestrator]
templates_dir = "templates"

[[repos]]
name = "no-slash-here"
"#,
        );
        assert!(Config::load(f.path()).is_err());
    }

    #[test]
    fn disabled_repos_filtered() {
        let f = write_temp_config(
            r#"
[orchestrator]
templates_dir = "templates"

[[repos]]
name = "org/active"

[[repos]]
name = "org/disabled"
enabled = false
"#,
        );
        let config = Config::load(f.path()).unwrap();
        let active = config.active_repos();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "org/active");
    }
}
