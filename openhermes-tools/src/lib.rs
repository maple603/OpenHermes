//! Tool system for OpenHermes Agent.

pub mod registry;
pub mod file_tools;
pub mod terminal_tool;

pub use registry::{discover_tools, handle_function_call, Tool, ToolRegistry, REGISTRY};

use async_openai::types::ChatCompletionTool;

/// Get all available tool definitions
pub fn get_available_definitions() -> Vec<ChatCompletionTool> {
    REGISTRY.get_all_definitions()
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect()
}
