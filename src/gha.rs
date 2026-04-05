pub fn is_github_actions() -> bool {
    std::env::var("GITHUB_ACTIONS").is_ok()
}

pub fn group(title: &str) {
    if is_github_actions() {
        println!("::group::{title}");
    }
}

pub fn endgroup() {
    if is_github_actions() {
        println!("::endgroup::");
    }
}

pub fn warning(msg: &str) {
    if is_github_actions() {
        println!("::warning::{msg}");
    }
}

pub fn error(msg: &str) {
    if is_github_actions() {
        println!("::error::{msg}");
    }
}

pub fn write_summary(content: &str) {
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&path) {
            let _ = f.write_all(content.as_bytes());
        }
    }
}
