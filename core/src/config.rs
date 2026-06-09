//! LLM provider configuration — persisted to disk.
//!
//! Stored as JSON at `{data_local_dir}/clawtao/config.json`.
//! Falls back to `OPENAI_API_KEY` etc. env vars on first run (one-shot migration).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn serde_error(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
}

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";
const DEFAULT_LOG_LEVEL: &str = "info";

/// Persistent application configuration.
/// Covers LLM provider plus logging behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    #[serde(default)]
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    /// Log level: "trace" | "debug" | "info" | "warn" | "error"
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Commands blocked from Bash tool execution (substring match).
    #[serde(default = "default_blocked_commands")]
    pub bash_blocked_commands: Vec<String>,
    /// LLM API protocol: "openai" or "anthropic"
    #[serde(default = "default_api_protocol")]
    pub api_protocol: String,
}

fn default_api_protocol() -> String {
    "openai".into()
}

fn default_blocked_commands() -> Vec<String> {
    vec![
        "rm -rf /".into(),
        "rm -rf /*".into(),
        "rm -rf ~".into(),
        "sudo rm".into(),
        "mkfs.".into(),
        "dd if=".into(),
        ":(){ :|:& };:".into(),
        "chmod 777 /".into(),
        "> /dev/sda".into(),
        "> /dev/nvme".into(),
        "format c:".into(),
    ]
}

fn default_log_level() -> String {
    DEFAULT_LOG_LEVEL.into()
}

impl LlmConfig {
    pub fn effective_log_level(&self) -> String {
        if self.log_level.is_empty() { DEFAULT_LOG_LEVEL.into() } else { self.log_level.clone() }
    }

    /// Full path to the config file.
    fn path() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("clawtao")
            .join("config.json")
    }

    /// Load from disk. Falls back to env vars on first run.
    pub fn load() -> Self {
        let path = Self::path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<Self>(&content) {
                tracing::info!("Loaded config from {}", path.display());
                return config;
            }
        }

        // First run — try env vars, then defaults
        let config = Self::from_env_or_default();
        tracing::info!("No config file found, saving default to {}", path.display());
        if let Err(e) = config.save() {
            tracing::error!("Failed to save initial config: {e}");
        }
        config
    }

    /// Try env vars first, then hard-coded defaults.
    fn from_env_or_default() -> Self {
        Self {
            api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            base_url: std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_OPENAI_BASE_URL.into()),
            model: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| DEFAULT_OPENAI_MODEL.into()),
            provider: "openai".into(),
            log_level: DEFAULT_LOG_LEVEL.into(),
            bash_blocked_commands: default_blocked_commands(),
            api_protocol: default_api_protocol(),
        }
    }

    /// Write config to disk. api_key is excluded (managed by Electron safeStorage).
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut value = serde_json::to_value(self).map_err(serde_error)?;
        value.as_object_mut().and_then(|o| o.remove("api_key"));
        let json = serde_json::to_string_pretty(&value).map_err(serde_error)?;
        std::fs::write(&path, json)?;
        tracing::info!("Config saved to {}", path.display());
        Ok(())
    }

    /// Return a copy with the API key masked for safe display (e.g. "sk-1****cdef").
    pub fn masked(&self) -> Self {
        let mask = |key: &str| {
            if key.len() <= 8 {
                "***".into()
            } else {
                format!("{}**{}", &key[..4], &key[key.len()-4..])
            }
        };
        Self {
            api_key: mask(&self.api_key),
            ..self.clone()
        }
    }

    /// Validate by making a lightweight API call.
    pub fn validate(&self) -> Result<(), String> {
        Self::test_connection(&self.base_url, &self.model, &self.api_key)
    }

    /// Test connectivity with explicit credentials (does not modify config).
    pub fn test_connection(base_url: &str, model: &str, api_key: &str) -> Result<(), String> {
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 5,
            "stream": false,
        });

        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&body).map_err(|e| e.to_string())?)
            .send()
            .map_err(|e| format!("Network error: {e}"))?;

        let status = resp.status();
        let resp_body = resp.text().unwrap_or_default();

        // Check HTTP status
        if !status.is_success() {
            return Err(format!("HTTP {}: {}", status.as_u16(), resp_body));
        }

        // Check response body for API error (some servers return 200 with error JSON)
        if let Ok(error) = serde_json::from_str::<serde_json::Value>(&resp_body) {
            if error.get("error").is_some() {
                let msg = error["error"].get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                return Err(msg.to_string());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
