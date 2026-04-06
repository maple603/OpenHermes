//! Context compressor for managing conversation length.

use anyhow::Result;
use async_openai::types::ChatCompletionRequestMessage;
use tracing::info;

/// Estimate tokens in a message (rough approximation)
fn estimate_tokens(text: &str) -> usize {
    // Rough estimate: ~4 characters per token for English
    // This is a simplification - production should use tiktoken
    text.len() / 4
}

/// Estimate total tokens in messages
fn estimate_total_tokens(messages: &[ChatCompletionRequestMessage]) -> usize {
    messages.iter().map(|msg| {
        match msg {
            ChatCompletionRequestMessage::System(m) => {
                match &m.content {
                    async_openai::types::ChatCompletionRequestSystemMessageContent::Text(t) => estimate_tokens(t),
                    async_openai::types::ChatCompletionRequestSystemMessageContent::Array(_) => 0,
                }
            }
            ChatCompletionRequestMessage::User(m) => {
                match &m.content {
                    async_openai::types::ChatCompletionRequestUserMessageContent::Text(t) => estimate_tokens(t),
                    async_openai::types::ChatCompletionRequestUserMessageContent::Array(_) => 0,
                }
            }
            ChatCompletionRequestMessage::Assistant(m) => {
                m.content.as_ref().map(|c| match c {
                    async_openai::types::ChatCompletionRequestAssistantMessageContent::Text(t) => estimate_tokens(t),
                    async_openai::types::ChatCompletionRequestAssistantMessageContent::Array(_) => 0,
                }).unwrap_or(0)
            }
            ChatCompletionRequestMessage::Tool(m) => {
                match &m.content {
                    async_openai::types::ChatCompletionRequestToolMessageContent::Text(t) => estimate_tokens(t),
                    async_openai::types::ChatCompletionRequestToolMessageContent::Array(_) => 0,
                }
            }
            _ => 0,
        }
    }).sum()
}

/// Context compressor
pub struct ContextCompressor {
    compression_threshold: usize,
    target_context_size: usize,
}

impl ContextCompressor {
    pub fn new() -> Self {
        Self {
            compression_threshold: openhermes_constants::DEFAULT_COMPRESSION_THRESHOLD,
            target_context_size: openhermes_constants::DEFAULT_TARGET_CONTEXT_SIZE,
        }
    }

    /// Check if compression is needed and compress if necessary
    pub async fn compress_if_needed(
        &self,
        messages: &mut Vec<ChatCompletionRequestMessage>,
    ) -> Result<bool> {
        let current_size = estimate_total_tokens(messages);
        
        if current_size > self.compression_threshold {
            info!(
                current_tokens = current_size,
                threshold = self.compression_threshold,
                "Context exceeds threshold, compressing..."
            );
            self.compress(messages).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Compress conversation context
    async fn compress(
        &self,
        messages: &mut Vec<ChatCompletionRequestMessage>,
    ) -> Result<()> {
        if messages.len() <= 3 {
            // Keep system prompt and at least 1 exchange
            info!("Context too short to compress");
            return Ok(());
        }

        let system_msg = messages[0].clone();
        
        // Keep the last N messages that fit within target size
        let mut kept_messages = vec![system_msg];
        let mut current_tokens = estimate_tokens(
            match &kept_messages[0] {
                ChatCompletionRequestMessage::System(m) => match &m.content {
                    async_openai::types::ChatCompletionRequestSystemMessageContent::Text(t) => t,
                    async_openai::types::ChatCompletionRequestSystemMessageContent::Array(_) => "",
                }
                _ => "",
            }
        );

        // Work backwards from the end
        let mut compressed_count = 0;
        for msg in messages.iter().rev() {
            let msg_tokens = match msg {
                ChatCompletionRequestMessage::System(m) => match &m.content {
                    async_openai::types::ChatCompletionRequestSystemMessageContent::Text(t) => estimate_tokens(t),
                    async_openai::types::ChatCompletionRequestSystemMessageContent::Array(_) => 0,
                }
                ChatCompletionRequestMessage::User(m) => match &m.content {
                    async_openai::types::ChatCompletionRequestUserMessageContent::Text(t) => estimate_tokens(t),
                    async_openai::types::ChatCompletionRequestUserMessageContent::Array(_) => 0,
                }
                ChatCompletionRequestMessage::Assistant(m) => m.content.as_ref().map(|c| match c {
                    async_openai::types::ChatCompletionRequestAssistantMessageContent::Text(t) => estimate_tokens(t),
                    async_openai::types::ChatCompletionRequestAssistantMessageContent::Array(_) => 0,
                }).unwrap_or(0),
                ChatCompletionRequestMessage::Tool(m) => match &m.content {
                    async_openai::types::ChatCompletionRequestToolMessageContent::Text(t) => estimate_tokens(t),
                    async_openai::types::ChatCompletionRequestToolMessageContent::Array(_) => 0,
                }
                _ => 0,
            };

            if current_tokens + msg_tokens <= self.target_context_size {
                kept_messages.insert(1, msg.clone());
                current_tokens += msg_tokens;
            } else {
                compressed_count += 1;
            }
        }

        // If we compressed anything, add a summary message
        if compressed_count > 0 {
            let summary = format!(
                "[Previous conversation compressed: {} messages summarized to save context tokens]",
                compressed_count
            );
            
            let summary_msg = ChatCompletionRequestMessage::System(
                async_openai::types::ChatCompletionRequestSystemMessage {
                    content: async_openai::types::ChatCompletionRequestSystemMessageContent::Text(summary),
                    name: None,
                }
            );
            
            kept_messages.insert(1, summary_msg);
            
            info!(
                compressed_messages = compressed_count,
                remaining_tokens = current_tokens,
                "Context compressed successfully"
            );
        }

        *messages = kept_messages;
        Ok(())
    }
}

impl Default for ContextCompressor {
    fn default() -> Self {
        Self::new()
    }
}
