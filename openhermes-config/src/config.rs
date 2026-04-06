//! Configuration loading and saving.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use tracing::{info, warn};

use super::types::{default_config, HermesConfig};

/// Default configuration (empty)
pub static DEFAULT_CONFIG: LazyLock<HermesConfig> = LazyLock::new(|| HermesConfig {
    agent: super::types::AgentConfig {
        model: String::new(),
        max_iterations: 0,
        enabled_toolsets: Vec::new(),
        disabled_toolsets: Vec::new(),
        quiet_mode: false,
        save_trajectories: false,
        reasoning_effort: None,
    },
    terminal: super::types::TerminalConfig {
        backend: String::new(),
        cwd: None,
        timeout: 0,
        lifetime_seconds: None,
        docker_image: None,
        ssh_host: None,
        ssh_user: None,
        ssh_port: None,
        ssh_key: None,
        container_cpu: None,
        container_memory: None,
        container_disk: None,
        container_persistent: None,
    },
    display: super::types::DisplayConfig {
        skin: None,
        tool_preview: false,
        tool_prefix: String::new(),
        background_process_notifications: String::new(),
    },
    memory: super::types::MemoryConfig {
        enabled: false,
        provider: None,
        memory_file: None,
        user_file: None,
    },
    gateway: super::types::GatewayConfig {
        timeout: 0,
        platforms: HashMap::new(),
        allowed_users: Vec::new(),
    },
    delegation: super::types::DelegationConfig {
        max_iterations: 0,
        parallel_enabled: false,
    },
    auxiliary: super::types::AuxiliaryConfig {
        vision: super::types::AuxiliaryTaskConfig {
            provider: None,
            model: None,
            base_url: None,
            api_key: None,
        },
        web_extract: super::types::AuxiliaryTaskConfig {
            provider: None,
            model: None,
            base_url: None,
            api_key: None,
        },
        approval: super::types::AuxiliaryTaskConfig {
            provider: None,
            model: None,
            base_url: None,
            api_key: None,
        },
    },
    security: super::types::SecurityConfig {
        redact_secrets: false,
        approval_patterns: Vec::new(),
        dangerous_patterns: Vec::new(),
    },
    timezone: None,
    extra: HashMap::new(),
});

/// Load configuration from ~/.hermes/config.yaml
pub fn load_config() -> Result<HermesConfig> {
    let config_path = openhermes_constants::get_hermes_home().join("config.yaml");

    if !config_path.exists() {
        info!(
            "Config file not found at {}, using defaults",
            openhermes_constants::display_hermes_home()
        );
        return Ok(default_config());
    }

    info!(
        "Loading configuration from {}",
        config_path.display()
    );

    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

    let config: HermesConfig = serde_yaml::from_str(&content)
        .with_context(|| "Failed to parse config.yaml")?;

    info!("Configuration loaded successfully");
    Ok(config)
}

/// Save configuration to ~/.hermes/config.yaml
pub fn save_config(config: &HermesConfig) -> Result<()> {
    let config_path = openhermes_constants::get_hermes_home().join("config.yaml");

    // Ensure directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let content = serde_yaml::to_string(config)
        .with_context(|| "Failed to serialize config")?;

    std::fs::write(&config_path, content)
        .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

    info!(
        "Configuration saved to {}",
        openhermes_constants::display_hermes_home()
    );

    Ok(())
}

/// Load configuration with custom path
pub fn load_config_from_path(path: &PathBuf) -> Result<HermesConfig> {
    if !path.exists() {
        warn!("Config file not found at {}", path.display());
        return Ok(default_config());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let config: HermesConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

    Ok(config)
}
