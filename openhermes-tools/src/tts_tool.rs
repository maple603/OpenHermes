//! Text-to-speech tool (structured stub).
//!
//! Supports multiple TTS providers: Edge TTS (free, default), OpenAI TTS,
//! and ElevenLabs. Returns the path to the generated audio file.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::info;

use crate::registry::Tool;

/// TTS tool.
pub struct TtsTool;

#[async_trait]
impl Tool for TtsTool {
    fn name(&self) -> &str {
        "text_to_speech"
    }

    fn toolset(&self) -> &str {
        "tts"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "text_to_speech",
            "description": "Convert text to speech audio. Supports multiple providers: 'edge' (free, default), 'openai' (requires OPENAI_API_KEY), 'elevenlabs' (requires ELEVENLABS_API_KEY).",
            "parameters": {
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to convert to speech"
                    },
                    "voice": {
                        "type": "string",
                        "description": "Voice name/ID (provider-specific, default: auto-select)",
                        "default": "auto"
                    },
                    "provider": {
                        "type": "string",
                        "description": "TTS provider: 'edge', 'openai', or 'elevenlabs'",
                        "enum": ["edge", "openai", "elevenlabs"],
                        "default": "edge"
                    },
                    "output_format": {
                        "type": "string",
                        "description": "Output format: 'mp3' or 'wav'",
                        "default": "mp3"
                    }
                },
                "required": ["text"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let text = args["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: text"))?;
        let provider = args["provider"].as_str().unwrap_or("edge");
        let voice = args["voice"].as_str().unwrap_or("auto");
        let format = args["output_format"].as_str().unwrap_or("mp3");

        info!(provider = provider, text_len = text.len(), "Generating speech");

        match provider {
            "edge" => {
                // Edge TTS requires the edge-tts Python package or equivalent
                Ok(serde_json::json!({
                    "success": false,
                    "error": "Edge TTS not yet implemented in Rust. Install edge-tts Python package and use code execution tool, or configure OpenAI/ElevenLabs provider.",
                    "suggestion": "pip install edge-tts && edge-tts --voice en-US-AriaNeural --text '<text>' --write-media output.mp3"
                }).to_string())
            }
            "openai" => {
                let api_key = match std::env::var("OPENAI_API_KEY") {
                    Ok(k) => k,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "error": "OPENAI_API_KEY not set. Required for OpenAI TTS.",
                            "success": false,
                        }).to_string());
                    }
                };

                let voice = if voice == "auto" { "alloy" } else { voice };
                let base_url = std::env::var("OPENAI_BASE_URL")
                    .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

                let client = reqwest::Client::new();
                let resp = client
                    .post(format!("{}/audio/speech", base_url))
                    .header("Authorization", format!("Bearer {}", api_key))
                    .json(&serde_json::json!({
                        "model": "tts-1",
                        "input": text,
                        "voice": voice,
                        "response_format": format,
                    }))
                    .send()
                    .await?;

                if resp.status().is_success() {
                    let bytes = resp.bytes().await?;
                    let save_dir = openhermes_constants::get_hermes_dir("cache/audio", "audio_cache");
                    let _ = std::fs::create_dir_all(&save_dir);
                    let filename = format!("tts_{}.{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"), format);
                    let save_path = save_dir.join(&filename);
                    std::fs::write(&save_path, &bytes)?;

                    Ok(serde_json::json!({
                        "success": true,
                        "file_path": save_path.display().to_string(),
                        "size_bytes": bytes.len(),
                        "provider": "openai",
                        "voice": voice,
                    }).to_string())
                } else {
                    let body: Value = resp.json().await.unwrap_or_default();
                    Ok(serde_json::json!({
                        "success": false,
                        "error": body.to_string(),
                    }).to_string())
                }
            }
            "elevenlabs" => {
                let api_key = match std::env::var("ELEVENLABS_API_KEY") {
                    Ok(k) => k,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "error": "ELEVENLABS_API_KEY not set. Required for ElevenLabs TTS.",
                            "success": false,
                        }).to_string());
                    }
                };

                let voice_id = if voice == "auto" {
                    "21m00Tcm4TlvDq8ikWAM" // Rachel (default)
                } else {
                    voice
                };

                let client = reqwest::Client::new();
                let resp = client
                    .post(format!(
                        "https://api.elevenlabs.io/v1/text-to-speech/{}",
                        voice_id
                    ))
                    .header("xi-api-key", &api_key)
                    .json(&serde_json::json!({
                        "text": text,
                        "model_id": "eleven_monolingual_v1",
                    }))
                    .send()
                    .await?;

                if resp.status().is_success() {
                    let bytes = resp.bytes().await?;
                    let save_dir = openhermes_constants::get_hermes_dir("cache/audio", "audio_cache");
                    let _ = std::fs::create_dir_all(&save_dir);
                    let filename = format!("tts_{}.mp3", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
                    let save_path = save_dir.join(&filename);
                    std::fs::write(&save_path, &bytes)?;

                    Ok(serde_json::json!({
                        "success": true,
                        "file_path": save_path.display().to_string(),
                        "size_bytes": bytes.len(),
                        "provider": "elevenlabs",
                        "voice_id": voice_id,
                    }).to_string())
                } else {
                    let body: Value = resp.json().await.unwrap_or_default();
                    Ok(serde_json::json!({
                        "success": false,
                        "error": body.to_string(),
                    }).to_string())
                }
            }
            _ => Ok(serde_json::json!({
                "error": format!("Unknown TTS provider: {}. Use: edge, openai, elevenlabs", provider),
                "success": false,
            }).to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = TtsTool;
        assert_eq!(tool.name(), "text_to_speech");
        assert_eq!(tool.toolset(), "tts");
    }
}
