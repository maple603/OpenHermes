//! Built-in memory provider with SQLite storage.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::{info, warn};

use crate::memory_manager::MemoryProvider;

/// Built-in memory provider
pub struct BuiltinMemoryProvider;

impl BuiltinMemoryProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BuiltinMemoryProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryProvider for BuiltinMemoryProvider {
    fn name(&self) -> &str {
        "builtin"
    }

    fn get_tool_schemas(&self) -> Vec<Value> {
        vec![
            serde_json::json!({
                "name": "memory_read",
                "description": "Read from memory. Search for previously stored information.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Search query"},
                        "category": {"type": "string", "description": "Filter by category (optional)"},
                        "limit": {"type": "integer", "description": "Maximum results (default: 5)", "default": 5}
                    },
                    "required": ["query"]
                }
            }),
            serde_json::json!({
                "name": "memory_write",
                "description": "Write to memory. Store important information for later retrieval.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "key": {"type": "string", "description": "Memory key"},
                        "value": {"type": "string", "description": "Memory value"},
                        "category": {"type": "string", "description": "Category (default: general)", "default": "general"},
                        "tags": {"type": "array", "items": {"type": "string"}, "description": "Tags for organization"}
                    },
                    "required": ["key", "value"]
                }
            }),
            serde_json::json!({
                "name": "memory_search",
                "description": "Search session history",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Search query"},
                        "limit": {"type": "integer", "description": "Maximum results (default: 5)", "default": 5}
                    },
                    "required": ["query"]
                }
            })
        ]
    }

    async fn prefetch(&self, user_message: &str) -> Result<String> {
        // TODO: Implement actual memory retrieval from database
        info!("Prefetching memories for: {}", user_message);
        Ok(String::new())
    }

    async fn sync(&self, user_msg: &str, assistant_response: &str) -> Result<()> {
        // TODO: Implement actual memory storage to database
        info!("Syncing memory: user={}, assistant={}", user_msg.len(), assistant_response.len());
        Ok(())
    }
}
