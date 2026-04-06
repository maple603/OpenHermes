//! MCP Client implementation.

use anyhow::Result;
use dashmap::DashMap;
use reqwest::Client;
use tracing::info;

use crate::types::{
    JsonRpcRequest, JsonRpcResponse, McpServerConfig, McpToolDefinition,
    McpInitializeResult, McpToolListResponse,
    McpToolCallResponse,
};

/// MCP Client for connecting to MCP servers
pub struct McpClient {
    /// HTTP client
    http_client: Client,
    /// Connected servers
    servers: DashMap<String, McpServerConfig>,
    /// Discovered tools (tool_name -> server_name)
    tool_registry: DashMap<String, McpToolDefinition>,
}

impl McpClient {
    /// Create new MCP client
    pub fn new() -> Self {
        Self {
            http_client: Client::new(),
            servers: DashMap::new(),
            tool_registry: DashMap::new(),
        }
    }

    /// Connect to an MCP server
    pub async fn connect(&self, config: McpServerConfig) -> Result<()> {
        info!(server = %config.name, url = %config.url, "Connecting to MCP server");

        // Store server config
        self.servers.insert(config.name.clone(), config.clone());

        // Initialize connection
        self.initialize_server(&config.name).await?;

        // Discover tools
        self.discover_tools(&config.name).await?;

        info!(server = %config.name, "MCP server connected");
        Ok(())
    }

    /// Initialize connection to server
    async fn initialize_server(&self, server_name: &str) -> Result<()> {
        let config = self.servers.get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server not found: {}", server_name))?;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(1),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "OpenHermes",
                    "version": "0.1.0"
                }
            })),
        };

        let response = self.send_request(&config.url, &request, &config).await?;
        
        let init_result: McpInitializeResult = serde_json::from_value(
            response.result.ok_or_else(|| anyhow::anyhow!("No result in initialize response"))?
        )?;

        info!(
            server = server_name,
            protocol = %init_result.protocol_version,
            server_name = %init_result.server_info.name,
            "MCP server initialized"
        );

        // Send initialized notification
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });

        self.http_client
            .post(&config.url)
            .json(&notification)
            .send()
            .await?;

        Ok(())
    }

    /// Discover tools from server
    async fn discover_tools(&self, server_name: &str) -> Result<()> {
        let config = self.servers.get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server not found: {}", server_name))?;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(2),
            method: "tools/list".to_string(),
            params: None,
        };

        let response = self.send_request(&config.url, &request, &config).await?;
        
        let tool_list: McpToolListResponse = serde_json::from_value(
            response.result.ok_or_else(|| anyhow::anyhow!("No result in tools/list response"))?
        )?;

        info!(server = server_name, count = tool_list.tools.len(), "Discovered MCP tools");

        // Register tools
        for tool_info in tool_list.tools {
            let tool_def = McpToolDefinition {
                name: format!("mcp_{}_{}", server_name, tool_info.name),
                description: tool_info.description,
                input_schema: tool_info.input_schema,
                server_name: server_name.to_string(),
            };

            self.tool_registry.insert(tool_def.name.clone(), tool_def);
        }

        Ok(())
    }

    /// Call an MCP tool
    pub async fn call_tool(&self, tool_name: &str, arguments: serde_json::Value) -> Result<String> {
        let tool_def = self.tool_registry.get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("MCP tool not found: {}", tool_name))?;

        let server_name = &tool_def.server_name;
        let config = self.servers.get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server not found: {}", server_name))?;

        // Extract original tool name (remove server prefix)
        let original_name = tool_name
            .strip_prefix(&format!("mcp_{}_", server_name))
            .unwrap_or(tool_name);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(3),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": original_name,
                "arguments": arguments
            })),
        };

        let response = self.send_request(&config.url, &request, &config).await?;
        
        let call_result: McpToolCallResponse = serde_json::from_value(
            response.result.ok_or_else(|| anyhow::anyhow!("No result in tools/call response"))?
        )?;

        if call_result.is_error {
            let error_text = call_result.content
                .iter()
                .filter_map(|c| c.text.as_ref())
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            
            return Err(anyhow::anyhow!("MCP tool error: {}", error_text));
        }

        // Combine all text content
        let result_text = call_result.content
            .iter()
            .filter_map(|c| c.text.as_ref())
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(result_text)
    }

    /// Send JSON-RPC request
    async fn send_request(
        &self,
        url: &str,
        request: &JsonRpcRequest,
        config: &McpServerConfig,
    ) -> Result<JsonRpcResponse> {
        let mut http_request = self.http_client.post(url).json(request);

        // Add authentication
        if let Some(api_key) = &config.api_key {
            http_request = http_request.header("Authorization", format!("Bearer {}", api_key));
        }

        // Add custom headers
        if let Some(headers) = &config.headers {
            for (key, value) in headers {
                http_request = http_request.header(key, value);
            }
        }

        // Add timeout
        if let Some(timeout) = config.timeout_secs {
            http_request = http_request.timeout(std::time::Duration::from_secs(timeout));
        }

        let response = http_request.send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            return Err(anyhow::anyhow!("HTTP error {}: {}", status, body));
        }

        let json_response: JsonRpcResponse = response.json().await?;

        // Check for JSON-RPC error
        if let Some(error) = &json_response.error {
            return Err(anyhow::anyhow!(
                "JSON-RPC error {}: {}",
                error.code,
                error.message
            ));
        }

        Ok(json_response)
    }

    /// Get all discovered tool definitions
    pub fn get_tool_definitions(&self) -> Vec<McpToolDefinition> {
        self.tool_registry
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Check if a tool exists
    pub fn has_tool(&self, tool_name: &str) -> bool {
        self.tool_registry.contains_key(tool_name)
    }

    /// Disconnect from a server
    pub async fn disconnect(&self, server_name: &str) -> Result<()> {
        self.servers.remove(server_name);
        
        // Remove tools from this server
        let tools_to_remove: Vec<String> = self.tool_registry
            .iter()
            .filter(|entry| entry.value().server_name == server_name)
            .map(|entry| entry.key().clone())
            .collect();

        for tool_name in tools_to_remove {
            self.tool_registry.remove(&tool_name);
        }

        info!(server = server_name, "Disconnected from MCP server");
        Ok(())
    }

    /// Get connected server count
    pub fn server_count(&self) -> usize {
        self.servers.len()
    }

    /// Get discovered tool count
    pub fn tool_count(&self) -> usize {
        self.tool_registry.len()
    }
}

impl Default for McpClient {
    fn default() -> Self {
        Self::new()
    }
}
