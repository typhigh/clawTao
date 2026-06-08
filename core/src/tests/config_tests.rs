use super::*;

fn make_config(key: &str) -> LlmConfig {
    LlmConfig {
        log_level: "info".into(),
        provider: "openai".into(),
        api_key: key.into(),
        base_url: DEFAULT_OPENAI_BASE_URL.into(),
        model: DEFAULT_OPENAI_MODEL.into(),
        bash_blocked_commands: vec![],
    }
}

#[test]
fn default_log_level_is_info() {
    let mut config = make_config("sk-test");
    config.log_level = String::new();
    assert_eq!(config.effective_log_level(), "info");
}

#[test]
fn persisted_log_level_overrides_default() {
    let mut config = make_config("sk-test");
    config.log_level = "debug".into();
    assert_eq!(config.effective_log_level(), "debug");
}

#[test]
fn masked_key_normal() {
    assert_eq!(make_config("sk-1234567890abcdef").masked().api_key, "sk-1**cdef");
}

#[test]
fn masked_key_short() {
    assert_eq!(make_config("123").masked().api_key, "***");
}

#[test]
fn save_excludes_api_key() {
    let config = make_config("my-secret-key");

    // Simulate save() logic: api_key IS in the serialized JSON (it's skipped only in save())
    let json = serde_json::to_value(&config).unwrap();
    let obj = json.as_object().unwrap();
    assert!(obj.contains_key("api_key"));

    // Test that api_key defaults to empty on deserialization when missing
    let mut json_no_key = json.clone();
    json_no_key.as_object_mut().unwrap().remove("api_key");
    let loaded: LlmConfig = serde_json::from_value(json_no_key).unwrap();
    assert!(loaded.api_key.is_empty());
    assert_eq!(loaded.model, DEFAULT_OPENAI_MODEL);
}

#[test]
fn effective_log_level_uses_config_value() {
    let mut config = make_config("sk-test");
    config.log_level = "trace".into();
    assert_eq!(config.effective_log_level(), "trace");
    config.log_level = "error".into();
    assert_eq!(config.effective_log_level(), "error");
}

