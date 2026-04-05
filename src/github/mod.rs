use octocrab::Octocrab;
use tracing::{debug, info, warn};

use crate::error::Error;

pub struct GitHubClient {
    crab: Octocrab,
    dry_run: bool,
}

pub struct WorkflowFile {
    pub path: String,
}

pub struct FileCommit<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub path: &'a str,
    pub content: &'a str,
    pub message: &'a str,
    pub branch: &'a str,
    pub existing_sha: Option<&'a str>,
}

pub struct CreatePrRequest<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub head: &'a str,
    pub base: &'a str,
    pub labels: &'a [String],
}

pub struct PrInfo {
    pub number: u64,
    pub url: String,
}

impl GitHubClient {
    pub fn new(crab: Octocrab, dry_run: bool) -> Self {
        Self { crab, dry_run }
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    pub async fn list_workflows(
        &self,
        owner: &str,
        repo: &str,
    ) -> crate::error::Result<Vec<WorkflowFile>> {
        let result = self
            .crab
            .repos(owner, repo)
            .get_content()
            .path(".github/workflows")
            .send()
            .await;

        match result {
            Ok(items) => Ok(items
                .items
                .into_iter()
                .filter(|c| c.name.ends_with(".yml") || c.name.ends_with(".yaml"))
                .map(|c| WorkflowFile { path: c.path })
                .collect()),
            Err(e) if is_not_found(&e) => {
                debug!("{owner}/{repo} has no .github/workflows/");
                Ok(vec![])
            }
            Err(e) => Err(Error::GitHub(format!(
                "list workflows for {owner}/{repo}: {e}"
            ))),
        }
    }

    pub async fn get_file_content(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
    ) -> crate::error::Result<(String, String)> {
        let items = self
            .crab
            .repos(owner, repo)
            .get_content()
            .path(path)
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("get {owner}/{repo}/{path}: {e}")))?;

        let file = items
            .items
            .into_iter()
            .next()
            .ok_or_else(|| Error::GitHub(format!("{owner}/{repo}/{path}: empty response")))?;

        let decoded = file.decoded_content().ok_or_else(|| {
            Error::GitHub(format!(
                "{owner}/{repo}/{path}: content missing or not decodable"
            ))
        })?;

        Ok((decoded, file.sha))
    }

    pub async fn get_default_branch(
        &self,
        owner: &str,
        repo: &str,
    ) -> crate::error::Result<String> {
        let repo_info = self
            .crab
            .repos(owner, repo)
            .get()
            .await
            .map_err(|e| Error::GitHub(format!("get repo {owner}/{repo}: {e}")))?;

        repo_info
            .default_branch
            .ok_or_else(|| Error::GitHub("missing default_branch".into()))
    }

    pub async fn get_branch_sha(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> crate::error::Result<String> {
        let route = format!("/repos/{owner}/{repo}/git/refs/heads/{branch}");
        let ref_info: serde_json::Value =
            self.crab.get(&route, None::<&()>).await.map_err(|e| {
                Error::GitHub(format!("get branch sha {owner}/{repo}@{branch}: {e}"))
            })?;

        ref_info["object"]["sha"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| Error::GitHub("missing sha in ref response".into()))
    }

    pub async fn create_branch(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        from_sha: &str,
    ) -> crate::error::Result<()> {
        if self.dry_run {
            info!("[dry-run] would create branch {branch} in {owner}/{repo}");
            return Ok(());
        }

        let route = format!("/repos/{owner}/{repo}/git/refs");
        let body = serde_json::json!({
            "ref": format!("refs/heads/{branch}"),
            "sha": from_sha,
        });

        let result: Result<serde_json::Value, _> = self.crab.post(&route, Some(&body)).await;

        match result {
            Ok(_) => Ok(()),
            Err(e) if is_unprocessable(&e) => {
                debug!("branch {branch} already exists, force-updating");
                let update_route = format!("/repos/{owner}/{repo}/git/refs/heads/{branch}");
                let _: serde_json::Value = self
                    .crab
                    .patch(
                        &update_route,
                        Some(&serde_json::json!({"sha": from_sha, "force": true})),
                    )
                    .await
                    .map_err(|e| {
                        Error::GitHub(format!("update branch {branch} in {owner}/{repo}: {e}"))
                    })?;
                Ok(())
            }
            Err(e) => Err(Error::GitHub(format!(
                "create branch {branch} in {owner}/{repo}: {e}"
            ))),
        }
    }

    pub async fn commit_file(&self, req: &FileCommit<'_>) -> crate::error::Result<()> {
        if self.dry_run {
            info!(
                "[dry-run] would update {} in {}/{} on {}",
                req.path, req.owner, req.repo, req.branch
            );
            return Ok(());
        }

        match req.existing_sha {
            Some(sha) => {
                self.crab
                    .repos(req.owner, req.repo)
                    .update_file(req.path, req.message, req.content, sha)
                    .branch(req.branch)
                    .send()
                    .await
                    .map_err(|e| {
                        Error::GitHub(format!(
                            "update {} in {}/{}: {e}",
                            req.path, req.owner, req.repo
                        ))
                    })?;
            }
            None => {
                self.crab
                    .repos(req.owner, req.repo)
                    .create_file(req.path, req.message, req.content)
                    .branch(req.branch)
                    .send()
                    .await
                    .map_err(|e| {
                        Error::GitHub(format!(
                            "create {} in {}/{}: {e}",
                            req.path, req.owner, req.repo
                        ))
                    })?;
            }
        }

        Ok(())
    }

    pub async fn find_existing_pr(
        &self,
        owner: &str,
        repo: &str,
        head_branch: &str,
    ) -> crate::error::Result<Option<PrInfo>> {
        let prs = self
            .crab
            .pulls(owner, repo)
            .list()
            .state(octocrab::params::State::Open)
            .head(format!("{owner}:{head_branch}"))
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("list PRs for {owner}/{repo}: {e}")))?;

        Ok(prs.items.into_iter().next().map(|pr| PrInfo {
            number: pr.number,
            url: pr.html_url.map(|u| u.to_string()).unwrap_or_default(),
        }))
    }

    pub async fn create_pr(&self, req: &CreatePrRequest<'_>) -> crate::error::Result<PrInfo> {
        if self.dry_run {
            info!(
                "[dry-run] would create PR '{}' in {}/{} ({} -> {})",
                req.title, req.owner, req.repo, req.head, req.base
            );
            return Ok(PrInfo {
                number: 0,
                url: format!("https://github.com/{}/{}/pull/0", req.owner, req.repo),
            });
        }

        let pr = self
            .crab
            .pulls(req.owner, req.repo)
            .create(req.title, req.head, req.base)
            .body(req.body)
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("create PR in {}/{}: {e}", req.owner, req.repo)))?;

        let info = PrInfo {
            number: pr.number,
            url: pr.html_url.map(|u| u.to_string()).unwrap_or_default(),
        };

        if !req.labels.is_empty() {
            let labels: Vec<String> = req.labels.to_vec();
            if let Err(e) = self
                .crab
                .issues(req.owner, req.repo)
                .add_labels(info.number, &labels)
                .await
            {
                warn!("failed to add labels to PR #{}: {e}", info.number);
            }
        }

        Ok(info)
    }

    pub async fn update_pr(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
        title: &str,
        body: &str,
    ) -> crate::error::Result<()> {
        if self.dry_run {
            info!("[dry-run] would update PR #{pr_number} in {owner}/{repo}");
            return Ok(());
        }

        self.crab
            .pulls(owner, repo)
            .update(pr_number)
            .title(title)
            .body(body)
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("update PR #{pr_number} in {owner}/{repo}: {e}")))?;

        Ok(())
    }

    pub async fn discover_repos(
        &self,
        org: &str,
        topic: Option<&str>,
    ) -> crate::error::Result<Vec<String>> {
        let query = match topic {
            Some(t) => format!("org:{org} topic:{t}"),
            None => format!("org:{org}"),
        };

        let mut results = self
            .crab
            .search()
            .repositories(&query)
            .per_page(100)
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("search repos: {e}")))?;

        let mut repos: Vec<String> = results
            .items
            .into_iter()
            .filter_map(|r| r.full_name)
            .collect();

        while let Some(mut next_page) = self
            .crab
            .get_page::<octocrab::models::Repository>(&results.next)
            .await
            .map_err(|e| Error::GitHub(format!("search repos next page: {e}")))?
        {
            repos.extend(
                std::mem::take(&mut next_page.items)
                    .into_iter()
                    .filter_map(|r| r.full_name),
            );
            results = next_page;
        }

        Ok(repos)
    }
}

fn is_not_found(e: &octocrab::Error) -> bool {
    matches!(e, octocrab::Error::GitHub { source, .. } if source.status_code == http::StatusCode::NOT_FOUND)
}

fn is_unprocessable(e: &octocrab::Error) -> bool {
    matches!(e, octocrab::Error::GitHub { source, .. } if source.status_code == http::StatusCode::UNPROCESSABLE_ENTITY)
}
