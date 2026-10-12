use std::collections::HashMap;

use serde_norway::Value;

use crate::error::{Error, Result};

struct ActionRef {
    job: String,
    step: Option<usize>,
    id: Option<String>,
    identity: String,
    occurrence: usize,
    value: String,
}

fn action_refs(document: &Value) -> Vec<ActionRef> {
    let mut refs = Vec::new();
    let Some(jobs) = document["jobs"].as_mapping() else {
        return refs;
    };
    for (job, config) in jobs {
        let Some(job) = job.as_str() else { continue };
        let mut occurrences = HashMap::new();
        let entries = std::iter::once((None, config)).chain(
            config["steps"]
                .as_sequence()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(index, step)| (Some(index), step)),
        );
        for (step, entry) in entries {
            if let Some(value) = entry["uses"].as_str() {
                let identity = value.rsplit_once('@').map_or(value, |(action, _)| action);
                let occurrence = occurrences.entry(identity.to_string()).or_insert(0);
                refs.push(ActionRef {
                    job: job.to_string(),
                    step,
                    id: entry["id"].as_str().map(str::to_string),
                    identity: identity.to_string(),
                    occurrence: *occurrence,
                    value: value.to_string(),
                });
                *occurrence += 1;
            }
        }
    }
    refs
}

// shortcut: edits require block-style uses fields; use a span-aware YAML parser for other layouts.
fn source_lines<'a>(content: &'a str, refs: &[ActionRef]) -> Result<Vec<(usize, usize, &'a str)>> {
    let mut sources = Vec::new();
    for (index, line) in content.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_start();
        let entry = trimmed.strip_prefix("- ").unwrap_or(trimmed).trim_start();
        if !entry.starts_with("uses:") {
            continue;
        }
        let fragment: Value = serde_norway::from_str(entry)?;
        let Some(value) = fragment["uses"].as_str() else {
            return Err(unsupported_layout());
        };
        if refs.get(sources.len()).map(|r| r.value.as_str()) != Some(value) {
            return Err(unsupported_layout());
        }
        let colon = line.find(':').unwrap();
        sources.push((
            index,
            colon + 1,
            line[colon + 1..].trim_end_matches(['\r', '\n']),
        ));
    }
    if sources.len() != refs.len() {
        return Err(unsupported_layout());
    }
    Ok(sources)
}

fn unsupported_layout() -> Error {
    Error::YamlValidation(
        "cannot safely preserve action versions; use single-line block-style uses fields, or set ignore_action_versions = false".into(),
    )
}

pub fn preserve_action_versions(current: &str, rendered: &str) -> Result<String> {
    let current_document: Value = serde_norway::from_str(current)?;
    let mut expected: Value = serde_norway::from_str(rendered)?;
    let current_refs = action_refs(&current_document);
    let rendered_refs = action_refs(&expected);
    let mut replacements = Vec::new();
    for (rendered_index, target) in rendered_refs.iter().enumerate() {
        if !target.value.contains('@')
            || target.value.starts_with("./")
            || target.value.starts_with("docker://")
        {
            continue;
        }
        let matches_action = |r: &ActionRef| {
            r.job == target.job
                && r.step.is_some() == target.step.is_some()
                && r.identity == target.identity
                && r.value.contains('@')
        };
        let source = target
            .id
            .as_ref()
            .and_then(|id| {
                current_refs
                    .iter()
                    .enumerate()
                    .find(|(_, r)| matches_action(r) && r.id.as_ref() == Some(id))
            })
            .or_else(|| {
                current_refs
                    .iter()
                    .enumerate()
                    .find(|(_, r)| matches_action(r) && r.occurrence == target.occurrence)
            });
        if let Some((current_index, source)) = source {
            replacements.push((rendered_index, current_index));
            let entry = match target.step {
                Some(index) => &mut expected["jobs"][&target.job]["steps"][index],
                None => &mut expected["jobs"][&target.job],
            };
            entry["uses"] = Value::String(source.value.clone());
        }
    }
    if replacements.is_empty() {
        return Ok(rendered.to_string());
    }

    let current_sources = source_lines(current, &current_refs)?;
    let rendered_sources = source_lines(rendered, &rendered_refs)?;
    let mut lines: Vec<String> = rendered.split_inclusive('\n').map(str::to_string).collect();
    for (rendered_index, current_index) in replacements {
        let (line_index, prefix_end, _) = rendered_sources[rendered_index];
        let suffix = current_sources[current_index].2;
        let line = &lines[line_index];
        let ending = if line.ends_with("\r\n") {
            "\r\n"
        } else if line.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        lines[line_index] = format!("{}{suffix}{ending}", &line[..prefix_end]);
    }
    let preserved = lines.concat();
    // Check that source matching changed only the intended YAML fields, never script text.
    if serde_norway::from_str::<Value>(&preserved)? != expected {
        return Err(unsupported_layout());
    }
    Ok(preserved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow(steps: &str) -> String {
        format!("name: CI\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n{steps}")
    }

    #[test]
    fn preserves_tags_hashes_quotes_and_version_comments() {
        for reference in [
            "actions/checkout@v7",
            "'actions/checkout@v7'",
            "\"actions/checkout@abcdef123456\" # v7.0.1",
        ] {
            let current = workflow(&format!("      - uses: {reference}\n"));
            let rendered = workflow("      - uses: actions/checkout@v4 # v4.0.0\n");
            assert_eq!(
                preserve_action_versions(&current, &rendered).unwrap(),
                current
            );
        }
    }

    #[test]
    fn preserves_versions_when_workflow_structure_changes() {
        let current = workflow("      - uses: actions/checkout@v7 # v7\n      - run: cargo test\n");
        let rendered = workflow("      - name: Checkout\n        uses: actions/checkout@v4\n      - uses: actions/setup-node@v4\n      - run: cargo test --all-features\n");
        let expected = workflow("      - name: Checkout\n        uses: actions/checkout@v7 # v7\n      - uses: actions/setup-node@v4\n      - run: cargo test --all-features\n");
        assert_eq!(
            preserve_action_versions(&current, &rendered).unwrap(),
            expected
        );
    }

    #[test]
    fn action_replacements_local_actions_and_containers_are_not_ignored() {
        let current = workflow("      - uses: actions/checkout@v7\n      - uses: ./local\n      - uses: docker://alpine@sha256:new\n      - uses: org/unversioned\n");
        let rendered = workflow("      - uses: other/checkout@v1\n      - uses: ./different\n      - uses: docker://alpine@sha256:old\n      - uses: org/unversioned@v1\n");
        assert_eq!(
            preserve_action_versions(&current, &rendered).unwrap(),
            rendered
        );
    }

    #[test]
    fn matches_refs_within_jobs_including_reusable_workflows() {
        let current = "jobs:\n  first:\n    steps:\n      - uses: actions/checkout@v7\n  second:\n    steps:\n      - uses: actions/checkout@v6\n  reusable:\n    uses: org/repo/.github/workflows/ci.yml@new\n";
        let rendered = "jobs:\n  second:\n    steps:\n      - uses: actions/checkout@v4\n  first:\n    steps:\n      - uses: actions/checkout@v4\n  reusable:\n    uses: org/repo/.github/workflows/ci.yml@old\n";
        let expected = "jobs:\n  second:\n    steps:\n      - uses: actions/checkout@v6\n  first:\n    steps:\n      - uses: actions/checkout@v7\n  reusable:\n    uses: org/repo/.github/workflows/ci.yml@new\n";
        assert_eq!(
            preserve_action_versions(current, rendered).unwrap(),
            expected
        );
    }

    #[test]
    fn repeated_actions_keep_their_own_refs() {
        let current = workflow("      - uses: org/action@v2\n      - uses: org/action@v3\n");
        let rendered = workflow("      - uses: org/action@v1\n      - uses: org/action@v1\n");
        assert_eq!(
            preserve_action_versions(&current, &rendered).unwrap(),
            current
        );
    }

    #[test]
    fn step_ids_preserve_refs_when_repeated_actions_are_reordered() {
        let current = workflow("      - id: first\n        uses: org/action@v2\n      - id: second\n        uses: org/action@v3\n");
        let rendered = workflow("      - id: second\n        uses: org/action@v1\n      - id: first\n        uses: org/action@v1\n");
        let expected = workflow("      - id: second\n        uses: org/action@v3\n      - id: first\n        uses: org/action@v2\n");
        assert_eq!(
            preserve_action_versions(&current, &rendered).unwrap(),
            expected
        );
    }

    #[test]
    fn refuses_to_rewrite_ambiguous_script_text_or_inline_yaml() {
        let script = workflow("      - run: |\n          uses: actions/checkout@v7\n      - uses: actions/checkout@v7\n");
        let rendered = workflow("      - uses: actions/checkout@v4\n");
        assert!(preserve_action_versions(&script, &rendered).is_err());
        let inline = "jobs: {test: {steps: [{uses: actions/checkout@v7}]}}\n";
        assert!(preserve_action_versions(inline, &rendered).is_err());
        let ambiguous = workflow("      - run: |\n          uses: actions/checkout@v4\n      - {uses: actions/checkout@v4}\n");
        let current = workflow("      - uses: actions/checkout@v7\n");
        assert!(preserve_action_versions(&current, &ambiguous).is_err());
    }
}
