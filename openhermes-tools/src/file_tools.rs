//! File operation tools (read, write, search, patch).

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use crate::registry::{Tool, REGISTRY};

/// Read file tool
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn toolset(&self) -> &str {
        "file"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "read_file",
            "description": "Read the contents of a file",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read"
                    },
                    "max_lines": {
                        "type": "integer",
                        "description": "Maximum number of lines to read (optional)"
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: path"))?;

        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read file"))?;

        Ok(serde_json::json!({
            "success": true,
            "content": content
        })
        .to_string())
    }
}

/// Write file tool
pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn toolset(&self) -> &str {
        "file"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "write_file",
            "description": "Write content to a file, creating it if it doesn't exist",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: path"))?;

        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: content"))?;

        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(path).parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory"))?;
        }

        fs::write(path, content)
            .await
            .with_context(|| format!("Failed to write file"))?;

        Ok(serde_json::json!({
            "success": true,
            "message": format!("Written {} bytes to {}", content.len(), path)
        })
        .to_string())
    }
}

/// Register all file tools
pub fn register_tools() {
    REGISTRY.register(Arc::new(ReadFileTool));
    REGISTRY.register(Arc::new(WriteFileTool));
}
