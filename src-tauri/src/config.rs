use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Where the platform gets its packages from and how strictly it validates
/// them. The choice is made on first run and stored per profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    /// A person installs any package (signed or unsigned) and cannot edit it
    /// afterwards. Integrity is enforced by the signature when present and by
    /// a local seal otherwise. No trust anchor or allowlist is required.
    Local,
    /// Corporate contour: only packages signed by an active anchor and
    /// permitted by the allowlist are installed.
    Corporate,
}

impl Environment {
    pub fn as_str(self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Corporate => "corporate",
        }
    }
}

/// Who is using the platform. Authors get the package master and may rebuild
/// sources; users only consume installed apps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Developer,
    User,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Developer => "developer",
            Role::User => "user",
        }
    }
}

fn default_environment() -> Environment {
    // Preserve the previous fail-closed behaviour until the first-run choice.
    Environment::Corporate
}

fn default_role() -> Role {
    Role::User
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub environment: Environment,
    pub role: Role,
}

impl Policy {
    pub fn local(self) -> bool {
        self.environment == Environment::Local
    }

    pub fn require_anchor(self) -> bool {
        !self.local()
    }

    pub fn enforce_allowlist(self) -> bool {
        !self.local()
    }

    pub fn allow_unsigned(self) -> bool {
        self.local()
    }

    pub fn allow_authoring(self) -> bool {
        self.role == Role::Developer
    }
}

/// Mode written by the installer under `%ProgramData%\EcoAIOps`. When the
/// file exists the environment is fixed for this machine and the app cannot
/// switch it at runtime.
#[derive(Debug, Clone, Deserialize)]
struct InstallMode {
    environment: Environment,
}

pub fn install_mode_path() -> PathBuf {
    crate::allowlist::program_data()
        .join("EcoAIOps")
        .join("install-mode.json")
}

pub fn read_install_mode() -> Option<Environment> {
    // Development and unit tests must not inherit the machine-wide mode left by
    // an installed release build.
    if cfg!(debug_assertions) {
        return None;
    }
    let path = install_mode_path();
    match fs::metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            eprintln!(
                "eco-platform: cannot stat {} ({e}); treating the install mode as corporate",
                path.display()
            );
            return Some(Environment::Corporate);
        }
        Ok(_) => {}
    }
    match fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|raw| serde_json::from_str::<InstallMode>(&raw).map_err(|e| e.to_string()))
    {
        Ok(mode) => Some(mode.environment),
        Err(e) => {
            // The installer pinned a mode, but the file is unreadable or
            // corrupt. Stay locked and fail closed to corporate instead of
            // silently unlocking the contour.
            eprintln!(
                "eco-platform: cannot read {} ({e}); treating the install mode as corporate",
                path.display()
            );
            Some(Environment::Corporate)
        }
    }
}

pub fn install_mode_locked() -> bool {
    read_install_mode().is_some()
}

fn apply_install_mode(mut cfg: UserConfig, mode: Option<Environment>) -> UserConfig {
    if let Some(environment) = mode {
        cfg.environment = environment;
        cfg.mode_chosen = true;
    }
    cfg
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(default = "default_environment")]
    pub environment: Environment,
    #[serde(default = "default_role")]
    pub role: Role,
    /// False until the first-run dialog has stored an explicit choice.
    #[serde(default)]
    pub mode_chosen: bool,
    #[serde(default)]
    pub llm_endpoint: Option<String>,
    #[serde(default)]
    pub llm_model: Option<String>,
    /// Declared model capabilities. The gateway does not expose them, so the
    /// operator sets them explicitly; the engine gets them in the provider
    /// model block.
    #[serde(default)]
    pub llm_capabilities: ModelCapabilities,
    /// Closed-contour escape hatch: an internal gateway may present a private
    /// CA or a self-signed certificate. Off by default.
    #[serde(default)]
    pub llm_allow_invalid_certs: bool,
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
    /// Approval defaults and remembered grants. Absent in older profiles and
    /// defaulted to ask-on-write/exec.
    #[serde(default)]
    pub permissions: crate::permissions::PermissionsConfig,
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
            environment: default_environment(),
            role: default_role(),
            mode_chosen: false,
            llm_endpoint: None,
            llm_model: None,
            llm_capabilities: ModelCapabilities::default(),
            llm_allow_invalid_certs: false,
            enabled_apps: Vec::new(),
            workspaces: BTreeMap::new(),
            session_quota_mb: default_session_mb(),
            global_quota_mb: default_global_mb(),
            ttl_hours: default_ttl_hours(),
            permissions: crate::permissions::PermissionsConfig::default(),
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
    let file = path(home);
    let cfg = match fs::read_to_string(&file) {
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!(
                    "eco-platform: config {} is invalid ({e}); using defaults",
                    file.display()
                );
                UserConfig::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => UserConfig::default(),
        Err(e) => {
            eprintln!(
                "eco-platform: cannot read config {} ({e}); using defaults",
                file.display()
            );
            UserConfig::default()
        }
    };
    apply_install_mode(cfg, read_install_mode())
}

static CONFIG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn save(home: &Path, cfg: &UserConfig) -> Result<()> {
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    fs::create_dir_all(home)?;
    let raw = serde_json::to_string_pretty(cfg)?;
    let target = path(home);
    let tmp = home.join(format!("config.json.tmp-{}", std::process::id()));
    {
        use std::io::Write;
        let mut file = fs::File::create(&tmp)?;
        file.write_all(raw.as_bytes())?;
        file.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, &target) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

impl UserConfig {
    pub fn policy(&self) -> Policy {
        Policy {
            environment: self.environment,
            role: self.role,
        }
    }
}

pub fn policy_for(home: &Path) -> Policy {
    load(home).policy()
}

pub fn set_mode(home: &Path, environment: Environment, role: Role) -> Result<UserConfig> {
    if install_mode_locked() {
        return crate::error::msg(
            "режим задан при установке; чтобы его изменить, переустановите приложение",
        );
    }
    let mut cfg = load(home);
    cfg.environment = environment;
    cfg.role = role;
    cfg.mode_chosen = true;
    save(home, &cfg)?;
    Ok(cfg)
}

/// Role is an interface choice, not part of the installed environment, so it
/// stays switchable even when the installer pinned the mode.
pub fn set_role(home: &Path, role: Role) -> Result<UserConfig> {
    let mut cfg = load(home);
    cfg.role = role;
    cfg.mode_chosen = true;
    save(home, &cfg)?;
    Ok(cfg)
}

pub fn policy(home: &Path) -> crate::artifacts::Policy {
    let cfg = load(home);
    let mut policy = crate::artifacts::Policy::default();
    policy.session_quota_bytes = cfg.session_quota_mb * 1024 * 1024;
    policy.global_quota_bytes = cfg.global_quota_mb * 1024 * 1024;
    policy.ttl_hours = cfg.ttl_hours;
    policy.orphan_hours = cfg.ttl_hours;
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
        assert_eq!(cfg.permissions.defaults.write.as_str(), "ask");
        assert_eq!(cfg.permissions.defaults.exec.as_str(), "ask");
        assert!(cfg.permissions.grants.is_empty());
    }

    #[test]
    fn install_mode_overrides_profile_and_marks_choice() {
        let cfg = apply_install_mode(UserConfig::default(), Some(Environment::Local));
        assert_eq!(cfg.environment, Environment::Local);
        assert!(cfg.mode_chosen);

        let cfg = apply_install_mode(cfg, None);
        assert_eq!(cfg.environment, Environment::Local);
    }

    #[test]
    fn install_mode_json_requires_known_environment() {
        let mode: InstallMode = serde_json::from_str(r#"{"environment":"local"}"#).unwrap();
        assert_eq!(mode.environment, Environment::Local);
        let mode: InstallMode = serde_json::from_str(r#"{"environment":"corporate"}"#).unwrap();
        assert_eq!(mode.environment, Environment::Corporate);
        assert!(serde_json::from_str::<InstallMode>(r#"{"environment":"other"}"#).is_err());
    }

    #[test]
    fn role_change_persists_without_touching_environment_choice() {
        let home = std::env::temp_dir().join(format!("eco-role-{}", std::process::id()));
        fs::create_dir_all(&home).unwrap();
        let mut cfg = UserConfig::default();
        cfg.environment = Environment::Corporate;
        cfg.role = Role::User;
        cfg.mode_chosen = true;
        save(&home, &cfg).unwrap();

        let updated = set_role(&home, Role::Developer).unwrap();
        assert_eq!(updated.role, Role::Developer);

        let raw = fs::read_to_string(home.join("config.json")).unwrap();
        let disk: UserConfig = serde_json::from_str(&raw).unwrap();
        assert_eq!(disk.role, Role::Developer);
        let _ = fs::remove_file(home.join("config.json"));
    }

    #[test]
    fn effort_list_is_sanitized_and_bounded() {        let caps = ModelCapabilities {
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
