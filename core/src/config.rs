//! LLM provider configuration — persisted to disk.
//!
//! Stored as JSON at `{data_local_dir}/clawtao/config.json`.
//! Falls back to `OPENAI_API_KEY` etc. env vars on first run (one-shot migration).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";
const DEFAULT_LOG_LEVEL: &str = "info";

/// Persistent application configuration.
/// Covers LLM provider plus logging behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    /// Log level: "trace" | "debug" | "info" | "warn" | "error"
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    DEFAULT_LOG_LEVEL.into()
}

impl LlmConfig {
    /// Effective log level: RUST_LOG env var wins, then persisted config, then "info".
    pub fn effective_log_level(&self) -> String {
        std::env::var("RUST_LOG").unwrap_or_else(|_| {
            if self.log_level.is_empty() { DEFAULT_LOG_LEVEL.into() } else { self.log_level.clone() }
        })
    }
}

impl LlmConfig {
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
        }
    }

    /// Write config to disk.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, json)?;
        tracing::info!("Config saved to {}", path.display());
        Ok(())
    }

    /// Return a copy with the API key partially masked for safe display.
    pub fn masked(&self) -> Self {
        let mask = |key: &str| {
            if key.len() <= 8 {
                "***".into()
            } else {
                format!("{}...{}", &key[..4], &key[key.len()-4..])
            }
        };
        Self {
            api_key: mask(&self.api_key),
            ..self.clone()
        }
    }

    /// Validate by making a lightweight API call.
    pub fn validate(&self) -> Result<(), String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 5,
            "stream": false,
        });

        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&body).map_err(|e| e.to_string())?)
            .send()
            .map_err(|e| format!("Network error: {e}"))?;

        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = resp.text().unwrap_or_default();
            Err(format!("{}: {}", status.as_u16(), body))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_log_level_is_info() {
        let config = LlmConfig {
            log_level: String::new(),
            provider: "openai".into(),
            api_key: "sk-test".into(),
            base_url: DEFAULT_OPENAI_BASE_URL.into(),
            model: DEFAULT_OPENAI_MODEL.into(),
        };
        assert_eq!(config.effective_log_level(), "info");
    }

    #[test]
    fn persisted_log_level_overrides_default() {
        let config = LlmConfig {
            log_level: "debug".into(),
            provider: "openai".into(),
            api_key: "sk-test".into(),
            base_url: DEFAULT_OPENAI_BASE_URL.into(),
            model: DEFAULT_OPENAI_MODEL.into(),
        };
        assert_eq!(config.effective_log_level(), "debug");
    }

    #[test]
    fn masked_key() {
        let config = LlmConfig {
            log_level: "info".into(),
            provider: "openai".into(),
            api_key: "sk-1234567890abcdef".into(),
            base_url: DEFAULT_OPENAI_BASE_URL.into(),
            model: DEFAULT_OPENAI_MODEL.into(),
        };
        assert_eq!(config.masked().api_key, "sk-1...cdef");
    }
}
