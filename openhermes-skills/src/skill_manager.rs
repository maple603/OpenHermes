//! Skill manager for loading and managing skills.

use std::path::Path;

use anyhow::Result;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Skill definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub instructions: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
}

/// Skill manager
pub struct SkillManager {
    skills: DashMap<String, Skill>,
}

impl SkillManager {
    pub fn new() -> Self {
        Self {
            skills: DashMap::new(),
        }
    }

    pub fn load_skills(&self, skills_dir: &Path) -> Result<()> {
        if !skills_dir.exists() {
            info!("Skills directory not found: {}", skills_dir.display());
            return Ok(());
        }

        let mut count = 0;
        for entry in std::fs::read_dir(skills_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() && path.join("SKILL.md").exists() {
                match std::fs::read_to_string(path.join("SKILL.md")) {
                    Ok(content) => {
                        // Simple parsing - just use the whole content as instructions
                        let skill = Skill {
                            name: path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string(),
                            description: String::new(),
                            instructions: content,
                            triggers: Vec::new(),
                            tools: Vec::new(),
                        };

                        info!("Loaded skill: {}", skill.name);
                        self.skills.insert(skill.name.clone(), skill);
                        count += 1;
                    }
                    Err(e) => {
                        warn!("Failed to read SKILL.md in {}: {}", path.display(), e);
                    }
                }
            }
        }

        info!("Loaded {} skills from {}", count, skills_dir.display());
        Ok(())
    }

    pub fn build_skills_context(&self, active_skills: &[&str]) -> String {
        if active_skills.is_empty() {
            return String::new();
        }

        active_skills
            .iter()
            .filter_map(|name| self.skills.get(*name))
            .map(|skill| {
                format!("## Skill: {}\n\n{}", skill.name, skill.instructions)
            })
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")
    }

    pub fn get_skill(&self, name: &str) -> Option<Skill> {
        self.skills.get(name).map(|r| r.value().clone())
    }

    pub fn list_skills(&self) -> Vec<String> {
        self.skills.iter().map(|r| r.key().clone()).collect()
    }
}

impl Default for SkillManager {
    fn default() -> Self {
        Self::new()
    }
}
