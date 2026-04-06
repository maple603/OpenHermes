//! Tool system for OpenHermes Agent.

pub mod registry;
pub mod file_tools;
pub mod terminal_tool;
pub mod web_tools;
pub mod file_tools_enhanced;
pub mod todo_tool;
pub mod clarify_tool;
pub mod checkpoint_tool;
pub mod memory_tools;

pub use registry::{discover_tools, handle_function_call, Tool, ToolRegistry, REGISTRY};

use std::sync::Arc;

use async_openai::types::ChatCompletionTool;

/// Get all available tool definitions
pub fn get_available_definitions() -> Vec<ChatCompletionTool> {
    REGISTRY.get_all_definitions()
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect()
}

/// Initialize all built-in tools
pub fn init_tools() {
    // File tools
    REGISTRY.register(Arc::new(file_tools::ReadFileTool));
    REGISTRY.register(Arc::new(file_tools::WriteFileTool));
    
    // Terminal tool (enhanced with background process support)
    REGISTRY.register(Arc::new(terminal_tool::TerminalTool::default()));
    
    // Web tools
    REGISTRY.register(Arc::new(web_tools::WebSearchTool::new()));
    REGISTRY.register(Arc::new(web_tools::WebExtractTool));
    REGISTRY.register(Arc::new(web_tools::UrlSafetyTool));
    
    // Enhanced file tools
    REGISTRY.register(Arc::new(file_tools_enhanced::SearchFilesTool));
    REGISTRY.register(Arc::new(file_tools_enhanced::ListDirectoryTool));
    REGISTRY.register(Arc::new(file_tools_enhanced::CopyFileTool));
    REGISTRY.register(Arc::new(file_tools_enhanced::MoveFileTool));
    REGISTRY.register(Arc::new(file_tools_enhanced::DeleteFileTool));
    REGISTRY.register(Arc::new(file_tools_enhanced::CreateDirectoryTool));
    REGISTRY.register(Arc::new(file_tools_enhanced::FileEditTool));
    
    // Assistant tools
    REGISTRY.register(Arc::new(todo_tool::TodoTool));
    REGISTRY.register(Arc::new(clarify_tool::ClarifyTool));
    REGISTRY.register(Arc::new(checkpoint_tool::CheckpointTool));
    
    // Memory tools
    REGISTRY.register(Arc::new(memory_tools::MemoryReadTool));
    REGISTRY.register(Arc::new(memory_tools::MemoryWriteTool));
    REGISTRY.register(Arc::new(memory_tools::MemorySearchTool));
    
    let tool_count = REGISTRY.get_all_tool_names().len();
    tracing::info!("Initialized {} built-in tools", tool_count);
}
