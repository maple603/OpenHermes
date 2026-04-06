//! Platform adapter trait for messaging platforms.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Incoming message from any platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    /// Platform-specific message ID
    pub message_id: String,
    /// Sender ID
    pub sender_id: String,
    /// Chat/Channel ID
    pub chat_id: String,
    /// Message text
    pub text: String,
    /// Platform name
    pub platform: String,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Outgoing message to any platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    /// Recipient chat ID
    pub chat_id: String,
    /// Message text
    pub text: String,
    /// Reply to message ID
    pub reply_to: Option<String>,
    /// Parse mode (markdown, html, plain)
    pub parse_mode: Option<String>,
}

/// Platform adapter trait
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Get platform name
    fn name(&self) -> &str;

    /// Initialize platform connection
    async fn initialize(&self, config: &PlatformConfig) -> anyhow::Result<()>;

    /// Start listening for messages
    async fn start(&self, handler: Box<dyn MessageHandler>) -> anyhow::Result<()>;

    /// Send a message
    async fn send_message(&self, message: &OutgoingMessage) -> anyhow::Result<String>;

    /// Stop platform connection
    async fn stop(&self) -> anyhow::Result<()>;

    /// Check if platform is connected
    fn is_connected(&self) -> bool;
}

/// Platform configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    /// Platform name
    pub platform: String,
    /// API token/key
    pub token: String,
    /// Additional configuration
    pub options: HashMap<String, serde_json::Value>,
}

/// Message handler trait
#[async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle incoming message
    async fn handle_message(&self, message: IncomingMessage) -> anyhow::Result<OutgoingMessage>;
}
