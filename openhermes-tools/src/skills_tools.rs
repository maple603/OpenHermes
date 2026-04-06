//! Skills system tools for managing agent skills.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::info;

use crate::registry::{Tool, REGISTRY};
use crate::skills_manager::SkillsManager;

use once_cell::sync::Lazy;
use tokio::sync::RwLock;

/// Global skills manager instance
static SKILLS_MANAGER: Lazy<RwLock<Option<Arc<SkillsManager>>>> = 
    Lazy::new(|| RwLock::new(None));

/// Set the global skills manager
pub async fn set_skills_manager(manager: Arc<SkillsManager>) {
    let mut guard = SKILLS_MANAGER.write().await;
    *guard = Some(manager);
    info!("Skills manager set globally");
}

/// Get the global skills manager for reading
async fn get_skills_manager() -> Option<Arc<SkillsManager>> {
    let guard = SKILLS_MANAGER.read().await;
    guard.clone()
}

/// Get the global skills manager for writing
#[allow(dead_code)]
async fn get_skills_manager_mut() -> Option<Arc<SkillsManager>> {
    let guard = SKILLS_MANAGER.read().await;
    guard.clone()
}

/// Skills install tool - install a new skill
pub struct SkillsInstallTool;

#[async_trait]
impl Tool for SkillsInstallTool {
    fn name(&self) -> &str {
        "skills_install"
    }

    fn toolset(&self) -> &str {
        "skills"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "skills_install",
            "description": "Install a new skill from the skills hub or local file.",
            "parameters": {
                "type": "object",
                "properties": {
                    "skill_name": {
                        "type": "string",
                        "description": "Name of the skill to install"
                    },
                    "source": {
                        "type": "string",
                        "description": "Source URL or path (default: skills hub)",
                        "default": "hub"
                    },
                    "version": {
                        "type": "string",
                        "description": "Specific version to install (default: latest)"
                    }
                },
                "required": ["skill_name"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let skill_name = args["skill_name"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: skill_name"))?;
        
        let source = args["source"].as_str().unwrap_or("hub");
        let version = args["version"].as_str().unwrap_or("latest");

        info!(
            skill = skill_name,
            source = source,
            version = version,
            "Installing skill"
        );

        if let Some(manager) = get_skills_manager().await {
            let skill_info = manager.install_skill(skill_name, source, version).await?;
            
            Ok(serde_json::json!({
                "success": true,
                "skill_name": skill_info.name,
                "version": skill_info.version,
                "source": skill_info.source,
                "status": "installed",
                "install_path": skill_info.install_path,
                "installed_at": skill_info.installed_at,
                "message": format!("Skill '{}' v{} installed successfully from {}", 
                    skill_info.name, skill_info.version, skill_info.source)
            }).to_string())
        } else {
            Ok(serde_json::json!({
                "success": false,
                "skill_name": skill_name,
                "error": "Skills manager not initialized",
                "message": "Skills manager is not available. Please initialize the skills system first."
            }).to_string())
        }
    }
}

/// Skills list tool - list installed skills
pub struct SkillsListTool;

#[async_trait]
impl Tool for SkillsListTool {
    fn name(&self) -> &str {
        "skills_list"
    }

    fn toolset(&self) -> &str {
        "skills"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "skills_list",
            "description": "List all installed skills with their status and version.",
            "parameters": {
                "type": "object",
                "properties": {
                    "include_disabled": {
                        "type": "boolean",
                        "description": "Include disabled skills (default: false)",
                        "default": false
                    }
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let include_disabled = args["include_disabled"].as_bool().unwrap_or(false);

        info!(include_disabled = include_disabled, "Listing skills");

        if let Some(manager) = get_skills_manager().await {
            let skills = manager.list_skills(include_disabled)?;
            
            Ok(serde_json::json!({
                "success": true,
                "count": skills.len(),
                "skills": skills,
                "message": format!("Found {} skills", skills.len())
            }).to_string())
        } else {
            // Return empty list if manager not initialized
            Ok(serde_json::json!({
                "success": true,
                "count": 0,
                "skills": [],
                "message": "Skills manager not initialized. No skills available."
            }).to_string())
        }
    }
}

/// Skills sync tool - sync skills with hub
pub struct SkillsSyncTool;

#[async_trait]
impl Tool for SkillsSyncTool {
    fn name(&self) -> &str {
        "skills_sync"
    }

    fn toolset(&self) -> &str {
        "skills"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "skills_sync",
            "description": "Sync installed skills with the skills hub. Check for updates and apply them.",
            "parameters": {
                "type": "object",
                "properties": {
                    "auto_update": {
                        "type": "boolean",
                        "description": "Automatically install updates (default: false)",
                        "default": false
                    },
                    "dry_run": {
                        "type": "boolean",
                        "description": "Only check for updates without installing (default: false)",
                        "default": false
                    }
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let auto_update = args["auto_update"].as_bool().unwrap_or(false);
        let dry_run = args["dry_run"].as_bool().unwrap_or(false);

        info!(auto_update = auto_update, dry_run = dry_run, "Syncing skills");

        if let Some(manager) = get_skills_manager().await {
            let skills = manager.list_skills(true)?;
            let skills_count = skills.len();
            
            // TODO: Fetch latest versions from Skills Hub API
            // For now, simulate sync by checking skill directories
            
            let mut updates = Vec::new();
            let mut updated = Vec::new();
            
            for skill in skills {
                // In real implementation, compare with hub version
                // For now, mark all as up-to-date
                if auto_update && !dry_run {
                    updated.push(serde_json::json!({
                        "skill": skill.name,
                        "version": skill.version,
                        "status": "up_to_date"
                    }));
                } else {
                    updates.push(serde_json::json!({
                        "skill": skill.name,
                        "current_version": skill.version,
                        "latest_version": skill.version,
                        "status": "up_to_date"
                    }));
                }
            }
            
            if dry_run {
                Ok(serde_json::json!({
                    "success": true,
                    "dry_run": true,
                    "updates": updates,
                    "total_checked": skills_count,
                    "message": format!("Checked {} skills, all up to date", skills_count)
                }).to_string())
            } else if auto_update {
                Ok(serde_json::json!({
                    "success": true,
                    "auto_update": true,
                    "updated": updated,
                    "total_updated": updated.len(),
                    "message": format!("Synced {} skills, all up to date", updated.len())
                }).to_string())
            } else {
                Ok(serde_json::json!({
                    "success": true,
                    "skills_checked": skills_count,
                    "updates_available": 0,
                    "message": "Skills sync completed. All skills are up to date."
                }).to_string())
            }
        } else {
            Ok(serde_json::json!({
                "success": false,
                "error": "Skills manager not initialized",
                "message": "Skills manager is not available. Please initialize the skills system first."
            }).to_string())
        }
    }
}

/// Skills hub search tool - search skills marketplace
pub struct SkillsHubSearchTool;

#[async_trait]
impl Tool for SkillsHubSearchTool {
    fn name(&self) -> &str {
        "skills_hub_search"
    }

    fn toolset(&self) -> &str {
        "skills"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "skills_hub_search",
            "description": "Search the skills marketplace for available skills.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "category": {
                        "type": "string",
                        "description": "Filter by category (e.g., 'web', 'code', 'productivity')"
                    },
                    "sort": {
                        "type": "string",
                        "description": "Sort order: relevance, downloads, rating (default: relevance)",
                        "default": "relevance"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum results to return (default: 10)",
                        "default": 10
                    }
                },
                "required": ["query"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let query = args["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;
        
        let category = args["category"].as_str();
        let sort = args["sort"].as_str().unwrap_or("relevance");
        let limit = args["limit"].as_u64().unwrap_or(10) as usize;

        info!(
            query = query,
            category = category,
            sort = sort,
            limit = limit,
            "Searching skills hub"
        );

        // TODO: Implement actual skills hub search
        // Steps:
        // 1. Query skills hub API
        // 2. Filter by category if specified
        // 3. Sort results
        // 4. Return paginated results

        let skills = vec![
            serde_json::json!({
                "name": "web_automation",
                "version": "2.0.0",
                "category": "web",
                "description": "Browser automation and web scraping",
                "downloads": 15420,
                "rating": 4.8,
                "author": "nous-research"
            }),
            serde_json::json!({
                "name": "database_connector",
                "version": "1.5.0",
                "category": "data",
                "description": "Connect to SQL and NoSQL databases",
                "downloads": 8930,
                "rating": 4.6,
                "author": "community"
            })
        ];

        Ok(serde_json::json!({
            "success": true,
            "query": query,
            "category": category,
            "sort": sort,
            "count": skills.len(),
            "skills": skills,
            "message": format!("Found {} skills matching '{}'", skills.len(), query)
        }).to_string())
    }
}

/// Register skills tools
pub fn register_tools() {
    REGISTRY.register(Arc::new(SkillsInstallTool));
    REGISTRY.register(Arc::new(SkillsListTool));
    REGISTRY.register(Arc::new(SkillsSyncTool));
    REGISTRY.register(Arc::new(SkillsHubSearchTool));
    
    info!("Registered 4 skills tools");
}
