//! Built-in memory provider (placeholder).

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use super::memory_manager::MemoryProvider;

/// Built-in memory provider
pub struct BuiltinMemoryProvider;

#[async_trait]
impl MemoryProvider for BuiltinMemoryProvider {
    fn name(&self) -> &str {
        "builtin"
    }

    fn get_tool_schemas(&self) -> Vec<Value> {
        vec![
            serde_json::json!({
                "name": "memory_read",
                "description": "Read from memory",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Search query"}
                    },
                    "required": ["query"]
                }
            }),
            serde_json::json!({
                "name": "memory_write",
                "description": "Write to memory",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "key": {"type": "string", "description": "Memory key"},
                        "value": {"type": "string", "description": "Memory value"}
                    },
                    "required": ["key", "value"]
                }
            })
        ]
    }

    async fn prefetch(&self, _user_message: &str) -> Result<String> {
        // TODO: Implement actual memory retrieval
        Ok(String::new())
    }

    async fn sync(&self, _user_msg: &str, _assistant_response: &str) -> Result<()> {
        // TODO: Implement actual memory storage
        Ok(())
    }
}
