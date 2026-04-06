//! Skills management system.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::skills_hub_client::SkillsHubClient;

/// Skill manifest metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub dependencies: Option<Vec<String>>,
    pub tools: Option<Vec<String>>,
    pub entry_point: Option<String>,
}

/// Skill installation status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub status: SkillStatus,
    pub install_path: String,
    pub installed_at: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    Enabled,
    Disabled,
    Outdated,
    Error(String),
}

/// Skills manager
pub struct SkillsManager {
    skills_dir: PathBuf,
    hub_client: Option<SkillsHubClient>,
}

impl SkillsManager {
    /// Create a new skills manager
    pub fn new() -> Result<Self> {
        let skills_dir = Self::get_skills_dir()?;
        
        // Create skills directory if it doesn't exist
        if !skills_dir.exists() {
            fs::create_dir_all(&skills_dir).with_context(|| {
                format!("Failed to create skills directory: {:?}", skills_dir)
            })?;
            info!(path = ?skills_dir, "Created skills directory");
        }

        Ok(Self { 
            skills_dir,
            hub_client: None,
        })
    }

    /// Create with Hub client
    pub fn with_hub(hub_url: Option<String>, api_key: Option<String>) -> Result<Self> {
        let mut manager = Self::new()?;
        manager.hub_client = Some(SkillsHubClient::new(hub_url, api_key));
        Ok(manager)
    }

    /// Get the skills directory path
    fn get_skills_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".openhermes").join("skills"))
    }

    /// Install a skill from URL or path
    pub async fn install_skill(
        &self,
        skill_name: &str,
        source: &str,
        version: &str,
    ) -> Result<SkillInfo> {
        info!(
            skill = skill_name,
            source = source,
            version = version,
            "Installing skill"
        );

        let skill_dir = self.skills_dir.join(skill_name);

        // Create skill directory
        if skill_dir.exists() {
            warn!(skill = skill_name, "Skill already exists, will overwrite");
            fs::remove_dir_all(&skill_dir)?;
        }

        fs::create_dir_all(&skill_dir).with_context(|| {
            format!("Failed to create skill directory: {:?}", skill_dir)
        })?;

        // Download or copy skill files
        if source == "hub" {
            if let Some(hub) = &self.hub_client {
                // Download from Hub
                hub.download_skill(skill_name, version, &skill_dir).await?;
            } else {
                return Err(anyhow::anyhow!(
                    "Skills Hub client not configured. Cannot install from hub."
                ));
            }
        } else if source.starts_with("http") {
            // Download from URL
            self.download_skill_from_url(source, &skill_dir).await?;
        } else {
            // Copy from local path
            self.copy_skill_from_path(source, &skill_dir)?;
        }

        // Read and validate manifest
        let manifest = self.read_manifest(&skill_dir)?;
        
        // Create skill info
        let skill_info = SkillInfo {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            status: SkillStatus::Enabled,
            install_path: skill_dir.to_string_lossy().to_string(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            source: source.to_string(),
        };

        // Write skill metadata
        self.write_metadata(&skill_info, &skill_dir)?;

        info!(
            skill = skill_name,
            version = &manifest.version,
            "Skill installed successfully"
        );

        Ok(skill_info)
    }

    /// Download skill from hub or URL
    async fn download_skill(
        &self,
        skill_name: &str,
        source: &str,
        version: &str,
        target_dir: &Path,
    ) -> Result<()> {
        // TODO: Implement actual download from Skills Hub API
        // For now, create a placeholder structure
        
        info!(skill = skill_name, "Creating placeholder skill structure");
        
        // Create placeholder manifest
        let manifest = SkillManifest {
            name: skill_name.to_string(),
            version: version.to_string(),
            description: format!("Skill: {}", skill_name),
            author: Some("Skills Hub".to_string()),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            dependencies: None,
            tools: None,
            entry_point: None,
        };

        let manifest_path = target_dir.join("skill.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(&manifest_path, manifest_json).with_context(|| {
            format!("Failed to write manifest: {:?}", manifest_path)
        })?;

        // Create tools directory
        let tools_dir = target_dir.join("tools");
        fs::create_dir_all(&tools_dir)?;

        Ok(())
    }

    /// Download skill from URL
    async fn download_skill_from_url(&self, url: &str, target_dir: &Path) -> Result<()> {
        info!(url = url, "Downloading skill from URL");
        
        let response = reqwest::get(url).await
            .with_context(|| format!("Failed to download from URL: {}", url))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Download error {}: {}", status, error_text));
        }

        let bytes = response.bytes().await?;
        
        // Save to temporary file
        let temp_file = target_dir.join("download.zip");
        std::fs::write(&temp_file, &bytes).with_context(|| {
            format!("Failed to write downloaded file: {:?}", temp_file)
        })?;

        // TODO: Extract ZIP when zip crate is added
        warn!("ZIP extraction not yet implemented, saved as ZIP file");

        Ok(())
    }

    /// Copy skill from local path
    fn copy_skill_from_path(&self, source: &str, target_dir: &Path) -> Result<()> {
        let source_path = Path::new(source);
        
        if !source_path.exists() {
            return Err(anyhow::anyhow!("Source path does not exist: {}", source));
        }

        if source_path.is_dir() {
            // Copy directory contents
            for entry in fs::read_dir(source_path)? {
                let entry = entry?;
                let dest = target_dir.join(entry.file_name());
                
                if entry.path().is_dir() {
                    fs::create_dir_all(&dest)?;
                    // Recursive copy would go here
                } else {
                    fs::copy(entry.path(), &dest)?;
                }
            }
        } else {
            return Err(anyhow::anyhow!("Source must be a directory: {}", source));
        }

        Ok(())
    }

    /// Read skill manifest
    fn read_manifest(&self, skill_dir: &Path) -> Result<SkillManifest> {
        let manifest_path = skill_dir.join("skill.json");
        
        if !manifest_path.exists() {
            return Err(anyhow::anyhow!(
                "Skill manifest not found at: {:?}",
                manifest_path
            ));
        }

        let manifest_content = fs::read_to_string(&manifest_path).with_context(|| {
            format!("Failed to read manifest: {:?}", manifest_path)
        })?;

        let manifest: SkillManifest = serde_json::from_str(&manifest_content).with_context(|| {
            "Invalid skill manifest format"
        })?;

        Ok(manifest)
    }

    /// Write skill metadata
    fn write_metadata(&self, info: &SkillInfo, skill_dir: &Path) -> Result<()> {
        let metadata_path = skill_dir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(info)?;
        
        fs::write(&metadata_path, metadata_json).with_context(|| {
            format!("Failed to write metadata: {:?}", metadata_path)
        })?;

        Ok(())
    }

    /// List all installed skills
    pub fn list_skills(&self, include_disabled: bool) -> Result<Vec<SkillInfo>> {
        let mut skills = Vec::new();

        if !self.skills_dir.exists() {
            return Ok(skills);
        }

        for entry in fs::read_dir(&self.skills_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let metadata_path = path.join("metadata.json");
                
                if metadata_path.exists() {
                    let metadata_content = fs::read_to_string(&metadata_path)?;
                    let skill_info: SkillInfo = serde_json::from_str(&metadata_content)?;

                    // Filter by status
                    if include_disabled || matches!(skill_info.status, SkillStatus::Enabled) {
                        skills.push(skill_info);
                    }
                } else {
                    // Try to read manifest directly
                    if let Ok(manifest) = self.read_manifest(&path) {
                        skills.push(SkillInfo {
                            name: manifest.name,
                            version: manifest.version,
                            description: manifest.description,
                            status: SkillStatus::Enabled,
                            install_path: path.to_string_lossy().to_string(),
                            installed_at: "unknown".to_string(),
                            source: "manual".to_string(),
                        });
                    }
                }
            }
        }

        // Sort by name
        skills.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(skills)
    }

    /// Get skill information by name
    pub fn get_skill(&self, name: &str) -> Result<Option<SkillInfo>> {
        let skill_dir = self.skills_dir.join(name);
        
        if !skill_dir.exists() {
            return Ok(None);
        }

        let metadata_path = skill_dir.join("metadata.json");
        
        if metadata_path.exists() {
            let metadata_content = fs::read_to_string(&metadata_path)?;
            let skill_info: SkillInfo = serde_json::from_str(&metadata_content)?;
            Ok(Some(skill_info))
        } else {
            // Try to read manifest
            let manifest = self.read_manifest(&skill_dir)?;
            Ok(Some(SkillInfo {
                name: manifest.name,
                version: manifest.version,
                description: manifest.description,
                status: SkillStatus::Enabled,
                install_path: skill_dir.to_string_lossy().to_string(),
                installed_at: "unknown".to_string(),
                source: "manual".to_string(),
            }))
        }
    }

    /// Uninstall a skill
    pub fn uninstall_skill(&self, name: &str) -> Result<()> {
        let skill_dir = self.skills_dir.join(name);
        
        if !skill_dir.exists() {
            return Err(anyhow::anyhow!("Skill not found: {}", name));
        }

        fs::remove_dir_all(&skill_dir).with_context(|| {
            format!("Failed to remove skill directory: {:?}", skill_dir)
        })?;

        info!(skill = name, "Skill uninstalled");
        Ok(())
    }

    /// Enable or disable a skill
    pub fn set_skill_status(&self, name: &str, enabled: bool) -> Result<()> {
        let skill_dir = self.skills_dir.join(name);
        
        if !skill_dir.exists() {
            return Err(anyhow::anyhow!("Skill not found: {}", name));
        }

        let metadata_path = skill_dir.join("metadata.json");
        
        if metadata_path.exists() {
            let metadata_content = fs::read_to_string(&metadata_path)?;
            let mut skill_info: SkillInfo = serde_json::from_str(&metadata_content)?;
            
            skill_info.status = if enabled {
                SkillStatus::Enabled
            } else {
                SkillStatus::Disabled
            };

            self.write_metadata(&skill_info, &skill_dir)?;
            
            info!(
                skill = name,
                enabled = enabled,
                "Skill status updated"
            );
        } else {
            return Err(anyhow::anyhow!("Skill metadata not found: {}", name));
        }

        Ok(())
    }

    /// Check for skill updates
    pub async fn check_skill_updates(&self, skill_name: &str) -> Result<Option<String>> {
        let skill_info = self.get_skill(skill_name)?;
        
        if let Some(info) = skill_info {
            if let Some(hub) = &self.hub_client {
                if let Some(latest_version) = hub.check_updates(&info.name, &info.version).await? {
                    return Ok(Some(latest_version.version));
                }
            }
        }
        
        Ok(None)
    }

    /// Get skills directory
    pub fn skills_dir(&self) -> &Path {
        &self.skills_dir
    }
}
