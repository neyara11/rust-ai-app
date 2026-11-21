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
    /// Named gateways (endpoint + key + models). When empty, the legacy
    /// `llm_*` fields above are used as one implicit provider named `gw`.
    #[serde(default)]
    pub llm_providers: Vec<LlmProvider>,
    /// Default `provider/model` selection.
    #[serde(default)]
    pub llm_default: Option<String>,
    /// Per-app `provider/model` override; missing means "use the default".
    #[serde(default)]
    pub llm_models: BTreeMap<String, String>,
    /// Auto-compact the chat when the context window is this full. Off by
    /// default; manual compaction is always available.
    #[serde(default)]
    pub compaction_auto: bool,
    #[serde(default = "default_compaction_threshold")]
    pub compaction_threshold_percent: u8,
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

/// One gateway: endpoint and key are shared by all of its models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProvider {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub allow_invalid_certs: bool,
    #[serde(default)]
    pub models: Vec<LlmModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModel {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
}

/// Fully resolved model choice for one turn.
#[derive(Debug, Clone)]
pub struct ModelSelection {
    pub provider_id: String,
    pub provider_name: String,
    pub endpoint: String,
    pub allow_invalid_certs: bool,
    pub model_id: String,
    pub capabilities: ModelCapabilities,
}

/// Provider ids end up in secret names and env vars, so keep them simple.
pub fn provider_id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Splits `provider/model`. Model ids may contain `/`, so only the first slash
/// separates the provider.
pub fn parse_selection(raw: &str) -> Option<(&str, &str)> {
    let (provider, model) = raw.trim().split_once('/')?;
    if !provider_id_ok(provider) || model.trim().is_empty() {
        return None;
    }
    Some((provider, model))
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

fn default_compaction_threshold() -> u8 {
    80
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
            llm_providers: Vec::new(),
            llm_default: None,
            llm_models: BTreeMap::new(),
            enabled_apps: Vec::new(),
            workspaces: BTreeMap::new(),
            session_quota_mb: default_session_mb(),
            global_quota_mb: default_global_mb(),
            ttl_hours: default_ttl_hours(),
            compaction_auto: false,
            compaction_threshold_percent: default_compaction_threshold(),
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

/// Applies a settings snapshot from the UI. Fields that are owned by their own
/// commands are kept from disk: the settings form sends whatever it loaded at
/// start-up, so trusting it would roll back a workspace chosen or an approval
/// granted since then.
pub fn merge_settings(existing: UserConfig, mut incoming: UserConfig) -> UserConfig {
    incoming.workspaces = existing.workspaces;
    incoming.enabled_apps = existing.enabled_apps;
    incoming.permissions.grants = existing.permissions.grants;
    incoming
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
    // Persist the effective provider list, so the first save after an upgrade
    // turns the legacy single gateway into a normal provider entry.
    let mut cfg = cfg.clone();
    cfg.normalize_llm();
    cfg.compaction_threshold_percent = cfg.compaction_threshold_percent.clamp(10, 100);
    let raw = serde_json::to_string_pretty(&cfg)?;
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
    /// Explicit providers, or one implicit provider built from the legacy
    /// `llm_endpoint`/`llm_model`/`llm_capabilities` fields. Providers without
    /// an endpoint are ignored; a missing key is allowed for local gateways.
    pub fn llm_providers_effective(&self) -> Vec<LlmProvider> {
        if !self.llm_providers.is_empty() {
            let mut providers = Vec::new();
            for provider in &self.llm_providers {
                let endpoint = provider.endpoint.trim();
                if !provider_id_ok(&provider.id) || endpoint.is_empty() {
                    continue;
                }
                let models: Vec<LlmModel> = provider
                    .models
                    .iter()
                    .filter(|model| !model.id.trim().is_empty())
                    .cloned()
                    .collect();
                if models.is_empty() {
                    continue;
                }
                providers.push(LlmProvider {
                    id: provider.id.clone(),
                    name: provider.name.clone(),
                    endpoint: endpoint.to_string(),
                    allow_invalid_certs: provider.allow_invalid_certs,
                    models,
                });
            }
            return providers;
        }
        let endpoint = self.llm_endpoint.clone().unwrap_or_default();
        if endpoint.trim().is_empty() {
            return Vec::new();
        }
        let model_id = self
            .llm_model
            .clone()
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| "stub-model".to_string());
        vec![LlmProvider {
            id: "gw".to_string(),
            name: Some("LLM Gateway".to_string()),
            endpoint: endpoint.trim().to_string(),
            allow_invalid_certs: self.llm_allow_invalid_certs,
            models: vec![LlmModel {
                id: model_id,
                name: None,
                capabilities: self.llm_capabilities.clone(),
            }],
        }]
    }

    /// Global default: the explicit choice, else the first model of the first
    /// provider.
    pub fn llm_default_selection(&self) -> Option<(String, String)> {
        let providers = self.llm_providers_effective();
        if let Some(selection) = self
            .llm_default
            .as_deref()
            .and_then(|raw| resolve_selection(&providers, raw))
        {
            return Some((selection.provider_id, selection.model_id));
        }
        providers.first().and_then(|provider| {
            provider
                .models
                .first()
                .map(|model| (provider.id.clone(), model.id.clone()))
        })
    }

    /// Per-app override, then the global default, then the first available
    /// model. Returns `None` only when no provider is configured.
    pub fn selected_model(&self, app_id: &str) -> Option<ModelSelection> {
        let providers = self.llm_providers_effective();
        if let Some(selection) = self
            .llm_models
            .get(app_id)
            .and_then(|raw| resolve_selection(&providers, raw))
        {
            return Some(selection);
        }
        if let Some(selection) = self
            .llm_default
            .as_deref()
            .and_then(|raw| resolve_selection(&providers, raw))
        {
            return Some(selection);
        }
        providers.first().and_then(|provider| {
            provider.models.first().map(|model| ModelSelection {
                provider_id: provider.id.clone(),
                provider_name: provider_display_name(provider),
                endpoint: provider.endpoint.clone(),
                allow_invalid_certs: provider.allow_invalid_certs,
                model_id: model.id.clone(),
                capabilities: model.capabilities.clone(),
            })
        })
    }

    /// Fills `llm_providers` from the legacy fields and drops selections that
    /// point to a missing provider/model. Called before writing config.
    pub fn normalize_llm(&mut self) {
        self.llm_providers = self.llm_providers_effective();
        self.llm_default = self
            .llm_default
            .take()
            .filter(|raw| resolve_selection(&self.llm_providers, raw).is_some());
        self.llm_models
            .retain(|_, raw| resolve_selection(&self.llm_providers, raw).is_some());
    }

    pub fn policy(&self) -> Policy {
        Policy {
            environment: self.environment,
            role: self.role,
        }
    }
}

fn provider_display_name(provider: &LlmProvider) -> String {
    provider
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| provider.id.clone())
}

fn resolve_selection(providers: &[LlmProvider], raw: &str) -> Option<ModelSelection> {
    let (provider_id, model_id) = parse_selection(raw)?;
    let provider = providers.iter().find(|item| item.id == provider_id)?;
    let model = provider.models.iter().find(|item| item.id == model_id)?;
    Some(ModelSelection {
        provider_id: provider.id.clone(),
        provider_name: provider_display_name(provider),
        endpoint: provider.endpoint.clone(),
        allow_invalid_certs: provider.allow_invalid_certs,
        model_id: model.id.clone(),
        capabilities: model.capabilities.clone(),
    })
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
    fn compaction_defaults_are_manual_with_80_percent() {
        let cfg: UserConfig = serde_json::from_str("{}").unwrap();
        assert!(!cfg.compaction_auto);
        assert_eq!(cfg.compaction_threshold_percent, 80);
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
    fn settings_save_keeps_workspace_choices() {
        let mut existing = UserConfig::default();
        existing
            .workspaces
            .insert("doc-writer".to_string(), r"D:\projects\docs".to_string());
        existing.enabled_apps.push("doc-writer".to_string());

        // The UI form was loaded before the folder was picked, so it carries an
        // empty map and must not erase the choice.
        let incoming = UserConfig::default();
        let merged = merge_settings(existing, incoming);
        assert_eq!(
            merged.workspaces.get("doc-writer").map(String::as_str),
            Some(r"D:\projects\docs")
        );
        assert_eq!(merged.enabled_apps, vec!["doc-writer".to_string()]);
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

    fn two_providers() -> UserConfig {
        UserConfig {
            llm_providers: vec![
                LlmProvider {
                    id: "gw".to_string(),
                    name: Some("Gateway".to_string()),
                    endpoint: "https://gw/v1".to_string(),
                    allow_invalid_certs: false,
                    models: vec![
                        LlmModel {
                            id: "~vendor/fast".to_string(),
                            name: None,
                            capabilities: ModelCapabilities::default(),
                        },
                        LlmModel {
                            id: "~vendor/smart".to_string(),
                            name: None,
                            capabilities: ModelCapabilities {
                                vision: true,
                                ..ModelCapabilities::default()
                            },
                        },
                    ],
                },
                LlmProvider {
                    id: "local".to_string(),
                    name: None,
                    endpoint: "http://127.0.0.1:11434/v1".to_string(),
                    allow_invalid_certs: true,
                    models: vec![LlmModel {
                        id: "llama".to_string(),
                        name: None,
                        capabilities: ModelCapabilities::default(),
                    }],
                },
            ],
            ..UserConfig::default()
        }
    }

    #[test]
    fn legacy_fields_become_one_implicit_provider() {
        let cfg: UserConfig = serde_json::from_str(
            r#"{
              "llm_endpoint": "https://gw/v1",
              "llm_model": "~vendor/fast",
              "llm_capabilities": { "vision": true, "context_tokens": 256000 }
            }"#,
        )
        .unwrap();
        let providers = cfg.llm_providers_effective();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "gw");
        assert_eq!(providers[0].models[0].id, "~vendor/fast");
        assert!(providers[0].models[0].capabilities.vision);
        let selection = cfg.selected_model("doc-writer").unwrap();
        assert_eq!(selection.provider_id, "gw");
        assert_eq!(selection.model_id, "~vendor/fast");
        assert_eq!(selection.capabilities.context_tokens, Some(256000));
    }

    #[test]
    fn selection_prefers_app_override_then_default() {
        let mut cfg = two_providers();
        assert_eq!(
            cfg.selected_model("doc-writer").map(|s| (s.provider_id, s.model_id)),
            Some(("gw".to_string(), "~vendor/fast".to_string()))
        );

        cfg.llm_default = Some("gw/~vendor/smart".to_string());
        assert_eq!(
            cfg.selected_model("doc-writer").map(|s| s.model_id),
            Some("~vendor/smart".to_string())
        );

        // Model ids contain slashes; only the first slash is the separator.
        cfg.llm_models
            .insert("doc-writer".to_string(), "local/llama".to_string());
        assert_eq!(
            cfg.selected_model("doc-writer")
                .map(|s| (s.provider_id, s.model_id)),
            Some(("local".to_string(), "llama".to_string()))
        );
        assert!(cfg.selected_model("doc-writer").unwrap().allow_invalid_certs);
    }

    #[test]
    fn normalize_materializes_legacy_and_drops_bad_selections() {
        let mut cfg = UserConfig {
            llm_endpoint: Some("https://gw/v1".to_string()),
            llm_model: Some("m".to_string()),
            llm_default: Some("missing/model".to_string()),
            ..UserConfig::default()
        };
        cfg.llm_models
            .insert("app".to_string(), "gw/missing".to_string());
        cfg.normalize_llm();
        assert_eq!(cfg.llm_providers.len(), 1);
        assert_eq!(cfg.llm_providers[0].id, "gw");
        assert!(cfg.llm_default.is_none());
        assert!(cfg.llm_models.is_empty());
    }
}
