//! Memory tools for reading, writing, and searching memory.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::info;

use crate::registry::{Tool, REGISTRY};

use once_cell::sync::Lazy;
use tokio::sync::RwLock;

/// Global database instance (initialized by agent)
static MEMORY_DB: Lazy<RwLock<Option<Arc<openhermes_memory::database::MemoryDatabase>>>> = 
    Lazy::new(|| RwLock::new(None));

/// Set the global database instance
pub async fn set_memory_db(db: Arc<openhermes_memory::database::MemoryDatabase>) {
    let mut guard = MEMORY_DB.write().await;
    *guard = Some(db);
    info!("Memory database set globally");
}

/// Get the global database instance
async fn get_memory_db() -> Option<Arc<openhermes_memory::database::MemoryDatabase>> {
    let guard = MEMORY_DB.read().await;
    guard.clone()
}

/// Memory read tool - search for stored memories
pub struct MemoryReadTool;

#[async_trait]
impl Tool for MemoryReadTool {
    fn name(&self) -> &str {
        "memory_read"
    }

    fn toolset(&self) -> &str {
        "memory"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "memory_read",
            "description": "Read from memory. Search for previously stored information using keywords.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query to find relevant memories"
                    },
                    "category": {
                        "type": "string",
                        "description": "Filter by category (e.g., 'preferences', 'facts', 'notes')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let query = args["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;
        
        let category = args["category"].as_str();
        let limit = args["limit"].as_u64().unwrap_or(5) as usize;

        info!(query = query, category = category, limit = limit, "Reading from memory");

        // Prepare FTS5 query
        let fts_query = openhermes_memory::fts5::FTSSearch::prepare_query(query);

        // Execute actual database query
        if let Some(db) = get_memory_db().await {
            let results = db.search_memories(&fts_query, category, limit).await?;
            
            Ok(serde_json::json!({
                "success": true,
                "query": query,
                "fts_query": fts_query,
                "category": category,
                "limit": limit,
                "results": results,
                "count": results.len(),
                "message": format!("Found {} memories", results.len())
            }).to_string())
        } else {
            Ok(serde_json::json!({
                "success": false,
                "query": query,
                "message": "Memory database not initialized. Use set_memory_db() to initialize."
            }).to_string())
        }
    }
}

/// Memory write tool - store information
pub struct MemoryWriteTool;

#[async_trait]
impl Tool for MemoryWriteTool {
    fn name(&self) -> &str {
        "memory_write"
    }

    fn toolset(&self) -> &str {
        "memory"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "memory_write",
            "description": "Write to memory. Store important information for later retrieval.",
            "parameters": {
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Memory key (unique identifier)"
                    },
                    "value": {
                        "type": "string",
                        "description": "Memory value (content to store)"
                    },
                    "category": {
                        "type": "string",
                        "description": "Category for organization (default: 'general')",
                        "default": "general"
                    },
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Tags for better organization"
                    },
                    "importance": {
                        "type": "number",
                        "description": "Importance score 0.0-1.0 (default: 0.5)",
                        "default": 0.5
                    }
                },
                "required": ["key", "value"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let key = args["key"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: key"))?;
        
        let value = args["value"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: value"))?;

        let category = args["category"].as_str().unwrap_or("general");
        let tags = args["tags"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<String>>()
        }).unwrap_or_default();
        
        let importance = args["importance"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0);

        info!(
            key = key,
            category = category,
            importance = importance,
            tags_count = tags.len(),
            "Writing to memory"
        );

        // Generate unique ID
        let id = format!("mem_{}", chrono::Utc::now().timestamp_millis());
        let tags_json = serde_json::to_string(&tags).unwrap_or("[]".to_string());

        // Execute actual database insert
        if let Some(db) = get_memory_db().await {
            db.insert_memory(&id, key, value, category, &tags_json, importance).await?;
            
            Ok(serde_json::json!({
                "success": true,
                "id": id,
                "key": key,
                "value": value,
                "category": category,
                "tags": tags,
                "importance": importance,
                "message": format!("Memory '{}' stored successfully (ID: {})", key, id)
            }).to_string())
        } else {
            Ok(serde_json::json!({
                "success": false,
                "key": key,
                "message": "Memory database not initialized. Use set_memory_db() to initialize."
            }).to_string())
        }
    }
}

/// Memory search tool - search session history
pub struct MemorySearchTool;

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn toolset(&self) -> &str {
        "memory"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "memory_search",
            "description": "Search session history. Find previous conversations and interactions.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query to find relevant sessions"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let query = args["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;
        
        let limit = args["limit"].as_u64().unwrap_or(5) as usize;

        info!(query = query, limit = limit, "Searching session history");

        // Prepare FTS5 query
        let fts_query = openhermes_memory::fts5::FTSSearch::prepare_query(query);

        // Execute actual database search
        if let Some(db) = get_memory_db().await {
            let results = db.search_sessions(&fts_query, limit).await?;
            
            Ok(serde_json::json!({
                "success": true,
                "query": query,
                "fts_query": fts_query,
                "limit": limit,
                "results": results,
                "count": results.len(),
                "message": format!("Found {} sessions", results.len())
            }).to_string())
        } else {
            Ok(serde_json::json!({
                "success": false,
                "query": query,
                "message": "Memory database not initialized. Use set_memory_db() to initialize."
            }).to_string())
        }
    }
}

/// Register memory tools
pub fn register_tools() {
    REGISTRY.register(Arc::new(MemoryReadTool));
    REGISTRY.register(Arc::new(MemoryWriteTool));
    REGISTRY.register(Arc::new(MemorySearchTool));
    
    info!("Registered 3 memory tools");
}
