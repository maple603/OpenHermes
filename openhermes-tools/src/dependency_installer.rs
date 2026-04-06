//! Skill dependency installer for automatic dependency management.

use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::{info, warn, error};

/// Dependency installer for skills
pub struct DependencyInstaller;

impl DependencyInstaller {
    /// Create a new dependency installer
    pub fn new() -> Self {
        Self
    }

    /// Install dependencies for a skill
    pub async fn install_dependencies(&self, skill_dir: &Path) -> Result<()> {
        info!(skill_dir = ?skill_dir, "Installing skill dependencies");

        // Check for requirements.txt (Python)
        let requirements_path = skill_dir.join("requirements.txt");
        if requirements_path.exists() {
            self.install_python_requirements(&requirements_path).await?;
        }

        // Check for package.json (Node.js)
        let package_json_path = skill_dir.join("package.json");
        if package_json_path.exists() {
            self.install_node_packages(&package_json_path).await?;
        }

        // Check for Cargo.toml (Rust)
        let cargo_toml_path = skill_dir.join("Cargo.toml");
        if cargo_toml_path.exists() {
            self.build_rust_package(&cargo_toml_path).await?;
        }

        info!("Skill dependencies installed successfully");
        Ok(())
    }

    /// Install Python requirements
    async fn install_python_requirements(&self, requirements_path: &Path) -> Result<()> {
        info!(path = ?requirements_path, "Installing Python requirements");

        // Check if pip is available
        if !self.check_command_exists("pip3").await && !self.check_command_exists("pip").await {
            warn!("pip/pip3 not found, skipping Python requirements installation");
            return Ok(());
        }

        let pip_cmd = if self.check_command_exists("pip3").await {
            "pip3"
        } else {
            "pip"
        };

        // Install with pip
        let mut cmd = Command::new(pip_cmd);
        cmd.arg("install");
        cmd.arg("-r");
        cmd.arg(requirements_path);
        cmd.arg("--quiet");
        cmd.arg("--no-warn-script-location");
        
        // Use user install to avoid permission issues
        cmd.arg("--user");

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await
            .with_context(|| format!("Failed to run {}", pip_cmd))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(error = %stderr, "Failed to install Python requirements");
            return Err(anyhow::anyhow!(
                "Failed to install Python requirements: {}",
                stderr
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        info!(output = %stdout, "Python requirements installed");
        Ok(())
    }

    /// Install Node.js packages
    async fn install_node_packages(&self, package_json_path: &Path) -> Result<()> {
        info!(path = ?package_json_path, "Installing Node.js packages");

        // Check if npm is available
        if !self.check_command_exists("npm").await {
            warn!("npm not found, skipping Node.js packages installation");
            return Ok(());
        }

        let package_dir = package_json_path.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid package.json path"))?;

        // Run npm install
        let mut cmd = Command::new("npm");
        cmd.arg("install");
        cmd.arg("--production");
        cmd.arg("--silent");
        cmd.current_dir(package_dir);

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await
            .with_context(|| "Failed to run npm install")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(error = %stderr, "Failed to install Node.js packages");
            return Err(anyhow::anyhow!(
                "Failed to install Node.js packages: {}",
                stderr
            ));
        }

        info!("Node.js packages installed");
        Ok(())
    }

    /// Build Rust package
    async fn build_rust_package(&self, cargo_toml_path: &Path) -> Result<()> {
        info!(path = ?cargo_toml_path, "Building Rust package");

        // Check if cargo is available
        if !self.check_command_exists("cargo").await {
            warn!("cargo not found, skipping Rust package build");
            return Ok(());
        }

        let package_dir = cargo_toml_path.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid Cargo.toml path"))?;

        // Run cargo build
        let mut cmd = Command::new("cargo");
        cmd.arg("build");
        cmd.arg("--release");
        cmd.arg("--quiet");
        cmd.current_dir(package_dir);

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await
            .with_context(|| "Failed to run cargo build")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(error = %stderr, "Failed to build Rust package");
            return Err(anyhow::anyhow!(
                "Failed to build Rust package: {}",
                stderr
            ));
        }

        info!("Rust package built successfully");
        Ok(())
    }

    /// Check if a command exists
    async fn check_command_exists(&self, command: &str) -> bool {
        Command::new(command)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check dependencies without installing
    pub async fn check_dependencies(&self, skill_dir: &Path) -> Vec<String> {
        let mut missing = Vec::new();

        // Check Python requirements
        let requirements_path = skill_dir.join("requirements.txt");
        if requirements_path.exists() {
            if !self.check_command_exists("pip3").await && !self.check_command_exists("pip").await {
                missing.push("pip/pip3 (for Python requirements)".to_string());
            }
        }

        // Check Node.js packages
        let package_json_path = skill_dir.join("package.json");
        if package_json_path.exists() {
            if !self.check_command_exists("npm").await {
                missing.push("npm (for Node.js packages)".to_string());
            }
        }

        // Check Rust package
        let cargo_toml_path = skill_dir.join("Cargo.toml");
        if cargo_toml_path.exists() {
            if !self.check_command_exists("cargo").await {
                missing.push("cargo (for Rust packages)".to_string());
            }
        }

        missing
    }

    /// Uninstall dependencies (cleanup)
    pub async fn uninstall_dependencies(&self, skill_dir: &Path) -> Result<()> {
        info!(skill_dir = ?skill_dir, "Uninstalling skill dependencies");

        // Remove Python packages (note: this is tricky, better to use virtualenv)
        let requirements_path = skill_dir.join("requirements.txt");
        if requirements_path.exists() {
            if self.check_command_exists("pip3").await {
                let mut cmd = Command::new("pip3");
                cmd.arg("uninstall");
                cmd.arg("-y");
                cmd.arg("-r");
                cmd.arg(&requirements_path);
                cmd.arg("--quiet");
                
                // This might fail if packages are used by other projects
                let _ = cmd.output().await;
            }
        }

        // Remove node_modules
        let node_modules = skill_dir.join("node_modules");
        if node_modules.exists() {
            tokio::fs::remove_dir_all(&node_modules).await.ok();
        }

        // Remove Rust target
        let target_dir = skill_dir.join("target");
        if target_dir.exists() {
            tokio::fs::remove_dir_all(&target_dir).await.ok();
        }

        info!("Skill dependencies uninstalled");
        Ok(())
    }
}
