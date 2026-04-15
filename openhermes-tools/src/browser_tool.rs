//! Browser tools (structured stubs) — 10 tools for web browser automation.
//!
//! When the `agent-browser` CLI is detected, these tools shell out to it.
//! Without it, they return a helpful installation message.

use std::process::Stdio;

use anyhow::Result;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde_json::Value;
use tracing::warn;

use crate::registry::Tool;

/// Check if agent-browser CLI is available.
static BROWSER_AVAILABLE: Lazy<bool> = Lazy::new(|| {
    std::process::Command::new("agent-browser")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
});

const NOT_AVAILABLE_MSG: &str =
    "Browser tools require the agent-browser CLI. Install with: `agent-browser install` or `cargo install agent-browser`";

/// Execute a browser action via the agent-browser CLI.
async fn browser_action(action: &str, args: &Value) -> Result<String> {
    if !*BROWSER_AVAILABLE {
        return Ok(serde_json::json!({
            "error": NOT_AVAILABLE_MSG,
            "success": false,
        })
        .to_string());
    }

    let args_str = serde_json::to_string(args)?;
    let output = tokio::process::Command::new("agent-browser")
        .arg(action)
        .arg("--json")
        .arg(&args_str)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        Ok(stdout.to_string())
    } else {
        warn!(action = action, stderr = %stderr, "Browser action failed");
        Ok(serde_json::json!({
            "success": false,
            "error": stderr.to_string(),
        })
        .to_string())
    }
}

// ── Macro for defining browser tools ────────────────────────────────────

macro_rules! browser_tool {
    ($struct_name:ident, $tool_name:expr, $description:expr, $params:expr) => {
        pub struct $struct_name;

        #[async_trait]
        impl Tool for $struct_name {
            fn name(&self) -> &str {
                $tool_name
            }

            fn toolset(&self) -> &str {
                "browser"
            }

            fn schema(&self) -> Value {
                serde_json::json!({
                    "name": $tool_name,
                    "description": $description,
                    "parameters": $params
                })
            }

            async fn execute(&self, args: Value) -> Result<String> {
                browser_action($tool_name, &args).await
            }
        }
    };
}

// ── Tool definitions ────────────────────────────────────────────────────

browser_tool!(
    BrowserNavigateTool,
    "browser_navigate",
    "Navigate the browser to a URL.",
    serde_json::json!({
        "type": "object",
        "properties": {
            "url": { "type": "string", "description": "URL to navigate to" }
        },
        "required": ["url"]
    })
);

browser_tool!(
    BrowserSnapshotTool,
    "browser_snapshot",
    "Take a snapshot (screenshot + accessibility tree) of the current page.",
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
);

browser_tool!(
    BrowserClickTool,
    "browser_click",
    "Click an element on the page by selector or coordinates.",
    serde_json::json!({
        "type": "object",
        "properties": {
            "selector": { "type": "string", "description": "CSS selector or accessibility ID" },
            "x": { "type": "integer", "description": "X coordinate (alternative to selector)" },
            "y": { "type": "integer", "description": "Y coordinate (alternative to selector)" }
        }
    })
);

browser_tool!(
    BrowserTypeTool,
    "browser_type",
    "Type text into the currently focused element.",
    serde_json::json!({
        "type": "object",
        "properties": {
            "text": { "type": "string", "description": "Text to type" },
            "selector": { "type": "string", "description": "CSS selector to focus before typing (optional)" }
        },
        "required": ["text"]
    })
);

browser_tool!(
    BrowserScrollTool,
    "browser_scroll",
    "Scroll the page up or down.",
    serde_json::json!({
        "type": "object",
        "properties": {
            "direction": { "type": "string", "enum": ["up", "down"], "description": "Scroll direction" },
            "amount": { "type": "integer", "description": "Scroll amount in pixels (default: 500)", "default": 500 }
        },
        "required": ["direction"]
    })
);

browser_tool!(
    BrowserBackTool,
    "browser_back",
    "Go back to the previous page.",
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
);

browser_tool!(
    BrowserPressTool,
    "browser_press",
    "Press a keyboard key (e.g., Enter, Tab, Escape).",
    serde_json::json!({
        "type": "object",
        "properties": {
            "key": { "type": "string", "description": "Key to press (e.g., 'Enter', 'Tab', 'Escape', 'ArrowDown')" }
        },
        "required": ["key"]
    })
);

browser_tool!(
    BrowserGetImagesTool,
    "browser_get_images",
    "Get all images on the current page with their URLs and alt text.",
    serde_json::json!({
        "type": "object",
        "properties": {}
    })
);

browser_tool!(
    BrowserVisionTool,
    "browser_vision",
    "Analyze the current page screenshot using a vision model.",
    serde_json::json!({
        "type": "object",
        "properties": {
            "prompt": { "type": "string", "description": "What to analyze on the page", "default": "Describe what you see on this page." }
        }
    })
);

browser_tool!(
    BrowserConsoleTool,
    "browser_console",
    "Execute JavaScript in the browser console and return the result.",
    serde_json::json!({
        "type": "object",
        "properties": {
            "expression": { "type": "string", "description": "JavaScript expression to evaluate" }
        },
        "required": ["expression"]
    })
);

/// Get all browser tool instances.
pub fn all_browser_tools() -> Vec<std::sync::Arc<dyn Tool>> {
    vec![
        std::sync::Arc::new(BrowserNavigateTool),
        std::sync::Arc::new(BrowserSnapshotTool),
        std::sync::Arc::new(BrowserClickTool),
        std::sync::Arc::new(BrowserTypeTool),
        std::sync::Arc::new(BrowserScrollTool),
        std::sync::Arc::new(BrowserBackTool),
        std::sync::Arc::new(BrowserPressTool),
        std::sync::Arc::new(BrowserGetImagesTool),
        std::sync::Arc::new(BrowserVisionTool),
        std::sync::Arc::new(BrowserConsoleTool),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_browser_tools_count() {
        let tools = all_browser_tools();
        assert_eq!(tools.len(), 10);
    }

    #[test]
    fn test_all_browser_tools_unique_names() {
        let tools = all_browser_tools();
        let names: std::collections::HashSet<_> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(names.len(), 10, "All browser tools should have unique names");
    }

    #[test]
    fn test_browser_tools_toolset() {
        let tools = all_browser_tools();
        for tool in &tools {
            assert_eq!(tool.toolset(), "browser");
        }
    }

    #[test]
    fn test_browser_tool_names() {
        let tools = all_browser_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"browser_navigate"));
        assert!(names.contains(&"browser_snapshot"));
        assert!(names.contains(&"browser_click"));
        assert!(names.contains(&"browser_type"));
        assert!(names.contains(&"browser_scroll"));
        assert!(names.contains(&"browser_back"));
        assert!(names.contains(&"browser_press"));
        assert!(names.contains(&"browser_get_images"));
        assert!(names.contains(&"browser_vision"));
        assert!(names.contains(&"browser_console"));
    }
}
