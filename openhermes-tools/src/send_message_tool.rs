//! Send message tool — cross-platform messaging via gateway adapters.
//!
//! Sends messages to Telegram, Discord, or other configured platforms.
//! Gated on gateway being active.

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::registry::Tool;

/// Whether the gateway is running and available for sending.
static GATEWAY_AVAILABLE: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

/// Set gateway availability status.
pub fn set_gateway_available(available: bool) {
    GATEWAY_AVAILABLE.store(available, Ordering::Relaxed);
    info!(available = available, "Gateway availability updated");
}

/// Registered message targets (populated by gateway on startup).
static TARGETS: Lazy<parking_lot::Mutex<Vec<MessageTarget>>> =
    Lazy::new(|| parking_lot::Mutex::new(Vec::new()));

/// A messaging target (chat, channel, etc.).
#[derive(Debug, Clone)]
pub struct MessageTarget {
    pub platform: String,
    pub target_id: String,
    pub name: String,
}

/// Register a messaging target.
pub fn register_target(platform: &str, target_id: &str, name: &str) {
    let mut targets = TARGETS.lock();
    targets.push(MessageTarget {
        platform: platform.to_string(),
        target_id: target_id.to_string(),
        name: name.to_string(),
    });
}

/// Send message tool.
pub struct SendMessageTool;

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "send_message"
    }

    fn toolset(&self) -> &str {
        "messaging"
    }

    fn check_fn(&self) -> bool {
        GATEWAY_AVAILABLE.load(Ordering::Relaxed)
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "send_message",
            "description": "Send a message to a specific platform and target (chat/channel). Requires gateway to be running. Use list_targets action to see available destinations.",
            "parameters": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "description": "Action: 'send' or 'list_targets'",
                        "enum": ["send", "list_targets"],
                        "default": "send"
                    },
                    "platform": {
                        "type": "string",
                        "description": "Platform: 'telegram', 'discord', etc."
                    },
                    "target": {
                        "type": "string",
                        "description": "Target chat/channel ID or name"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message content to send"
                    },
                    "media_path": {
                        "type": "string",
                        "description": "Optional path to a media file to attach"
                    }
                },
                "required": ["action"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let action = args["action"].as_str().unwrap_or("send");

        match action {
            "list_targets" => {
                let targets = TARGETS.lock();
                let entries: Vec<Value> = targets
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "platform": t.platform,
                            "target_id": t.target_id,
                            "name": t.name,
                        })
                    })
                    .collect();
                Ok(serde_json::json!({
                    "success": true,
                    "targets": entries,
                    "count": entries.len(),
                }).to_string())
            }
            "send" => {
                let platform = args["platform"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: platform"))?;
                let target = args["target"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: target"))?;
                let message = args["message"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?;
                let _media_path = args["media_path"].as_str();

                if !GATEWAY_AVAILABLE.load(Ordering::Relaxed) {
                    return Ok(serde_json::json!({
                        "error": "Gateway is not running. Start with `openhermes gateway`.",
                        "success": false,
                    }).to_string());
                }

                debug!(platform = platform, target = target, "Sending message");

                // Delegate to platform-specific sender
                match platform.to_lowercase().as_str() {
                    "telegram" => send_telegram(target, message).await,
                    "discord" => send_discord(target, message).await,
                    _ => Ok(serde_json::json!({
                        "error": format!("Unsupported platform: {}. Supported: telegram, discord", platform),
                        "success": false,
                    }).to_string()),
                }
            }
            _ => Ok(serde_json::json!({
                "error": format!("Unknown action: {}. Use 'send' or 'list_targets'.", action),
                "success": false,
            }).to_string()),
        }
    }
}

/// Send a message via Telegram (uses bot API).
async fn send_telegram(chat_id: &str, message: &str) -> Result<String> {
    let token = match std::env::var("TELEGRAM_BOT_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            return Ok(serde_json::json!({
                "error": "TELEGRAM_BOT_TOKEN not set",
                "success": false,
            }).to_string());
        }
    };

    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        token
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": message,
            "parse_mode": "Markdown",
        }))
        .send()
        .await?;

    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or_default();

    if status.is_success() && body["ok"].as_bool() == Some(true) {
        Ok(serde_json::json!({
            "success": true,
            "platform": "telegram",
            "message_id": body["result"]["message_id"],
        }).to_string())
    } else {
        let error = redact_error(&body.to_string());
        warn!(error = %error, "Telegram send failed");
        Ok(serde_json::json!({
            "success": false,
            "error": error,
        }).to_string())
    }
}

/// Send a message via Discord (uses webhook or bot API).
async fn send_discord(channel_id: &str, message: &str) -> Result<String> {
    let token = match std::env::var("DISCORD_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            return Ok(serde_json::json!({
                "error": "DISCORD_TOKEN not set",
                "success": false,
            }).to_string());
        }
    };

    let url = format!(
        "https://discord.com/api/v10/channels/{}/messages",
        channel_id
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bot {}", token))
        .json(&serde_json::json!({
            "content": message,
        }))
        .send()
        .await?;

    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or_default();

    if status.is_success() {
        Ok(serde_json::json!({
            "success": true,
            "platform": "discord",
            "message_id": body["id"],
        }).to_string())
    } else {
        let error = redact_error(&body.to_string());
        warn!(error = %error, "Discord send failed");
        Ok(serde_json::json!({
            "success": false,
            "error": error,
        }).to_string())
    }
}

/// Redact potential secret leakage from error messages.
fn redact_error(msg: &str) -> String {
    let mut result = msg.to_string();
    // Redact bot tokens that might appear in error messages
    for var in &["TELEGRAM_BOT_TOKEN", "DISCORD_TOKEN"] {
        if let Ok(val) = std::env::var(var) {
            if !val.is_empty() {
                result = result.replace(&val, "[REDACTED]");
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_availability() {
        assert!(!GATEWAY_AVAILABLE.load(Ordering::Relaxed));
        set_gateway_available(true);
        assert!(GATEWAY_AVAILABLE.load(Ordering::Relaxed));
        set_gateway_available(false);
    }

    #[test]
    fn test_register_target() {
        register_target("telegram", "12345", "Test Chat");
        let targets = TARGETS.lock();
        assert!(targets.iter().any(|t| t.target_id == "12345"));
    }

    #[test]
    fn test_tool_check_fn() {
        set_gateway_available(false);
        let tool = SendMessageTool;
        assert!(!tool.check_fn());
        set_gateway_available(true);
        assert!(tool.check_fn());
        set_gateway_available(false);
    }

    #[test]
    fn test_redact_error() {
        let msg = "normal error message without secrets";
        assert_eq!(redact_error(msg), msg);
    }
}
