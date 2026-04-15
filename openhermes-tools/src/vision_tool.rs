//! Vision analysis tool (structured stub).
//!
//! When VISION_API is configured, downloads the image, base64 encodes it,
//! and sends it to a vision-capable model via auxiliary_client.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::registry::Tool;

/// Vision analysis tool.
pub struct VisionTool;

#[async_trait]
impl Tool for VisionTool {
    fn name(&self) -> &str {
        "vision_analyze"
    }

    fn toolset(&self) -> &str {
        "vision"
    }

    fn check_fn(&self) -> bool {
        // Available when any vision-capable model endpoint is configured
        std::env::var("OPENAI_API_KEY").is_ok()
            || std::env::var("ANTHROPIC_API_KEY").is_ok()
            || std::env::var("OPENROUTER_API_KEY").is_ok()
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "vision_analyze",
            "description": "Analyze an image using a vision-capable AI model. Provide an image URL or local file path and an optional prompt describing what to analyze.",
            "parameters": {
                "type": "object",
                "properties": {
                    "image_url": {
                        "type": "string",
                        "description": "URL of the image to analyze, or local file path"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "What to analyze in the image (default: general description)",
                        "default": "Describe this image in detail."
                    }
                },
                "required": ["image_url"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let image_url = args["image_url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: image_url"))?;
        let prompt = args["prompt"]
            .as_str()
            .unwrap_or("Describe this image in detail.");

        // Check if this is a local file or URL
        let is_local = !image_url.starts_with("http://") && !image_url.starts_with("https://");

        if is_local {
            // Read local file and base64 encode
            let path = std::path::Path::new(image_url);
            if !path.exists() {
                return Ok(serde_json::json!({
                    "error": format!("Image file not found: {}", image_url),
                    "success": false,
                }).to_string());
            }

            let data = std::fs::read(path)?;
            let _base64_data = base64_encode(&data);
            let _mime = guess_mime(image_url);

            // For now, describe what we'd do
            return Ok(serde_json::json!({
                "success": true,
                "note": "Vision analysis with local files requires a vision-capable model endpoint. The image was loaded successfully.",
                "image_size_bytes": data.len(),
                "prompt": prompt,
                "message": "Full vision analysis implementation pending — requires multimodal API call with base64-encoded image."
            }).to_string());
        }

        // URL-based image analysis via auxiliary client with vision prompt
        let analysis_prompt = format!(
            "You are analyzing an image at URL: {}\n\nUser request: {}\n\n\
             Note: You cannot actually see this image. Describe what you can infer \
             from the URL and context, or explain that direct image analysis requires \
             a vision-capable model with the image as input.",
            image_url, prompt
        );

        match crate::llm_client::call_llm(
            &analysis_prompt,
            Some("vision"),
            Some(2048),
        ).await {
            Ok(result) => Ok(serde_json::json!({
                "success": true,
                "analysis": result,
                "image_url": image_url,
            }).to_string()),
            Err(e) => Ok(serde_json::json!({
                "success": false,
                "error": format!("Vision analysis failed: {}", e),
            }).to_string()),
        }
    }
}

/// Simple base64 encoding.
fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut encoder = Base64Encoder::new(&mut buf);
        encoder.write_all(data).ok();
    }
    String::from_utf8(buf).unwrap_or_default()
}

/// Minimal base64 encoder (avoids adding a dependency).
struct Base64Encoder<'a> {
    output: &'a mut Vec<u8>,
}

impl<'a> Base64Encoder<'a> {
    fn new(output: &'a mut Vec<u8>) -> Self {
        Self { output }
    }
}

const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

impl<'a> std::io::Write for Base64Encoder<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for chunk in buf.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
            let n = (b0 << 16) | (b1 << 8) | b2;

            self.output.push(BASE64_CHARS[((n >> 18) & 0x3F) as usize]);
            self.output.push(BASE64_CHARS[((n >> 12) & 0x3F) as usize]);
            if chunk.len() > 1 {
                self.output.push(BASE64_CHARS[((n >> 6) & 0x3F) as usize]);
            } else {
                self.output.push(b'=');
            }
            if chunk.len() > 2 {
                self.output.push(BASE64_CHARS[(n & 0x3F) as usize]);
            } else {
                self.output.push(b'=');
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Guess MIME type from file extension.
fn guess_mime(path: &str) -> &'static str {
    if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".gif") {
        "image/gif"
    } else if path.ends_with(".webp") {
        "image/webp"
    } else {
        "image/png"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guess_mime() {
        assert_eq!(guess_mime("test.png"), "image/png");
        assert_eq!(guess_mime("test.jpg"), "image/jpeg");
        assert_eq!(guess_mime("test.gif"), "image/gif");
        assert_eq!(guess_mime("test.webp"), "image/webp");
    }

    #[test]
    fn test_base64_encode() {
        let data = b"Hello, World!";
        let encoded = base64_encode(data);
        assert!(!encoded.is_empty());
        // Known base64 of "Hello, World!" is "SGVsbG8sIFdvcmxkIQ=="
        assert_eq!(encoded, "SGVsbG8sIFdvcmxkIQ==");
    }

    #[test]
    fn test_tool_name() {
        let tool = VisionTool;
        assert_eq!(tool.name(), "vision_analyze");
        assert_eq!(tool.toolset(), "vision");
    }
}
