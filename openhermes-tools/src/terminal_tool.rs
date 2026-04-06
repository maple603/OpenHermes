//! Terminal execution tool with background process support.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::registry::{Tool, REGISTRY};

/// Background process info
struct BackgroundProcess {
    child: Arc<Mutex<tokio::process::Child>>,
    command: String,
    start_time: std::time::Instant,
}

/// Global background process registry
static BACKGROUND_PROCESSES: Lazy<DashMap<String, BackgroundProcess>> =
    Lazy::new(DashMap::new);

/// Terminal execution tool
pub struct TerminalTool {
    timeout_secs: u64,
}

impl TerminalTool {
    pub fn new(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }

    async fn execute_command(&self, command: &str, working_dir: Option<&str>) -> Result<String> {
        debug!(command = command, "Executing command");

        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(command);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

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

    async fn execute_background(&self, command: &str, working_dir: Option<&str>) -> Result<String> {
        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);
        let process_id = format!("bg_{}", pid);

        BACKGROUND_PROCESSES.insert(process_id.clone(), BackgroundProcess {
            child: Arc::new(Mutex::new(child)),
            command: command.to_string(),
            start_time: std::time::Instant::now(),
        });

        Ok(serde_json::json!({
            "success": true,
            "process_id": process_id,
            "pid": pid,
            "command": command,
            "message": "Process started in background"
        }).to_string())
    }

    async fn check_background_process(&self, process_id: &str) -> Result<String> {
        // First, check if process exists and get info
        let process_info = {
            let entry = BACKGROUND_PROCESSES.get(process_id)
                .ok_or_else(|| anyhow::anyhow!("Process not found: {}", process_id))?;

            let mut child = entry.child.lock().await;
            
            // Try to get exit status without blocking
            match child.try_wait()? {
                Some(status) => {
                    let exit_code = status.code().unwrap_or(-1);
                    let duration = entry.start_time.elapsed();
                    
                    // Read output if available
                    let mut stdout = String::new();
                    let mut stderr = String::new();
                    
                    if let Some(ref mut out) = child.stdout {
                        let _ = out.read_to_string(&mut stdout).await;
                    }
                    if let Some(ref mut err) = child.stderr {
                        let _ = err.read_to_string(&mut stderr).await;
                    }

                    Some(serde_json::json!({
                        "process_id": process_id,
                        "status": "completed",
                        "exit_code": exit_code,
                        "duration_secs": duration.as_secs(),
                        "stdout": stdout,
                        "stderr": stderr
                    }))
                }
                None => {
                    let duration = entry.start_time.elapsed();
                    Some(serde_json::json!({
                        "process_id": process_id,
                        "status": "running",
                        "pid": child.id(),
                        "duration_secs": duration.as_secs(),
                        "command": entry.command
                    }))
                }
            }
        };

        // If completed, remove from registry
        if let Some(result) = process_info {
            if result["status"] == "completed" {
                BACKGROUND_PROCESSES.remove(process_id);
            }
            Ok(result.to_string())
        } else {
            Err(anyhow::anyhow!("Process not found"))
        }
    }

    async fn kill_background_process(&self, process_id: &str) -> Result<String> {
        // Kill the process
        {
            let entry = BACKGROUND_PROCESSES.get(process_id)
                .ok_or_else(|| anyhow::anyhow!("Process not found: {}", process_id))?;

            let mut child = entry.child.lock().await;
            
            if let Err(e) = child.kill().await {
                warn!("Failed to kill process {}: {}", process_id, e);
            }
        }

        // Remove from registry
        BACKGROUND_PROCESSES.remove(process_id);

        Ok(serde_json::json!({
            "success": true,
            "process_id": process_id,
            "message": "Process killed"
        }).to_string())
    }

    async fn list_background_processes(&self) -> Result<String> {
        let processes: Vec<Value> = BACKGROUND_PROCESSES.iter().map(|entry| {
            let process = entry.value();
            let duration = process.start_time.elapsed();
            serde_json::json!({
                "process_id": entry.key(),
                "command": process.command,
                "duration_secs": duration.as_secs(),
                "status": "running"
            })
        }).collect();

        Ok(serde_json::json!({
            "count": processes.len(),
            "processes": processes
        }).to_string())
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
            "description": "Execute shell commands. Supports foreground execution with timeout, background execution, and process management.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute"
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Working directory for the command (optional)"
                    },
                    "background": {
                        "type": "boolean",
                        "description": "Run in background (default: false)",
                        "default": false
                    },
                    "process_id": {
                        "type": "string",
                        "description": "Process ID for background operations (check/kill)"
                    },
                    "action": {
                        "type": "string",
                        "description": "Action: execute, check, kill, or list",
                        "enum": ["execute", "check", "kill", "list"],
                        "default": "execute"
                    }
                },
                "required": ["command"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let action = args["action"].as_str().unwrap_or("execute");
        let working_dir = args["working_dir"].as_str();
        
        match action {
            "execute" => {
                let command = args["command"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: command"))?;
                
                let background = args["background"].as_bool().unwrap_or(false);

                if background {
                    self.execute_background(command, working_dir).await
                } else {
                    self.execute_command(command, working_dir).await
                }
            }
            "check" => {
                let process_id = args["process_id"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: process_id"))?;
                self.check_background_process(process_id).await
            }
            "kill" => {
                let process_id = args["process_id"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: process_id"))?;
                self.kill_background_process(process_id).await
            }
            "list" => {
                self.list_background_processes().await
            }
            _ => {
                Err(anyhow::anyhow!("Unknown action: {}", action))
            }
        }
    }
}

/// Register terminal tool
pub fn register_tools() {
    REGISTRY.register(Arc::new(TerminalTool::default()));
}
