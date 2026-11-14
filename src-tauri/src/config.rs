use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(default)]
    pub llm_endpoint: Option<String>,
    #[serde(default)]
    pub llm_model: Option<String>,
    /// Declared model capabilities. The gateway does not expose them, so the
    /// operator sets them explicitly; the engine gets them in the provider
    /// model block.
    #[serde(default)]
    pub llm_capabilities: ModelCapabilities,
    #[serde(default)]
    pub enabled_apps: Vec<String>,
    /// User-chosen working folder per app. The agent's file tools are confined
    /// to this directory.
    #[serde(default)]
    pub workspaces: BTreeMap<String, String>,
    #[serde(default = "default_session_mb")]
    pub session_quota_mb: u64,
    #[serde(default = "default_global_mb")]
    pub global_quota_mb: u64,
    #[serde(default = "default_ttl_hours")]
    pub ttl_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Model accepts image inputs (`attachment`/`modalities.input`).
    #[serde(default)]
    pub vision: bool,
    /// Model supports reasoning variants (`reasoning`/`variants`).
    #[serde(default = "default_true")]
    pub reasoning: bool,
    /// Variant ids sent as `variant` and `reasoningEffort` values.
    #[serde(default = "default_efforts")]
    pub reasoning_efforts: Vec<String>,
    #[serde(default)]
    pub context_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
}

fn default_true() -> bool {
    true
}

fn default_efforts() -> Vec<String> {
    vec!["low".to_string(), "medium".to_string(), "high".to_string()]
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            vision: false,
            reasoning: true,
            reasoning_efforts: default_efforts(),
            context_tokens: None,
            output_tokens: None,
        }
    }
}

impl ModelCapabilities {
    /// Variant ids safe to place in generated config: trimmed, lowercase,
    /// deduplicated, ASCII-only, at most six. Falls back to the defaults when
    /// the operator left the list empty or invalid.
    pub fn efforts(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for raw in &self.reasoning_efforts {
            let value = raw.trim().to_lowercase();
            if value.is_empty() || value.len() > 32 {
                continue;
            }
            if !value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                continue;
            }
            if !out.contains(&value) {
                out.push(value);
            }
            if out.len() == 6 {
                break;
            }
        }
        if out.is_empty() {
            default_efforts()
        } else {
            out
        }
    }

    pub fn allows_variant(&self, variant: &str) -> bool {
        self.reasoning && self.efforts().iter().any(|effort| effort == variant)
    }
}


fn default_session_mb() -> u64 {
    500
}

fn default_global_mb() -> u64 {
    5 * 1024
}

fn default_ttl_hours() -> u64 {
    168
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            llm_endpoint: None,
            llm_model: None,
            llm_capabilities: ModelCapabilities::default(),
            enabled_apps: Vec::new(),
            workspaces: BTreeMap::new(),
            session_quota_mb: default_session_mb(),
            global_quota_mb: default_global_mb(),
            ttl_hours: default_ttl_hours(),
        }
    }
}

/// Working folder for an app: the user's choice or a default under the
/// platform profile. Created on demand.
pub fn workspace(home: &Path, app_id: &str) -> PathBuf {
    let path = load(home)
        .workspaces
        .get(app_id)
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("workspaces").join(app_id));
    let _ = fs::create_dir_all(&path);
    path
}

pub fn set_workspace(home: &Path, app_id: &str, path: &Path) -> Result<()> {
    let mut cfg = load(home);
    cfg.workspaces
        .insert(app_id.to_string(), path.display().to_string());
    save(home, &cfg)
}

fn path(home: &Path) -> std::path::PathBuf {
    home.join("config.json")
}

pub fn load(home: &Path) -> UserConfig {
    fs::read_to_string(path(home))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(home: &Path, cfg: &UserConfig) -> Result<()> {
    fs::create_dir_all(home)?;
    let raw = serde_json::to_string_pretty(cfg)?;
    fs::write(path(home), raw)?;
    Ok(())
}

pub fn policy(home: &Path) -> crate::artifacts::Policy {
    let cfg = load(home);
    let mut policy = crate::artifacts::Policy::default();
    policy.session_quota_bytes = cfg.session_quota_mb * 1024 * 1024;
    policy.global_quota_bytes = cfg.global_quota_mb * 1024 * 1024;
    policy.ttl_hours = cfg.ttl_hours;
    policy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_without_capabilities_uses_defaults() {
        let cfg: UserConfig = serde_json::from_str(
            r#"{"llm_endpoint":"https://gw/v1","llm_model":"m","session_quota_mb":10}"#,
        )
        .unwrap();
        assert_eq!(cfg.llm_endpoint.as_deref(), Some("https://gw/v1"));
        assert!(!cfg.llm_capabilities.vision);
        assert!(cfg.llm_capabilities.reasoning);
        assert_eq!(cfg.llm_capabilities.efforts(), vec!["low", "medium", "high"]);
        assert_eq!(cfg.llm_capabilities.context_tokens, None);
    }

    #[test]
    fn effort_list_is_sanitized_and_bounded() {
        let caps = ModelCapabilities {
            reasoning_efforts: vec![
                " High ".to_string(),
                "high".to_string(),
                "bad value".to_string(),
                "".to_string(),
                "max".to_string(),
                "x1".to_string(),
                "x2".to_string(),
                "x3".to_string(),
            ],
            ..ModelCapabilities::default()
        };
        assert_eq!(caps.efforts(), vec!["high", "max", "x1", "x2", "x3"]);

        let empty = ModelCapabilities {
            reasoning_efforts: Vec::new(),
            ..ModelCapabilities::default()
        };
        assert_eq!(empty.efforts(), vec!["low", "medium", "high"]);

        let no_reasoning = ModelCapabilities {
            reasoning: false,
            ..ModelCapabilities::default()
        };
        assert!(!no_reasoning.allows_variant("high"));
    }
}
