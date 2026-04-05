use octocrab::Octocrab;
use tracing::{debug, info, warn};

use crate::error::Error;

const BOT_NAME: &str = "github-actions[bot]";
const BOT_EMAIL: &str = "41898282+github-actions[bot]@users.noreply.github.com";

pub struct GitHubClient {
    crab: Octocrab,
    pr_crab: Option<Octocrab>,
    is_app_auth: bool,
    dry_run: bool,
}

pub struct WorkflowFile {
    pub path: String,
}

pub struct FileChange {
    pub path: String,
    pub content: String,
}

pub struct CommitRequest<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub branch: &'a str,
    pub message: &'a str,
    pub base_sha: &'a str,
    pub files: &'a [FileChange],
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
    pub fn new(
        crab: Octocrab,
        pr_crab: Option<Octocrab>,
        is_app_auth: bool,
        dry_run: bool,
    ) -> Self {
        Self {
            crab,
            pr_crab,
            is_app_auth,
            dry_run,
        }
    }

    fn pr_client(&self) -> &Octocrab {
        self.pr_crab.as_ref().unwrap_or(&self.crab)
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

    pub async fn commit_files(&self, req: &CommitRequest<'_>) -> crate::error::Result<()> {
        if self.dry_run {
            for file in req.files {
                info!(
                    "[dry-run] would update {} in {}/{}",
                    file.path, req.owner, req.repo
                );
            }
            return Ok(());
        }

        let tree_items: Vec<serde_json::Value> = req
            .files
            .iter()
            .map(|f| {
                serde_json::json!({
                    "path": f.path,
                    "mode": "100644",
                    "type": "blob",
                    "content": f.content,
                })
            })
            .collect();

        let tree_route = format!("/repos/{}/{}/git/trees", req.owner, req.repo);
        let tree: serde_json::Value = self
            .crab
            .post(
                &tree_route,
                Some(&serde_json::json!({
                    "base_tree": req.base_sha,
                    "tree": tree_items,
                })),
            )
            .await
            .map_err(|e| {
                Error::GitHub(format!(
                    "create tree in {}/{}: {}",
                    req.owner,
                    req.repo,
                    e.to_string().replace('\n', " ")
                ))
            })?;

        let tree_sha = tree["sha"]
            .as_str()
            .ok_or_else(|| Error::GitHub("missing sha in tree response".into()))?;

        let mut commit_body = serde_json::json!({
            "message": req.message,
            "tree": tree_sha,
            "parents": [req.base_sha],
        });

        if !self.is_app_auth {
            let bot = serde_json::json!({
                "name": BOT_NAME,
                "email": BOT_EMAIL,
            });
            commit_body["author"] = bot.clone();
            commit_body["committer"] = bot;
        }

        let commit_route = format!("/repos/{}/{}/git/commits", req.owner, req.repo);
        let commit: serde_json::Value = self
            .crab
            .post(&commit_route, Some(&commit_body))
            .await
            .map_err(|e| {
                Error::GitHub(format!(
                    "create commit in {}/{}: {}",
                    req.owner,
                    req.repo,
                    e.to_string().replace('\n', " ")
                ))
            })?;

        let commit_sha = commit["sha"]
            .as_str()
            .ok_or_else(|| Error::GitHub("missing sha in commit response".into()))?;

        let ref_route = format!(
            "/repos/{}/{}/git/refs/heads/{}",
            req.owner, req.repo, req.branch
        );
        let _: serde_json::Value = self
            .crab
            .patch(
                &ref_route,
                Some(&serde_json::json!({
                    "sha": commit_sha,
                    "force": true,
                })),
            )
            .await
            .map_err(|e| {
                Error::GitHub(format!(
                    "update ref {} in {}/{}: {}",
                    req.branch,
                    req.owner,
                    req.repo,
                    e.to_string().replace('\n', " ")
                ))
            })?;

        Ok(())
    }

    pub async fn find_existing_pr(
        &self,
        owner: &str,
        repo: &str,
        head_branch: &str,
    ) -> crate::error::Result<Option<PrInfo>> {
        let prs = self
            .pr_client()
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
            .pr_client()
            .pulls(req.owner, req.repo)
            .create(req.title, req.head, req.base)
            .body(req.body)
            .send()
            .await
            .map_err(|e| {
                Error::GitHub(format!("create PR in {}/{}: {e:?}", req.owner, req.repo,))
            })?;

        let info = PrInfo {
            number: pr.number,
            url: pr.html_url.map(|u| u.to_string()).unwrap_or_default(),
        };

        if !req.labels.is_empty() {
            let labels: Vec<String> = req.labels.to_vec();
            if let Err(e) = self
                .pr_client()
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

        self.pr_client()
            .pulls(owner, repo)
            .update(pr_number)
            .title(title)
            .body(body)
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("update PR #{pr_number} in {owner}/{repo}: {e}")))?;

        Ok(())
    }

    pub async fn find_existing_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
    ) -> crate::error::Result<Option<PrInfo>> {
        let issues = self
            .pr_client()
            .issues(owner, repo)
            .list()
            .state(octocrab::params::State::Open)
            .per_page(100)
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("list issues for {owner}/{repo}: {e}")))?;

        Ok(issues
            .items
            .into_iter()
            .find(|i| i.title == title && i.pull_request.is_none())
            .map(|i| PrInfo {
                number: i.number,
                url: i.html_url.to_string(),
            }))
    }

    pub async fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> crate::error::Result<PrInfo> {
        if self.dry_run {
            info!("[dry-run] would create issue '{title}' in {owner}/{repo}");
            return Ok(PrInfo {
                number: 0,
                url: format!("https://github.com/{owner}/{repo}/issues/0"),
            });
        }

        let issue = self
            .pr_client()
            .issues(owner, repo)
            .create(title)
            .body(body)
            .labels(labels.to_vec())
            .send()
            .await
            .map_err(|e| Error::GitHub(format!("create issue in {owner}/{repo}: {e:?}",)))?;

        Ok(PrInfo {
            number: issue.number,
            url: issue.html_url.to_string(),
        })
    }

    pub async fn update_issue(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
        body: &str,
    ) -> crate::error::Result<()> {
        if self.dry_run {
            info!("[dry-run] would update issue #{issue_number} in {owner}/{repo}");
            return Ok(());
        }

        self.pr_client()
            .issues(owner, repo)
            .update(issue_number)
            .body(body)
            .send()
            .await
            .map_err(|e| {
                Error::GitHub(format!(
                    "update issue #{issue_number} in {owner}/{repo}: {e}"
                ))
            })?;

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
