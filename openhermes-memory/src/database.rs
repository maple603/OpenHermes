//! SQLite database layer for memory system.

use std::path::PathBuf;

use anyhow::Result;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{SqlitePool, Row};
use tracing::info;

/// Memory database manager
pub struct MemoryDatabase {
    pool: SqlitePool,
}

impl MemoryDatabase {
    /// Create new database or connect to existing
    pub async fn new(db_path: &PathBuf) -> Result<Self> {
        // Ensure directory exists
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let db_url = format!("sqlite://{}", db_path.display());
        
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .connect(&db_url)
            .await?;

        info!("Connected to memory database: {}", db_path.display());

        let db = Self { pool };
        db.initialize().await?;

        Ok(db)
    }

    /// Get database pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Initialize database schema
    async fn initialize(&self) -> Result<()> {
        info!("Initializing memory database schema");

        // Enable WAL mode for better concurrent performance
        sqlx::query("PRAGMA journal_mode=WAL;")
            .execute(&self.pool)
            .await?;

        // Enable foreign keys
        sqlx::query("PRAGMA foreign_keys=ON;")
            .execute(&self.pool)
            .await?;

        // Create memory_entries table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memory_entries (
                id TEXT PRIMARY KEY,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                category TEXT DEFAULT 'general',
                tags TEXT DEFAULT '[]',
                importance REAL DEFAULT 0.5,
                access_count INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create sessions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT,
                summary TEXT,
                message_count INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create session_messages table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS session_messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create FTS5 virtual table for memory search
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS memory_search USING fts5(
                key,
                value,
                category,
                tags,
                content='memory_entries',
                content_rowid='rowid'
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create FTS5 virtual table for session search
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS session_search USING fts5(
                title,
                summary,
                content='sessions',
                content_rowid='rowid'
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create triggers to keep FTS index in sync
        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS memory_ai AFTER INSERT ON memory_entries BEGIN
                INSERT INTO memory_search(rowid, key, value, category, tags)
                VALUES (new.rowid, new.key, new.value, new.category, new.tags);
            END;
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS memory_ad AFTER DELETE ON memory_entries BEGIN
                INSERT INTO memory_search(memory_search, rowid, key, value, category, tags)
                VALUES('delete', old.rowid, old.key, old.value, old.category, old.tags);
            END;
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS memory_au AFTER UPDATE ON memory_entries BEGIN
                INSERT INTO memory_search(memory_search, rowid, key, value, category, tags)
                VALUES('delete', old.rowid, old.key, old.value, old.category, old.tags);
                INSERT INTO memory_search(rowid, key, value, category, tags)
                VALUES (new.rowid, new.key, new.value, new.category, new.tags);
            END;
            "#,
        )
        .execute(&self.pool)
        .await?;

        info!("Memory database schema initialized");
        Ok(())
    }

    /// Rebuild FTS index (useful after manual data imports)
    pub async fn rebuild_fts_index(&self) -> Result<()> {
        info!("Rebuilding FTS index");
        
        sqlx::query("INSERT INTO memory_search(memory_search) VALUES('rebuild');")
            .execute(&self.pool)
            .await?;

        sqlx::query("INSERT INTO session_search(session_search) VALUES('rebuild');")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Close database connection
    pub async fn close(self) -> Result<()> {
        self.pool.close().await;
        info!("Memory database connection closed");
        Ok(())
    }

    /// Insert a memory entry
    pub async fn insert_memory(
        &self,
        id: &str,
        key: &str,
        value: &str,
        category: &str,
        tags: &str,
        importance: f64,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO memory_entries (id, key, value, category, tags, importance)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(key)
        .bind(value)
        .bind(category)
        .bind(tags)
        .bind(importance)
        .execute(&self.pool)
        .await?;

        info!(id = id, key = key, "Memory inserted");
        Ok(())
    }

    /// Search memories using FTS5
    pub async fn search_memories(
        &self,
        query: &str,
        category: Option<&str>,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>> {
        let mut results = Vec::new();

        if let Some(cat) = category {
            let rows = sqlx::query(
                r#"
                SELECT m.id, m.key, m.value, m.category, m.tags,
                       m.importance, m.access_count, ms.rank
                FROM memory_search ms
                JOIN memory_entries m ON m.rowid = ms.rowid
                WHERE memory_search MATCH ?
                  AND m.category = ?
                ORDER BY ms.rank
                LIMIT ?
                "#,
            )
            .bind(query)
            .bind(cat)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;

            for row in rows {
                results.push(memory_row_to_json(&row));
            }
        } else {
            let rows = sqlx::query(
                r#"
                SELECT m.id, m.key, m.value, m.category, m.tags,
                       m.importance, m.access_count, ms.rank
                FROM memory_search ms
                JOIN memory_entries m ON m.rowid = ms.rowid
                WHERE memory_search MATCH ?
                ORDER BY ms.rank
                LIMIT ?
                "#,
            )
            .bind(query)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;

            for row in rows {
                results.push(memory_row_to_json(&row));
            }
        }

        // Update access count
        for result in &results {
            if let Some(id) = result["id"].as_str() {
                sqlx::query(
                    "UPDATE memory_entries SET access_count = access_count + 1 WHERE id = ?",
                )
                .bind(id)
                .execute(&self.pool)
                .await?;
            }
        }

        info!(query = query, count = results.len(), "Memory search completed");
        Ok(results)
    }

    /// Search sessions using FTS5
    pub async fn search_sessions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            r#"
            SELECT s.id, s.title, s.summary, s.message_count,
                   s.created_at, s.updated_at, ss.rank
            FROM session_search ss
            JOIN sessions s ON s.rowid = ss.rowid
            WHERE session_search MATCH ?
            ORDER BY ss.rank
            LIMIT ?
            "#,
        )
        .bind(query)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let results: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "title": row.get::<String, _>("title"),
                    "summary": row.get::<String, _>("summary"),
                    "message_count": row.get::<i64, _>("message_count"),
                    "created_at": row.get::<String, _>("created_at"),
                    "updated_at": row.get::<String, _>("updated_at"),
                    "rank": row.get::<f64, _>("rank")
                })
            })
            .collect();

        info!(query = query, count = results.len(), "Session search completed");
        Ok(results)
    }

    /// Update memory importance
    pub async fn update_memory_importance(&self, id: &str, importance: f64) -> Result<()> {
        sqlx::query(
            "UPDATE memory_entries SET importance = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        )
        .bind(importance)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a memory entry
    pub async fn delete_memory(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM memory_entries WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        info!(id = id, "Memory deleted");
        Ok(())
    }
}

/// Convert database row to JSON
fn memory_row_to_json(row: &sqlx::sqlite::SqliteRow) -> serde_json::Value {
    let tags_str: String = row.get("tags");
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

    serde_json::json!({
        "id": row.get::<String, _>("id"),
        "key": row.get::<String, _>("key"),
        "value": row.get::<String, _>("value"),
        "category": row.get::<String, _>("category"),
        "tags": tags,
        "importance": row.get::<f64, _>("importance"),
        "access_count": row.get::<i64, _>("access_count"),
        "rank": row.get::<f64, _>("rank")
    })
}
