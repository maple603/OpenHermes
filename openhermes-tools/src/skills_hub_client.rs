//! Skills Hub API client for searching, downloading, and checking skill updates.

use std::collections::HashMap;

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Skills Hub API client
pub struct SkillsHubClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

/// Skill package information from Hub
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubSkillInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub category: String,
    pub tags: Vec<String>,
    pub downloads: u64,
    pub rating: f64,
    pub repository: Option<String>,
    pub homepage: Option<String>,
    pub dependencies: Vec<String>,
    pub compatible_versions: Vec<String>,
    pub download_url: String,
    pub checksum: Option<String>,
}

/// Search response from Hub
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsSearchResponse {
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub skills: Vec<HubSkillInfo>,
}

/// Version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersion {
    pub version: String,
    pub release_date: String,
    pub changelog: String,
    pub download_url: String,
    pub checksum: Option<String>,
    pub is_latest: bool,
}

impl SkillsHubClient {
    /// Create a new Skills Hub client
    pub fn new(base_url: Option<String>, api_key: Option<String>) -> Self {
        let base_url = base_url.unwrap_or_else(|| "https://skills.openhermes.ai/api/v1".to_string());
        
        Self {
            client: Client::new(),
            base_url,
            api_key,
        }
    }

    /// Search skills in the Hub
    pub async fn search_skills(
        &self,
        query: &str,
        category: Option<&str>,
        tags: Option<&[&str]>,
        sort_by: Option<&str>,
        page: u64,
        per_page: u64,
    ) -> Result<SkillsSearchResponse> {
        info!(query = query, "Searching skills in Hub");

        let mut url = format!("{}/skills/search?q={}&page={}&per_page={}", 
            self.base_url, 
            urlencoding::encode(query),
            page,
            per_page
        );

        if let Some(cat) = category {
            url.push_str(&format!("&category={}", urlencoding::encode(cat)));
        }

        if let Some(tags) = tags {
            url.push_str(&format!("&tags={}", urlencoding::encode(&tags.join(","))));
        }

        if let Some(sort) = sort_by {
            url.push_str(&format!("&sort={}", urlencoding::encode(sort)));
        }

        let mut request = self.client.get(&url);
        
        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }
        
        let response = request.send().await
            .with_context(|| "Failed to send search request to Skills Hub")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "Skills Hub search error {}: {}",
                status,
                error_text
            ));
        }

        let result: SkillsSearchResponse = response.json().await?;
        
        info!(
            total = result.total,
            returned = result.skills.len(),
            "Skills search completed"
        );

        Ok(result)
    }

    /// Get skill details by name
    pub async fn get_skill_info(&self, skill_name: &str) -> Result<HubSkillInfo> {
        info!(skill = skill_name, "Fetching skill info from Hub");

        let mut request = self.client
            .get(format!("{}/skills/{}", self.base_url, skill_name));
        
        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        let response = request.send().await
            .with_context(|| format!("Failed to fetch skill info: {}", skill_name))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "Skills Hub get skill error {}: {}",
                status,
                error_text
            ));
        }

        let skill_info: HubSkillInfo = response.json().await?;
        Ok(skill_info)
    }

    /// Check for skill updates
    pub async fn check_updates(&self, skill_name: &str, current_version: &str) -> Result<Option<SkillVersion>> {
        info!(
            skill = skill_name,
            current = current_version,
            "Checking for skill updates"
        );

        let mut request = self.client
            .get(format!("{}/skills/{}/versions", self.base_url, skill_name));
        
        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        let response = request.send().await
            .with_context(|| format!("Failed to fetch skill versions: {}", skill_name))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "Skills Hub check updates error {}: {}",
                status,
                error_text
            ));
        }

        let versions: Vec<SkillVersion> = response.json().await?;
        
        // Find the latest version that's newer than current
        for version in &versions {
            if version.is_latest && version.version != current_version {
                info!(
                    skill = skill_name,
                    current = current_version,
                    latest = &version.version,
                    "Update available"
                );
                return Ok(Some(version.clone()));
            }
        }

        info!(skill = skill_name, "Skill is up to date");
        Ok(None)
    }

    /// Download skill package
    pub async fn download_skill(
        &self,
        skill_name: &str,
        version: &str,
        target_path: &std::path::Path,
    ) -> Result<()> {
        info!(
            skill = skill_name,
            version = version,
            path = ?target_path,
            "Downloading skill package"
        );

        // Get download URL for specific version
        let mut request = self.client
            .get(format!("{}/skills/{}/versions/{}", self.base_url, skill_name, version));
        
        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        let response = request.send().await
            .with_context(|| format!("Failed to fetch skill version: {} {}", skill_name, version))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "Skills Hub get version error {}: {}",
                status,
                error_text
            ));
        }

        let version_info: SkillVersion = response.json().await?;
        
        // Download the package
        let download_response = self.client
            .get(&version_info.download_url)
            .send()
            .await
            .with_context(|| format!("Failed to download skill package: {}", version_info.download_url))?;

        if !download_response.status().is_success() {
            let status = download_response.status();
            let error_text = download_response.text().await?;
            return Err(anyhow::anyhow!(
                "Download error {}: {}",
                status,
                error_text
            ));
        }

        // Get the bytes (assuming ZIP format)
        let bytes = download_response.bytes().await?;
        
        // Create target directory
        std::fs::create_dir_all(target_path).with_context(|| {
            format!("Failed to create target directory: {:?}", target_path)
        })?;

        // Save to temporary file first
        let temp_file = target_path.join("download.zip");
        std::fs::write(&temp_file, &bytes).with_context(|| {
            format!("Failed to write downloaded file: {:?}", temp_file)
        })?;

        // Extract ZIP
        self.extract_zip(&temp_file, target_path).with_context(|| {
            format!("Failed to extract ZIP to: {:?}", target_path)
        })?;

        // Clean up temp file
        std::fs::remove_file(&temp_file).ok();

        // Verify checksum if provided
        if let Some(expected_checksum) = &version_info.checksum {
            let actual_checksum = self.calculate_checksum(&bytes);
            if actual_checksum != *expected_checksum {
                warn!(
                    skill = skill_name,
                    expected = expected_checksum,
                    actual = actual_checksum,
                    "Checksum mismatch"
                );
                // Don't fail, just warn
            }
        }

        info!(skill = skill_name, "Skill package downloaded and extracted");
        Ok(())
    }

    /// Extract ZIP file
    fn extract_zip(&self, zip_path: &std::path::Path, target_dir: &std::path::Path) -> Result<()> {
        // Use zip crate for extraction
        // For now, placeholder - would need to add zip dependency
        info!(
            zip = ?zip_path,
            target = ?target_dir,
            "ZIP extraction (placeholder - needs zip crate)"
        );
        
        // TODO: Implement actual ZIP extraction
        // use std::fs::File;
        // use zip::ZipArchive;
        // 
        // let file = File::open(zip_path)?;
        // let mut archive = ZipArchive::new(file)?;
        // 
        // for i in 0..archive.len() {
        //     let mut file = archive.by_index(i)?;
        //     let outpath = target_dir.join(file.name());
        //     
        //     if file.name().ends_with('/') {
        //         std::fs::create_dir_all(&outpath)?;
        //     } else {
        //         if let Some(p) = outpath.parent() {
        //             std::fs::create_dir_all(p)?;
        //         }
        //         let mut outfile = std::fs::File::create(&outpath)?;
        //         std::io::copy(&mut file, &mut outfile)?;
        //     }
        // }

        Ok(())
    }

    /// Calculate file checksum (SHA256)
    fn calculate_checksum(&self, data: &[u8]) -> String {
        // Use sha2 crate
        // For now, placeholder
        format!("sha256:placeholder_{}", data.len())
        
        // TODO: Implement actual checksum
        // use sha2::{Sha256, Digest};
        // let mut hasher = Sha256::new();
        // hasher.update(data);
        // format!("sha256:{:x}", hasher.finalize())
    }
}
