//! Delegate tool — subagent spawning with isolated context.
//!
//! Spawns child agents with restricted toolsets to handle subtasks.
//! Supports single-task and batch modes. Child agents cannot recurse
//! (delegate_task is blocked), and depth is limited to MAX_DEPTH=2.

use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::registry::Tool;

/// Maximum delegation depth.
const MAX_DEPTH: usize = 2;

/// Maximum iterations for delegated agents.
#[allow(dead_code)]
const DELEGATION_MAX_ITERATIONS: usize = 50;

/// Tools that are blocked in delegated contexts.
const BLOCKED_TOOLS: &[&str] = &[
    "delegate_task",
    "clarify",
    "memory_write",
    "send_message",
    "execute_code",
    "cronjob",
];

/// Current delegation depth (tracked globally via thread-local-like atomic).
static CURRENT_DEPTH: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));

/// Delegate task tool.
pub struct DelegateTaskTool;

#[async_trait]
impl Tool for DelegateTaskTool {
    fn name(&self) -> &str {
        "delegate_task"
    }

    fn toolset(&self) -> &str {
        "delegation"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "delegate_task",
            "description": "Delegate a task to a sub-agent with its own isolated context. The sub-agent has access to most tools but cannot delegate further, access memory writes, or send messages. Use for parallel subtasks or when you need isolated execution.",
            "parameters": {
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Clear description of the task to delegate"
                    },
                    "context": {
                        "type": "string",
                        "description": "Additional context to provide to the sub-agent (files, background info, etc.)"
                    },
                    "mode": {
                        "type": "string",
                        "description": "Execution mode: 'single' for one task, 'batch' for parallel tasks",
                        "enum": ["single", "batch"],
                        "default": "single"
                    },
                    "tasks": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of tasks for batch mode (each runs as a separate sub-agent)"
                    }
                },
                "required": ["task"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let mode = args["mode"].as_str().unwrap_or("single");

        // Check depth limit
        let depth = CURRENT_DEPTH.load(Ordering::Relaxed);
        if depth >= MAX_DEPTH {
            return Ok(serde_json::json!({
                "error": format!("Maximum delegation depth ({}) reached. Cannot delegate further.", MAX_DEPTH),
                "success": false
            }).to_string());
        }

        match mode {
            "single" => self.execute_single(&args).await,
            "batch" => self.execute_batch(&args).await,
            _ => Ok(serde_json::json!({
                "error": format!("Unknown mode: {}. Use 'single' or 'batch'.", mode),
                "success": false
            }).to_string()),
        }
    }
}

impl DelegateTaskTool {
    async fn execute_single(&self, args: &Value) -> Result<String> {
        let task = args["task"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: task"))?;
        let context = args["context"].as_str().unwrap_or("");

        info!(task = task, "Delegating single task to sub-agent");

        // Increment depth
        CURRENT_DEPTH.fetch_add(1, Ordering::Relaxed);

        let result = run_delegated_task(task, context).await;

        // Decrement depth
        CURRENT_DEPTH.fetch_sub(1, Ordering::Relaxed);

        match result {
            Ok(output) => Ok(serde_json::json!({
                "success": true,
                "task": task,
                "result": output,
            }).to_string()),
            Err(e) => Ok(serde_json::json!({
                "success": false,
                "task": task,
                "error": e.to_string(),
            }).to_string()),
        }
    }

    async fn execute_batch(&self, args: &Value) -> Result<String> {
        let tasks = match args["tasks"].as_array() {
            Some(arr) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>(),
            None => {
                return Ok(serde_json::json!({
                    "error": "Batch mode requires 'tasks' array parameter.",
                    "success": false
                }).to_string());
            }
        };

        if tasks.is_empty() {
            return Ok(serde_json::json!({
                "error": "Tasks array is empty.",
                "success": false
            }).to_string());
        }

        let context = args["context"].as_str().unwrap_or("");
        info!(count = tasks.len(), "Delegating batch tasks to sub-agents");

        // Increment depth
        CURRENT_DEPTH.fetch_add(1, Ordering::Relaxed);

        // Run all tasks in parallel
        let mut join_set = tokio::task::JoinSet::new();
        for task in tasks.clone() {
            let ctx = context.to_string();
            join_set.spawn(async move { (task.clone(), run_delegated_task(&task, &ctx).await) });
        }

        let mut results = Vec::new();
        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok((task, Ok(output))) => {
                    results.push(serde_json::json!({
                        "task": task,
                        "success": true,
                        "result": output,
                    }));
                }
                Ok((task, Err(e))) => {
                    results.push(serde_json::json!({
                        "task": task,
                        "success": false,
                        "error": e.to_string(),
                    }));
                }
                Err(e) => {
                    warn!("Batch task panicked: {}", e);
                    results.push(serde_json::json!({
                        "task": "unknown",
                        "success": false,
                        "error": format!("Task panicked: {}", e),
                    }));
                }
            }
        }

        // Decrement depth
        CURRENT_DEPTH.fetch_sub(1, Ordering::Relaxed);

        let success_count = results.iter().filter(|r| r["success"] == true).count();
        Ok(serde_json::json!({
            "success": true,
            "mode": "batch",
            "total": tasks.len(),
            "succeeded": success_count,
            "failed": tasks.len() - success_count,
            "results": results,
        }).to_string())
    }
}

/// Run a single delegated task using auxiliary LLM.
///
/// In a full implementation, this would spawn an AIAgent child with
/// restricted tools. For now, we use the auxiliary client with a focused
/// system prompt that instructs single-shot task completion.
async fn run_delegated_task(task: &str, context: &str) -> Result<String> {
    debug!(task = task, "Running delegated task");

    let prompt = if context.is_empty() {
        format!(
            "You are a focused sub-agent. Complete this task directly and concisely.\n\n\
             Task: {}\n\n\
             Respond with ONLY the task result. Do not ask questions.",
            task
        )
    } else {
        format!(
            "You are a focused sub-agent. Complete this task directly and concisely.\n\n\
             Context: {}\n\n\
             Task: {}\n\n\
             Respond with ONLY the task result. Do not ask questions.",
            context, task
        )
    };

    openhermes_tools_llm::call_llm(&prompt, Some("delegation"), Some(4096)).await
}

/// Alias for the crate's LLM client.
mod openhermes_tools_llm {
    pub use crate::llm_client::call_llm;
}

/// Get the list of tools blocked in delegated contexts.
pub fn get_blocked_tools() -> &'static [&'static str] {
    BLOCKED_TOOLS
}

/// Check if the current execution is in a delegated context.
pub fn is_delegated() -> bool {
    CURRENT_DEPTH.load(Ordering::Relaxed) > 0
}

/// Get the current delegation depth.
pub fn current_depth() -> usize {
    CURRENT_DEPTH.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_tools() {
        let blocked = get_blocked_tools();
        assert!(blocked.contains(&"delegate_task"));
        assert!(blocked.contains(&"clarify"));
        assert!(!blocked.contains(&"read_file"));
    }

    #[test]
    fn test_delegation_depth() {
        assert!(!is_delegated());
        CURRENT_DEPTH.store(1, Ordering::Relaxed);
        assert!(is_delegated());
        assert_eq!(current_depth(), 1);
        CURRENT_DEPTH.store(0, Ordering::Relaxed);
    }

    #[test]
    fn test_max_depth_constant() {
        assert_eq!(MAX_DEPTH, 2);
        assert_eq!(DELEGATION_MAX_ITERATIONS, 50);
    }
}
