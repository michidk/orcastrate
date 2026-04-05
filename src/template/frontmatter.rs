use serde::Deserialize;
use std::collections::HashMap;

use crate::error::Error;

const MARKER_START: &str = "# @orcastrate";
const MARKER_END: &str = "# @end-orcastrate";

#[derive(Debug, Clone, Deserialize)]
pub struct Frontmatter {
    pub template: String,
    #[serde(default)]
    pub params: HashMap<String, serde_norway::Value>,
}

#[derive(Debug)]
pub struct ParsedWorkflow {
    pub frontmatter: Option<Frontmatter>,
    pub raw_block: Option<String>,
}

pub fn parse(content: &str) -> crate::error::Result<ParsedWorkflow> {
    let start_idx = content.find(MARKER_START);
    let end_idx = content.find(MARKER_END);

    match (start_idx, end_idx) {
        (Some(start), Some(end)) if start < end => {
            let block_end = end + MARKER_END.len();
            let raw_block = &content[start..block_end];

            let yaml_content: String = raw_block
                .lines()
                .filter(|line| {
                    let trimmed = line.trim();
                    trimmed != MARKER_START.trim() && trimmed != MARKER_END.trim()
                })
                .map(|line| {
                    let trimmed = line.trim();
                    trimmed
                        .strip_prefix("# ")
                        .or_else(|| trimmed.strip_prefix("#"))
                        .unwrap_or(trimmed)
                })
                .collect::<Vec<_>>()
                .join("\n");

            let frontmatter: Frontmatter =
                serde_norway::from_str(&yaml_content).map_err(|e| Error::Frontmatter {
                    file: String::new(),
                    message: format!("invalid YAML in frontmatter: {e}"),
                })?;

            Ok(ParsedWorkflow {
                frontmatter: Some(frontmatter),
                raw_block: Some(raw_block.to_string()),
            })
        }
        (Some(_), None) => Err(Error::Frontmatter {
            file: String::new(),
            message: format!("found {MARKER_START} but missing {MARKER_END}"),
        }),
        (None, Some(_)) => Err(Error::Frontmatter {
            file: String::new(),
            message: format!("found {MARKER_END} but missing {MARKER_START}"),
        }),
        _ => Ok(ParsedWorkflow {
            frontmatter: None,
            raw_block: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_frontmatter() {
        let input = r#"# @orcastrate
# template: rust-ci
# params:
#   toolchain: stable
#   features: ["default"]
# @end-orcastrate

name: CI
on: [push]
"#;
        let parsed = parse(input).unwrap();
        let fm = parsed.frontmatter.unwrap();
        assert_eq!(fm.template, "rust-ci");
        assert_eq!(
            fm.params.get("toolchain").and_then(|v| v.as_str()),
            Some("stable")
        );
    }

    #[test]
    fn parse_no_frontmatter() {
        let input = "name: CI\non: [push]\n";
        let parsed = parse(input).unwrap();
        assert!(parsed.frontmatter.is_none());
    }

    #[test]
    fn reject_unclosed_block() {
        let input = "# @orcastrate\n# template: foo\n";
        assert!(parse(input).is_err());
    }

    #[test]
    fn reject_orphan_end_marker() {
        let input = "# @end-orcastrate\nname: CI\n";
        assert!(parse(input).is_err());
    }

    #[test]
    fn preserves_raw_block() {
        let input = "# @orcastrate\n# template: foo\n# @end-orcastrate\n\nname: CI\n";
        let parsed = parse(input).unwrap();
        assert!(parsed.raw_block.is_some());
        assert!(parsed.raw_block.unwrap().contains("@orcastrate"));
    }
}
