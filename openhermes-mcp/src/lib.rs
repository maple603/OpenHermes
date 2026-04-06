//! MCP (Model Context Protocol) client module.

pub mod client;
pub mod types;
pub mod tool_wrapper;
pub mod integration;

pub use client::McpClient;
pub use types::{McpServerConfig, McpToolDefinition};
pub use tool_wrapper::McpToolWrapper;
pub use integration::{connect_mcp_server, disconnect_mcp_server, get_mcp_client, mcp_stats};
