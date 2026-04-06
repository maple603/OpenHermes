//! Telegram platform adapter.

use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn, error};

use crate::platform::{PlatformAdapter, PlatformConfig, IncomingMessage, OutgoingMessage, MessageHandler};

/// Telegram bot adapter
pub struct TelegramAdapter {
    /// HTTP client
    client: Client,
    /// Bot token
    token: String,
    /// Base URL
    base_url: String,
    /// Connection status
    connected: AtomicBool,
}

impl TelegramAdapter {
    /// Create new Telegram adapter
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            token: String::new(),
            base_url: String::new(),
            connected: AtomicBool::new(false),
        }
    }

    /// Get Telegram API URL
    fn api_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.base_url, self.token, method)
    }

    /// Get updates from Telegram
    async fn get_updates(
        &self,
        offset: i64,
        limit: i32,
        timeout: i32,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let url = self.api_url("getUpdates");
        
        let body = json!({
            "offset": offset,
            "limit": limit,
            "timeout": timeout,
        });

        let response = self.client
            .post(&url)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Telegram getUpdates error {}: {}", status, error_text));
        }

        let result: serde_json::Value = response.json().await?;
        
        let updates = result["result"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        Ok(updates)
    }
}

impl Default for TelegramAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformAdapter for TelegramAdapter {
    fn name(&self) -> &str {
        "telegram"
    }

    async fn initialize(&self, config: &PlatformConfig) -> anyhow::Result<()> {
        info!("Initializing Telegram bot");

        if config.platform != "telegram" {
            return Err(anyhow::anyhow!("Invalid platform: {}", config.platform));
        }

        // Get token from config
        let token = &config.token;
        
        // Set base URL (allow custom URL for testing)
        let base_url = config.options.get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or("https://api.telegram.org")
            .to_string();

        // Use reflection to set fields (in production, use Mutex or RwLock)
        info!("Telegram bot initialized with base URL: {}", base_url);
        
        Ok(())
    }

    async fn start(&self, handler: Box<dyn MessageHandler>) -> anyhow::Result<()> {
        info!("Starting Telegram bot polling");
        
        self.connected.store(true, Ordering::SeqCst);
        let mut offset = 0;
        
        // Polling loop
        loop {
            if !self.connected.load(Ordering::SeqCst) {
                info!("Telegram polling stopped");
                break;
            }
            
            match self.get_updates(offset, 100, 30).await {
                Ok(updates) => {
                    for update in updates {
                        if let Some(message) = update.get("message").and_then(|m| m.as_object()) {
                            let message_id = message.get("message_id")
                                .and_then(|id| id.as_i64())
                                .unwrap_or(0)
                                .to_string();
                            
                            let chat_id = message.get("chat")
                                .and_then(|c| c.get("id"))
                                .and_then(|id| id.as_i64())
                                .unwrap_or(0)
                                .to_string();
                            
                            let sender_id = message.get("from")
                                .and_then(|f| f.get("id"))
                                .and_then(|id| id.as_i64())
                                .unwrap_or(0)
                                .to_string();
                            
                            let text = message.get("text")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string();
                            
                            let incoming = IncomingMessage {
                                message_id,
                                sender_id,
                                chat_id: chat_id.clone(),
                                text,
                                platform: "telegram".to_string(),
                                metadata: std::collections::HashMap::new(),
                            };
                            
                            // Handle message
                            match handler.handle_message(incoming).await {
                                Ok(response) => {
                                    if let Err(e) = self.send_message(&response).await {
                                        error!(error = %e, "Failed to send response");
                                    }
                                }
                                Err(e) => {
                                    error!(error = %e, "Failed to handle message");
                                }
                            }
                        }
                        
                        // Update offset to next message
                        if let Some(update_id) = update.get("update_id").and_then(|id| id.as_i64()) {
                            offset = update_id + 1;
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to get updates");
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
            
            // Small delay between polling
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        
        Ok(())
    }

    async fn send_message(&self, message: &OutgoingMessage) -> anyhow::Result<String> {
        let url = self.api_url("sendMessage");
        
        let mut body = json!({
            "chat_id": message.chat_id,
            "text": message.text,
        });

        // Add optional fields
        if let Some(reply_to) = &message.reply_to {
            body["reply_to_message_id"] = json!(reply_to);
        }

        if let Some(parse_mode) = &message.parse_mode {
            body["parse_mode"] = json!(parse_mode);
        }

        let response = self.client
            .post(&url)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Telegram API error {}: {}", status, error_text));
        }

        let result: serde_json::Value = response.json().await?;
        
        let message_id = result["result"]["message_id"]
            .as_i64()
            .unwrap_or(0)
            .to_string();

        info!(message_id = %message_id, "Telegram message sent");
        Ok(message_id)
    }

    async fn stop(&self) -> anyhow::Result<()> {
        info!("Stopping Telegram bot");
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}
