//! Memory manager orchestrating built-in and external providers.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::info;

use crate::database::MemoryDatabase;
use crate::fts5::FTSSearch;
use crate::builtin_provider::BuiltinMemoryProvider;

/// Memory provider trait
#[async_trait]
pub trait MemoryProvider: Send + Sync {
    fn name(&self) -> &str;
    fn get_tool_schemas(&self) -> Vec<Value>;
    async fn prefetch(&self, user_message: &str) -> Result<String>;
    async fn sync(&self, user_msg: &str, assistant_response: &str) -> Result<()>;
}

/// Memory manager
pub struct MemoryManager {
    providers: Vec<Box<dyn MemoryProvider>>,
    has_external: bool,
    database: Option<Arc<MemoryDatabase>>,
    search: Option<FTSSearch>,
}

impl MemoryManager {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            has_external: false,
            database: None,
            search: None,
        }
    }

    /// Initialize with database
    pub async fn with_database(db_path: PathBuf) -> Result<Self> {
        let database = Arc::new(MemoryDatabase::new(&db_path).await?);
        let search = FTSSearch::new(database.pool().clone());
        
        let mut manager = Self {
            providers: Vec::new(),
            has_external: false,
            database: Some(database),
            search: Some(search),
        };

        // Add builtin provider
        manager.add_provider(Box::new(BuiltinMemoryProvider::new()))?;

        info!("Memory manager initialized with database: {}", db_path.display());
        Ok(manager)
    }

    pub fn add_provider(&mut self, provider: Box<dyn MemoryProvider>) -> Result<()> {
        let is_builtin = provider.name() == "builtin";

        if !is_builtin && self.has_external {
            let existing = self.providers
                .iter()
                .find(|p| p.name() != "builtin")
                .map(|p| p.name())
                .unwrap_or("unknown");

            tracing::warn!(
                "Rejected memory provider '{}' — external provider '{}' is already registered",
                provider.name(),
                existing
            );
            return Ok(());
        }

        if !is_builtin {
            self.has_external = true;
        }

        tracing::info!(
            "Memory provider '{}' registered ({} tools)",
            provider.name(),
            provider.get_tool_schemas().len()
        );

        self.providers.push(provider);
        Ok(())
    }

    pub async fn prefetch_all(&self, user_message: &str) -> String {
        let mut contexts = Vec::new();

        for provider in &self.providers {
            match provider.prefetch(user_message).await {
                Ok(ctx) => {
                    if !ctx.trim().is_empty() {
                        contexts.push(ctx);
                    }
                }
                Err(e) => {
                    tracing::warn!("Memory provider {} failed: {}", provider.name(), e);
                }
            }
        }

        contexts.join("\n\n")
    }

    pub async fn sync_all(&self, user_msg: &str, assistant_response: &str) {
        for provider in &self.providers {
            if let Err(e) = provider.sync(user_msg, assistant_response).await {
                tracing::warn!("Memory sync failed for {}: {}", provider.name(), e);
            }
        }
    }

    pub fn build_system_prompt(&self) -> String {
        self.providers
            .iter()
            .filter_map(|p| {
                let schemas = p.get_tool_schemas();
                if schemas.is_empty() {
                    None
                } else {
                    Some(format!("## Memory Tools ({})\n\n{}", p.name(), serde_json::to_string(&schemas).unwrap_or_default()))
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Get database reference
    pub fn database(&self) -> Option<&Arc<MemoryDatabase>> {
        self.database.as_ref()
    }

    /// Get search engine
    pub fn search(&self) -> Option<&FTSSearch> {
        self.search.as_ref()
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new()
    }
}
