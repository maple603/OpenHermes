//! Discord platform adapter.

use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};

use crate::platform::{PlatformAdapter, PlatformConfig, IncomingMessage, OutgoingMessage, MessageHandler};

/// Discord bot adapter
pub struct DiscordAdapter {
    /// HTTP client
    client: Client,
    /// Bot token
    token: String,
    /// Base URL
    base_url: String,
    /// Connection status
    connected: AtomicBool,
}

impl DiscordAdapter {
    /// Create new Discord adapter
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            token: String::new(),
            base_url: "https://discord.com/api/v10".to_string(),
            connected: AtomicBool::new(false),
        }
    }
}

impl Default for DiscordAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformAdapter for DiscordAdapter {
    fn name(&self) -> &str {
        "discord"
    }

    async fn initialize(&self, config: &PlatformConfig) -> anyhow::Result<()> {
        info!("Initializing Discord bot");

        if config.platform != "discord" {
            return Err(anyhow::anyhow!("Invalid platform: {}", config.platform));
        }

        info!("Discord bot initialized");
        Ok(())
    }

    async fn start(&self, handler: Box<dyn MessageHandler>) -> anyhow::Result<()> {
        info!("Starting Discord bot gateway");
        
        // TODO: Implement Discord gateway connection
        // Discord uses WebSocket for real-time message receiving
        // This requires the discord gateway protocol
        
        warn!("Discord gateway not yet implemented");
        Ok(())
    }

    async fn send_message(&self, message: &OutgoingMessage) -> anyhow::Result<String> {
        let url = format!("{}/channels/{}/messages", self.base_url, message.chat_id);
        
        let mut body = json!({
            "content": message.text,
        });

        // Add optional fields
        if let Some(reply_to) = &message.reply_to {
            body["message_reference"] = json!({
                "message_id": reply_to
            });
        }

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bot {}", self.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Discord API error {}: {}", status, error_text));
        }

        let result: serde_json::Value = response.json().await?;
        
        let message_id = result["id"]
            .as_str()
            .unwrap_or("0")
            .to_string();

        info!(message_id = %message_id, "Discord message sent");
        Ok(message_id)
    }

    async fn stop(&self) -> anyhow::Result<()> {
        info!("Stopping Discord bot");
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}
