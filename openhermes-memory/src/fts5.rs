//! FTS5 full-text search implementation.

use anyhow::Result;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tracing::debug;

/// Memory search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub id: String,
    pub key: String,
    pub value: String,
    pub category: String,
    pub tags: Vec<String>,
    pub importance: f64,
    pub access_count: i64,
    pub rank: f64,
}

/// Session search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSearchResult {
    pub id: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub message_count: i64,
    pub created_at: Option<NaiveDateTime>,
    pub rank: f64,
}

/// FTS5 search engine
pub struct FTSSearch {
    pool: SqlitePool,
}

impl FTSSearch {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Search memory entries
    pub async fn search_memories(
        &self,
        query: &str,
        limit: usize,
        category: Option<&str>,
    ) -> Result<Vec<MemorySearchResult>> {
        debug!(query = query, limit = limit, "Searching memories");

        let results = if let Some(cat) = category {
            sqlx::query_as::<_, (String, String, String, String, String, f64, i64, f64)>(
                r#"
                SELECT 
                    m.id, m.key, m.value, m.category, m.tags,
                    m.importance, m.access_count,
                    rank
                FROM memory_search ms
                JOIN memory_entries m ON m.rowid = ms.rowid
                WHERE memory_search MATCH ?
                  AND m.category = ?
                ORDER BY rank
                LIMIT ?
                "#,
            )
            .bind(query)
            .bind(cat)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, (String, String, String, String, String, f64, i64, f64)>(
                r#"
                SELECT 
                    m.id, m.key, m.value, m.category, m.tags,
                    m.importance, m.access_count,
                    rank
                FROM memory_search ms
                JOIN memory_entries m ON m.rowid = ms.rowid
                WHERE memory_search MATCH ?
                ORDER BY rank
                LIMIT ?
                "#,
            )
            .bind(query)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(results
            .into_iter()
            .map(|(id, key, value, category, tags, importance, access_count, rank)| {
                let tags_vec: Vec<String> = serde_json::from_str(&tags).unwrap_or_default();
                MemorySearchResult {
                    id,
                    key,
                    value,
                    category,
                    tags: tags_vec,
                    importance,
                    access_count,
                    rank,
                }
            })
            .collect())
    }

    /// Search sessions
    pub async fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSearchResult>> {
        debug!(query = query, limit = limit, "Searching sessions");

        let results = sqlx::query_as::<_, (String, Option<String>, Option<String>, i64, Option<NaiveDateTime>, f64)>(
            r#"
            SELECT 
                s.id, s.title, s.summary, s.message_count,
                s.created_at, rank
            FROM session_search ss
            JOIN sessions s ON s.rowid = ss.rowid
            WHERE session_search MATCH ?
            ORDER BY rank
            LIMIT ?
            "#,
        )
        .bind(query)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(results
            .into_iter()
            .map(|(id, title, summary, message_count, created_at, rank)| {
                SessionSearchResult {
                    id,
                    title,
                    summary,
                    message_count,
                    created_at,
                    rank,
                }
            })
            .collect())
    }

    /// Simple keyword search (without FTS5 syntax)
    pub fn prepare_query(query: &str) -> String {
        // Split query into terms and join with AND for FTS5
        let terms: Vec<&str> = query.split_whitespace().collect();
        if terms.is_empty() {
            return query.to_string();
        }

        // Escape special FTS5 characters
        let escaped: Vec<String> = terms
            .iter()
            .map(|term| {
                term.replace('"', "\"\"")
                    .replace('*', "\\*")
                    .replace(':', "\\:")
            })
            .collect();

        escaped.join(" AND ")
    }
}
