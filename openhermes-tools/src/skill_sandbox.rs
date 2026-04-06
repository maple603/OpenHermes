//! Skill sandbox execution environment for secure skill code execution.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{info, warn};

/// Sandbox execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Maximum execution time in seconds
    pub timeout_secs: u64,
    /// Maximum memory usage in MB
    pub max_memory_mb: u64,
    /// Allow network access
    pub allow_network: bool,
    /// Allow file system access
    pub allow_filesystem: bool,
    /// Allowed directories for file access
    pub allowed_dirs: Vec<String>,
    /// Environment variables
    pub env_vars: HashMap<String, String>,
    /// Working directory
    pub work_dir: Option<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_memory_mb: 512,
            allow_network: false,
            allow_filesystem: true,
            allowed_dirs: vec![],
            env_vars: HashMap::new(),
            work_dir: None,
        }
    }
}

/// Execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub error: Option<String>,
}

/// Skill sandbox executor
pub struct SkillSandbox {
    config: SandboxConfig,
    skill_dir: PathBuf,
}

impl SkillSandbox {
    /// Create a new sandbox for a skill
    pub fn new(skill_dir: PathBuf, config: Option<SandboxConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
            skill_dir,
        }
    }

    /// Execute a Python script in sandbox
    pub async fn execute_python(
        &self,
        script_path: &str,
        args: &[String],
    ) -> Result<ExecutionResult> {
        let script_full_path = self.skill_dir.join(script_path);
        
        if !script_full_path.exists() {
            return Err(anyhow::anyhow!("Script not found: {:?}", script_full_path));
        }

        info!(
            script = script_path,
            skill_dir = ?self.skill_dir,
            "Executing Python script in sandbox"
        );

        let mut cmd = Command::new("python3");
        cmd.arg(&script_full_path);
        cmd.args(args);
        
        // Configure sandbox
        self.configure_command(&mut cmd);

        // Execute with timeout
        let start = std::time::Instant::now();
        let result = timeout(
            Duration::from_secs(self.config.timeout_secs),
            self.execute_command(cmd)
        ).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(exec_result) => {
                let mut exec_result = exec_result?;
                exec_result.duration_ms = duration_ms;
                Ok(exec_result)
            }
            Err(_) => {
                warn!(
                    script = script_path,
                    timeout = self.config.timeout_secs,
                    "Script execution timed out"
                );
                Ok(ExecutionResult {
                    success: false,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("Execution timed out after {} seconds", self.config.timeout_secs),
                    duration_ms,
                    error: Some("timeout".to_string()),
                })
            }
        }
    }

    /// Execute a shell command in sandbox
    pub async fn execute_shell(
        &self,
        command: &str,
        args: &[String],
    ) -> Result<ExecutionResult> {
        info!(command = command, "Executing shell command in sandbox");

        let mut cmd = Command::new("bash");
        cmd.arg("-c");
        cmd.arg(command);
        cmd.args(args);
        
        self.configure_command(&mut cmd);

        let start = std::time::Instant::now();
        let result = timeout(
            Duration::from_secs(self.config.timeout_secs),
            self.execute_command(cmd)
        ).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(exec_result) => {
                let mut exec_result = exec_result?;
                exec_result.duration_ms = duration_ms;
                Ok(exec_result)
            }
            Err(_) => {
                Ok(ExecutionResult {
                    success: false,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("Execution timed out after {} seconds", self.config.timeout_secs),
                    duration_ms,
                    error: Some("timeout".to_string()),
                })
            }
        }
    }

    /// Configure command with sandbox restrictions
    fn configure_command(&self, cmd: &mut Command) {
        // Set environment variables
        for (key, value) in &self.config.env_vars {
            cmd.env(key, value);
        }

        // Set working directory
        if let Some(ref work_dir) = self.config.work_dir {
            cmd.current_dir(work_dir);
        } else {
            cmd.current_dir(&self.skill_dir);
        }

        // Restrict network access (using namespace isolation if available)
        if !self.config.allow_network {
            // On Linux, could use `unshare --net`
            // For now, we rely on script-level restrictions
            cmd.env("http_proxy", "");
            cmd.env("https_proxy", "");
            cmd.env("no_proxy", "*");
        }

        // Configure stdio
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Note: Memory limits would require cgroups on Linux or ulimit
        // This is a basic implementation
    }

    /// Execute the command and collect output
    async fn execute_command(&self, mut cmd: Command) -> Result<ExecutionResult> {
        let output = cmd.output().await
            .with_context(|| "Failed to execute command")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code();
        let success = output.status.success();

        if !success {
            warn!(
                exit_code = exit_code,
                stderr = %stderr,
                "Command execution failed"
            );
        }

        let error_msg = if success { None } else { Some(stderr.clone()) };

        Ok(ExecutionResult {
            success,
            exit_code,
            stdout,
            stderr,
            duration_ms: 0, // Will be set by caller
            error: error_msg,
        })
    }

    /// Validate skill sandbox requirements
    pub fn validate(&self) -> Result<()> {
        // Check if skill directory exists
        if !self.skill_dir.exists() {
            return Err(anyhow::anyhow!(
                "Skill directory does not exist: {:?}",
                self.skill_dir
            ));
        }

        // Check for required files
        let manifest_path = self.skill_dir.join("skill.json");
        if !manifest_path.exists() {
            return Err(anyhow::anyhow!(
                "Skill manifest not found: {:?}",
                manifest_path
            ));
        }

        // Check if Python is available
        let python_available = self.check_python_available();
        if !python_available {
            warn!("Python3 not found in PATH");
        }

        info!("Skill sandbox validation passed");
        Ok(())
    }

    /// Check if Python is available
    fn check_python_available(&self) -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Get sandbox configuration
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// Get skill directory
    pub fn skill_dir(&self) -> &Path {
        &self.skill_dir
    }
}

/// Sandbox manager for multiple skills
pub struct SandboxManager {
    sandboxes: HashMap<String, SkillSandbox>,
    default_config: SandboxConfig,
}

impl SandboxManager {
    /// Create a new sandbox manager
    pub fn new(default_config: Option<SandboxConfig>) -> Self {
        Self {
            sandboxes: HashMap::new(),
            default_config: default_config.unwrap_or_default(),
        }
    }

    /// Register a skill sandbox
    pub fn register_skill(&mut self, skill_name: String, skill_dir: PathBuf) {
        let sandbox = SkillSandbox::new(skill_dir, Some(self.default_config.clone()));
        info!(skill = %skill_name, "Skill sandbox registered");
        self.sandboxes.insert(skill_name, sandbox);
    }

    /// Get a skill sandbox
    pub fn get_sandbox(&self, skill_name: &str) -> Option<&SkillSandbox> {
        self.sandboxes.get(skill_name)
    }

    /// Execute a skill tool
    pub async fn execute_skill_tool(
        &self,
        skill_name: &str,
        tool_name: &str,
        args: &[String],
    ) -> Result<ExecutionResult> {
        let sandbox = self.sandboxes.get(skill_name)
            .ok_or_else(|| anyhow::anyhow!("Skill not registered: {}", skill_name))?;

        // Tool script path convention: tools/{tool_name}.py
        let script_path = format!("tools/{}.py", tool_name);
        
        sandbox.execute_python(&script_path, args).await
    }

    /// Remove a skill sandbox
    pub fn remove_skill(&mut self, skill_name: &str) {
        if self.sandboxes.remove(skill_name).is_some() {
            info!(skill = skill_name, "Skill sandbox removed");
        }
    }

    /// List registered skills
    pub fn list_skills(&self) -> Vec<&String> {
        self.sandboxes.keys().collect()
    }
}
