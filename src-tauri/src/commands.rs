use crate::artifacts;
use crate::config::{self, UserConfig};
use crate::error::{msg, Error, Result};
use crate::manifest;
use crate::mcp::{self, McpSession};
use crate::registry;
use crate::secrets::SecretStore;
use crate::state::AppState;
use base64::Engine as _;
use serde::Serialize;
use tauri::Manager;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
pub struct PlatformInfo {
    pub home: String,
    pub version: String,
    pub schema: u32,
    pub trusted_anchor: bool,
    pub anchor_fingerprint: Option<String>,
    pub anchor_fingerprints: Vec<String>,
    pub anchor_source: Option<String>,
    pub anchor_count: usize,
    pub allowlist_status: String,
    pub allowlist_source: String,
    pub allowlist_apps: usize,
    pub allowlist_revoked: usize,
    pub allowlist_expires_at: Option<u64>,
    pub allowlist_problem: Option<String>,
    pub dev_mode: bool,
    /// `local` or `corporate` — chosen on first run.
    pub environment: String,
    /// `developer` or `user`.
    pub role: String,
    /// False until the first-run dialog stores an explicit choice.
    pub mode_chosen: bool,
    /// True when the installer pinned the environment for this machine.
    pub mode_locked: bool,
    pub allow_authoring: bool,
}

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub app_id: String,
    pub workspace: String,
}

#[derive(Debug, Serialize)]
pub struct Probe {
    pub ok: bool,
    pub millis: u64,
    pub detail: String,
}

#[tauri::command(rename_all = "snake_case")]
pub fn platform_info(state: tauri::State<'_, AppState>) -> PlatformInfo {
    let anchor = registry::trusted_anchor(&state.home);
    let keys = registry::load_trusted_keys(&state.home);
    let allow = crate::allowlist::info(&state.home, &keys);
    let cfg = config::load(&state.home);
    let policy = cfg.policy();
    PlatformInfo {
        environment: cfg.environment.as_str().to_string(),
        role: cfg.role.as_str().to_string(),
        mode_chosen: cfg.mode_chosen,
        mode_locked: config::install_mode_locked(),
        allow_authoring: policy.allow_authoring(),
        home: state.home.display().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema: manifest::SUPPORTED_SCHEMA,
        trusted_anchor: anchor.is_some(),
        anchor_fingerprint: registry::anchor_fingerprint(&state.home),
        anchor_fingerprints: registry::anchor_fingerprints(&state.home),
        anchor_source: anchor.map(|(_, source)| source.as_str().to_string()),
        anchor_count: keys.len(),
        allowlist_status: allow.status,
        allowlist_source: allow.source,
        allowlist_apps: allow.apps,
        allowlist_revoked: allow.revoked,
        allowlist_expires_at: allow.expires_at,
        allowlist_problem: allow.problems.first().cloned(),
        dev_mode: registry::dev_mode(),
    }
}

/// Stores the first-run (or later) choice of environment and role. Changing
/// the environment does not touch installed packages; switching to corporate
/// simply makes them subject to the anchor/allowlist rules.
#[tauri::command(rename_all = "snake_case")]
pub fn set_mode(
    state: tauri::State<'_, AppState>,
    environment: config::Environment,
    role: config::Role,
) -> Result<UserConfig> {
    let cfg = config::set_mode(&state.home, environment, role)?;
    // Engine config depends on the profile, so running engines are restarted.
    if let Ok(mut engines) = state.engines.lock() {
        engines.clear();
    }
    Ok(cfg)
}

/// Switches the interface role. The installed environment stays locked, but
/// the role only controls authoring UI, so it remains user-switchable.
#[tauri::command(rename_all = "snake_case")]
pub fn set_role(state: tauri::State<'_, AppState>, role: config::Role) -> Result<UserConfig> {
    config::set_role(&state.home, role)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_apps(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<registry::InstalledApp>> {
    let home = state.home.clone();
    Ok(tauri::async_runtime::spawn_blocking(move || registry::list(&home))
        .await
        .unwrap_or_default())
}

/// Resolves a package path typed by the user. Relative paths are looked up in
/// the app's resource directory (bundled `dev-apps/`), then in the current
/// working directory, so the quick-install chips work in the installed build.
fn resolve_package_path(app: &tauri::AppHandle, raw: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(raw);
    if direct.exists() {
        return Some(direct);
    }
    if direct.is_absolute() {
        return None;
    }
    let mut roots = Vec::new();
    if let Ok(dir) = app.path().resource_dir() {
        roots.push(dir);
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    for root in roots {
        let candidate = root.join(raw);
        if candidate.exists() {
            return Some(candidate);
        }
        let nested = root.join("dev-apps").join(raw);
        if nested.exists() {
            return Some(nested);
        }
    }
    None
}

#[tauri::command(rename_all = "snake_case")]
pub async fn install_app(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<registry::InstalledApp> {
    let src = resolve_package_path(&app, &path)
        .ok_or_else(|| Error::Msg(format!("package path does not exist: {path}")))?;
    let home = state.home.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry::install_with(&home, &src, |app_id| {
            // Verification and downgrade checks passed: only now stop the
            // target app's engine, right before its directory is replaced.
            let state = handle.state::<AppState>();
            if let Ok(mut engines) = state.engines.lock() {
                engines.remove(app_id);
            };
        })
    })
    .await
    .map_err(|e| Error::Msg(format!("install task failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn pick_package(kind: Option<String>) -> Result<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let dialog = rfd::FileDialog::new().set_title("Пакет мини-приложения");
        let picked = match kind.as_deref() {
            Some("file") => dialog.add_filter("Пакет", &["zip"]).pick_file(),
            _ => dialog.pick_folder(),
        };
        Ok(picked.map(|path| path.display().to_string()))
    })
    .await
    .map_err(|e| Error::Msg(format!("package dialog failed: {e}")))?
}

fn safe_diagram_name(raw: &str, ext: &str) -> String {
    let file = raw.rsplit(['/', '\\']).next().unwrap_or("diagram");
    let stem = file.rsplit_once('.').map(|(name, _)| name).unwrap_or(file);
    let cleaned: String = stem
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .take(80)
        .collect();
    if cleaned.is_empty() {
        format!("diagram.{ext}")
    } else {
        format!("{cleaned}.{ext}")
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn save_diagram(
    state: tauri::State<'_, AppState>,
    app_id: String,
    filename: String,
    format: String,
    content: String,
) -> Result<Option<String>> {
    registry::verified_manifest(&state.home, &app_id)?;
    let (ext, label, bytes) = match format.to_ascii_lowercase().as_str() {
        "svg" => ("svg", "SVG", content.into_bytes()),
        "png" => {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(content.trim())
                .map_err(|e| Error::Msg(format!("png decode failed: {e}")))?;
            ("png", "PNG", decoded)
        }
        _ => return msg("unsupported diagram format"),
    };
    if bytes.len() > 20 * 1024 * 1024 {
        return msg("diagram is too large to save");
    }
    let default_name = safe_diagram_name(&filename, ext);
    tauri::async_runtime::spawn_blocking(move || {
        let Some(mut path) = rfd::FileDialog::new()
            .set_title("Сохранить схему")
            .set_file_name(&default_name)
            .add_filter(label, &[ext])
            .save_file()
        else {
            return Ok(None);
        };
        let has_expected_ext = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case(ext))
            .unwrap_or(false);
        if !has_expected_ext {
            path.set_extension(ext);
        }
        std::fs::write(&path, bytes)?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| Error::Msg(format!("diagram save task failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub fn remove_app(state: tauri::State<'_, AppState>, app_id: String) -> Result<()> {
    let home = state.home.clone();
    if let Ok(mut engines) = state.engines.lock() {
        engines.remove(&app_id);
    }
    registry::remove(&home, &app_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn app_history(state: tauri::State<'_, AppState>, app_id: String) -> Vec<registry::ArchivedVersion> {
    registry::history(&state.home, &app_id)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn rollback_app(
    state: tauri::State<'_, AppState>,
    app_id: String,
    version: Option<String>,
) -> Result<registry::InstalledApp> {
    let home = state.home.clone();
    if let Ok(mut engines) = state.engines.lock() {
        engines.remove(&app_id);
    }
    tauri::async_runtime::spawn_blocking(move || {
        registry::rollback(&home, &app_id, version.as_deref())
    })
    .await
    .map_err(|e| Error::Msg(format!("rollback task failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_config(state: tauri::State<'_, AppState>) -> UserConfig {
    config::load(&state.home)
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_config(state: tauri::State<'_, AppState>, config: UserConfig) -> Result<()> {
    config::save(&state.home, &config)?;
    // Engine config is generated at start, so running engines must be restarted
    // to pick up a new endpoint, model or key.
    if let Ok(mut engines) = state.engines.lock() {
        engines.clear();
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn probe_endpoint(url: String) -> Probe {
    tauri::async_runtime::spawn_blocking(move || probe_endpoint_blocking(url))
        .await
        .unwrap_or(Probe {
            ok: false,
            millis: 0,
            detail: "probe task failed".into(),
        })
}

fn probe_endpoint_blocking(url: String) -> Probe {
    let Some((host, port)) = parse_host_port(&url) else {
        return Probe {
            ok: false,
            millis: 0,
            detail: "cannot parse host and port".into(),
        };
    };
    let addrs = match (host.as_str(), port).to_socket_addrs() {
        Ok(a) => a,
        Err(e) => {
            return Probe {
                ok: false,
                millis: 0,
                detail: format!("resolve failed: {e}"),
            }
        }
    };
    let start = Instant::now();
    for addr in addrs {
        if std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok() {
            return Probe {
                ok: true,
                millis: start.elapsed().as_millis() as u64,
                detail: format!("TCP connect ok ({addr})"),
            };
        }
    }
    Probe {
        ok: false,
        millis: start.elapsed().as_millis() as u64,
        detail: "TCP connect failed".into(),
    }
}

fn parse_host_port(url: &str) -> Option<(String, u16)> {
    let trimmed = url.trim();
    let (rest, default_port) = if let Some(r) = trimmed.strip_prefix("https://") {
        (r, 443u16)
    } else if let Some(r) = trimmed.strip_prefix("http://") {
        (r, 80u16)
    } else {
        (trimmed, 0u16)
    };
    let hostport = rest.split('/').next()?;
    if hostport.is_empty() {
        return None;
    }
    if let Some((h, p)) = hostport.rsplit_once(':') {
        if let Ok(port) = p.parse::<u16>() {
            return Some((h.to_string(), port));
        }
    }
    if default_port == 0 {
        None
    } else {
        Some((hostport.to_string(), default_port))
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn secrets_list(state: tauri::State<'_, AppState>) -> Result<Vec<String>> {
    Ok(SecretStore::new(&state.home)
        .list()?
        .into_iter()
        .filter(|name| !name.starts_with("app."))
        .collect())
}

#[tauri::command(rename_all = "snake_case")]
pub fn secret_store(state: tauri::State<'_, AppState>, name: String, value: String) -> Result<()> {
    SecretStore::new(&state.home).store(&name, &value)
}

#[tauri::command(rename_all = "snake_case")]
pub fn secret_delete(state: tauri::State<'_, AppState>, name: String) -> Result<()> {
    SecretStore::new(&state.home).delete(&name)
}

fn declared_tool_secrets(
    app: &crate::manifest::Manifest,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut out: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for tool in &app.tools {
        for (env, secret) in &tool.secrets {
            let entry = out.entry(secret.clone()).or_default();
            if !entry.contains(env) {
                entry.push(env.clone());
            }
        }
    }
    for envs in out.values_mut() {
        envs.sort();
    }
    out
}

#[tauri::command(rename_all = "snake_case")]
pub fn app_secrets(
    state: tauri::State<'_, AppState>,
    app_id: String,
) -> Result<Vec<AppSecretInfo>> {
    let app = registry::verified_manifest(&state.home, &app_id)?;
    let secrets = SecretStore::new(&state.home);
    let mut out = Vec::new();
    for (name, envs) in declared_tool_secrets(&app) {
        let scope = if secrets.load_scoped(&app_id, &name)?.is_some() {
            "app"
        } else if secrets.load(&name)?.is_some() {
            "global"
        } else {
            "none"
        };
        out.push(AppSecretInfo {
            name,
            envs,
            scope: scope.to_string(),
        });
    }
    Ok(out)
}

#[tauri::command(rename_all = "snake_case")]
pub fn app_secret_set(
    state: tauri::State<'_, AppState>,
    app_id: String,
    name: String,
    value: String,
) -> Result<()> {
    if value.is_empty() {
        return msg("secret value is empty");
    }
    if manifest::is_reserved_secret(&name) {
        return msg(format!("secret name '{name}' is reserved by the platform"));
    }
    let app = registry::verified_manifest(&state.home, &app_id)?;
    if !declared_tool_secrets(&app).contains_key(&name) {
        return msg(format!(
            "secret '{name}' is not declared by the signed manifest of {app_id}"
        ));
    }
    SecretStore::new(&state.home).store(
        &SecretStore::scoped_name(&app_id, &name),
        &value,
    )
}

#[tauri::command(rename_all = "snake_case")]
pub fn app_secret_delete(
    state: tauri::State<'_, AppState>,
    app_id: String,
    name: String,
) -> Result<()> {
    let app = registry::verified_manifest(&state.home, &app_id)?;
    if !declared_tool_secrets(&app).contains_key(&name) {
        return msg(format!(
            "secret '{name}' is not declared by the signed manifest of {app_id}"
        ));
    }
    SecretStore::new(&state.home).delete_scoped(&app_id, &name)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_app(state: tauri::State<'_, AppState>, app_id: String) -> Result<SessionInfo> {
    manifest::parse_id(&app_id, "app")?;
    let home = state.home.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<SessionInfo> {
        // Fail-closed: the package is re-verified on every open.
        registry::verified_manifest(&home, &app_id)?;
        let workspace = config::workspace(&home, &app_id);
        Ok(SessionInfo {
            app_id,
            workspace: workspace.display().to_string(),
        })
    })
    .await
    .map_err(|e| Error::Msg(format!("open task failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub fn close_app(state: tauri::State<'_, AppState>, app_id: String) -> Result<()> {
    if let Ok(mut engines) = state.engines.lock() {
        engines.remove(&app_id);
    }
    Ok(())
}

/// Closing the workspace returns to the catalog, so every app engine is
/// stopped. Switching between apps keeps engines alive instead.
#[tauri::command(rename_all = "snake_case")]
pub fn close_all_apps(state: tauri::State<'_, AppState>) -> Result<()> {
    if let Ok(mut engines) = state.engines.lock() {
        engines.clear();
    }
    Ok(())
}


/// Lets the user pick the working folder for an app. The agent's file tools
/// are confined to this folder.
#[tauri::command(rename_all = "snake_case")]
pub async fn choose_workspace(
    app: tauri::AppHandle,
    app_id: String,
) -> Result<Option<String>> {
    manifest::parse_id(&app_id, "app")?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let Some(path) = rfd::FileDialog::new()
            .set_title("Рабочая папка приложения")
            .pick_folder()
        else {
            return Ok(None);
        };
        config::set_workspace(&state.home, &app_id, &path)?;
        // Only this app's engine is bound to the old folder; keep other apps'
        // running turns alive.
        if let Ok(mut engines) = state.engines.lock() {
            engines.remove(&app_id);
        }
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| Error::Msg(format!("workspace dialog failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn artifacts_usage(
    state: tauri::State<'_, AppState>,
    app_id: String,
) -> Result<artifacts::Usage> {
    manifest::parse_id(&app_id, "app")?;
    let home = state.home.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = artifacts::sessions_root(&home, &app_id);
        Ok(artifacts::usage(&root))
    })
    .await
    .map_err(|e| Error::Msg(format!("artifact usage scan failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn artifact_grep(
    state: tauri::State<'_, AppState>,
    app_id: String,
    query: String,
    session: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<artifacts::GrepHit>> {
    manifest::parse_id(&app_id, "app")?;
    let root = artifacts::sessions_root(&state.home, &app_id);
    tauri::async_runtime::spawn_blocking(move || {
        artifacts::grep(
            &root,
            &query,
            session.as_deref(),
            limit.unwrap_or(50).clamp(1, 500),
        )
    })
    .await
    .map_err(|e| Error::Msg(format!("artifact grep failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn artifact_dumps(
    state: tauri::State<'_, AppState>,
    app_id: String,
) -> Result<Vec<artifacts::DumpFile>> {
    manifest::parse_id(&app_id, "app")?;
    let home = state.home.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = artifacts::sessions_root(&home, &app_id);
        Ok(artifacts::list_dumps(&root))
    })
    .await
    .map_err(|e| Error::Msg(format!("artifact dump scan failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn export_artifacts(
    app: tauri::AppHandle,
    app_id: String,
    session: Option<String>,
) -> Result<Option<artifacts::ExportReport>> {
    manifest::parse_id(&app_id, "app")?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let Some(dest) = rfd::FileDialog::new()
            .set_title("Экспорт дампов артефактов")
            .pick_folder()
        else {
            return Ok(None);
        };
        let root = artifacts::sessions_root(&state.home, &app_id);
        artifacts::export(&root, session.as_deref(), &dest).map(Some)
    })
    .await
    .map_err(|e| Error::Msg(format!("artifact export dialog failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub fn cleanup_artifacts(
    state: tauri::State<'_, AppState>,
    app_id: String,
) -> Result<artifacts::SweepReport> {
    manifest::parse_id(&app_id, "app")?;
    let policy = config::policy(&state.home);
    let root = artifacts::sessions_root(&state.home, &app_id);
    artifacts::sweep(&root, &policy)
}

#[cfg(test)]
mod tests {
    use super::{parse_host_port, safe_diagram_name};

    #[test]
    fn diagram_names_cannot_escape_or_keep_a_foreign_extension() {
        assert_eq!(safe_diagram_name("../../evil.svg", "svg"), "evil.svg");
        assert_eq!(safe_diagram_name("C:\\tmp\\chart.png", "png"), "chart.png");
        assert_eq!(safe_diagram_name("", "svg"), "diagram.svg");
        assert_eq!(safe_diagram_name("схема.svg", "svg"), "diagram.svg");
    }

    #[test]
    fn parses_endpoints() {
        assert_eq!(
            parse_host_port("https://llm.gpb.local/v1"),
            Some(("llm.gpb.local".to_string(), 443))
        );
        assert_eq!(
            parse_host_port("http://10.0.0.5:8080/api"),
            Some(("10.0.0.5".to_string(), 8080))
        );
        assert_eq!(
            parse_host_port("https://gw:9443"),
            Some(("gw".to_string(), 9443))
        );
        assert_eq!(parse_host_port("just-a-host"), None);
    }
}

#[derive(Debug, Serialize)]
pub struct McpServerInfo {
    pub id: String,
    pub url: String,
    pub has_token: bool,
    pub allow_invalid_certs: bool,
    /// `global`, `app` or `none`.
    pub scope: String,
    pub has_app_override: bool,
}

#[derive(Debug, Serialize)]
pub struct AppSecretInfo {
    pub name: String,
    pub envs: Vec<String>,
    /// `global`, `app` or `none`.
    pub scope: String,
}

#[derive(Debug, Serialize)]
pub struct McpCallResult {
    pub bytes: usize,
    pub truncated: bool,
    pub result: String,
}

pub fn audit(home: &std::path::Path, app_id: &str, entry: serde_json::Value) {
    use std::io::Write;
    let dir = home.join("audit");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let line = format!("{entry}\n");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{app_id}.jsonl")))
    {
        let _ = file.write_all(line.as_bytes());
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn mcp_servers(state: tauri::State<'_, AppState>) -> Result<Vec<String>> {
    mcp::list(&SecretStore::new(&state.home))
}

#[tauri::command(rename_all = "snake_case")]
pub fn mcp_server_get(state: tauri::State<'_, AppState>, id: String) -> Result<McpServerInfo> {
    let def = mcp::get(&SecretStore::new(&state.home), &id)?;
    Ok(McpServerInfo {
        id: def.id,
        url: mask_url(&def.url),
        has_token: def.token.is_some(),
        allow_invalid_certs: def.allow_invalid_certs,
        scope: "global".to_string(),
        has_app_override: false,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn mcp_server_save(
    state: tauri::State<'_, AppState>,
    id: String,
    url: String,
    token: Option<String>,
    allow_invalid_certs: Option<bool>,
) -> Result<()> {
    let secrets = SecretStore::new(&state.home);
    let def = mcp::ServerDef {
        id,
        url: url.trim().to_string(),
        token: token.filter(|t| !t.trim().is_empty()),
        allow_invalid_certs: allow_invalid_certs.unwrap_or(false),
    };
    if def.url.is_empty() {
        return msg("mcp server url is empty");
    }
    mcp::put(&secrets, &def)
}

#[tauri::command(rename_all = "snake_case")]
pub fn mcp_server_delete(state: tauri::State<'_, AppState>, id: String) -> Result<()> {
    mcp::delete(&SecretStore::new(&state.home), &id)
}

fn declared_mcp_ids(app: &crate::manifest::Manifest) -> Vec<String> {
    app.connections
        .iter()
        .filter(|connection| connection.kind == "http-mcp")
        .map(|connection| connection.id.clone())
        .collect()
}

fn mcp_app_info(
    home: &std::path::Path,
    app: &crate::manifest::Manifest,
    id: &str,
) -> Result<McpServerInfo> {
    let secrets = SecretStore::new(home);
    let scoped = mcp::get_scoped(&secrets, &app.app_id, id)?;
    let global = mcp::get(&secrets, id).ok();
    let (def, scope, has_app_override) = match (scoped, global) {
        (Some(def), _) => (Some(def), "app", true),
        (None, Some(def)) => (Some(def), "global", false),
        (None, None) => (None, "none", false),
    };
    Ok(McpServerInfo {
        id: id.to_string(),
        url: def.as_ref().map(|d| mask_url(&d.url)).unwrap_or_default(),
        has_token: def.as_ref().and_then(|d| d.token.as_ref()).is_some(),
        allow_invalid_certs: def.as_ref().map(|d| d.allow_invalid_certs).unwrap_or(false),
        scope: scope.to_string(),
        has_app_override,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn mcp_app_servers(
    state: tauri::State<'_, AppState>,
    app_id: String,
) -> Result<Vec<McpServerInfo>> {
    let app = registry::verified_manifest(&state.home, &app_id)?;
    let mut out = Vec::new();
    for id in declared_mcp_ids(&app) {
        out.push(mcp_app_info(&state.home, &app, &id)?);
    }
    Ok(out)
}

#[tauri::command(rename_all = "snake_case")]
pub fn mcp_app_server_save(
    state: tauri::State<'_, AppState>,
    app_id: String,
    id: String,
    url: String,
    token: Option<String>,
    allow_invalid_certs: Option<bool>,
) -> Result<()> {
    let app = registry::verified_manifest(&state.home, &app_id)?;
    if !declared_mcp_ids(&app).iter().any(|declared| declared == &id) {
        return msg(format!(
            "mcp server {id} is not declared by the signed manifest of {app_id}"
        ));
    }
    let secrets = SecretStore::new(&state.home);
    let existing = mcp::get_scoped(&secrets, &app_id, &id)?;
    let global = mcp::get(&secrets, &id).ok();
    let base_url = url.trim().to_string();
    let resolved_url = if !base_url.is_empty() {
        base_url
    } else {
        existing
            .as_ref()
            .map(|def| def.url.clone())
            .or_else(|| global.as_ref().map(|def| def.url.clone()))
            .ok_or_else(|| {
                Error::Msg("mcp server url is empty: there is no existing value to keep".into())
            })?
    };
    let token = match token {
        Some(value) if value.trim().is_empty() => None,
        Some(value) => Some(value),
        None => existing
            .as_ref()
            .and_then(|def| def.token.clone())
            .or_else(|| global.as_ref().and_then(|def| def.token.clone())),
    };
    let allow_invalid = allow_invalid_certs.unwrap_or_else(|| {
        existing
            .as_ref()
            .map(|def| def.allow_invalid_certs)
            .or_else(|| global.as_ref().map(|def| def.allow_invalid_certs))
            .unwrap_or(false)
    });
    let def = mcp::ServerDef {
        id,
        url: resolved_url,
        token,
        allow_invalid_certs: allow_invalid,
    };
    mcp::put_for_app(&secrets, &app_id, &def)
}

#[tauri::command(rename_all = "snake_case")]
pub fn mcp_app_server_delete(
    state: tauri::State<'_, AppState>,
    app_id: String,
    id: String,
) -> Result<()> {
    mcp::delete_for_app(&SecretStore::new(&state.home), &app_id, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn mcp_probe(app: tauri::AppHandle, id: String) -> Result<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let def = mcp::get(&SecretStore::new(&state.home), &id)?;
        probe_mcp(def)
    })
    .await
    .map_err(|e| Error::Msg(format!("mcp probe task failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn mcp_app_probe(
    app: tauri::AppHandle,
    app_id: String,
    id: String,
) -> Result<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let def = mcp::get_for_app(&SecretStore::new(&state.home), &app_id, &id)?;
        probe_mcp(def)
    })
    .await
    .map_err(|e| Error::Msg(format!("mcp app probe task failed: {e}")))?
}

fn probe_mcp(def: mcp::ServerDef) -> Result<serde_json::Value> {
    let session = McpSession::connect(def)?;
    let tools = session.list_tools()?;
    let names: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| t.get("name").cloned().unwrap_or(serde_json::Value::Null))
        .collect();
    Ok(serde_json::json!({
        "protocol": session.protocol,
        "server": session.server,
        "tools": names
    }))
}
#[tauri::command(rename_all = "snake_case")]
pub async fn mcp_call(
    app: tauri::AppHandle,
    app_id: String,
    server_id: String,
    tool: String,
    arguments: serde_json::Value,
) -> Result<McpCallResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (bytes, truncated, result) =
            crate::mcp_call_for_app(&state.home, &app_id, &server_id, &tool, arguments)?;
        Ok(McpCallResult {
            bytes,
            truncated,
            result,
        })
    })
    .await
    .map_err(|e| Error::Msg(format!("mcp call task failed: {e}")))?
}
#[tauri::command(rename_all = "snake_case")]
pub fn audit_tail(
    state: tauri::State<'_, AppState>,
    app_id: String,
    limit: Option<usize>,
) -> Result<Vec<String>> {
    manifest::parse_id(&app_id, "app")?;
    let path = state.home.join("audit").join(format!("{app_id}.jsonl"));
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(path)?;
    let mut lines: Vec<String> = raw.lines().map(str::to_string).collect();
    let limit = limit.unwrap_or(20).min(200);
    if lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }
    Ok(lines)
}

/// Keeps the secret part of an MCP URL out of the UI and out of logs.
fn mask_url(url: &str) -> String {
    let (head, tail) = match url.split_once("://") {
        Some((scheme, rest)) => (format!("{scheme}://"), rest.to_string()),
        None => (String::new(), url.to_string()),
    };
    let (host, path) = match tail.split_once('/') {
        Some((h, p)) => (h.to_string(), p.to_string()),
        None => (tail, String::new()),
    };
    if path.is_empty() {
        format!("{head}{host}")
    } else {
        format!("{head}{host}/…")
    }
}

fn engines_lock<'a>(
    state: &'a AppState,
) -> Result<std::sync::MutexGuard<'a, std::collections::HashMap<String, crate::engine::Engine>>> {
    state
        .engines
        .lock()
        .map_err(|_| Error::Msg("engine lock poisoned".to_string()))
}

/// Starts (or returns) the engine for an app. Runs off the UI thread: engine
/// startup takes seconds and must never block the window.
#[tauri::command(rename_all = "snake_case")]
pub async fn chat_open(
    app: tauri::AppHandle,
    app_id: String,
) -> Result<crate::engine::ChatInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        chat_open_blocking(&state, app_id)
    })
    .await
    .map_err(|e| Error::Msg(format!("engine start task failed: {e}")))?
}

struct StartingGuard<'a> {
    state: &'a AppState,
    app_id: String,
}

impl Drop for StartingGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut starting) = self.state.starting.lock() {
            starting.remove(&self.app_id);
        }
    }
}

fn chat_open_blocking(state: &AppState, app_id: String) -> Result<crate::engine::ChatInfo> {
    {
        let engines = engines_lock(state)?;
        if let Some(engine) = engines.get(&app_id) {
            return Ok(engine.info());
        }
    }

    {
        let mut starting = state
            .starting
            .lock()
            .map_err(|_| Error::Msg("engine start lock poisoned".to_string()))?;
        if !starting.insert(app_id.clone()) {
            return msg("the engine is already starting for this app");
        }
    }
    let _starting = StartingGuard {
        state,
        app_id: app_id.clone(),
    };
    let result = (|| -> Result<crate::engine::ChatInfo> {
        // The agent works in the user-chosen workspace.
        let workspace = config::workspace(&state.home, &app_id);
        let engine = crate::engine::Engine::start(&app_id, &state.home, &workspace)?;

        let mut engines = engines_lock(state)?;
        if let Some(existing) = engines.get(&app_id) {
            return Ok(existing.info());
        }
        let info = engine.info();
        let repair = engine.snapshot();
        engines.insert(app_id.clone(), engine);
        // Legacy chats still titled by app name are renamed from their first
        // user message in the background; this never blocks opening the app.
        std::thread::spawn(move || {
            let _ = repair.repair_generic_titles();
        });
        Ok(info)
    })();
    result
}
#[tauri::command(rename_all = "snake_case")]
pub fn chat_send(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    app_id: String,
    text: String,
    variant: Option<String>,
    attachments: Option<Vec<crate::engine::AttachmentInput>>,
) -> Result<()> {
    let session = {
        let engines = engines_lock(&state)?;
        engines
            .get(&app_id)
            .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?
            .snapshot()
    };

    let attachments = attachments.unwrap_or_default();
    let caps = config::load(&state.home).llm_capabilities;
    if text.trim().is_empty() && attachments.is_empty() {
        return msg("message is empty");
    }
    crate::engine::validate_attachments(&attachments, caps.vision)?;

    if let Some(variant) = variant.as_deref() {
        if !caps.allows_variant(variant) {
            return msg(if caps.reasoning {
                format!("reasoning variant '{variant}' is not enabled for this model")
            } else {
                "model is configured without reasoning support".to_string()
            });
        }
    }



    let busy_key = format!("{}:{}", app_id, session.session_id);
    {
        let mut busy = state
            .busy
            .lock()
            .map_err(|_| Error::Msg("busy lock poisoned".to_string()))?;
        if !busy.insert(busy_key.clone()) {
            return msg("this chat is already working");
        }
    }

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tauri::Emitter;

    let _ = app.emit(
        "chat://busy",
        serde_json::json!({
            "app_id": app_id,
            "session": session.session_id,
            "busy": true
        }),
    );

    let stop_stream = Arc::new(AtomicBool::new(false));

    // Subscribe to the engine bus before sending the prompt so reasoning and
    // the first text deltas are not lost. The final authoritative history is
    // still fetched when the prompt call returns.
    if let Ok(stream) = session.event_stream() {
        let stream_stop = stop_stream.clone();
        let stream_app = app.clone();
        let stream_app_id = app_id.clone();
        let stream_session = session.clone();
        let stream_id = session.session_id.clone();
        std::thread::spawn(move || {
            let result = stream_session.consume_events(stream, &stream_stop, |items| {
                let _ = stream_app.emit(
                    "chat://stream",
                    serde_json::json!({
                        "app_id": stream_app_id,
                        "session": stream_id,
                        "items": items
                    }),
                );
            });
            if let Err(e) = result {
                eprintln!("chat event stream failed: {e}");
            }
        });
    } else {
        eprintln!("chat event stream unavailable: live deltas disabled");
    }

    std::thread::spawn(move || {
        let title_source = if text.trim().is_empty() {
            attachments
                .first()
                .map(|attachment| attachment.filename.clone())
                .unwrap_or_default()
        } else {
            text.clone()
        };
        session.title_from_first_message(&title_source);
        let result = session.prompt_with_attachments(&text, variant.as_deref(), &attachments);
        stop_stream.store(true, Ordering::Relaxed);
        match result {
            Ok(()) => {
                if let Ok(raw) = session.history_raw() {
                    let items = crate::engine::parse_history(&raw);
                    let _ = app.emit(
                        "chat://update",
                        serde_json::json!({
                            "app_id": app_id,
                            "session": session.session_id,
                            "items": items
                        }),
                    );
                }
            }
            Err(e) => {
                let _ = app.emit(
                    "chat://error",
                    serde_json::json!({
                        "app_id": app_id,
                        "session": session.session_id,
                        "message": e.to_string()
                    }),
                );
            }
        }
        if let Ok(mut busy) = app.state::<AppState>().busy.lock() {
            busy.remove(&busy_key);
        }
        let _ = app.emit(
            "chat://busy",
            serde_json::json!({
                "app_id": app_id,
                "session": session.session_id,
                "busy": false
            }),
        );
    });

    Ok(())
}
#[derive(Debug, Serialize)]
pub struct ChatStatus {
    pub busy: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub fn chat_status(
    state: tauri::State<'_, AppState>,
    app_id: String,
    session_id: Option<String>,
) -> Result<ChatStatus> {
    let session = match session_id {
        Some(id) => id,
        None => {
            let engines = engines_lock(&state)?;
            engines
                .get(&app_id)
                .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?
                .session_id
                .clone()
        }
    };
    let busy = state
        .busy
        .lock()
        .map_err(|_| Error::Msg("busy lock poisoned".to_string()))?
        .contains(&format!("{}:{}", app_id, session));
    Ok(ChatStatus { busy })
}
#[tauri::command(rename_all = "snake_case")]
pub fn chat_abort(state: tauri::State<'_, AppState>, app_id: String) -> Result<()> {
    let engines = engines_lock(&state)?;
    let engine = engines
        .get(&app_id)
        .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
    engine.snapshot().abort()
}
#[tauri::command(rename_all = "snake_case")]
pub async fn chat_list(
    app: tauri::AppHandle,
    app_id: String,
) -> Result<Vec<crate::engine::ChatSummary>> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<crate::engine::ChatSummary>> {
        let state = app.state::<AppState>();
        let session = {
            let engines = engines_lock(&state)?;
            let engine = engines
                .get(&app_id)
                .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
            engine.snapshot()
        };
        // HTTP runs without holding the engine map lock.
        session.sessions()
    })
    .await
    .map_err(|e| Error::Msg(format!("chat list task failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn chat_new(
    app: tauri::AppHandle,
    app_id: String,
) -> Result<crate::engine::ChatInfo> {
    tauri::async_runtime::spawn_blocking(move || -> Result<crate::engine::ChatInfo> {
        let state = app.state::<AppState>();
        let session = {
            let engines = engines_lock(&state)?;
            let engine = engines
                .get(&app_id)
                .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
            engine.snapshot()
        };
        let id = session.new_session("\u{041d}\u{043e}\u{0432}\u{044b}\u{0439} \u{0447}\u{0430}\u{0442}")?;
        let mut engines = engines_lock(&state)?;
        let engine = engines
            .get_mut(&app_id)
            .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
        engine.select_session(&id)?;
        Ok(engine.info())
    })
    .await
    .map_err(|e| Error::Msg(format!("chat create task failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub fn chat_select(
    state: tauri::State<'_, AppState>,
    app_id: String,
    session_id: String,
) -> Result<crate::engine::ChatInfo> {

    let mut engines = engines_lock(&state)?;
    let engine = engines
        .get_mut(&app_id)
        .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
    engine.select_session(&session_id)?;
    Ok(engine.info())
}
#[tauri::command(rename_all = "snake_case")]
pub async fn chat_delete(
    app: tauri::AppHandle,
    app_id: String,
    session_id: String,
) -> Result<crate::engine::ChatInfo> {
    tauri::async_runtime::spawn_blocking(move || -> Result<crate::engine::ChatInfo> {
        let state = app.state::<AppState>();
        let busy_key = format!("{}:{}", app_id, session_id);
        if let Ok(busy) = state.busy.lock() {
            if busy.contains(&busy_key) {
                return msg("stop this chat before deleting it");
            }
        }
        let (session, was_current) = {
            let engines = engines_lock(&state)?;
            let engine = engines
                .get(&app_id)
                .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
            (engine.snapshot(), engine.session_id == session_id)
        };
        session.delete_session(&session_id)?;
        let next = if was_current {
            let sessions = session.sessions()?;
            Some(match sessions.first() {
                Some(first) => first.id.clone(),
                None => session.new_session("\u{041d}\u{043e}\u{0432}\u{044b}\u{0439} \u{0447}\u{0430}\u{0442}")?,
            })
        } else {
            None
        };
        let mut engines = engines_lock(&state)?;
        let engine = engines
            .get_mut(&app_id)
            .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
        if let Some(id) = next {
            engine.select_session(&id)?;
        }
        Ok(engine.info())
    })
    .await
    .map_err(|e| Error::Msg(format!("chat delete task failed: {e}")))?
}
#[tauri::command(rename_all = "snake_case")]
pub async fn chat_history(
    app: tauri::AppHandle,
    app_id: String,
) -> Result<Vec<crate::engine::ChatItem>> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<crate::engine::ChatItem>> {
        let state = app.state::<AppState>();
        let session = {
            let engines = engines_lock(&state)?;
            let engine = engines
                .get(&app_id)
                .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
            engine.snapshot()
        };
        session.history()
    })
    .await
    .map_err(|e| Error::Msg(format!("chat history task failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn chat_reset(app: tauri::AppHandle, app_id: String) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let state = app.state::<AppState>();
        let session = {
            let engines = engines_lock(&state)?;
            let engine = engines
                .get(&app_id)
                .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
            engine.snapshot()
        };
        let id = session.new_session("chat")?;
        let mut engines = engines_lock(&state)?;
        let engine = engines
            .get_mut(&app_id)
            .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
        engine.select_session(&id)?;
        Ok(())
    })
    .await
    .map_err(|e| Error::Msg(format!("chat reset task failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn chat_stop(app: tauri::AppHandle, app_id: String) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let state = app.state::<AppState>();
        let mut engines = engines_lock(&state)?;
        engines.remove(&app_id);
        Ok(())
    })
    .await
    .map_err(|e| Error::Msg(format!("chat stop task failed: {e}")))?
}

