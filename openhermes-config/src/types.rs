//! Configuration types for OpenHermes Agent.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesConfig {
    /// Agent configuration
    #[serde(default)]
    pub agent: AgentConfig,

    /// Terminal backend configuration
    #[serde(default)]
    pub terminal: TerminalConfig,

    /// Display/UI configuration
    #[serde(default)]
    pub display: DisplayConfig,

    /// Memory system configuration
    #[serde(default)]
    pub memory: MemoryConfig,

    /// Gateway configuration
    #[serde(default)]
    pub gateway: GatewayConfig,

    /// Delegation configuration
    #[serde(default)]
    pub delegation: DelegationConfig,

    /// Auxiliary model configuration
    #[serde(default)]
    pub auxiliary: AuxiliaryConfig,

    /// Security configuration
    #[serde(default)]
    pub security: SecurityConfig,

    /// Timezone configuration
    #[serde(default)]
    pub timezone: Option<String>,

    /// Additional custom configuration
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Model identifier (e.g., "anthropic/claude-opus-4-20250514")
    #[serde(default = "default_model")]
    pub model: String,

    /// Maximum iterations per conversation turn
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,

    /// Enabled toolsets
    #[serde(default)]
    pub enabled_toolsets: Vec<String>,

    /// Disabled toolsets
    #[serde(default)]
    pub disabled_toolsets: Vec<String>,

    /// Quiet mode (reduced output)
    #[serde(default)]
    pub quiet_mode: bool,

    /// Save trajectories to disk
    #[serde(default)]
    pub save_trajectories: bool,

    /// Reasoning effort level
    pub reasoning_effort: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            max_iterations: default_max_iterations(),
            enabled_toolsets: Vec::new(),
            disabled_toolsets: Vec::new(),
            quiet_mode: false,
            save_trajectories: false,
            reasoning_effort: None,
        }
    }
}

/// Terminal backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Backend type: local, docker, ssh, daytona, modal, singularity
    #[serde(default = "default_terminal_backend")]
    pub backend: String,

    /// Working directory for terminal
    pub cwd: Option<String>,

    /// Command timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Container lifetime in seconds
    pub lifetime_seconds: Option<u64>,

    /// Docker image
    pub docker_image: Option<String>,

    /// SSH configuration
    pub ssh_host: Option<String>,
    pub ssh_user: Option<String>,
    pub ssh_port: Option<u16>,
    pub ssh_key: Option<String>,

    /// Container resources
    pub container_cpu: Option<u32>,
    pub container_memory: Option<String>,
    pub container_disk: Option<String>,
    pub container_persistent: Option<bool>,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            backend: default_terminal_backend(),
            cwd: None,
            timeout: default_timeout(),
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
        }
    }
}

/// Display/UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    /// Skin/theme name
    pub skin: Option<String>,

    /// Show tool preview
    #[serde(default = "default_true")]
    pub tool_preview: bool,

    /// Tool output prefix character
    #[serde(default = "default_tool_prefix")]
    pub tool_prefix: String,

    /// Background process notification level
    #[serde(default = "default_notification_level")]
    pub background_process_notifications: String,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            skin: None,
            tool_preview: default_true(),
            tool_prefix: default_tool_prefix(),
            background_process_notifications: default_notification_level(),
        }
    }
}

/// Memory system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Enable memory system
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Memory provider (builtin or plugin name)
    pub provider: Option<String>,

    /// Memory file path
    pub memory_file: Option<String>,

    /// User profile file path
    pub user_file: Option<String>,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            provider: None,
            memory_file: None,
            user_file: None,
        }
    }
}

/// Gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Gateway timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Platform configurations
    #[serde(default)]
    pub platforms: HashMap<String, PlatformConfig>,

    /// Allowed users (for DM pairing)
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            timeout: default_timeout(),
            platforms: HashMap::new(),
            allowed_users: Vec::new(),
        }
    }
}

/// Platform-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    /// Enable/disable platform
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Platform token/API key
    pub token: Option<String>,

    /// Working directory for this platform
    pub cwd: Option<String>,

    /// Additional platform-specific settings
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

/// Delegation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationConfig {
    /// Maximum iterations for subagents
    #[serde(default = "default_delegation_max_iterations")]
    pub max_iterations: usize,

    /// Enable parallel execution
    #[serde(default = "default_true")]
    pub parallel_enabled: bool,
}

impl Default for DelegationConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_delegation_max_iterations(),
            parallel_enabled: default_true(),
        }
    }
}

/// Auxiliary model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxiliaryConfig {
    /// Vision model configuration
    #[serde(default)]
    pub vision: AuxiliaryTaskConfig,

    /// Web extraction configuration
    #[serde(default)]
    pub web_extract: AuxiliaryTaskConfig,

    /// Approval model configuration
    #[serde(default)]
    pub approval: AuxiliaryTaskConfig,
}

impl Default for AuxiliaryConfig {
    fn default() -> Self {
        Self {
            vision: AuxiliaryTaskConfig::default(),
            web_extract: AuxiliaryTaskConfig::default(),
            approval: AuxiliaryTaskConfig::default(),
        }
    }
}

/// Auxiliary task configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxiliaryTaskConfig {
    /// Provider (auto, openai, anthropic, etc.)
    pub provider: Option<String>,

    /// Model name
    pub model: Option<String>,

    /// Base URL for custom endpoints
    pub base_url: Option<String>,

    /// API key
    pub api_key: Option<String>,
}

impl Default for AuxiliaryTaskConfig {
    fn default() -> Self {
        Self {
            provider: None,
            model: None,
            base_url: None,
            api_key: None,
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Redact secrets from logs
    #[serde(default = "default_true")]
    pub redact_secrets: bool,

    /// Command approval patterns
    #[serde(default)]
    pub approval_patterns: Vec<String>,

    /// Dangerous command patterns
    #[serde(default)]
    pub dangerous_patterns: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            redact_secrets: default_true(),
            approval_patterns: Vec::new(),
            dangerous_patterns: Vec::new(),
        }
    }
}

// ============================================================================
// Default Value Functions
// ============================================================================

fn default_model() -> String {
    "anthropic/claude-opus-4-20250514".to_string()
}

fn default_max_iterations() -> usize {
    openhermes_constants::DEFAULT_MAX_ITERATIONS
}

fn default_delegation_max_iterations() -> usize {
    openhermes_constants::DEFAULT_DELEGATION_MAX_ITERATIONS
}

fn default_terminal_backend() -> String {
    "local".to_string()
}

fn default_timeout() -> u64 {
    openhermes_constants::DEFAULT_TOOL_TIMEOUT_SECS
}

fn default_true() -> bool {
    true
}

fn default_tool_prefix() -> String {
    "┊".to_string()
}

fn default_notification_level() -> String {
    "all".to_string()
}

/// Default configuration
pub fn default_config() -> HermesConfig {
    HermesConfig::default()
}

impl Default for HermesConfig {
    fn default() -> Self {
        Self {
            agent: AgentConfig::default(),
            terminal: TerminalConfig::default(),
            display: DisplayConfig::default(),
            memory: MemoryConfig::default(),
            gateway: GatewayConfig::default(),
            delegation: DelegationConfig::default(),
            auxiliary: AuxiliaryConfig::default(),
            security: SecurityConfig::default(),
            timezone: None,
            extra: HashMap::new(),
        }
    }
}
