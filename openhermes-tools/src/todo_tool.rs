//! Todo list management tool.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::{Tool, REGISTRY};

/// Todo item
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TodoItem {
    id: String,
    content: String,
    completed: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Global todo storage
static TODOS: Lazy<DashMap<String, Vec<TodoItem>>> =
    Lazy::new(DashMap::new);

/// Todo management tool
pub struct TodoTool;

#[async_trait]
impl Tool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn toolset(&self) -> &str {
        "assistant"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "todo",
            "description": "Manage todo lists. Supports create, update, delete, and list operations.",
            "parameters": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "description": "Action to perform",
                        "enum": ["create", "update", "delete", "list", "complete"]
                    },
                    "content": {
                        "type": "string",
                        "description": "Todo content (for create/update)"
                    },
                    "todo_id": {
                        "type": "string",
                        "description": "Todo ID (for update/delete/complete)"
                    }
                },
                "required": ["action"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let action = args["action"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: action"))?;

        let session_id = "default".to_string(); // In real implementation, use session ID

        match action {
            "create" => {
                let content = args["content"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing content for create action"))?;
                
                self.create_todo(&session_id, content).await
            }
            "update" => {
                let todo_id = args["todo_id"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing todo_id for update action"))?;
                let content = args["content"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing content for update action"))?;
                
                self.update_todo(&session_id, todo_id, content).await
            }
            "delete" => {
                let todo_id = args["todo_id"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing todo_id for delete action"))?;
                
                self.delete_todo(&session_id, todo_id).await
            }
            "complete" => {
                let todo_id = args["todo_id"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing todo_id for complete action"))?;
                
                self.complete_todo(&session_id, todo_id).await
            }
            "list" => {
                self.list_todos(&session_id).await
            }
            _ => {
                Err(anyhow::anyhow!("Unknown action: {}", action))
            }
        }
    }
}

impl TodoTool {
    async fn create_todo(&self, session_id: &str, content: &str) -> Result<String> {
        let todo_id = format!("todo_{}", chrono::Utc::now().timestamp_millis());
        let todo = TodoItem {
            id: todo_id.clone(),
            content: content.to_string(),
            completed: false,
            created_at: chrono::Utc::now(),
        };

        TODOS.entry(session_id.to_string())
            .or_insert_with(Vec::new)
            .push(todo);

        Ok(serde_json::json!({
            "success": true,
            "action": "create",
            "todo_id": todo_id,
            "content": content,
            "message": "Todo created"
        }).to_string())
    }

    async fn update_todo(&self, session_id: &str, todo_id: &str, content: &str) -> Result<String> {
        let mut todos = TODOS.get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("No todos found"))?;

        for todo in todos.iter_mut() {
            if todo.id == todo_id {
                todo.content = content.to_string();
                return Ok(serde_json::json!({
                    "success": true,
                    "action": "update",
                    "todo_id": todo_id,
                    "message": "Todo updated"
                }).to_string());
            }
        }

        Err(anyhow::anyhow!("Todo not found: {}", todo_id))
    }

    async fn delete_todo(&self, session_id: &str, todo_id: &str) -> Result<String> {
        let mut todos = TODOS.get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("No todos found"))?;

        let initial_len = todos.len();
        todos.retain(|todo| todo.id != todo_id);

        if todos.len() < initial_len {
            Ok(serde_json::json!({
                "success": true,
                "action": "delete",
                "todo_id": todo_id,
                "message": "Todo deleted"
            }).to_string())
        } else {
            Err(anyhow::anyhow!("Todo not found: {}", todo_id))
        }
    }

    async fn complete_todo(&self, session_id: &str, todo_id: &str) -> Result<String> {
        let mut todos = TODOS.get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("No todos found"))?;

        for todo in todos.iter_mut() {
            if todo.id == todo_id {
                todo.completed = true;
                return Ok(serde_json::json!({
                    "success": true,
                    "action": "complete",
                    "todo_id": todo_id,
                    "message": "Todo completed"
                }).to_string());
            }
        }

        Err(anyhow::anyhow!("Todo not found: {}", todo_id))
    }

    async fn list_todos(&self, session_id: &str) -> Result<String> {
        let todos = TODOS.get(session_id);
        
        let todo_list = match todos {
            Some(entry) => entry.value().clone(),
            None => Vec::new(),
        };

        let completed_count = todo_list.iter().filter(|t| t.completed).count();
        let pending_count = todo_list.len() - completed_count;

        Ok(serde_json::json!({
            "action": "list",
            "total": todo_list.len(),
            "completed": completed_count,
            "pending": pending_count,
            "todos": todo_list
        }).to_string())
    }
}

/// Register todo tool
pub fn register_tools() {
    REGISTRY.register(Arc::new(TodoTool));
}
