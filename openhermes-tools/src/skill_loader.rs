//! Dynamic skill loading and hot-update system.
//!
//! Loads skills at runtime from the skills directory, watches for changes,
//! and registers/unregisters tools dynamically.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::registry::{Tool, REGISTRY};
use crate::skills_manager::SkillManifest;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Represents a dynamically loaded skill
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub name: String,
    pub version: String,
    pub description: String,
    pub skill_dir: PathBuf,
    pub manifest: SkillManifest,
    pub tools: Vec<String>,
    pub loaded_at: SystemTime,
    pub last_modified: SystemTime,
}

/// Event emitted by the skill loader
#[derive(Debug, Clone)]
pub enum SkillEvent {
    Loaded { skill_name: String },
    Unloaded { skill_name: String },
    Reloaded { skill_name: String },
    Error { skill_name: String, error: String },
}

/// Callback type for skill events
pub type SkillEventCallback = Arc<dyn Fn(SkillEvent) + Send + Sync>;

// ---------------------------------------------------------------------------
// SkillLoader
// ---------------------------------------------------------------------------

/// Dynamic skill loader with file watching for hot updates
pub struct SkillLoader {
    skills_dir: PathBuf,
    loaded_skills: Arc<RwLock<HashMap<String, LoadedSkill>>>,
    watcher: Option<RecommendedWatcher>,
    event_tx: Option<mpsc::UnboundedSender<SkillEvent>>,
    event_callbacks: Arc<RwLock<Vec<SkillEventCallback>>>,
}

impl SkillLoader {
    /// Create a new skill loader
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skills_dir,
            loaded_skills: Arc::new(RwLock::new(HashMap::new())),
            watcher: None,
            event_tx: None,
            event_callbacks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create from default skills directory (~/.openhermes/skills)
    pub fn from_default() -> Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        let skills_dir = home.join(".openhermes").join("skills");
        Ok(Self::new(skills_dir))
    }

    // ------------------------------------------------------------------
    // Bulk operations
    // ------------------------------------------------------------------

    /// Scan the skills directory and load all valid skills
    pub async fn load_all(&self) -> Result<Vec<String>> {
        info!(dir = ?self.skills_dir, "Scanning skills directory");

        if !self.skills_dir.exists() {
            info!("Skills directory does not exist, nothing to load");
            return Ok(vec![]);
        }

        let mut loaded = Vec::new();

        let entries = std::fs::read_dir(&self.skills_dir)
            .with_context(|| format!("Failed to read skills directory: {:?}", self.skills_dir))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let skill_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                if skill_name.starts_with('.') {
                    continue; // skip hidden dirs
                }

                match self.load_skill(&skill_name).await {
                    Ok(_) => loaded.push(skill_name),
                    Err(e) => {
                        warn!(skill = %skill_name, error = %e, "Failed to load skill");
                        self.emit_event(SkillEvent::Error {
                            skill_name,
                            error: e.to_string(),
                        })
                        .await;
                    }
                }
            }
        }

        info!(count = loaded.len(), "Skills loaded");
        Ok(loaded)
    }

    /// Unload all skills
    pub async fn unload_all(&self) -> Result<()> {
        let names: Vec<String> = {
            let skills = self.loaded_skills.read().await;
            skills.keys().cloned().collect()
        };

        for name in names {
            self.unload_skill(&name).await?;
        }

        Ok(())
    }

    // ------------------------------------------------------------------
    // Single-skill lifecycle
    // ------------------------------------------------------------------

    /// Load a single skill by name
    pub async fn load_skill(&self, skill_name: &str) -> Result<LoadedSkill> {
        let skill_dir = self.skills_dir.join(skill_name);

        if !skill_dir.exists() {
            return Err(anyhow::anyhow!("Skill directory not found: {:?}", skill_dir));
        }

        // Read manifest
        let manifest_path = skill_dir.join("skill.json");
        if !manifest_path.exists() {
            return Err(anyhow::anyhow!(
                "No skill.json found in {:?}",
                skill_dir
            ));
        }

        let manifest_content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read {:?}", manifest_path))?;
        let manifest: SkillManifest = serde_json::from_str(&manifest_content)
            .with_context(|| format!("Invalid manifest in {:?}", manifest_path))?;

        // Discover tool scripts under tools/
        let tools_dir = skill_dir.join("tools");
        let tool_names = self.discover_tool_scripts(&tools_dir, skill_name);

        // Register dynamic proxy tools in the global registry
        for tool_name in &tool_names {
            let proxy = DynamicSkillTool {
                name: tool_name.clone(),
                skill_name: skill_name.to_string(),
                skill_dir: skill_dir.clone(),
                script_path: format!("tools/{}.py", tool_name.strip_prefix(&format!("{}.", skill_name)).unwrap_or(tool_name)),
                description: format!("Tool from skill '{}'", skill_name),
            };
            REGISTRY.register(Arc::new(proxy));
        }

        let last_modified = std::fs::metadata(&manifest_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::now());

        let loaded = LoadedSkill {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            skill_dir: skill_dir.clone(),
            manifest,
            tools: tool_names.clone(),
            loaded_at: SystemTime::now(),
            last_modified,
        };

        // Store in loaded map
        {
            let mut skills = self.loaded_skills.write().await;
            skills.insert(skill_name.to_string(), loaded.clone());
        }

        info!(
            skill = skill_name,
            tools_count = tool_names.len(),
            "Skill loaded"
        );

        self.emit_event(SkillEvent::Loaded {
            skill_name: skill_name.to_string(),
        })
        .await;

        Ok(loaded)
    }

    /// Unload a skill – removes its tools from the registry
    pub async fn unload_skill(&self, skill_name: &str) -> Result<()> {
        let loaded = {
            let mut skills = self.loaded_skills.write().await;
            skills.remove(skill_name)
        };

        if let Some(_loaded) = loaded {
            // Unregister all tools provided by this skill
            let toolset_name = format!("skill:{}", skill_name);
            let removed = REGISTRY.unregister_toolset(&toolset_name);

            info!(
                skill = skill_name,
                removed_tools = removed.len(),
                "Skill unloaded"
            );

            self.emit_event(SkillEvent::Unloaded {
                skill_name: skill_name.to_string(),
            })
            .await;
        } else {
            debug!(skill = skill_name, "Skill was not loaded, nothing to unload");
        }

        Ok(())
    }

    /// Reload a skill (unload + load)
    pub async fn reload_skill(&self, skill_name: &str) -> Result<LoadedSkill> {
        info!(skill = skill_name, "Reloading skill");

        self.unload_skill(skill_name).await?;
        let loaded = self.load_skill(skill_name).await?;

        self.emit_event(SkillEvent::Reloaded {
            skill_name: skill_name.to_string(),
        })
        .await;

        Ok(loaded)
    }

    // ------------------------------------------------------------------
    // File watching (hot updates)
    // ------------------------------------------------------------------

    /// Start watching the skills directory for changes
    pub async fn start_watching(&mut self) -> Result<()> {
        let skills_dir = self.skills_dir.clone();
        if !skills_dir.exists() {
            std::fs::create_dir_all(&skills_dir)?;
        }

        let loaded_skills = Arc::clone(&self.loaded_skills);
        let event_callbacks = Arc::clone(&self.event_callbacks);
        let skills_dir_inner = skills_dir.clone();

        // Channel for notify -> tokio bridge
        let (fs_tx, mut fs_rx) = mpsc::unbounded_channel::<PathBuf>();

        // Set up file watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| match res {
                Ok(event) => {
                    let dominated = matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    );
                    if dominated {
                        for path in event.paths {
                            let _ = fs_tx.send(path);
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "File watcher error");
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        watcher.watch(&skills_dir, RecursiveMode::Recursive)?;
        self.watcher = Some(watcher);

        info!(dir = ?skills_dir, "Started watching skills directory");

        // Spawn background task to process file changes
        tokio::spawn(async move {
            // Debounce: collect paths over a short window then process
            let mut pending_skills: HashMap<String, SystemTime> = HashMap::new();
            let debounce = Duration::from_secs(1);

            loop {
                tokio::select! {
                    Some(path) = fs_rx.recv() => {
                        // Extract skill name from changed path
                        if let Some(skill_name) = Self::extract_skill_name(&skills_dir_inner, &path) {
                            pending_skills.insert(skill_name, SystemTime::now());
                        }
                    }
                    _ = tokio::time::sleep(debounce) => {
                        let now = SystemTime::now();
                        let ready: Vec<String> = pending_skills
                            .iter()
                            .filter(|(_, ts)| now.duration_since(**ts).unwrap_or_default() >= debounce)
                            .map(|(name, _)| name.clone())
                            .collect();

                        for skill_name in ready {
                            pending_skills.remove(&skill_name);
                            info!(skill = %skill_name, "Detected change, reloading skill");

                            let skill_dir = skills_dir_inner.join(&skill_name);

                            if !skill_dir.exists() {
                                // Skill directory removed → unload
                                let mut skills = loaded_skills.write().await;
                                if let Some(_loaded) = skills.remove(&skill_name) {
                                    let toolset = format!("skill:{}", skill_name);
                                    REGISTRY.unregister_toolset(&toolset);
                                    let cbs = event_callbacks.read().await;
                                    for cb in cbs.iter() {
                                        cb(SkillEvent::Unloaded { skill_name: skill_name.clone() });
                                    }
                                    info!(skill = %skill_name, "Skill removed (directory deleted)");
                                }
                                continue;
                            }

                            // Skill changed → reload
                            // We need a temp loader for the reload logic
                            let manifest_path = skill_dir.join("skill.json");
                            if !manifest_path.exists() {
                                continue;
                            }

                            // Unregister existing tools
                            let toolset = format!("skill:{}", skill_name);
                            REGISTRY.unregister_toolset(&toolset);

                            // Re-discover and register
                            match Self::load_skill_static(
                                &skill_name,
                                &skill_dir,
                                &loaded_skills,
                            ).await {
                                Ok(_) => {
                                    let cbs = event_callbacks.read().await;
                                    for cb in cbs.iter() {
                                        cb(SkillEvent::Reloaded { skill_name: skill_name.clone() });
                                    }
                                }
                                Err(e) => {
                                    error!(skill = %skill_name, error = %e, "Failed to reload skill");
                                    let cbs = event_callbacks.read().await;
                                    for cb in cbs.iter() {
                                        cb(SkillEvent::Error {
                                            skill_name: skill_name.clone(),
                                            error: e.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop watching for changes
    pub fn stop_watching(&mut self) {
        if self.watcher.take().is_some() {
            info!("Stopped watching skills directory");
        }
    }

    // ------------------------------------------------------------------
    // Event system
    // ------------------------------------------------------------------

    /// Register an event callback
    pub async fn on_event(&self, callback: SkillEventCallback) {
        let mut cbs = self.event_callbacks.write().await;
        cbs.push(callback);
    }

    /// Subscribe to events via a channel
    pub fn subscribe(&mut self) -> mpsc::UnboundedReceiver<SkillEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.event_tx = Some(tx);
        rx
    }

    // ------------------------------------------------------------------
    // Query
    // ------------------------------------------------------------------

    /// Get loaded skills snapshot
    pub async fn get_loaded_skills(&self) -> Vec<LoadedSkill> {
        let skills = self.loaded_skills.read().await;
        skills.values().cloned().collect()
    }

    /// Check if a skill is loaded
    pub async fn is_loaded(&self, skill_name: &str) -> bool {
        let skills = self.loaded_skills.read().await;
        skills.contains_key(skill_name)
    }

    /// Get a loaded skill by name
    pub async fn get_skill(&self, skill_name: &str) -> Option<LoadedSkill> {
        let skills = self.loaded_skills.read().await;
        skills.get(skill_name).cloned()
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Discover tool scripts under a tools/ directory
    fn discover_tool_scripts(&self, tools_dir: &Path, skill_name: &str) -> Vec<String> {
        let mut tools = Vec::new();

        if !tools_dir.exists() {
            return tools;
        }

        if let Ok(entries) = std::fs::read_dir(tools_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "py" || ext == "sh" {
                            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                let tool_name = format!("{}.{}", skill_name, stem);
                                tools.push(tool_name);
                            }
                        }
                    }
                }
            }
        }

        tools
    }

    /// Extract skill name from a file-system path within the skills directory
    fn extract_skill_name(skills_dir: &Path, changed_path: &Path) -> Option<String> {
        changed_path
            .strip_prefix(skills_dir)
            .ok()
            .and_then(|rel| rel.components().next())
            .and_then(|comp| comp.as_os_str().to_str())
            .map(String::from)
    }

    /// Static helper for loading a skill (used by the watcher task)
    async fn load_skill_static(
        skill_name: &str,
        skill_dir: &Path,
        loaded_skills: &Arc<RwLock<HashMap<String, LoadedSkill>>>,
    ) -> Result<LoadedSkill> {
        let manifest_path = skill_dir.join("skill.json");
        let manifest_content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read {:?}", manifest_path))?;
        let manifest: SkillManifest = serde_json::from_str(&manifest_content)?;

        // Discover tools
        let tools_dir = skill_dir.join("tools");
        let mut tool_names = Vec::new();

        if tools_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&tools_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if ext == "py" || ext == "sh" {
                                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                    let tool_name = format!("{}.{}", skill_name, stem);
                                    tool_names.push(tool_name);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Register tools
        for tool_name in &tool_names {
            let proxy = DynamicSkillTool {
                name: tool_name.clone(),
                skill_name: skill_name.to_string(),
                skill_dir: skill_dir.to_path_buf(),
                script_path: format!(
                    "tools/{}.py",
                    tool_name
                        .strip_prefix(&format!("{}.", skill_name))
                        .unwrap_or(tool_name)
                ),
                description: format!("Tool from skill '{}'", skill_name),
            };
            REGISTRY.register(Arc::new(proxy));
        }

        let last_modified = std::fs::metadata(&manifest_path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::now());

        let loaded = LoadedSkill {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            skill_dir: skill_dir.to_path_buf(),
            manifest,
            tools: tool_names,
            loaded_at: SystemTime::now(),
            last_modified,
        };

        {
            let mut skills = loaded_skills.write().await;
            skills.insert(skill_name.to_string(), loaded.clone());
        }

        info!(skill = skill_name, "Skill reloaded");
        Ok(loaded)
    }

    /// Emit a skill event
    async fn emit_event(&self, event: SkillEvent) {
        // Send to channel subscriber
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(event.clone());
        }

        // Notify callbacks
        let cbs = self.event_callbacks.read().await;
        for cb in cbs.iter() {
            cb(event.clone());
        }
    }
}

// ---------------------------------------------------------------------------
// DynamicSkillTool – proxy that delegates execution to a skill script
// ---------------------------------------------------------------------------

/// A dynamically registered tool that proxies calls to a skill script
struct DynamicSkillTool {
    name: String,
    skill_name: String,
    skill_dir: PathBuf,
    script_path: String,
    description: String,
}

#[async_trait::async_trait]
impl Tool for DynamicSkillTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn toolset(&self) -> &str {
        // Convention: "skill:{skill_name}" so we can bulk-unregister
        // We store it as a leaked &str for lifetime purposes (acceptable –
        // skills are seldom unloaded).
        Box::leak(format!("skill:{}", self.skill_name).into_boxed_str())
    }

    fn schema(&self) -> serde_json::Value {
        // Try to read a schema file next to the script
        let schema_path = self
            .skill_dir
            .join(self.script_path.replace(".py", ".json").replace(".sh", ".json"));

        if schema_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&schema_path) {
                if let Ok(schema) = serde_json::from_str::<serde_json::Value>(&content) {
                    return schema;
                }
            }
        }

        // Fallback: generic schema
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "parameters": {
                "type": "object",
                "properties": {
                    "args": {
                        "type": "string",
                        "description": "Arguments to pass to the skill tool"
                    }
                },
                "required": []
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<String> {
        use tokio::process::Command;

        let script_full = self.skill_dir.join(&self.script_path);

        if !script_full.exists() {
            return Err(anyhow::anyhow!("Script not found: {:?}", script_full));
        }

        // Decide interpreter
        let (interpreter, flag) = if self.script_path.ends_with(".py") {
            ("python3", None)
        } else {
            ("bash", Some("-c"))
        };

        let mut cmd = Command::new(interpreter);
        if let Some(f) = flag {
            cmd.arg(f);
        }
        cmd.arg(&script_full);

        // Pass args as JSON via stdin environment variable
        let args_json = serde_json::to_string(&args).unwrap_or_default();
        cmd.env("TOOL_ARGS", &args_json);
        cmd.current_dir(&self.skill_dir);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let output = tokio::time::timeout(
            Duration::from_secs(30),
            cmd.output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Skill tool execution timed out"))?
        .with_context(|| format!("Failed to execute skill tool: {}", self.name))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(stdout)
        } else {
            Err(anyhow::anyhow!(
                "Skill tool '{}' failed (exit {}): {}",
                self.name,
                output.status.code().unwrap_or(-1),
                stderr
            ))
        }
    }
}
