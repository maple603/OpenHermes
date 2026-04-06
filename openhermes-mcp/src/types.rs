//! MCP types and data structures.

use serde::{Deserialize, Serialize};

/// MCP Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Server name/identifier
    pub name: String,
    /// Server URL (HTTP/SSE endpoint)
    pub url: String,
    /// Optional authentication token
    pub api_key: Option<String>,
    /// Optional headers
    pub headers: Option<std::collections::HashMap<String, String>>,
    /// Connection timeout in seconds
    pub timeout_secs: Option<u64>,
}

impl McpServerConfig {
    /// Create new server config
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            api_key: None,
            headers: None,
            timeout_secs: Some(30),
        }
    }

    /// Set API key
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }
}

/// MCP Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON Schema for input validation
    pub input_schema: serde_json::Value,
    /// Server that provides this tool
    pub server_name: String,
}

/// MCP JSON-RPC Request
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version
    pub jsonrpc: String,
    /// Request ID
    pub id: serde_json::Value,
    /// Method name
    pub method: String,
    /// Parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// MCP JSON-RPC Response
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version
    pub jsonrpc: String,
    /// Response ID
    pub id: Option<serde_json::Value>,
    /// Result
    pub result: Option<serde_json::Value>,
    /// Error
    pub error: Option<JsonRpcError>,
}

/// MCP JSON-RPC Error
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Error data
    pub data: Option<serde_json::Value>,
}

/// MCP Initialize Result
#[derive(Debug, Deserialize)]
pub struct McpInitializeResult {
    /// Protocol version
    pub protocol_version: String,
    /// Server capabilities
    pub capabilities: McpCapabilities,
    /// Server info
    pub server_info: McpServerInfo,
}

/// MCP Server capabilities
#[derive(Debug, Deserialize)]
pub struct McpCapabilities {
    /// Tools capability
    pub tools: Option<McpToolsCapability>,
    /// Resources capability
    pub resources: Option<serde_json::Value>,
    /// Prompts capability
    pub prompts: Option<serde_json::Value>,
}

/// MCP Tools capability
#[derive(Debug, Deserialize)]
pub struct McpToolsCapability {
    /// Whether tools can be listed
    pub list_changed: Option<bool>,
}

/// MCP Server info
#[derive(Debug, Deserialize)]
pub struct McpServerInfo {
    /// Server name
    pub name: String,
    /// Server version
    pub version: String,
}

/// MCP Tool List Response
#[derive(Debug, Deserialize)]
pub struct McpToolListResponse {
    /// List of tools
    pub tools: Vec<McpToolInfo>,
}

/// MCP Tool info
#[derive(Debug, Deserialize)]
pub struct McpToolInfo {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Input schema (JSON Schema)
    pub input_schema: serde_json::Value,
}

/// MCP Tool Call Request
#[derive(Debug, Serialize)]
pub struct McpToolCallRequest {
    /// Tool name
    pub name: String,
    /// Tool arguments
    pub arguments: serde_json::Value,
}

/// MCP Tool Call Response
#[derive(Debug, Deserialize)]
pub struct McpToolCallResponse {
    /// Tool execution result
    pub content: Vec<McpContent>,
    /// Whether there's an error
    #[serde(default)]
    pub is_error: bool,
}

/// MCP Content
#[derive(Debug, Deserialize)]
pub struct McpContent {
    /// Content type
    #[serde(rename = "type")]
    pub content_type: String,
    /// Text content
    pub text: Option<String>,
    /// Image data (base64)
    pub data: Option<String>,
    /// Image MIME type
    pub mime_type: Option<String>,
}
