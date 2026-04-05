# orcastrate

GitHub Actions orchestrator — template, sync, and manage workflows at org scale.

Stop copy-pasting workflow files across repos. Define canonical templates once, sync them everywhere, and get PRs when repos drift.

## How it works

```
orchestrator repo
├── orchestrator.toml          # which repos to manage
├── templates/
│   ├── rust-ci.yml            # Tera-powered workflow templates
│   └── node-ci.yml
└── .github/workflows/
    └── orcastrate.yml         # scheduled Action that runs orcastrate
```

Orcastrate runs as a scheduled GitHub Action in your central orchestrator repo. On each run it:

1. Reads your repo list from `orchestrator.toml`
2. Scans each repo's `.github/workflows/` for files with `@orcastrate` frontmatter
3. Renders the referenced template with the declared params
4. Compares the rendered output against the current file
5. Opens a PR if drift is detected

## Managed workflow frontmatter

Workflow files opt in to management via a comment block at the top:

```yaml
# @orcastrate
# template: rust-ci
# params:
#   toolchain: stable
#   features: ["serde", "async"]
# @end-orcastrate

name: CI
on: [push]
# ... rest of workflow managed by orcastrate
```

Only files with this block are touched. Everything else is ignored.

## Quick start

### 1. Create the orchestrator repo

Create a new repo in your org (e.g. `myorg/workflow-orchestrator`).

### 2. Add config

```toml
# orchestrator.toml

[orchestrator]
templates_dir = "templates"

[[repos]]
name = "myorg/service-api"

[[repos]]
name = "myorg/service-web"
```

### 3. Add a template

```yaml
# templates/rust-ci.yml

name: CI
on:
  push:
    branches: [{{ default_branch | default(value="main") }}]
  pull_request:
    branches: [{{ default_branch | default(value="main") }}]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@{{ toolchain | default(value="stable") }}
      - run: cargo test --all-features
```

### 4. Set up the scheduled Action

```yaml
# .github/workflows/orcastrate.yml

name: Orcastrate Sync
on:
  schedule:
    - cron: "0 8 * * *"  # daily at 8am UTC
  workflow_dispatch:

jobs:
  sync:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: orcastrate/orcastrate@v0
        with:
          command: sync
          github-token: ${{ secrets.ORCASTRATE_TOKEN }}
```

### 5. Add frontmatter to target repos

In each managed repo, add the frontmatter block to workflow files:

```yaml
# @orcastrate
# template: rust-ci
# params:
#   toolchain: stable
# @end-orcastrate

name: CI
# ... orcastrate will manage the rest
```

## CLI usage

```
orcastrate sync              # sync all repos, open PRs for drift
orcastrate sync --dry-run    # see what would change without modifying anything
orcastrate validate          # check config + templates are valid
orcastrate drift             # check drift status without creating PRs
orcastrate list-repos        # show configured + discovered repos
orcastrate list-templates    # show available templates
```

## Configuration

### `orchestrator.toml`

```toml
[orchestrator]
templates_dir = "templates"        # where templates live
branch_prefix = "orcastrate/sync"  # PR branch naming
pr_label = "orcastrate"            # label added to PRs
dry_run = false                    # global dry-run toggle

[[repos]]
name = "myorg/repo-a"

[[repos]]
name = "myorg/repo-b"
enabled = false                    # temporarily skip

# auto-discover repos by org topic
[discovery]
org = "myorg"
topic = "managed-workflows"
```

### Template params

Templates use [Tera](https://keats.github.io/tera/) (Jinja2-style) syntax. Params declared in frontmatter are injected at render time:

```yaml
# @orcastrate
# template: rust-ci
# params:
#   toolchain: nightly
#   features: ["serde", "tokio"]
#   default_branch: develop
# @end-orcastrate
```

Templates can use defaults: `{{ toolchain | default(value="stable") }}`

## Authentication

### GitHub token (simplest)

Set `GITHUB_TOKEN` or `ORCASTRATE_TOKEN`. Needs `contents:write` and `pull_requests:write` on target repos.

### GitHub App (recommended for orgs)

Create a GitHub App with these permissions:
- **Repository contents**: Read & Write
- **Pull requests**: Read & Write

Install it on your org, then set:
- `ORCASTRATE_APP_ID`
- `ORCASTRATE_PRIVATE_KEY`
- `ORCASTRATE_INSTALLATION_ID`

## What gets PRed

When orcastrate detects drift, it opens a PR with:
- A summary of which workflows changed and which templates were used
- Per-file diffs in collapsible sections
- The `orcastrate` label for easy filtering

If a PR already exists for the same sync branch, it updates the existing PR instead of creating a new one.

## Design decisions

- **No external server** — runs entirely within GitHub Actions
- **State in git** — config and templates are version-controlled, auditable
- **PR-based updates** — never force-pushes to your default branch
- **Opt-in per file** — only workflows with `@orcastrate` frontmatter are managed
- **Template validation** — rendered output is validated as YAML before opening a PR

## License

MIT
