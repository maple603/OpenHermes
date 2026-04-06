//! Enhanced file operation tools.

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use crate::Tool;

/// Search for files by pattern
pub struct SearchFilesTool;

#[async_trait]
impl Tool for SearchFilesTool {
    fn name(&self) -> &str {
        "search_files"
    }

    fn toolset(&self) -> &str {
        "file"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "search_files",
            "description": "Search for files by glob pattern in a directory. Returns matching file paths.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match (e.g., '*.rs', '**/*.py')"
                    },
                    "search_dir": {
                        "type": "string",
                        "description": "Directory to search in (default: current directory)"
                    }
                },
                "required": ["pattern"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let pattern = args["pattern"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: pattern"))?;
        
        let search_dir = args["search_dir"].as_str().unwrap_or(".");

        let search_pattern = if Path::new(search_dir).is_absolute() {
            format!("{}/{}", search_dir, pattern)
        } else {
            let current_dir = std::env::current_dir()?;
            format!("{}/{}/{}", current_dir.display(), search_dir, pattern)
        };

        let mut matches = Vec::new();
        for entry in glob::glob(&search_pattern)? {
            match entry {
                Ok(path) => matches.push(path.display().to_string()),
                Err(e) => tracing::warn!("Glob error: {}", e),
            }
        }

        if matches.is_empty() {
            return Ok(format!("No files found matching pattern: {}", pattern));
        }

        let result = serde_json::json!({
            "pattern": pattern,
            "search_dir": search_dir,
            "count": matches.len(),
            "files": matches
        });

        Ok(result.to_string())
    }
}

/// List directory contents
pub struct ListDirectoryTool;

#[async_trait]
impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn toolset(&self) -> &str {
        "file"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "list_directory",
            "description": "List contents of a directory. Returns files and directories with metadata.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the directory to list"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Whether to list recursively (default: false)",
                        "default": false
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: path"))?;
        
        let recursive = args["recursive"].as_bool().unwrap_or(false);

        let mut entries = Vec::new();
        Self::list_dir_recursive(path, recursive, &mut entries, 0).await?;

        let result = serde_json::json!({
            "path": path,
            "entries": entries
        });

        Ok(result.to_string())
    }
}

impl ListDirectoryTool {
    async fn list_dir_recursive(
        path: &str,
        recursive: bool,
        entries: &mut Vec<Value>,
        depth: usize,
    ) -> Result<()> {
        let mut read_dir = fs::read_dir(path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let entry_path = entry.path();
            let metadata = entry.metadata().await?;
            let is_dir = metadata.is_dir();

            let entry_info = serde_json::json!({
                "name": entry.file_name().to_string_lossy(),
                "path": entry_path.display().to_string(),
                "type": if is_dir { "directory" } else { "file" },
                "size": if is_dir { serde_json::Value::Null } else { serde_json::json!(metadata.len()) },
                "depth": depth
            });

            entries.push(entry_info);

            if recursive && is_dir {
                if let Some(subdir) = entry_path.to_str() {
                    Box::pin(Self::list_dir_recursive(subdir, recursive, entries, depth + 1)).await?;
                }
            }
        }

        Ok(())
    }
}

/// Copy a file
pub struct CopyFileTool;

#[async_trait]
impl Tool for CopyFileTool {
    fn name(&self) -> &str {
        "copy_file"
    }

    fn toolset(&self) -> &str {
        "file"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "copy_file",
            "description": "Copy a file from source to destination",
            "parameters": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Source file path"
                    },
                    "destination": {
                        "type": "string",
                        "description": "Destination file path"
                    }
                },
                "required": ["source", "destination"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let source = args["source"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: source"))?;
        
        let destination = args["destination"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: destination"))?;

        fs::copy(source, destination).await?;

        Ok(serde_json::json!({
            "success": true,
            "source": source,
            "destination": destination,
            "message": format!("Copied {} to {}", source, destination)
        }).to_string())
    }
}

/// Move a file
pub struct MoveFileTool;

#[async_trait]
impl Tool for MoveFileTool {
    fn name(&self) -> &str {
        "move_file"
    }

    fn toolset(&self) -> &str {
        "file"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "move_file",
            "description": "Move or rename a file",
            "parameters": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Source file path"
                    },
                    "destination": {
                        "type": "string",
                        "description": "Destination file path"
                    }
                },
                "required": ["source", "destination"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let source = args["source"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: source"))?;
        
        let destination = args["destination"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: destination"))?;

        fs::rename(source, destination).await?;

        Ok(serde_json::json!({
            "success": true,
            "source": source,
            "destination": destination,
            "message": format!("Moved {} to {}", source, destination)
        }).to_string())
    }
}

/// Delete a file
pub struct DeleteFileTool;

#[async_trait]
impl Tool for DeleteFileTool {
    fn name(&self) -> &str {
        "delete_file"
    }

    fn toolset(&self) -> &str {
        "file"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "delete_file",
            "description": "Delete a file (use with caution!)",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to delete"
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: path"))?;

        fs::remove_file(path).await?;

        Ok(serde_json::json!({
            "success": true,
            "path": path,
            "message": format!("Deleted file: {}", path)
        }).to_string())
    }
}

/// Create a directory
pub struct CreateDirectoryTool;

#[async_trait]
impl Tool for CreateDirectoryTool {
    fn name(&self) -> &str {
        "create_directory"
    }

    fn toolset(&self) -> &str {
        "file"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "create_directory",
            "description": "Create a new directory (including parent directories if needed)",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the directory to create"
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: path"))?;

        fs::create_dir_all(path).await?;

        Ok(serde_json::json!({
            "success": true,
            "path": path,
            "message": format!("Created directory: {}", path)
        }).to_string())
    }
}

/// Edit file content with search/replace
pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn toolset(&self) -> &str {
        "file"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "file_edit",
            "description": "Edit a file by searching and replacing text. Supports multiple replacements.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to edit"
                    },
                    "replacements": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_text": {
                                    "type": "string",
                                    "description": "Text to search for"
                                },
                                "new_text": {
                                    "type": "string",
                                    "description": "Text to replace with"
                                }
                            },
                            "required": ["old_text", "new_text"]
                        },
                        "description": "List of replacements to apply"
                    }
                },
                "required": ["path", "replacements"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: path"))?;
        
        let replacements = args["replacements"].as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: replacements"))?;

        let mut content = fs::read_to_string(path).await?;
        let mut replaced_count = 0;

        for replacement in replacements {
            let old_text = replacement["old_text"].as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing old_text in replacement"))?;
            
            let new_text = replacement["new_text"].as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing new_text in replacement"))?;

            if content.contains(old_text) {
                content = content.replace(old_text, new_text);
                replaced_count += 1;
            } else {
                tracing::warn!("Text not found: {}", old_text);
            }
        }

        fs::write(path, &content).await?;

        Ok(serde_json::json!({
            "success": true,
            "path": path,
            "replacements_applied": replaced_count,
            "message": format!("Applied {} replacements to {}", replaced_count, path)
        }).to_string())
    }
}
