//! MCP Tool wrapper for integration with openhermes-tools.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::info;

use openhermes_tools::registry::Tool;
use crate::types::McpToolDefinition;
use crate::client::McpClient;

/// Wrapper that makes MCP tools compatible with openhermes-tools
pub struct McpToolWrapper {
    /// Tool definition
    definition: McpToolDefinition,
    /// Reference to MCP client
    mcp_client: Arc<McpClient>,
}

impl McpToolWrapper {
    /// Create new wrapper
    pub fn new(definition: McpToolDefinition, mcp_client: Arc<McpClient>) -> Self {
        Self {
            definition,
            mcp_client,
        }
    }
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn toolset(&self) -> &str {
        "mcp"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": self.definition.name,
            "description": self.definition.description,
            "parameters": self.definition.input_schema
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        info!(
            tool = %self.definition.name,
            server = %self.definition.server_name,
            "Calling MCP tool"
        );

        let result = self.mcp_client.call_tool(&self.definition.name, args).await?;
        
        Ok(result)
    }
}
