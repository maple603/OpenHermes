//! Session search tool — FTS5 search across past sessions with LLM summarization.
//!
//! Searches the memory system for past conversations matching a query,
//! returns session summaries, and optionally uses an auxiliary LLM to
//! synthesize the findings.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::registry::Tool;

/// Shared reference to the memory database (set by agent initialization).
/// Reuses the same global from memory_tools via re-export.
static SESSION_DB: Lazy<RwLock<Option<Arc<openhermes_memory::database::MemoryDatabase>>>> =
    Lazy::new(|| RwLock::new(None));

/// Set the database for session search (called during initialization).
pub async fn set_session_db(db: Arc<openhermes_memory::database::MemoryDatabase>) {
    let mut guard = SESSION_DB.write().await;
    *guard = Some(db);
}

/// Get the database for session search.
async fn get_db() -> Option<Arc<openhermes_memory::database::MemoryDatabase>> {
    let guard = SESSION_DB.read().await;
    guard.clone()
}

/// Session search tool.
pub struct SessionSearchTool;

#[async_trait]
impl Tool for SessionSearchTool {
    fn name(&self) -> &str {
        "session_search"
    }

    fn toolset(&self) -> &str {
        "memory"
    }

    fn check_fn(&self) -> bool {
        // Available when memory database is initialized (non-blocking check)
        true
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "session_search",
            "description": "Search past conversation sessions by keyword or topic. Returns relevant session summaries and excerpts from previous conversations.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query to find in past sessions"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of sessions to return (default: 3)",
                        "default": 3
                    },
                    "summarize": {
                        "type": "boolean",
                        "description": "Whether to LLM-summarize the results (default: true)",
                        "default": true
                    }
                },
                "required": ["query"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;
        let limit = args["limit"].as_u64().unwrap_or(3) as usize;
        let summarize = args["summarize"].as_bool().unwrap_or(true);

        debug!(query = query, limit = limit, "Searching past sessions");

        let db = match get_db().await {
            Some(db) => db,
            None => {
                return Ok(serde_json::json!({
                    "error": "Memory database not initialized. Session search is unavailable.",
                    "results": []
                }).to_string());
            }
        };

        // Search sessions using FTS5
        let results = match db.search_sessions(query, limit).await {
            Ok(r) => r,
            Err(e) => {
                warn!("Session search failed: {}", e);
                return Ok(serde_json::json!({
                    "error": format!("Search failed: {}", e),
                    "results": []
                }).to_string());
            }
        };

        if results.is_empty() {
            return Ok(serde_json::json!({
                "query": query,
                "results": [],
                "message": "No matching sessions found."
            }).to_string());
        }

        // Also search memories for additional context
        let memory_results = db
            .search_memories(query, None, limit * 3)
            .await
            .unwrap_or_default();

        // Build session entries
        let mut session_entries: Vec<Value> = Vec::new();
        for session in &results {
            let session_id = session["id"].as_str().unwrap_or("unknown");
            let title = session["title"].as_str().unwrap_or("Untitled");
            let summary = session["summary"].as_str().unwrap_or("");

            // Find related memories for this session
            let related_memories: Vec<&Value> = memory_results
                .iter()
                .filter(|m| {
                    m["value"].as_str().map_or(false, |v| {
                        v.to_lowercase().contains(&query.to_lowercase())
                    })
                })
                .take(3)
                .collect();

            let mut entry = serde_json::json!({
                "session_id": session_id,
                "title": title,
                "summary": truncate_text(summary, 500),
                "message_count": session["message_count"],
                "created_at": session["created_at"],
            });

            if !related_memories.is_empty() {
                let mem_excerpts: Vec<Value> = related_memories
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "key": m["key"],
                            "excerpt": truncate_text(
                                m["value"].as_str().unwrap_or(""),
                                300,
                            ),
                        })
                    })
                    .collect();
                entry["related_memories"] = Value::Array(mem_excerpts);
            }

            session_entries.push(entry);
        }

        // Optionally summarize with LLM
        if summarize && !session_entries.is_empty() {
            let raw_text = format_for_summarization(&results);
            match crate::llm_client::call_llm(
                &format!(
                    "Summarize these search results for the query \"{}\". \
                     Be concise and highlight the most relevant findings:\n\n{}",
                    query, raw_text
                ),
                Some("session_search"),
                Some(1024),
            )
            .await
            {
                Ok(summary) => {
                    return Ok(serde_json::json!({
                        "query": query,
                        "session_count": session_entries.len(),
                        "summary": summary,
                        "sessions": session_entries,
                    })
                    .to_string());
                }
                Err(e) => {
                    debug!("LLM summarization failed, returning raw results: {}", e);
                }
            }
        }

        Ok(serde_json::json!({
            "query": query,
            "session_count": session_entries.len(),
            "sessions": session_entries,
        })
        .to_string())
    }
}

/// Truncate text to max_chars.
fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", &text[..max_chars])
    }
}

/// Format session results for LLM summarization.
fn format_for_summarization(sessions: &[Value]) -> String {
    let mut parts = Vec::new();
    for session in sessions {
        let title = session["title"].as_str().unwrap_or("Untitled");
        let summary = session["summary"].as_str().unwrap_or("");
        let truncated = truncate_text(summary, 500);
        parts.push(format!("Session \"{}\": {}", title, truncated));
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 100), "short");
        let long = "a".repeat(200);
        let result = truncate_text(&long, 50);
        assert!(result.len() < 200);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_format_for_summarization() {
        let sessions = vec![
            serde_json::json!({
                "title": "Test Session",
                "summary": "This is a test session about Rust programming."
            }),
        ];
        let formatted = format_for_summarization(&sessions);
        assert!(formatted.contains("Test Session"));
        assert!(formatted.contains("Rust programming"));
    }
}
