use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(default)]
    pub llm_endpoint: Option<String>,
    #[serde(default)]
    pub llm_model: Option<String>,
    #[serde(default)]
    pub enabled_apps: Vec<String>,
    #[serde(default = "default_session_mb")]
    pub session_quota_mb: u64,
    #[serde(default = "default_global_mb")]
    pub global_quota_mb: u64,
    #[serde(default = "default_ttl_hours")]
    pub ttl_hours: u64,
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
            enabled_apps: Vec::new(),
            session_quota_mb: default_session_mb(),
            global_quota_mb: default_global_mb(),
            ttl_hours: default_ttl_hours(),
        }
    }
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
