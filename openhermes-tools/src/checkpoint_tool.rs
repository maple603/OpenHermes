//! Checkpoint tool for saving and restoring agent state.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::{Tool, REGISTRY};

/// Checkpoint data
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Checkpoint {
    id: String,
    name: String,
    description: String,
    created_at: chrono::DateTime<chrono::Utc>,
    data: Value,
}

/// Global checkpoint storage
static CHECKPOINTS: Lazy<DashMap<String, Checkpoint>> =
    Lazy::new(DashMap::new);

/// Checkpoint management tool
pub struct CheckpointTool;

#[async_trait]
impl Tool for CheckpointTool {
    fn name(&self) -> &str {
        "checkpoint"
    }

    fn toolset(&self) -> &str {
        "assistant"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "checkpoint",
            "description": "Save, restore, or manage checkpoints. Checkpoints allow you to save the current state and restore it later.",
            "parameters": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "description": "Action to perform",
                        "enum": ["save", "restore", "list", "delete"]
                    },
                    "checkpoint_id": {
                        "type": "string",
                        "description": "Checkpoint ID (for restore/delete)"
                    },
                    "name": {
                        "type": "string",
                        "description": "Checkpoint name (for save)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Checkpoint description (for save)"
                    },
                    "data": {
                        "type": "object",
                        "description": "Data to save (for save action)"
                    }
                },
                "required": ["action"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let action = args["action"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: action"))?;

        match action {
            "save" => {
                let name = args["name"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing name for save action"))?;
                let description = args["description"].as_str().unwrap_or("");
                let data = args["data"].clone();

                self.save_checkpoint(name, description, data).await
            }
            "restore" => {
                let checkpoint_id = args["checkpoint_id"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing checkpoint_id for restore action"))?;
                
                self.restore_checkpoint(checkpoint_id).await
            }
            "list" => {
                self.list_checkpoints().await
            }
            "delete" => {
                let checkpoint_id = args["checkpoint_id"].as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing checkpoint_id for delete action"))?;
                
                self.delete_checkpoint(checkpoint_id).await
            }
            _ => {
                Err(anyhow::anyhow!("Unknown action: {}", action))
            }
        }
    }
}

impl CheckpointTool {
    async fn save_checkpoint(&self, name: &str, description: &str, data: Value) -> Result<String> {
        let checkpoint_id = format!("ckpt_{}", chrono::Utc::now().timestamp_millis());
        let checkpoint = Checkpoint {
            id: checkpoint_id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            created_at: chrono::Utc::now(),
            data,
        };

        CHECKPOINTS.insert(checkpoint_id.clone(), checkpoint);

        Ok(serde_json::json!({
            "success": true,
            "action": "save",
            "checkpoint_id": checkpoint_id,
            "name": name,
            "message": "Checkpoint saved"
        }).to_string())
    }

    async fn restore_checkpoint(&self, checkpoint_id: &str) -> Result<String> {
        let checkpoint = CHECKPOINTS.get(checkpoint_id)
            .ok_or_else(|| anyhow::anyhow!("Checkpoint not found: {}", checkpoint_id))?;

        let data = checkpoint.data.clone();
        let name = checkpoint.name.clone();

        Ok(serde_json::json!({
            "success": true,
            "action": "restore",
            "checkpoint_id": checkpoint_id,
            "name": name,
            "data": data,
            "message": "Checkpoint restored"
        }).to_string())
    }

    async fn list_checkpoints(&self) -> Result<String> {
        let checkpoints: Vec<Value> = CHECKPOINTS.iter().map(|entry| {
            let cp = entry.value();
            serde_json::json!({
                "checkpoint_id": cp.id,
                "name": cp.name,
                "description": cp.description,
                "created_at": cp.created_at
            })
        }).collect();

        Ok(serde_json::json!({
            "action": "list",
            "count": checkpoints.len(),
            "checkpoints": checkpoints
        }).to_string())
    }

    async fn delete_checkpoint(&self, checkpoint_id: &str) -> Result<String> {
        let removed = CHECKPOINTS.remove(checkpoint_id);
        
        if removed.is_some() {
            Ok(serde_json::json!({
                "success": true,
                "action": "delete",
                "checkpoint_id": checkpoint_id,
                "message": "Checkpoint deleted"
            }).to_string())
        } else {
            Err(anyhow::anyhow!("Checkpoint not found: {}", checkpoint_id))
        }
    }
}

/// Register checkpoint tool
pub fn register_tools() {
    REGISTRY.register(Arc::new(CheckpointTool));
}
