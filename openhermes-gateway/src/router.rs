//! Message router for distributing messages to platform adapters.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

use crate::platform::{PlatformAdapter, PlatformConfig, IncomingMessage, OutgoingMessage, MessageHandler};

/// Message router
pub struct MessageRouter {
    /// Registered platform adapters (platform_name -> adapter)
    adapters: Arc<RwLock<HashMap<String, Box<dyn PlatformAdapter>>>>,
    /// Message handler
    handler: Option<Arc<dyn MessageHandler>>,
}

impl MessageRouter {
    /// Create new message router
    pub fn new() -> Self {
        Self {
            adapters: Arc::new(RwLock::new(HashMap::new())),
            handler: None,
        }
    }

    /// Register a platform adapter
    pub async fn register_platform(&self, adapter: Box<dyn PlatformAdapter>) {
        let name = adapter.name().to_string();
        let mut adapters = self.adapters.write().await;
        adapters.insert(name.clone(), adapter);
        info!(platform = %name, "Platform adapter registered");
    }

    /// Set message handler
    pub fn set_handler(&mut self, handler: Arc<dyn MessageHandler>) {
        self.handler = Some(handler);
    }

    /// Initialize all platforms
    pub async fn initialize_platforms(
        &self,
        configs: Vec<PlatformConfig>,
    ) -> anyhow::Result<()> {
        for config in configs {
            let adapters = self.adapters.read().await;
            
            if let Some(adapter) = adapters.get(&config.platform) {
                adapter.initialize(&config).await?;
                info!(platform = %config.platform, "Platform initialized");
            } else {
                warn!(platform = %config.platform, "Platform adapter not found");
            }
        }
        
        Ok(())
    }

    /// Start all platforms
    pub async fn start_all(&self) -> anyhow::Result<()> {
        let handler = self.handler.clone()
            .ok_or_else(|| anyhow::anyhow!("No message handler set"))?;
        
        let adapters = self.adapters.read().await;
        
        for (name, _adapter) in adapters.iter() {
            // Clone for async task
            let _handler_clone = handler.clone();
            let _adapter_clone: Box<dyn PlatformAdapter> = match name.as_str() {
                // In production, implement proper clone for each adapter
                _ => return Err(anyhow::anyhow!("Platform {} not supported", name)),
            };
            
            // Start in background task (unreachable for now, kept for future implementation)
            #[allow(unreachable_code)]
            {
                tokio::spawn(async move {
                    if let Err(e) = _adapter_clone.start(Box::new(MessageHandlerWrapper(_handler_clone))).await {
                        error!(platform = %name, error = %e, "Platform start failed");
                    }
                });
            }
        }
        
        info!("All platforms started");
        Ok(())
    }

    /// Send message to specific platform
    pub async fn send_to_platform(
        &self,
        platform: &str,
        message: &OutgoingMessage,
    ) -> anyhow::Result<String> {
        let adapters = self.adapters.read().await;
        
        if let Some(adapter) = adapters.get(platform) {
            adapter.send_message(message).await
        } else {
            Err(anyhow::anyhow!("Platform {} not found", platform))
        }
    }

    /// Broadcast message to all platforms
    pub async fn broadcast(&self, message: &OutgoingMessage) -> anyhow::Result<Vec<(String, String)>> {
        let adapters = self.adapters.read().await;
        let mut results = Vec::new();
        
        for (name, adapter) in adapters.iter() {
            match adapter.send_message(message).await {
                Ok(msg_id) => {
                    results.push((name.clone(), msg_id));
                }
                Err(e) => {
                    error!(platform = %name, error = %e, "Broadcast failed");
                }
            }
        }
        
        Ok(results)
    }

    /// Get registered platform count
    pub async fn platform_count(&self) -> usize {
        let adapters = self.adapters.read().await;
        adapters.len()
    }

    /// Get list of registered platforms
    pub async fn list_platforms(&self) -> Vec<String> {
        let adapters = self.adapters.read().await;
        adapters.keys().cloned().collect()
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper to adapt Arc<MessageHandler> to Box<MessageHandler>
struct MessageHandlerWrapper(Arc<dyn MessageHandler>);

#[async_trait::async_trait]
impl MessageHandler for MessageHandlerWrapper {
    async fn handle_message(&self, message: IncomingMessage) -> anyhow::Result<OutgoingMessage> {
        self.0.handle_message(message).await
    }
}
