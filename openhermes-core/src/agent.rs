//! AI Agent with tool calling capabilities.

use std::sync::Arc;

use anyhow::{Context, Result};
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, CreateChatCompletionRequest,
};
use async_openai::Client;
use parking_lot::Mutex;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use openhermes_config::HermesConfig;
use openhermes_tools::{discover_tools, init_tools, ToolRegistry};

use super::budget::IterationBudget;
use super::context_compressor::ContextCompressor;
use super::prompt_builder::build_system_prompt;

/// Main AI Agent structure
pub struct AIAgent {
    client: Client<OpenAIConfig>,
    model: String,
    max_iterations: usize,
    session_id: String,
    conversation_history: Arc<Mutex<Vec<ChatCompletionRequestMessage>>>,
    iteration_budget: Arc<IterationBudget>,
    context_compressor: Option<ContextCompressor>,
    platform: String,
}

impl AIAgent {
    /// Create a new agent from configuration
    pub async fn from_config(config: &HermesConfig) -> Result<Self> {
        // Discover and register tools
        discover_tools();
        
        // Initialize built-in tools
        init_tools();

        // Create OpenAI-compatible client
        let client = create_client(config).await?;

        let model = config.agent.model.clone();
        let max_iterations = if config.agent.max_iterations > 0 {
            config.agent.max_iterations
        } else {
            openhermes_constants::DEFAULT_MAX_ITERATIONS
        };

        // Create context compressor if needed
        let compressor = if config.agent.max_iterations > 0 {
            Some(ContextCompressor::new())
        } else {
            None
        };

        Ok(Self {
            client,
            model,
            max_iterations,
            session_id: uuid::Uuid::new_v4().to_string(),
            conversation_history: Arc::new(Mutex::new(Vec::new())),
            iteration_budget: Arc::new(IterationBudget::new(max_iterations)),
            context_compressor: compressor,
            platform: "cli".to_string(),
        })
    }

    /// Simple chat interface - returns final response string
    pub async fn chat(&self, message: &str) -> Result<String> {
        let result = self.run_conversation(message).await?;
        Ok(result.final_response)
    }

    /// Full conversation interface with tool calling loop
    pub async fn run_conversation(&self, user_message: &str) -> Result<ConversationResult> {
        let mut history = self.conversation_history.lock();

        // Add user message
        history.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(user_message.to_string()),
                name: None,
            },
        ));

        // Build system prompt
        let system_prompt = build_system_prompt(&self.platform, None, None, None);
        let mut messages = vec![ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(system_prompt),
                name: None,
            },
        )];

        messages.extend(history.clone());

        let mut api_call_count = 0;
        let mut tool_calls_total = 0;

        while api_call_count < self.max_iterations && self.iteration_budget.remaining() > 0 {
            // Get tool definitions
            let tool_definitions = openhermes_tools::get_available_definitions();

            // Create request
            let request = CreateChatCompletionRequest {
                model: self.model.clone(),
                messages: messages.clone(),
                tools: if tool_definitions.is_empty() {
                    None
                } else {
                    Some(tool_definitions)
                },
                ..Default::default()
            };

            // Call LLM API
            debug!("Calling LLM API (iteration {})", api_call_count + 1);
            let response = self.client.chat().create(request).await
                .with_context(|| "Failed to get response from LLM")?;

            api_call_count += 1;

            // Check for tool calls
            let choice = &response.choices[0];
            if let Some(tool_calls) = &choice.message.tool_calls {
                info!("LLM requested {} tool calls", tool_calls.len());
                tool_calls_total += tool_calls.len();

                // Execute tools in parallel using JoinSet
                let mut join_set = JoinSet::new();
                let tool_calls_clone = tool_calls.clone();
                
                // Spawn all tool calls
                for tool_call in &tool_calls_clone {
                    let tool_call_id = tool_call.id.clone();
                    let tool_name = tool_call.function.name.clone();
                    let tool_args = tool_call.function.arguments.clone();
                    
                    join_set.spawn(async move {
                        debug!("Executing tool: {}", tool_name);
                        let result = openhermes_tools::handle_function_call(&tool_name, &tool_args).await;
                        (tool_call_id, tool_name, result)
                    });
                }

                // Collect results and maintain order
                let mut tool_results: Vec<(String, String, String)> = Vec::new();
                while let Some(result) = join_set.join_next().await {
                    match result {
                        Ok((tool_call_id, tool_name, tool_result)) => {
                            let result_content = match tool_result {
                                Ok(output) => output,
                                Err(e) => {
                                    warn!("Tool {} failed: {}", tool_name, e);
                                    serde_json::json!({
                                        "error": e.to_string(),
                                        "success": false
                                    })
                                    .to_string()
                                }
                            };
                            tool_results.push((tool_call_id, tool_name, result_content));
                        }
                        Err(e) => {
                            warn!("Tool execution panicked: {}", e);
                        }
                    }
                }

                // Add all tool results to messages
                for (tool_call_id, _tool_name, result_content) in tool_results {
                    messages.push(ChatCompletionRequestMessage::Tool(
                        async_openai::types::ChatCompletionRequestToolMessage {
                            content: async_openai::types::ChatCompletionRequestToolMessageContent::Text(result_content),
                            tool_call_id,
                        },
                    ));
                }
            } else {
                // No tool calls - we have the final response
                let final_response = choice.message.content.clone().unwrap_or_default();

                // Update conversation history
                {
                    let mut history = self.conversation_history.lock();
                    // Convert response message to request message
                    let assistant_msg = ChatCompletionRequestMessage::Assistant(
                        async_openai::types::ChatCompletionRequestAssistantMessage {
                            content: choice.message.content.clone().map(
                                async_openai::types::ChatCompletionRequestAssistantMessageContent::Text
                            ),
                            tool_calls: choice.message.tool_calls.clone(),
                            ..Default::default()
                        },
                    );
                    history.push(assistant_msg);
                }

                info!(
                    "Conversation completed: {} API calls, {} tool calls",
                    api_call_count, tool_calls_total
                );

                return Ok(ConversationResult {
                    final_response,
                    messages: messages.len(),
                    api_calls: api_call_count,
                    tool_calls: tool_calls_total,
                });
            }
        }

        anyhow::bail!("Max iterations reached ({} iterations, {} tool calls)", api_call_count, tool_calls_total)
    }

    /// Reset conversation history
    pub fn reset(&self) {
        let mut history = self.conversation_history.lock();
        history.clear();
        info!("Conversation history cleared");
    }

    /// Get conversation history length
    pub fn history_len(&self) -> usize {
        self.conversation_history.lock().len()
    }

    /// Get session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get remaining iterations
    pub fn remaining_iterations(&self) -> usize {
        self.iteration_budget.remaining()
    }
}

/// Result of a conversation
pub struct ConversationResult {
    pub final_response: String,
    pub messages: usize,
    pub api_calls: usize,
    pub tool_calls: usize,
}

/// Create OpenAI-compatible client from config
async fn create_client(_config: &HermesConfig) -> Result<Client<OpenAIConfig>> {
    // Check for custom base URL
    if let Ok(base_url) = std::env::var("OPENAI_BASE_URL") {
        let config = OpenAIConfig::default().with_api_base(&base_url);
        return Ok(async_openai::Client::with_config(config));
    }

    // Get API key from environment
    let api_key = std::env::var("OPENAI_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .or_else(|_| std::env::var("OPENROUTER_API_KEY"))
        .context("No API key found. Set OPENAI_API_KEY, ANTHROPIC_API_KEY, or OPENROUTER_API_KEY")?;

    let config = OpenAIConfig::default().with_api_key(&api_key);
    Ok(async_openai::Client::with_config(config))
}
