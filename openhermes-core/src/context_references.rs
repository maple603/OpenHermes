//! Context reference system for `@file:`, `@folder:`, `@git:`, `@url:`, `@diff`, `@staged`.
//!
//! Parses user messages for `@` references, expands them into injected context,
//! and enforces security boundaries (sensitive path blocking, token budget limits).

use std::path::{Path, PathBuf};
use std::process::Command;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::model_metadata::estimate_tokens_rough;

// ── Reference pattern ───────────────────────────────────────────────────

static REFERENCE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    // Use a non-capturing prefix instead of lookbehind (unsupported by regex crate).
    // `(?:^|[^\w/])` ensures `@` is not preceded by a word char or `/`.
    // The actual reference is captured in the `refmatch` named group.
    Regex::new(
        r#"(?:^|[^\w/])(?P<refmatch>@(?:(?P<simple>diff|staged)\b|(?P<kind>file|folder|git|url):(?P<value>(?:`[^`\n]+`|"[^"\n]+"|'[^'\n]+')(?::\d+(?:-\d+)?)?|\S+)))"#
    ).expect("valid regex")
});

/// Trailing punctuation to strip from reference values.
const TRAILING_PUNCTUATION: &str = ",.;!?";

/// Sensitive directories under $HOME that cannot be attached.
const SENSITIVE_HOME_DIRS: &[&str] = &[
    ".ssh", ".aws", ".gnupg", ".kube", ".docker", ".azure", ".config/gh",
];

/// Sensitive files under $HOME that cannot be attached.
const SENSITIVE_HOME_FILES: &[&str] = &[
    ".ssh/authorized_keys",
    ".ssh/id_rsa",
    ".ssh/id_ed25519",
    ".ssh/config",
    ".bashrc",
    ".zshrc",
    ".profile",
    ".bash_profile",
    ".zprofile",
    ".netrc",
    ".pgpass",
    ".npmrc",
    ".pypirc",
];

// ── Types ───────────────────────────────────────────────────────────────

/// A parsed context reference from user input.
#[derive(Debug, Clone)]
pub struct ContextReference {
    pub raw: String,
    pub kind: String,
    pub target: String,
    pub start: usize,
    pub end: usize,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
}

/// Result of preprocessing context references.
#[derive(Debug, Clone)]
pub struct ContextReferenceResult {
    pub message: String,
    pub original_message: String,
    pub references: Vec<ContextReference>,
    pub warnings: Vec<String>,
    pub injected_tokens: usize,
    pub expanded: bool,
    pub blocked: bool,
}

// ── Public API ──────────────────────────────────────────────────────────

/// Parse all `@` references from a message.
pub fn parse_context_references(message: &str) -> Vec<ContextReference> {
    let mut refs = Vec::new();
    if message.is_empty() {
        return refs;
    }

    for cap in REFERENCE_PATTERN.captures_iter(message) {
        let full_match = cap.name("refmatch").unwrap();

        if let Some(simple) = cap.name("simple") {
            refs.push(ContextReference {
                raw: full_match.as_str().to_string(),
                kind: simple.as_str().to_string(),
                target: String::new(),
                start: full_match.start(),
                end: full_match.end(),
                line_start: None,
                line_end: None,
            });
            continue;
        }

        if let Some(kind) = cap.name("kind") {
            let value = cap.name("value").map(|v| v.as_str()).unwrap_or("");
            let value = strip_trailing_punctuation(value);
            let (target, line_start, line_end) = if kind.as_str() == "file" {
                parse_file_reference_value(&value)
            } else {
                (strip_reference_wrappers(&value), None, None)
            };

            refs.push(ContextReference {
                raw: full_match.as_str().to_string(),
                kind: kind.as_str().to_string(),
                target,
                start: full_match.start(),
                end: full_match.end(),
                line_start,
                line_end,
            });
        }
    }

    refs
}

/// Preprocess context references: parse, expand, and inject into message.
pub fn preprocess_context_references(
    message: &str,
    cwd: &Path,
    context_length: usize,
) -> ContextReferenceResult {
    let refs = parse_context_references(message);
    if refs.is_empty() {
        return ContextReferenceResult {
            message: message.to_string(),
            original_message: message.to_string(),
            references: refs,
            warnings: Vec::new(),
            injected_tokens: 0,
            expanded: false,
            blocked: false,
        };
    }

    let mut warnings = Vec::new();
    let mut blocks = Vec::new();
    let mut injected_tokens = 0;

    for r in &refs {
        let (warning, block) = expand_reference(r, cwd);
        if let Some(w) = warning {
            warnings.push(w);
        }
        if let Some(b) = block {
            injected_tokens += estimate_tokens_rough(&b);
            blocks.push(b);
        }
    }

    // Token budget enforcement
    let hard_limit = (context_length as f64 * 0.50).max(1.0) as usize;
    let soft_limit = (context_length as f64 * 0.25).max(1.0) as usize;

    if injected_tokens > hard_limit {
        warnings.push(format!(
            "@ context injection refused: {} tokens exceeds the 50% hard limit ({}).",
            injected_tokens, hard_limit
        ));
        return ContextReferenceResult {
            message: message.to_string(),
            original_message: message.to_string(),
            references: refs,
            warnings,
            injected_tokens,
            expanded: false,
            blocked: true,
        };
    }

    if injected_tokens > soft_limit {
        warnings.push(format!(
            "@ context injection warning: {} tokens exceeds the 25% soft limit ({}).",
            injected_tokens, soft_limit
        ));
    }

    // Build final message
    let stripped = remove_reference_tokens(message, &refs);
    let mut final_msg = stripped;

    if !warnings.is_empty() {
        final_msg.push_str("\n\n--- Context Warnings ---\n");
        for w in &warnings {
            final_msg.push_str(&format!("- {}\n", w));
        }
    }
    if !blocks.is_empty() {
        final_msg.push_str("\n\n--- Attached Context ---\n\n");
        final_msg.push_str(&blocks.join("\n\n"));
    }

    ContextReferenceResult {
        message: final_msg.trim().to_string(),
        original_message: message.to_string(),
        references: refs,
        warnings,
        injected_tokens,
        expanded: !blocks.is_empty(),
        blocked: false,
    }
}

// ── Expanders ───────────────────────────────────────────────────────────

fn expand_reference(
    reference: &ContextReference,
    cwd: &Path,
) -> (Option<String>, Option<String>) {
    match reference.kind.as_str() {
        "file" => expand_file_reference(reference, cwd),
        "folder" => expand_folder_reference(reference, cwd),
        "diff" => expand_git_reference(cwd, &["diff"], "git diff"),
        "staged" => expand_git_reference(cwd, &["diff", "--staged"], "git diff --staged"),
        "git" => {
            let count: usize = reference.target.parse().unwrap_or(1).max(1).min(10);
            expand_git_reference(
                cwd,
                &["log", &format!("-{}", count), "-p"],
                &format!("git log -{} -p", count),
            )
        }
        "url" => {
            // URL expansion requires async — return a note for now
            (
                None,
                Some(format!(
                    "URL: {} (content will be fetched by web_extract tool)",
                    reference.target
                )),
            )
        }
        _ => (
            Some(format!("{}: unsupported reference type", reference.raw)),
            None,
        ),
    }
}

fn expand_file_reference(
    reference: &ContextReference,
    cwd: &Path,
) -> (Option<String>, Option<String>) {
    let path = resolve_path(cwd, &reference.target);

    if let Err(e) = ensure_path_allowed(&path) {
        return (Some(format!("{}: {}", reference.raw, e)), None);
    }

    if !path.exists() {
        return (Some(format!("{}: file not found", reference.raw)), None);
    }
    if !path.is_file() {
        return (Some(format!("{}: path is not a file", reference.raw)), None);
    }

    match std::fs::read_to_string(&path) {
        Ok(mut text) => {
            if let Some(start) = reference.line_start {
                let lines: Vec<&str> = text.lines().collect();
                let start_idx = (start.saturating_sub(1)).min(lines.len());
                let end_idx = reference
                    .line_end
                    .unwrap_or(start)
                    .min(lines.len());
                text = lines[start_idx..end_idx].join("\n");
            }

            let lang = code_fence_language(&path);
            let tokens = estimate_tokens_rough(&text);
            (
                None,
                Some(format!(
                    "file: {} ({} tokens)\n```{}\n{}\n```",
                    reference.raw, tokens, lang, text
                )),
            )
        }
        Err(e) => (Some(format!("{}: {}", reference.raw, e)), None),
    }
}

fn expand_folder_reference(
    reference: &ContextReference,
    cwd: &Path,
) -> (Option<String>, Option<String>) {
    let path = resolve_path(cwd, &reference.target);

    if let Err(e) = ensure_path_allowed(&path) {
        return (Some(format!("{}: {}", reference.raw, e)), None);
    }

    if !path.exists() {
        return (Some(format!("{}: folder not found", reference.raw)), None);
    }
    if !path.is_dir() {
        return (
            Some(format!("{}: path is not a folder", reference.raw)),
            None,
        );
    }

    let listing = build_folder_listing(&path, 200);
    let tokens = estimate_tokens_rough(&listing);
    (
        None,
        Some(format!(
            "folder: {} ({} tokens)\n{}",
            reference.raw, tokens, listing
        )),
    )
}

fn expand_git_reference(
    cwd: &Path,
    args: &[&str],
    label: &str,
) -> (Option<String>, Option<String>) {
    match Command::new("git").args(args).current_dir(cwd).output() {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return (
                    Some(format!("{}: {}", label, stderr.trim())),
                    None,
                );
            }
            let content = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let content = if content.is_empty() {
                "(no output)".to_string()
            } else {
                content
            };
            let tokens = estimate_tokens_rough(&content);
            (
                None,
                Some(format!(
                    "{} ({} tokens)\n```diff\n{}\n```",
                    label, tokens, content
                )),
            )
        }
        Err(e) => (Some(format!("{}: {}", label, e)), None),
    }
}

// ── Security ────────────────────────────────────────────────────────────

fn ensure_path_allowed(path: &Path) -> Result<(), String> {
    let home = dirs::home_dir().unwrap_or_default();

    // Check sensitive files
    for sensitive in SENSITIVE_HOME_FILES {
        let blocked = home.join(sensitive);
        if path == blocked {
            return Err("path is a sensitive credential file and cannot be attached".to_string());
        }
    }

    // Check sensitive directories
    for sensitive_dir in SENSITIVE_HOME_DIRS {
        let blocked = home.join(sensitive_dir);
        if path.starts_with(&blocked) {
            return Err(
                "path is a sensitive credential or internal path and cannot be attached".to_string(),
            );
        }
    }

    // Check .env files in hermes home
    let hermes_home = openhermes_constants::get_hermes_home();
    if path == hermes_home.join(".env") {
        return Err("path is a sensitive credential file and cannot be attached".to_string());
    }

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn resolve_path(cwd: &Path, target: &str) -> PathBuf {
    let expanded = if target.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            home.join(target.trim_start_matches("~/"))
        } else {
            PathBuf::from(target)
        }
    } else {
        PathBuf::from(target)
    };

    if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(expanded)
    }
}

fn strip_trailing_punctuation(value: &str) -> String {
    let mut s = value.to_string();
    while s.ends_with(|c: char| TRAILING_PUNCTUATION.contains(c)) {
        s.pop();
    }
    s
}

fn strip_reference_wrappers(value: &str) -> String {
    if value.len() >= 2 {
        let first = value.chars().next().unwrap();
        let last = value.chars().last().unwrap();
        if first == last && matches!(first, '`' | '"' | '\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn parse_file_reference_value(value: &str) -> (String, Option<usize>, Option<usize>) {
    // Handle quoted values: `path`:start-end, "path":start-end, 'path':start-end
    // Avoids backreferences (unsupported by regex crate) by detecting quotes manually.
    let first_char = value.chars().next().unwrap_or('\0');
    if matches!(first_char, '`' | '"' | '\'') {
        let quote = &value[..1];
        if let Some(end_pos) = value[1..].find(quote) {
            let path = &value[1..1 + end_pos];
            let rest = &value[1 + end_pos + 1..]; // after closing quote
            if rest.starts_with(':') {
                let range_part = &rest[1..];
                if let Some(re) = Regex::new(r"^(?P<start>\d+)(?:-(?P<end>\d+))?$").ok() {
                    if let Some(cap) = re.captures(range_part) {
                        let start = cap.name("start").and_then(|m| m.as_str().parse().ok());
                        let end = cap.name("end").and_then(|m| m.as_str().parse().ok());
                        return (path.to_string(), start, end.or(start));
                    }
                }
            }
            return (path.to_string(), None, None);
        }
    }

    // Try unquoted: path:start-end
    if let Some(re) = Regex::new(r"^(?P<path>.+?):(?P<start>\d+)(?:-(?P<end>\d+))?$").ok() {
        if let Some(cap) = re.captures(value) {
            let path = cap.name("path").map(|m| m.as_str()).unwrap_or("");
            let start: Option<usize> = cap.name("start").and_then(|m| m.as_str().parse().ok());
            let end: Option<usize> = cap.name("end").and_then(|m| m.as_str().parse().ok());
            return (path.to_string(), start, end.or(start));
        }
    }

    (strip_reference_wrappers(value), None, None)
}

fn remove_reference_tokens(message: &str, refs: &[ContextReference]) -> String {
    let mut pieces = Vec::new();
    let mut cursor = 0;
    for r in refs {
        pieces.push(&message[cursor..r.start]);
        cursor = r.end;
    }
    pieces.push(&message[cursor..]);
    let text: String = pieces.join("");
    // Collapse multiple spaces
    let re = Regex::new(r"\s{2,}").unwrap();
    re.replace_all(&text, " ").trim().to_string()
}

fn code_fence_language(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("py") => "python",
        Some("js") => "javascript",
        Some("ts") => "typescript",
        Some("tsx") => "tsx",
        Some("jsx") => "jsx",
        Some("json") => "json",
        Some("md") => "markdown",
        Some("sh") | Some("bash") => "bash",
        Some("yml") | Some("yaml") => "yaml",
        Some("toml") => "toml",
        Some("rs") => "rust",
        Some("go") => "go",
        Some("java") => "java",
        Some("c") | Some("h") => "c",
        Some("cpp") | Some("hpp") => "cpp",
        Some("html") => "html",
        Some("css") => "css",
        Some("sql") => "sql",
        _ => "",
    }
}

fn build_folder_listing(path: &Path, limit: usize) -> String {
    let mut lines = vec![format!("{}/", path.file_name().unwrap_or_default().to_string_lossy())];
    let mut count = 0;

    if let Ok(entries) = std::fs::read_dir(path) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            if count >= limit {
                lines.push("  ...".to_string());
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                lines.push(format!("  {}/", name));
            } else {
                lines.push(format!("  {}", name));
            }
            count += 1;
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_reference() {
        let refs = parse_context_references("Check @file:src/main.rs for issues");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, "file");
        assert_eq!(refs[0].target, "src/main.rs");
    }

    #[test]
    fn test_parse_diff_reference() {
        let refs = parse_context_references("Look at @diff");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, "diff");
    }

    #[test]
    fn test_parse_staged_reference() {
        let refs = parse_context_references("Check @staged changes");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, "staged");
    }

    #[test]
    fn test_parse_folder_reference() {
        let refs = parse_context_references("Look at @folder:src/");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, "folder");
    }

    #[test]
    fn test_parse_git_reference() {
        let refs = parse_context_references("Show @git:3 commits");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, "git");
        assert_eq!(refs[0].target, "3");
    }

    #[test]
    fn test_parse_url_reference() {
        let refs = parse_context_references("Read @url:https://example.com");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, "url");
        assert_eq!(refs[0].target, "https://example.com");
    }

    #[test]
    fn test_parse_no_references() {
        let refs = parse_context_references("No references here");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_parse_file_with_line_range() {
        let (path, start, end) = parse_file_reference_value("`main.rs`:10-20");
        assert_eq!(path, "main.rs");
        assert_eq!(start, Some(10));
        assert_eq!(end, Some(20));
    }

    #[test]
    fn test_sensitive_path_blocked() {
        let home = dirs::home_dir().unwrap_or_default();
        let ssh_key = home.join(".ssh/id_rsa");
        assert!(ensure_path_allowed(&ssh_key).is_err());
    }

    #[test]
    fn test_safe_path_allowed() {
        let path = PathBuf::from("/tmp/test.txt");
        assert!(ensure_path_allowed(&path).is_ok());
    }

    #[test]
    fn test_strip_trailing_punctuation() {
        assert_eq!(strip_trailing_punctuation("file.rs,"), "file.rs");
        assert_eq!(strip_trailing_punctuation("file.rs."), "file.rs");
        assert_eq!(strip_trailing_punctuation("file.rs"), "file.rs");
    }

    #[test]
    fn test_remove_reference_tokens() {
        let refs = parse_context_references("Check @file:main.rs for bugs");
        let result = remove_reference_tokens("Check @file:main.rs for bugs", &refs);
        assert_eq!(result, "Check for bugs");
    }
}
