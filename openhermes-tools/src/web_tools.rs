//! Web search tools supporting multiple backends.

use std::env;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::Tool;

/// Web search backend
#[derive(Debug, Clone)]
enum SearchBackend {
    Tavily,
    DuckDuckGo,
}

impl SearchBackend {
    fn from_env() -> Self {
        if env::var("TAVILY_API_KEY").is_ok() {
            SearchBackend::Tavily
        } else {
            // Default to DuckDuckGo (no API key required)
            SearchBackend::DuckDuckGo
        }
    }
}

/// Web search tool
pub struct WebSearchTool {
    backend: SearchBackend,
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            backend: SearchBackend::from_env(),
        }
    }

    async fn search_tavily(&self, query: &str, max_results: usize) -> Result<String> {
        let api_key = env::var("TAVILY_API_KEY")
            .map_err(|_| anyhow::anyhow!("TAVILY_API_KEY not set"))?;

        let client = reqwest::Client::new();
        let response = client
            .post("https://api.tavily.com/search")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "api_key": api_key,
                "query": query,
                "max_results": max_results,
                "search_depth": "advanced",
                "include_answer": true,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Tavily API error: {}", response.status()));
        }

        let json: Value = response.json().await?;
        
        // Format results
        let mut results = Vec::new();
        
        if let Some(answer) = json.get("answer").and_then(|a| a.as_str()) {
            results.push(format!("## Answer\n\n{}", answer));
        }
        
        if let Some(results_array) = json.get("results").and_then(|r| r.as_array()) {
            results.push("## Search Results\n".to_string());
            for (i, result) in results_array.iter().enumerate() {
                let title = result.get("title").and_then(|t| t.as_str()).unwrap_or("");
                let url = result.get("url").and_then(|u| u.as_str()).unwrap_or("");
                let content = result.get("content").and_then(|c| c.as_str()).unwrap_or("");
                
                results.push(format!(
                    "### {}. {}\n{}\nURL: {}",
                    i + 1,
                    title,
                    content,
                    url
                ));
            }
        }

        Ok(results.join("\n\n"))
    }

    async fn search_duckduckgo(&self, query: &str, max_results: usize) -> Result<String> {
        // Use DuckDuckGo HTML search (no API key required)
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
            .build()?;

        let encoded_query = urlencoding::encode(query);
        let url = format!("https://html.duckduckgo.com/html/?q={}", encoded_query);

        let response = client.get(&url).send().await?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("DuckDuckGo error: {}", response.status()));
        }

        let html = response.text().await?;
        
        // Simple HTML parsing to extract results
        let results = self.parse_duckduckgo_results(&html, max_results)?;
        
        if results.is_empty() {
            return Ok("No results found.".to_string());
        }

        Ok(format!("## Search Results for: {}\n\n{}", query, results.join("\n\n")))
    }

    fn parse_duckduckgo_results(&self, html: &str, max_results: usize) -> Result<Vec<String>> {
        let mut results = Vec::new();
        
        // Simple regex-based extraction (production should use proper HTML parser)
        let result_regex = regex::Regex::new(
            r#"<a rel="nofollow" class="result__a" href="([^"]+)">([^<]+)</a>.*?<a class="result__snippet"[^>]*>([^<]*(?:<[^>]+>[^<]*</a>[^<]*)*)</a>"#
        )?;

        for (i, cap) in result_regex.captures_iter(html).enumerate().take(max_results) {
            let url = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let title = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let snippet = cap.get(3).map(|m| m.as_str()).unwrap_or("");
            
            // Clean HTML tags from snippet
            let clean_snippet = regex::Regex::new(r"<[^>]+>")?
                .replace_all(snippet, "")
                .to_string();

            results.push(format!(
                "### {}. {}\n{}\nURL: {}",
                i + 1,
                title,
                clean_snippet.trim(),
                url
            ));
        }

        Ok(results)
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn toolset(&self) -> &str {
        "web"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "web_search",
            "description": "Search the web for information using multiple backends (Tavily, DuckDuckGo). Returns formatted search results with titles, snippets, and URLs.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let query = args["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;
        
        let max_results = args["max_results"].as_u64().unwrap_or(5) as usize;

        tracing::info!(
            backend = match self.backend {
                SearchBackend::Tavily => "tavily",
                SearchBackend::DuckDuckGo => "duckduckgo",
            },
            query = query,
            "Executing web search"
        );

        match self.backend {
            SearchBackend::Tavily => self.search_tavily(query, max_results).await,
            SearchBackend::DuckDuckGo => self.search_duckduckgo(query, max_results).await,
        }
    }
}

/// Web extract tool - extract content from web pages
pub struct WebExtractTool;

#[async_trait]
impl Tool for WebExtractTool {
    fn name(&self) -> &str {
        "web_extract"
    }

    fn toolset(&self) -> &str {
        "web"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "web_extract",
            "description": "Extract and summarize content from web pages. Returns cleaned text content with metadata.",
            "parameters": {
                "type": "object",
                "properties": {
                    "urls": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "List of URLs to extract content from"
                    },
                    "format": {
                        "type": "string",
                        "description": "Output format (text, markdown)",
                        "enum": ["text", "markdown"],
                        "default": "markdown"
                    }
                },
                "required": ["urls"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let urls = args["urls"].as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: urls"))?;

        if urls.is_empty() {
            return Err(anyhow::anyhow!("urls array is empty"));
        }

        let format = args["format"].as_str().unwrap_or("markdown");
        let mut results = Vec::new();

        for url_value in urls {
            let url = url_value.as_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid URL in array"))?;

            match self.extract_url(url, format).await {
                Ok(content) => results.push(content),
                Err(e) => results.push(format!("Failed to extract {}: {}", url, e)),
            }
        }

        Ok(results.join("\n\n---\n\n"))
    }
}

impl WebExtractTool {
    async fn extract_url(&self, url: &str, format: &str) -> Result<String> {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (compatible; OpenHermes/1.0)")
            .build()?;

        let response = client.get(url).send().await?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("HTTP error: {}", response.status()));
        }

        let html = response.text().await?;
        
        // Simple HTML to text conversion
        let text = self.html_to_text(&html)?;

        let title = self.extract_title(&html).unwrap_or_else(|| url.to_string());

        if format == "markdown" {
            Ok(format!("# {}\n\nURL: {}\n\n{}", title, url, text))
        } else {
            Ok(format!("Title: {}\nURL: {}\n\n{}", title, url, text))
        }
    }

    fn html_to_text(&self, html: &str) -> Result<String> {
        // Remove script and style elements
        let html = regex::Regex::new(r"(?s)<script[^>]*>.*?</script>")?
            .replace_all(html, "");
        let html = regex::Regex::new(r"(?s)<style[^>]*>.*?</style>")?
            .replace_all(&html, "");
        
        // Remove all HTML tags
        let text = regex::Regex::new(r"<[^>]+>")?
            .replace_all(&html, "");
        
        // Decode HTML entities
        let text = text
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&nbsp;", " ");
        
        // Clean up whitespace
        let text = regex::Regex::new(r"\n\s*\n")?
            .replace_all(&text, "\n\n");
        
        Ok(text.trim().to_string())
    }

    fn extract_title(&self, html: &str) -> Option<String> {
        regex::Regex::new(r"<title[^>]*>([^<]+)</title>")
            .ok()?
            .captures(html)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().trim().to_string())
    }
}

/// URL safety check tool
pub struct UrlSafetyTool;

#[async_trait]
impl Tool for UrlSafetyTool {
    fn name(&self) -> &str {
        "url_safety_check"
    }

    fn toolset(&self) -> &str {
        "web"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "url_safety_check",
            "description": "Check if a URL is safe to visit. Returns safety assessment with risk factors.",
            "parameters": {
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "URL to check for safety"
                    }
                },
                "required": ["url"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let url_str = args["url"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: url"))?;

        let result = self.check_safety(url_str)?;
        
        Ok(serde_json::json!({
            "url": url_str,
            "safe": result.safe,
            "risk_level": result.risk_level,
            "warnings": result.warnings,
            "recommendation": result.recommendation
        }).to_string())
    }
}

struct SafetyResult {
    safe: bool,
    risk_level: String,
    warnings: Vec<String>,
    recommendation: String,
}

impl UrlSafetyTool {
    fn check_safety(&self, url_str: &str) -> Result<SafetyResult> {
        let parsed = url::Url::parse(url_str)
            .map_err(|e| anyhow::anyhow!("Invalid URL: {}", e))?;

        let mut warnings = Vec::new();
        let mut risk_score = 0;

        // Check scheme
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            warnings.push(format!("Non-standard scheme: {}", parsed.scheme()));
            risk_score += 3;
        }

        // Check for localhost/private IPs
        if let Some(host) = parsed.host_str() {
            if host == "localhost" || host == "127.0.0.1" || host == "0.0.0.0" {
                warnings.push("URL points to localhost".to_string());
                risk_score += 5;
            }

            // Check for private IP ranges
            if host.starts_with("192.168.") || 
               host.starts_with("10.") || 
               host.starts_with("172.") {
                warnings.push("URL points to private network".to_string());
                risk_score += 4;
            }
        }

        // Check for suspicious patterns in URL
        let url_lower = url_str.to_lowercase();
        if url_lower.contains("phishing") || 
           url_lower.contains("malware") ||
           url_lower.contains("exploit") {
            warnings.push("URL contains suspicious keywords".to_string());
            risk_score += 5;
        }

        // Check for very long URLs (potential buffer overflow attempts)
        if url_str.len() > 2000 {
            warnings.push("URL is unusually long".to_string());
            risk_score += 2;
        }

        // Check for encoded characters
        if url_str.contains("%2e") || url_str.contains("%2f") || url_str.contains("%00") {
            warnings.push("URL contains encoded special characters".to_string());
            risk_score += 3;
        }

        let (safe, risk_level, recommendation) = if risk_score == 0 {
            (true, "low".to_string(), "URL appears safe to visit".to_string())
        } else if risk_score <= 3 {
            (true, "medium".to_string(), "URL has minor concerns, proceed with caution".to_string())
        } else if risk_score <= 6 {
            (false, "high".to_string(), "URL has significant risks, avoid visiting".to_string())
        } else {
            (false, "critical".to_string(), "URL is potentially dangerous, do not visit".to_string())
        };

        Ok(SafetyResult {
            safe,
            risk_level,
            warnings,
            recommendation,
        })
    }
}
