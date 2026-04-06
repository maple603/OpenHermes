//! Central registry for all OpenHermes tools.
//!
//! Each tool file calls `registry.register()` at module level to declare its
//! schema, handler, toolset membership, and availability check.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde_json::Value;
use tracing::{debug, warn};

/// Tool trait that all tools must implement
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (unique identifier)
    fn name(&self) -> &str;

    /// Toolset this tool belongs to
    fn toolset(&self) -> &str;

    /// OpenAI-format tool schema
    fn schema(&self) -> Value;

    /// Check if tool is available (e.g., API keys present)
    fn check_fn(&self) -> bool {
        true
    }

    /// Execute the tool with given arguments
    async fn execute(&self, args: Value) -> Result<String>;
}

/// Tool registry
pub struct ToolRegistry {
    tools: DashMap<String, Arc<dyn Tool>>,
    toolset_checks: DashMap<String, Arc<dyn Fn() -> bool + Send + Sync>>,
}

impl ToolRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self {
            tools: DashMap::new(),
            toolset_checks: DashMap::new(),
        }
    }

    /// Register a tool
    pub fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        let toolset = tool.toolset().to_string();

        if let Some(existing) = self.tools.get(&name) {
            if existing.toolset() != toolset {
                warn!(
                    "Tool name collision: '{}' (toolset '{}') is being overwritten by toolset '{}'",
                    name,
                    existing.toolset(),
                    toolset
                );
            }
        }

        debug!("Registering tool: {} (toolset: {})", name, toolset);
        self.tools.insert(name.clone(), tool);

        // Store toolset check if available
        // Note: We can't store closures that reference the tool directly,
        // so we check availability when getting definitions
    }

    /// Get tool definitions for LLM
    pub fn get_definitions(&self, tool_names: &[&str]) -> Vec<Value> {
        tool_names
            .iter()
            .filter_map(|name| self.tools.get(*name))
            .filter(|entry| entry.check_fn())
            .map(|entry| {
                serde_json::json!({
                    "type": "function",
                    "function": entry.schema()
                })
            })
            .collect()
    }

    /// Get all available tool definitions
    pub fn get_all_definitions(&self) -> Vec<Value> {
        self.tools
            .iter()
            .filter(|entry| entry.value().check_fn())
            .map(|entry| {
                serde_json::json!({
                    "type": "function",
                    "function": entry.value().schema()
                })
            })
            .collect()
    }

    /// Execute a tool by name
    pub async fn dispatch(&self, name: &str, args: &str) -> Result<String> {
        let tool = self.tools
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", name))?;

        let args_value: Value = serde_json::from_str(args)
            .with_context(|| format!("Failed to parse arguments for tool {}", name))?;

        tool.execute(args_value).await
    }

    /// Get all registered tool names
    pub fn get_all_tool_names(&self) -> Vec<String> {
        self.tools.iter().map(|r| r.key().clone()).collect()
    }

    /// Get toolset for a tool
    pub fn get_toolset_for_tool(&self, name: &str) -> Option<String> {
        self.tools.get(name).map(|r| r.value().toolset().to_string())
    }

    /// Check if toolset requirements are met
    pub fn check_toolset_requirements(&self, toolset: &str) -> bool {
        self.toolset_checks
            .get(toolset)
            .map(|check| check())
            .unwrap_or(true)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global tool registry
pub static REGISTRY: Lazy<ToolRegistry> = Lazy::new(ToolRegistry::new);

/// Discover and register all tools
pub fn discover_tools() {
    debug!("Discovering tools...");

    // Register file tools
    crate::file_tools::register_tools();

    // Register terminal tool
    crate::terminal_tool::register_tools();

    // Additional tools will be registered here as they're implemented
    // crate::web_tools::register_tools();
    // crate::browser_tool::register_tools();
    // crate::mcp_tool::register_tools();

    debug!("Tool discovery complete");
}

/// Get available tool definitions
pub fn get_available_definitions() -> Vec<Value> {
    REGISTRY.get_all_definitions()
}

/// Execute a tool
pub async fn handle_function_call(name: &str, args: &str) -> Result<String> {
    REGISTRY.dispatch(name, args).await
}

use anyhow::Context;
