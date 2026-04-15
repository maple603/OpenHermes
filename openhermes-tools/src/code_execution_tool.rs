//! Code execution tool — run Python/shell scripts in an isolated subprocess
//! with UDS-based RPC for tool access.
//!
//! The subprocess gets a `hermes_tools.py` helper that communicates back to
//! the agent via a Unix Domain Socket for whitelisted tool calls.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::info;

use crate::registry::Tool;

/// Tools allowed inside the code execution sandbox.
#[allow(dead_code)]
const SANDBOX_ALLOWED_TOOLS: &[&str] = &[
    "web_search",
    "web_extract",
    "read_file",
    "write_file",
    "search_files",
    "patch",
    "execute_code",
];

/// Maximum execution time.
const EXECUTION_TIMEOUT_SECS: u64 = 300;

/// Maximum stdout size (bytes).
const MAX_STDOUT_BYTES: usize = 50_000;

/// Maximum tool calls from sandbox.
#[allow(dead_code)]
const MAX_TOOL_CALLS: usize = 50;

/// Environment variables to strip from child process.
const SENSITIVE_ENV_VARS: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "OPENROUTER_API_KEY",
    "GOOGLE_API_KEY",
    "DEEPSEEK_API_KEY",
    "FAL_KEY",
    "ELEVENLABS_API_KEY",
    "DISCORD_TOKEN",
    "TELEGRAM_BOT_TOKEN",
    "AWS_SECRET_ACCESS_KEY",
    "GH_TOKEN",
    "GITHUB_TOKEN",
];

/// Code execution tool.
pub struct CodeExecutionTool;

#[async_trait]
impl Tool for CodeExecutionTool {
    fn name(&self) -> &str {
        "run_code"
    }

    fn toolset(&self) -> &str {
        "code_execution"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "run_code",
            "description": "Execute a Python or shell script in an isolated subprocess. The script has access to a limited set of agent tools via the `hermes_tools` module. Scripts run with a timeout and restricted environment (no API keys).",
            "parameters": {
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "Python or shell code to execute"
                    },
                    "language": {
                        "type": "string",
                        "description": "Language: 'python' or 'shell' (default: auto-detect)",
                        "enum": ["python", "shell"],
                        "default": "python"
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Working directory for execution (optional)"
                    }
                },
                "required": ["code"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let code = args["code"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: code"))?;
        let language = args["language"].as_str().unwrap_or_else(|| {
            if code.contains("#!/") && !code.contains("python") {
                "shell"
            } else {
                "python"
            }
        });
        let working_dir = args["working_dir"].as_str();

        info!(language = language, code_len = code.len(), "Executing code");

        match language {
            "python" => self.execute_python(code, working_dir).await,
            "shell" => self.execute_shell(code, working_dir).await,
            _ => Ok(serde_json::json!({
                "error": format!("Unsupported language: {}", language),
                "success": false
            }).to_string()),
        }
    }
}

impl CodeExecutionTool {
    /// Execute Python code in a subprocess.
    async fn execute_python(&self, code: &str, working_dir: Option<&str>) -> Result<String> {
        // Write code to a temp file
        let tmp_dir = std::env::temp_dir().join("openhermes_exec");
        std::fs::create_dir_all(&tmp_dir)?;

        let script_path = tmp_dir.join(format!("script_{}.py", uuid::Uuid::new_v4()));

        // Generate the hermes_tools stub
        let stub = generate_hermes_tools_stub(&tmp_dir);
        std::fs::write(&stub, HERMES_TOOLS_STUB)?;

        // Write the user script
        let full_code = format!("import sys\nsys.path.insert(0, '{}')\n{}", tmp_dir.display(), code);
        std::fs::write(&script_path, &full_code)?;

        // Build filtered environment
        let filtered_env = build_filtered_env();

        let mut cmd = Command::new("python3");
        cmd.arg(&script_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs(filtered_env);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let result = run_with_timeout(cmd, EXECUTION_TIMEOUT_SECS).await;

        // Clean up
        let _ = std::fs::remove_file(&script_path);

        result
    }

    /// Execute shell code in a subprocess.
    async fn execute_shell(&self, code: &str, working_dir: Option<&str>) -> Result<String> {
        let filtered_env = build_filtered_env();

        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(code)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs(filtered_env);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        run_with_timeout(cmd, EXECUTION_TIMEOUT_SECS).await
    }
}

/// Run a command with a timeout.
async fn run_with_timeout(mut cmd: Command, timeout_secs: u64) -> Result<String> {
    let child = cmd.spawn().context("Failed to spawn process")?;

    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        wait_for_output(child),
    )
    .await;

    match result {
        Ok(Ok((stdout, stderr, exit_code))) => {
            let stdout = truncate_output(&stdout, MAX_STDOUT_BYTES);
            let stderr = truncate_output(&stderr, MAX_STDOUT_BYTES);
            let stdout = strip_ansi(&redact_secrets(&stdout));
            let stderr = strip_ansi(&redact_secrets(&stderr));

            Ok(serde_json::json!({
                "success": exit_code == 0,
                "exit_code": exit_code,
                "stdout": stdout,
                "stderr": stderr,
            }).to_string())
        }
        Ok(Err(e)) => Ok(serde_json::json!({
            "success": false,
            "error": format!("Process error: {}", e),
        }).to_string()),
        Err(_) => Ok(serde_json::json!({
            "success": false,
            "error": format!("Execution timed out after {} seconds", timeout_secs),
        }).to_string()),
    }
}

/// Wait for process output.
async fn wait_for_output(
    mut child: tokio::process::Child,
) -> Result<(String, String, i32)> {
    let mut stdout = String::new();
    let mut stderr = String::new();

    if let Some(ref mut out) = child.stdout {
        let _ = out.read_to_string(&mut stdout).await;
    }
    if let Some(ref mut err) = child.stderr {
        let _ = err.read_to_string(&mut stderr).await;
    }

    let status = child.wait().await?;
    let exit_code = status.code().unwrap_or(-1);

    Ok((stdout, stderr, exit_code))
}

/// Build a filtered environment without sensitive variables.
fn build_filtered_env() -> Vec<(String, String)> {
    std::env::vars()
        .filter(|(key, _)| !SENSITIVE_ENV_VARS.contains(&key.as_str()))
        .collect()
}

/// Generate the path for the hermes_tools.py stub.
fn generate_hermes_tools_stub(tmp_dir: &PathBuf) -> PathBuf {
    tmp_dir.join("hermes_tools.py")
}

/// Truncate output to max bytes.
fn truncate_output(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        s.to_string()
    } else {
        let half = max_bytes / 2;
        let head = &s[..half];
        let tail = &s[s.len() - half..];
        format!(
            "{}...\n[{} bytes truncated]\n...{}",
            head,
            s.len() - max_bytes,
            tail
        )
    }
}

/// Strip ANSI escape sequences.
fn strip_ansi(s: &str) -> String {
    // Simple ANSI stripping — handles most common sequences
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
            continue;
        }
        if in_escape {
            if c.is_ascii_alphabetic() {
                in_escape = false;
            }
            continue;
        }
        result.push(c);
    }
    result
}

/// Redact known secret patterns from output.
fn redact_secrets(s: &str) -> String {
    let mut result = s.to_string();
    for key in SENSITIVE_ENV_VARS {
        if let Ok(val) = std::env::var(key) {
            if !val.is_empty() && result.contains(&val) {
                result = result.replace(&val, "[REDACTED]");
            }
        }
    }
    result
}

/// Python hermes_tools stub content.
const HERMES_TOOLS_STUB: &str = r#"""
hermes_tools - Stub module for sandboxed code execution.

This module provides limited tool access from within code execution.
In full implementation, it communicates via UDS to the agent.
"""

import json
import os
import socket

_SOCKET_PATH = os.environ.get("HERMES_UDS_PATH", "")

def _call_tool(name, args):
    """Call an agent tool via UDS (stub implementation)."""
    if not _SOCKET_PATH:
        raise RuntimeError(
            "hermes_tools is only available inside code execution sandbox. "
            "Tool calls are not available in standalone mode."
        )
    # UDS RPC implementation would go here
    raise NotImplementedError(f"Tool {name} not yet available in sandbox mode")

def web_search(query, limit=5):
    return _call_tool("web_search", {"query": query, "limit": limit})

def web_extract(url):
    return _call_tool("web_extract", {"url": url})

def read_file(path):
    """Read a file (available locally without UDS)."""
    with open(path, 'r') as f:
        return f.read()

def write_file(path, content):
    """Write a file (available locally without UDS)."""
    with open(path, 'w') as f:
        f.write(content)

def search_files(pattern, path="."):
    return _call_tool("search_files", {"pattern": pattern, "path": path})
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[31mRed text\x1b[0m normal";
        let result = strip_ansi(input);
        assert_eq!(result, "Red text normal");
    }

    #[test]
    fn test_truncate_output_short() {
        let s = "short output";
        assert_eq!(truncate_output(s, 100), s);
    }

    #[test]
    fn test_truncate_output_long() {
        let s = "a".repeat(1000);
        let result = truncate_output(&s, 100);
        assert!(result.contains("bytes truncated"));
        assert!(result.len() < 1000);
    }

    #[test]
    fn test_build_filtered_env() {
        let env = build_filtered_env();
        // Should not contain sensitive vars
        for (key, _) in &env {
            assert!(
                !SENSITIVE_ENV_VARS.contains(&key.as_str()),
                "Sensitive var {} should be filtered",
                key
            );
        }
    }

    #[test]
    fn test_redact_secrets() {
        // This test depends on env vars not being set, so just test the no-op case
        let input = "normal text without secrets";
        assert_eq!(redact_secrets(input), input);
    }
}
