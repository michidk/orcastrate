# Orcastrate Agent Guide

## Project overview

Orcastrate is a Rust CLI for managing GitHub Actions workflows across many repositories from a central orchestrator repository. It is designed to run inside a scheduled GitHub Action, render canonical workflow templates, detect drift in managed workflow files, and open or update PRs or issues when repositories diverge from their templates.

## Architecture

### Module map

| Module | Responsibility |
|---|---|
| `src/main.rs` | CLI entrypoint, logging setup, command dispatch, client construction |
| `src/cli.rs` | Clap CLI definitions and sync mode selection |
| `src/config.rs` | `orchestrator.toml` parsing, validation, active repo filtering |
| `src/error.rs` | Central error enum and result alias |
| `src/diff.rs` | Drift detection, unified diff generation, diff summaries |
| `src/sync.rs` | Repo discovery, workflow scanning, drift detection, PR or issue creation, report generation |
| `src/github/mod.rs` | GitHub API wrapper for repo discovery, workflow reads, git data writes, PRs, and issues |
| `src/template/frontmatter.rs` | Managed workflow frontmatter parsing from YAML comment blocks |
| `src/template/render.rs` | Tera-backed template rendering plus YAML validation |
| `src/gha.rs` | GitHub Actions specific log grouping, annotations, and step summary helpers |

## Key design decisions

- Git writes use the Git data API for commits and refs instead of the contents API. This avoids the workflow scope problems that come with updating `.github/workflows/*` through the contents API.
- PAT-based operation uses a dual-token pattern: `ORCASTRATE_TOKEN` handles git writes, while `GITHUB_TOKEN` handles PR and issue operations.
- GitHub App auth uses a single installation token for everything.
- Drift is handled with one PR per workflow file so each workflow can be reviewed and merged independently.
- Managed workflow metadata lives in YAML frontmatter embedded in YAML comments between `# @orcastrate` and `# @end-orcastrate`.
- Templates are rendered with Tera and then validated as YAML after rendering.
- YAML parsing uses `serde_norway` instead of `serde_yaml` to avoid the relevant `RUSTSEC` issue.

## Testing

- Run `cargo test`.
- The current unit test suite contains 13 tests across `config`, `frontmatter`, and `diff`.

## CI/CD

- CI runs build, test, and clippy checks.
- Releases are generated with `git-cliff`.
- Docker images are published to GHCR.
- Release automation also produces binary artifacts.
- Floating major tags are maintained for releases.

## Authentication matrix

| Auth | Git writes | PRs | Commit author | PR author |
|---|---|---|---|---|
| App | App token | App token | App (implicit) | App bot |
| PAT + GITHUB_TOKEN | PAT | GITHUB_TOKEN | github-actions[bot] | github-actions[bot] |
| GITHUB_TOKEN only | GITHUB_TOKEN | GITHUB_TOKEN | github-actions[bot] | github-actions[bot] |
