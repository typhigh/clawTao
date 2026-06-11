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
    /// Configured model list (first = active). Backward compat with single `model`.
    #[serde(default)]
    pub models: Vec<String>,
    /// Log level: "trace" | "debug" | "info" | "warn" | "error"
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Commands blocked from Bash tool execution (substring match).
    #[serde(default = "default_blocked_commands")]
    pub bash_blocked_commands: Vec<String>,
    /// LLM API protocol: "openai" or "anthropic"
    #[serde(default = "default_api_protocol")]
    pub api_protocol: String,
    /// Bash command timeout in seconds. None = unlimited.
    #[serde(default = "default_bash_timeout")]
    pub bash_timeout_secs: Option<u64>,
}

pub const DEFAULT_BASH_TIMEOUT_SECS: u64 = 600;


fn default_bash_timeout() -> Option<u64> {
    Some(DEFAULT_BASH_TIMEOUT_SECS)
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
            bash_timeout_secs: default_bash_timeout(),
            models: vec![],
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

    pub fn validate(&self) -> Result<(), String> {
        match self.api_protocol.as_str() {
            "anthropic" => Self::validate_anthropic(&self.base_url, &self.model, &self.api_key),
            _ => Self::validate_openai(&self.base_url, &self.model, &self.api_key),
        }
    }

    /// Test connectivity with explicit credentials.
    pub fn test_connection(base_url: &str, model: &str, api_key: &str, api_protocol: &str) -> Result<(), String> {
        match api_protocol {
            "anthropic" => Self::validate_anthropic(base_url, model, api_key),
            _ => Self::validate_openai(base_url, model, api_key),
        }
    }

    fn validate_openai(base_url: &str, model: &str, api_key: &str) -> Result<(), String> {
        let client = reqwest::blocking::Client::new();
        let base = base_url.trim_end_matches('/');
        let models_url = format!("{base}/models");

        tracing::debug!("validate_openai: GET {models_url}");
        if let Ok(resp) = client.get(&models_url)
            .header("Authorization", format!("Bearer {api_key}"))
            .send()
        {
            let status = resp.status();
            tracing::debug!("validate_openai /models: status={status}");
            if status.is_success() || status.as_u16() == 429 { return Ok(()); }
            if status.as_u16() == 401 || status.as_u16() == 403 { return Err("Invalid API key".into()); }
        }

        let probe_url = format!("{base}/chat/completions");
        tracing::debug!("validate_openai probe: POST {probe_url}");
        let resp = client.post(&probe_url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&serde_json::json!({
                "model": model, "messages": [{"role": "user", "content": "hi"}],
                "max_tokens": 1, "stream": false,
            })).map_err(|e| e.to_string())?)
            .send()
            .map_err(|e| format!("Network error: {e}"))?;
        tracing::debug!("validate_openai probe: status={}", resp.status());
        Self::check_probe_response(resp)
    }

    fn validate_anthropic(base_url: &str, model: &str, api_key: &str) -> Result<(), String> {
        let client = reqwest::blocking::Client::new();
        let base = base_url.trim_end_matches('/');
        let headers = |r: reqwest::blocking::RequestBuilder| {
            r.header("x-api-key", api_key).header("anthropic-version", "2023-06-01")
        };
        let models_url = format!("{base}/v1/models?limit=1");

        tracing::debug!("validate_anthropic: GET {models_url}");
        if let Ok(resp) = headers(client.get(&models_url)).send() {
            let status = resp.status();
            tracing::debug!("validate_anthropic /models: status={status}");
            if status.is_success() || status.as_u16() == 429 { return Ok(()); }
            if status.as_u16() == 401 || status.as_u16() == 403 { return Err("Invalid API key".into()); }
        }

        let probe_url = format!("{base}/v1/messages");
        tracing::debug!("validate_anthropic probe: POST {probe_url}");
        let resp = headers(client.post(&probe_url)
            .header("Content-Type", "application/json"))
            .body(serde_json::to_string(&serde_json::json!({
                "model": model, "messages": [{"role": "user", "content": "hi"}],
                "max_tokens": 1, "stream": false,
            })).map_err(|e| e.to_string())?)
            .send()
            .map_err(|e| format!("Network error: {e}"))?;
        tracing::debug!("validate_anthropic probe: status={}", resp.status());
        Self::check_probe_response(resp)
    }

    fn check_probe_response(resp: reqwest::blocking::Response) -> Result<(), String> {
        let status = resp.status();
        if status.as_u16() == 429 { return Ok(()); }
        let body = resp.text().unwrap_or_default();
        tracing::debug!("check_probe_response: status={status} body={:.200}", body);
        if !status.is_success() {
            return Err(format!("HTTP {}: {}", status.as_u16(), body));
        }
        if let Ok(error) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(msg) = error["error"]["message"].as_str() {
                return Err(msg.to_string());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
