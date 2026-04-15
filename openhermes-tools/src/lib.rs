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
pub mod skills_tools;
pub mod skills_manager;
pub mod skills_hub_client;
pub mod skill_sandbox;
pub mod dependency_installer;
pub mod skill_loader;

// P1 Tools
pub mod approval_tool;
pub mod session_search_tool;
pub mod delegate_tool;
pub mod cronjob_tool;
pub mod code_execution_tool;
pub mod send_message_tool;
pub mod mixture_of_agents_tool;
pub mod vision_tool;
pub mod image_generation_tool;
pub mod tts_tool;
pub mod browser_tool;
pub mod llm_client;

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
    
    // Skills tools
    REGISTRY.register(Arc::new(skills_tools::SkillsInstallTool));
    REGISTRY.register(Arc::new(skills_tools::SkillsListTool));
    REGISTRY.register(Arc::new(skills_tools::SkillsSyncTool));
    REGISTRY.register(Arc::new(skills_tools::SkillsHubSearchTool));

    // P1 Tools: Core real implementations
    REGISTRY.register(Arc::new(session_search_tool::SessionSearchTool));
    REGISTRY.register(Arc::new(delegate_tool::DelegateTaskTool));
    REGISTRY.register(Arc::new(cronjob_tool::CronjobTool));
    REGISTRY.register(Arc::new(code_execution_tool::CodeExecutionTool));
    REGISTRY.register(Arc::new(send_message_tool::SendMessageTool));
    REGISTRY.register(Arc::new(mixture_of_agents_tool::MixtureOfAgentsTool));

    // P1 Tools: Structured stubs (service-dependent)
    REGISTRY.register(Arc::new(vision_tool::VisionTool));
    REGISTRY.register(Arc::new(image_generation_tool::ImageGenerationTool));
    REGISTRY.register(Arc::new(tts_tool::TtsTool));

    // P1 Tools: Browser tools (10)
    for tool in browser_tool::all_browser_tools() {
        REGISTRY.register(tool);
    }
    
    let tool_count = REGISTRY.get_all_tool_names().len();
    tracing::info!("Initialized {} built-in tools", tool_count);
}
