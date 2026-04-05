use similar::{ChangeTag, TextDiff};

pub fn has_drifted(current: &str, rendered: &str) -> bool {
    current != rendered
}

pub fn generate_diff(current: &str, rendered: &str, file_path: &str) -> String {
    TextDiff::from_lines(current, rendered)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{file_path}"), &format!("b/{file_path}"))
        .to_string()
}

pub fn diff_summary(current: &str, rendered: &str) -> DiffSummary {
    let diff = TextDiff::from_lines(current, rendered);
    let mut added = 0;
    let mut removed = 0;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => added += 1,
            ChangeTag::Delete => removed += 1,
            ChangeTag::Equal => {}
        }
    }

    DiffSummary { added, removed }
}

#[derive(Debug)]
pub struct DiffSummary {
    pub added: usize,
    pub removed: usize,
}

impl std::fmt::Display for DiffSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "+{} -{}", self.added, self.removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_no_drift() {
        let content = "name: CI\non: [push]\n";
        assert!(!has_drifted(content, content));
    }

    #[test]
    fn different_content_drifted() {
        assert!(has_drifted("a: 1\n", "a: 2\n"));
    }

    #[test]
    fn diff_shows_changes() {
        let diff = generate_diff("line1\nline2\n", "line1\nchanged\n", "workflow.yml");
        assert!(diff.contains("--- a/workflow.yml"));
        assert!(diff.contains("+++ b/workflow.yml"));
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+changed"));
    }

    #[test]
    fn summary_counts() {
        let summary = diff_summary("a\nb\nc\n", "a\nx\ny\nc\n");
        assert_eq!(summary.removed, 1);
        assert_eq!(summary.added, 2);
    }
}
