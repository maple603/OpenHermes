//! MCP tools integration with openhermes-tools.

use std::sync::Arc;

use anyhow::Result;
use tracing::{info, warn};

use openhermes_tools::registry::REGISTRY;
use crate::client::McpClient;
use crate::tool_wrapper::McpToolWrapper;

/// Global MCP client instance
static MCP_CLIENT: std::sync::OnceLock<Arc<McpClient>> = std::sync::OnceLock::new();

/// Get or create global MCP client
pub fn get_mcp_client() -> Arc<McpClient> {
    MCP_CLIENT.get_or_init(|| Arc::new(McpClient::new())).clone()
}

/// Connect to an MCP server and register its tools
pub async fn connect_mcp_server(server_config: crate::types::McpServerConfig) -> Result<()> {
    let client = get_mcp_client();
    
    info!(server = %server_config.name, url = %server_config.url, "Connecting to MCP server");
    
    // Connect to server
    client.connect(server_config.clone()).await?;
    
    // Register discovered tools
    let tool_defs = client.get_tool_definitions();
    let new_tools = tool_defs.len();
    
    for tool_def in tool_defs {
        let wrapper = McpToolWrapper::new(tool_def.clone(), client.clone());
        REGISTRY.register(Arc::new(wrapper));
    }
    
    info!(server = %server_config.name, tools = new_tools, "MCP tools registered");
    Ok(())
}

/// Disconnect from an MCP server
pub async fn disconnect_mcp_server(server_name: &str) -> Result<()> {
    let client = get_mcp_client();
    client.disconnect(server_name).await?;
    
    info!(server = server_name, "Disconnected from MCP server");
    Ok(())
}

/// Get MCP client statistics
pub fn mcp_stats() -> (usize, usize) {
    let client = get_mcp_client();
    (client.server_count(), client.tool_count())
}

/// Check if MCP system is initialized
pub fn is_mcp_initialized() -> bool {
    MCP_CLIENT.get().is_some()
}
