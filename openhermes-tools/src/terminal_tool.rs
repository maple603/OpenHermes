//! Terminal execution tool.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::timeout;

use crate::registry::{Tool, REGISTRY};

/// Terminal execution tool
pub struct TerminalTool {
    timeout_secs: u64,
}

impl TerminalTool {
    pub fn new(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }
}

impl Default for TerminalTool {
    fn default() -> Self {
        Self {
            timeout_secs: openhermes_constants::DEFAULT_TOOL_TIMEOUT_SECS,
        }
    }
}

#[async_trait]
impl Tool for TerminalTool {
    fn name(&self) -> &str {
        "execute_code"
    }

    fn toolset(&self) -> &str {
        "terminal"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "execute_code",
            "description": "Execute a shell command and return its output",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Working directory for the command (optional)"
                    }
                },
                "required": ["command"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: command"))?;

        let working_dir = args["working_dir"].as_str();

        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(command);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Execute with timeout
        let output = timeout(
            Duration::from_secs(self.timeout_secs),
            cmd.output(),
        )
        .await
        .with_context(|| format!("Command timed out after {} seconds", self.timeout_secs))??;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(serde_json::json!({
            "success": exit_code == 0,
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": stderr
        })
        .to_string())
    }
}

/// Register terminal tool
pub fn register_tools() {
    REGISTRY.register(Arc::new(TerminalTool::default()));
}
