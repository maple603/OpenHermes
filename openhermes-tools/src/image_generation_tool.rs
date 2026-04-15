//! Image generation tool (structured stub).
//!
//! When FAL_KEY is set, generates images via fal.ai FLUX 2 Pro API.
//! Without configuration, returns a helpful error.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, info};

use crate::registry::Tool;

/// Default image generation endpoint (fal.ai FLUX 2 Pro).
const FAL_ENDPOINT: &str = "https://queue.fal.run/fal-ai/flux-pro/v1.1";

/// Image generation tool.
pub struct ImageGenerationTool;

#[async_trait]
impl Tool for ImageGenerationTool {
    fn name(&self) -> &str {
        "image_generate"
    }

    fn toolset(&self) -> &str {
        "image_gen"
    }

    fn check_fn(&self) -> bool {
        std::env::var("FAL_KEY").is_ok()
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "image_generate",
            "description": "Generate an image from a text prompt using FLUX 2 Pro (fal.ai). Requires FAL_KEY environment variable.",
            "parameters": {
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Text description of the image to generate"
                    },
                    "image_size": {
                        "type": "string",
                        "description": "Image size: 'square_hd', 'landscape_4_3', 'landscape_16_9', 'portrait_4_3', 'portrait_16_9'",
                        "default": "landscape_4_3"
                    },
                    "num_images": {
                        "type": "integer",
                        "description": "Number of images to generate (1-4)",
                        "default": 1
                    }
                },
                "required": ["prompt"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: prompt"))?;
        let image_size = args["image_size"].as_str().unwrap_or("landscape_4_3");
        let num_images = args["num_images"].as_u64().unwrap_or(1).min(4) as usize;

        let fal_key = match std::env::var("FAL_KEY") {
            Ok(k) => k,
            Err(_) => {
                return Ok(serde_json::json!({
                    "error": "FAL_KEY environment variable not set. Set it to use image generation.",
                    "success": false,
                    "setup": "Get a key at https://fal.ai/dashboard/keys and set FAL_KEY=<your-key>"
                }).to_string());
            }
        };

        info!(prompt_len = prompt.len(), size = image_size, count = num_images, "Generating image");

        let client = reqwest::Client::new();
        let resp = client
            .post(FAL_ENDPOINT)
            .header("Authorization", format!("Key {}", fal_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "prompt": prompt,
                "image_size": image_size,
                "num_images": num_images,
                "enable_safety_checker": true,
            }))
            .send()
            .await?;

        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or_default();

        if status.is_success() {
            let images: Vec<String> = body["images"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .filter_map(|img| img["url"].as_str().map(String::from))
                .collect();

            // Save images locally
            let save_dir = openhermes_constants::get_hermes_dir("cache/images", "image_cache");
            let _ = std::fs::create_dir_all(&save_dir);

            let mut saved_paths = Vec::new();
            for (i, url) in images.iter().enumerate() {
                let filename = format!("gen_{}_{}.png", chrono::Utc::now().format("%Y%m%d_%H%M%S"), i);
                let save_path = save_dir.join(&filename);
                
                // Download and save
                if let Ok(img_resp) = reqwest::get(url).await {
                    if let Ok(bytes) = img_resp.bytes().await {
                        if std::fs::write(&save_path, &bytes).is_ok() {
                            saved_paths.push(save_path.display().to_string());
                            debug!(path = %save_path.display(), "Image saved");
                        }
                    }
                }
            }

            Ok(serde_json::json!({
                "success": true,
                "images": images,
                "saved_paths": saved_paths,
                "count": images.len(),
                "prompt": prompt,
            }).to_string())
        } else {
            Ok(serde_json::json!({
                "success": false,
                "error": body.to_string(),
                "status": status.as_u16(),
            }).to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = ImageGenerationTool;
        assert_eq!(tool.name(), "image_generate");
        assert_eq!(tool.toolset(), "image_gen");
    }

    #[test]
    fn test_check_fn_without_key() {
        // FAL_KEY is not set in test env
        if std::env::var("FAL_KEY").is_err() {
            let tool = ImageGenerationTool;
            assert!(!tool.check_fn());
        }
    }
}
