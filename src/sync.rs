use crate::cli::SyncMode;
use crate::config::Config;
use crate::diff;
use crate::gha;
use crate::github::{CommitRequest, CreatePrRequest, FileChange, GitHubClient};
use crate::template::{frontmatter, TemplateRenderer};
use tracing::{error, info, warn};

#[derive(Debug)]
pub struct SyncResult {
    pub repo: String,
    pub workflows_checked: usize,
    pub workflows_managed: usize,
    pub drifted: usize,
    pub prs_created: usize,
    pub prs_updated: usize,
    pub issues_created: usize,
    pub issues_updated: usize,
    pub errors: Vec<String>,
}

#[derive(Debug)]
pub struct SyncReport {
    pub results: Vec<SyncResult>,
    pub dry_run: bool,
    pub mode: SyncMode,
}

impl SyncReport {
    pub fn summary(&self) -> String {
        let total_repos = self.results.len();
        let total_checked: usize = self.results.iter().map(|r| r.workflows_checked).sum();
        let total_managed: usize = self.results.iter().map(|r| r.workflows_managed).sum();
        let total_drifted: usize = self.results.iter().map(|r| r.drifted).sum();
        let total_errors: usize = self.results.iter().map(|r| r.errors.len()).sum();

        let mut s = String::new();
        if self.dry_run {
            s.push_str("[dry-run] ");
        }
        s.push_str("Orcastrate Sync Report\n");
        s.push_str("======================\n\n");
        s.push_str(&format!("Repos processed:    {total_repos}\n"));
        s.push_str(&format!("Workflows checked:  {total_checked}\n"));
        s.push_str(&format!("Managed workflows:  {total_managed}\n"));
        s.push_str(&format!("Drifted:            {total_drifted}\n"));

        match self.mode {
            SyncMode::Pr => {
                let created: usize = self.results.iter().map(|r| r.prs_created).sum();
                let updated: usize = self.results.iter().map(|r| r.prs_updated).sum();
                s.push_str(&format!("PRs created:        {created}\n"));
                s.push_str(&format!("PRs updated:        {updated}\n"));
            }
            SyncMode::Issue => {
                let created: usize = self.results.iter().map(|r| r.issues_created).sum();
                let updated: usize = self.results.iter().map(|r| r.issues_updated).sum();
                s.push_str(&format!("Issues created:     {created}\n"));
                s.push_str(&format!("Issues updated:     {updated}\n"));
            }
            SyncMode::Silent => {}
        }

        s.push_str(&format!("Errors:             {total_errors}\n"));

        if total_errors > 0 {
            s.push_str("\nErrors:\n");
            for result in &self.results {
                for err in &result.errors {
                    s.push_str(&format!("  {}: {}\n", result.repo, err));
                }
            }
        }

        s
    }

    pub fn summary_markdown(&self) -> String {
        let total_repos = self.results.len();
        let total_checked: usize = self.results.iter().map(|r| r.workflows_checked).sum();
        let total_managed: usize = self.results.iter().map(|r| r.workflows_managed).sum();
        let total_drifted: usize = self.results.iter().map(|r| r.drifted).sum();

        let mut s = String::new();
        s.push_str("### Orcastrate Sync Report\n\n");

        match self.mode {
            SyncMode::Pr => {
                s.push_str("| Repo | Checked | Managed | Drifted | PRs | Errors |\n");
                s.push_str("|------|---------|---------|---------|-----|--------|\n");
                for result in &self.results {
                    s.push_str(&format!(
                        "| {} | {} | {} | {} | {} | {} |\n",
                        result.repo,
                        result.workflows_checked,
                        result.workflows_managed,
                        result.drifted,
                        format_created_updated(result.prs_created, result.prs_updated),
                        result.errors.len()
                    ));
                }

                let created: usize = self.results.iter().map(|r| r.prs_created).sum();
                let updated: usize = self.results.iter().map(|r| r.prs_updated).sum();
                s.push_str(&format!(
                    "\n**Total**: {} {}, {} workflows checked, {} managed, {} drifted, {}\n",
                    total_repos,
                    pluralize(total_repos, "repo", "repos"),
                    total_checked,
                    total_managed,
                    total_drifted,
                    format_created_updated(created, updated).replace("updated", "PR updated")
                ));
            }
            SyncMode::Issue => {
                s.push_str("| Repo | Checked | Managed | Drifted | Issues | Errors |\n");
                s.push_str("|------|---------|---------|---------|--------|--------|\n");
                for result in &self.results {
                    s.push_str(&format!(
                        "| {} | {} | {} | {} | {} | {} |\n",
                        result.repo,
                        result.workflows_checked,
                        result.workflows_managed,
                        result.drifted,
                        format_created_updated(result.issues_created, result.issues_updated),
                        result.errors.len()
                    ));
                }

                let created: usize = self.results.iter().map(|r| r.issues_created).sum();
                let updated: usize = self.results.iter().map(|r| r.issues_updated).sum();
                s.push_str(&format!(
                    "\n**Total**: {} {}, {} workflows checked, {} managed, {} drifted, {}\n",
                    total_repos,
                    pluralize(total_repos, "repo", "repos"),
                    total_checked,
                    total_managed,
                    total_drifted,
                    format_created_updated(created, updated).replace("updated", "issue updated")
                ));
            }
            SyncMode::Silent => {
                s.push_str("| Repo | Checked | Managed | Drifted | Errors |\n");
                s.push_str("|------|---------|---------|---------|--------|\n");
                for result in &self.results {
                    s.push_str(&format!(
                        "| {} | {} | {} | {} | {} |\n",
                        result.repo,
                        result.workflows_checked,
                        result.workflows_managed,
                        result.drifted,
                        result.errors.len()
                    ));
                }

                s.push_str(&format!(
                    "\n**Total**: {} {}, {} workflows checked, {} managed, {} drifted\n",
                    total_repos,
                    pluralize(total_repos, "repo", "repos"),
                    total_checked,
                    total_managed,
                    total_drifted
                ));
            }
        }

        s
    }
}

fn pluralize<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

fn format_created_updated(created: usize, updated: usize) -> String {
    match (created, updated) {
        (created, 0) => format!("{created} created"),
        (0, updated) => format!("{updated} updated"),
        (created, updated) => format!("{created} created, {updated} updated"),
    }
}

pub async fn run(
    config: &Config,
    renderer: &TemplateRenderer,
    client: &GitHubClient,
    filter_repo: Option<&str>,
    mode: SyncMode,
) -> crate::error::Result<SyncReport> {
    let repos = resolve_repos(config, client, filter_repo).await?;
    info!("syncing {} repos", repos.len());

    let mut results = Vec::new();

    for repo_name in &repos {
        match sync_repo(config, renderer, client, repo_name, mode).await {
            Ok(r) => results.push(r),
            Err(e) => {
                error!("failed to sync {repo_name}: {e}");
                gha::error(&format!("failed to sync {repo_name}: {e}"));
                results.push(SyncResult {
                    repo: repo_name.clone(),
                    workflows_checked: 0,
                    workflows_managed: 0,
                    drifted: 0,
                    prs_created: 0,
                    prs_updated: 0,
                    issues_created: 0,
                    issues_updated: 0,
                    errors: vec![e.to_string()],
                });
            }
        }
    }

    Ok(SyncReport {
        results,
        dry_run: client.is_dry_run(),
        mode,
    })
}

async fn resolve_repos(
    config: &Config,
    client: &GitHubClient,
    filter_repo: Option<&str>,
) -> crate::error::Result<Vec<String>> {
    let mut repos: Vec<String> = config
        .active_repos()
        .iter()
        .map(|r| r.name.clone())
        .collect();

    if let Some(ref discovery) = config.discovery {
        let discovered = client
            .discover_repos(&discovery.org, discovery.topic.as_deref())
            .await?;
        info!("discovered {} repos via org API", discovered.len());
        for repo in discovered {
            if !repos.contains(&repo) {
                repos.push(repo);
            }
        }
    }

    if let Some(filter_repo) = filter_repo {
        if !repos.iter().any(|repo| repo == filter_repo) {
            return Err(crate::error::Error::Config(format!(
                "repo '{}' was requested with --repo but is not configured or discovered",
                filter_repo
            )));
        }

        repos.retain(|repo| repo == filter_repo);
    }

    Ok(repos)
}

async fn sync_repo(
    config: &Config,
    renderer: &TemplateRenderer,
    client: &GitHubClient,
    repo_name: &str,
    mode: SyncMode,
) -> crate::error::Result<SyncResult> {
    let (owner, repo) = repo_name
        .split_once('/')
        .ok_or_else(|| crate::error::Error::Config(format!("invalid repo name: {repo_name}")))?;

    gha::group(&format!("Syncing {owner}/{repo}"));
    info!("syncing {owner}/{repo}");

    let outcome = async {
        let mut result = SyncResult {
            repo: repo_name.to_string(),
            workflows_checked: 0,
            workflows_managed: 0,
            drifted: 0,
            prs_created: 0,
            prs_updated: 0,
            issues_created: 0,
            issues_updated: 0,
            errors: Vec::new(),
        };

        let workflows = client.list_workflows(owner, repo).await?;
        result.workflows_checked = workflows.len();

        let mut updates: Vec<WorkflowUpdate> = Vec::new();

        for workflow in &workflows {
            let path = &workflow.path;

            let (content, _blob_sha) = match client.get_file_content(owner, repo, path).await {
                Ok(c) => c,
                Err(e) => {
                    warn!("failed to get {path}: {e}");
                    gha::warning(&format!("{path}: {e}"));
                    result.errors.push(format!("{path}: {e}"));
                    continue;
                }
            };

            let parsed = match frontmatter::parse(&content) {
                Ok(p) => p,
                Err(mut e) => {
                    if let crate::error::Error::Frontmatter { ref mut file, .. } = e {
                        *file = path.clone();
                    }
                    result.errors.push(format!("{path}: {e}"));
                    continue;
                }
            };

            let Some(fm) = parsed.frontmatter else {
                continue;
            };

            result.workflows_managed += 1;
            info!("  managed: {path} (template: {})", fm.template);

            let rendered = match renderer.render(&fm.template, &fm.params) {
                Ok(r) => r,
                Err(e) => {
                    gha::warning(&format!("{path}: render failed: {e}"));
                    result.errors.push(format!("{path}: render failed: {e}"));
                    continue;
                }
            };

            let full_rendered = match parsed.raw_block {
                Some(ref block) => format!("{block}\n\n{rendered}"),
                None => rendered,
            };

            if diff::has_drifted(&content, &full_rendered) {
                let summary = diff::diff_summary(&content, &full_rendered);
                info!("  drift detected in {path} ({summary})");

                updates.push(WorkflowUpdate {
                    path: path.clone(),
                    current: content,
                    rendered: full_rendered,
                    template: fm.template.clone(),
                });
            }
        }

        if updates.is_empty() {
            info!("{repo_name}: all workflows in sync");
            return Ok(result);
        }

        result.drifted = updates.len();

        if matches!(mode, SyncMode::Silent) {
            return Ok(result);
        }

        let labels = vec![config.orchestrator.pr_label.clone()];

        for update in &updates {
            let workflow_name = update
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&update.path)
                .trim_end_matches(".yml")
                .trim_end_matches(".yaml");

            let title = format!(
                "chore(ci): sync `{}` from template `{}`",
                workflow_name, update.template
            );
            let body = build_pr_body(update);

            match mode {
                SyncMode::Pr => {
                    let default_branch = client.get_default_branch(owner, repo).await?;
                    let base_sha = client.get_branch_sha(owner, repo, &default_branch).await?;
                    let branch_name =
                        format!("{}/{workflow_name}", config.orchestrator.branch_prefix);

                    client
                        .create_branch(owner, repo, &branch_name, &base_sha)
                        .await?;

                    client
                        .commit_files(&CommitRequest {
                            owner,
                            repo,
                            branch: &branch_name,
                            message: &title,
                            base_sha: &base_sha,
                            files: &[FileChange {
                                path: update.path.clone(),
                                content: update.rendered.clone(),
                            }],
                        })
                        .await?;

                    let existing = client.find_existing_pr(owner, repo, &branch_name).await?;

                    if let Some(pr) = existing {
                        client
                            .update_pr(owner, repo, pr.number, &title, &body)
                            .await?;
                        info!("  updated PR #{} ({})", pr.number, pr.url);
                        result.prs_updated += 1;
                    } else {
                        let pr = client
                            .create_pr(&CreatePrRequest {
                                owner,
                                repo,
                                title: &title,
                                body: &body,
                                head: &branch_name,
                                base: &default_branch,
                                labels: &labels,
                            })
                            .await?;
                        info!("  created PR #{} ({})", pr.number, pr.url);
                        result.prs_created += 1;
                    }
                }
                SyncMode::Issue => {
                    let existing = client.find_existing_issue(owner, repo, &title).await?;

                    if let Some(issue) = existing {
                        client
                            .update_issue(owner, repo, issue.number, &body)
                            .await?;
                        info!("  updated issue #{} ({})", issue.number, issue.url);
                        result.issues_updated += 1;
                    } else {
                        let issue = client
                            .create_issue(owner, repo, &title, &body, &labels)
                            .await?;
                        info!("  created issue #{} ({})", issue.number, issue.url);
                        result.issues_created += 1;
                    }
                }
                SyncMode::Silent => unreachable!(),
            }
        }

        Ok(result)
    }
    .await;

    gha::endgroup();
    outcome
}

struct WorkflowUpdate {
    path: String,
    current: String,
    rendered: String,
    template: String,
}

fn build_pr_body(update: &WorkflowUpdate) -> String {
    let mut body = String::new();

    body.push_str("## 🐬 Orcastrate Workflow Sync\n\n");
    body.push_str(
        "This PR was automatically generated by [orcastrate](https://github.com/orcastrate/orcastrate) \
         to sync workflow files with their canonical templates.\n\n",
    );

    let summary = diff::diff_summary(&update.current, &update.rendered);
    body.push_str(&format!(
        "**`{}`** — template `{}` ({summary})\n",
        update.path, update.template
    ));

    body.push_str("\n### Diff\n\n");

    let diff_text = diff::generate_diff(&update.current, &update.rendered, &update.path);
    let diff_lines: Vec<_> = diff_text.lines().collect();
    let truncated = diff_lines.len() > 200;
    let mut diff_text = diff_lines
        .into_iter()
        .take(200)
        .collect::<Vec<_>>()
        .join("\n");

    if truncated {
        diff_text.push_str("\n... (truncated, full diff in branch)\n");
    } else {
        diff_text.push('\n');
    }

    body.push_str(&format!("```diff\n{diff_text}```\n\n"));
    body.push_str("---\n*🐬 Managed by [orcastrate](https://github.com/orcastrate/orcastrate). Do not edit managed sections manually.*\n");

    body
}
