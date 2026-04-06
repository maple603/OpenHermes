//! Clarify tool for requesting user clarification.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::registry::{Tool, REGISTRY};

/// Clarify tool - asks user for clarification
pub struct ClarifyTool;

#[async_trait]
impl Tool for ClarifyTool {
    fn name(&self) -> &str {
        "clarify"
    }

    fn toolset(&self) -> &str {
        "assistant"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "clarify",
            "description": "Ask the user for clarification when the request is ambiguous or missing important details. This tool pauses execution and waits for user input.",
            "parameters": {
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The clarification question to ask the user"
                    },
                    "options": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Optional list of suggested answers"
                    }
                },
                "required": ["question"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let question = args["question"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: question"))?;

        let options = args["options"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<String>>()
        });

        // In a real implementation, this would wait for user input
        // For now, return a message indicating clarification is needed
        let options_for_json = options.clone();
        let response = if let Some(opts) = options {
            if !opts.is_empty() {
                format!(
                    "Clarification needed:\n{}\n\nSuggested options:\n{}",
                    question,
                    opts.iter()
                        .enumerate()
                        .map(|(i, opt)| format!("{}. {}", i + 1, opt))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            } else {
                format!("Clarification needed:\n{}", question)
            }
        } else {
            format!("Clarification needed:\n{}", question)
        };

        Ok(serde_json::json!({
            "type": "clarification_request",
            "question": question,
            "options": options_for_json.unwrap_or_default(),
            "message": response,
            "status": "waiting_for_user"
        }).to_string())
    }
}

/// Register clarify tool
pub fn register_tools() {
    REGISTRY.register(Arc::new(ClarifyTool));
}
