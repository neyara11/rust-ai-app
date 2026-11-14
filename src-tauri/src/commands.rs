use crate::artifacts;
use crate::config::{self, UserConfig};
use crate::docs::{DocMeta, DocView, DocStore};
use crate::error::{msg, Error, Result};
use crate::manifest;
use crate::mcp::{self, McpSession};
use crate::registry;
use crate::secrets::SecretStore;
use crate::state::{AppState, Session};
use serde::Serialize;
use tauri::Manager;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::sync::MutexGuard;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
pub struct PlatformInfo {
    pub home: String,
    pub version: String,
    pub schema: u32,
    pub trusted_anchor: bool,
    pub anchor_fingerprint: Option<String>,
    pub anchor_source: Option<String>,
    pub dev_mode: bool,
}

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub app_id: String,
    pub dir: String,
    pub workspace: String,
    pub docs: Vec<DocMeta>,
}

#[derive(Debug, Serialize)]
pub struct Probe {
    pub ok: bool,
    pub millis: u64,
    pub detail: String,
}

fn lock<'a>(state: &'a AppState) -> Result<MutexGuard<'a, std::collections::HashMap<String, Session>>> {
    state
        .sessions
        .lock()
        .map_err(|_| Error::Msg("session lock poisoned".to_string()))
}

fn with_session<T>(
    state: &AppState,
    app_id: &str,
    f: impl FnOnce(&Session) -> Result<T>,
) -> Result<T> {
    let guard = lock(state)?;
    let session = guard
        .get(app_id)
        .ok_or_else(|| Error::Msg(format!("app {app_id} is not open")))?;
    f(session)
}

#[tauri::command(rename_all = "snake_case")]
pub fn platform_info(state: tauri::State<'_, AppState>) -> PlatformInfo {
    let anchor = registry::trusted_anchor(&state.home);
    PlatformInfo {
        home: state.home.display().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema: manifest::SUPPORTED_SCHEMA,
        trusted_anchor: anchor.is_some(),
        anchor_fingerprint: registry::anchor_fingerprint(&state.home),
        anchor_source: anchor.map(|(_, source)| source.as_str().to_string()),
        dev_mode: registry::dev_mode(),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_apps(state: tauri::State<'_, AppState>) -> Vec<registry::InstalledApp> {
    registry::list(&state.home)
}

#[tauri::command(rename_all = "snake_case")]
pub fn install_app(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<registry::InstalledApp> {
    let src = PathBuf::from(&path);
    if !src.exists() {
        return msg(format!("package path does not exist: {path}"));
    }
    registry::install(&state.home, &src)
}

#[tauri::command(rename_all = "snake_case")]
pub fn remove_app(state: tauri::State<'_, AppState>, app_id: String) -> Result<()> {
    let home = state.home.clone();
    if let Ok(mut engines) = state.engines.lock() {
        engines.remove(&app_id);
    }
    let mut guard = lock(&state)?;
    if let Some(session) = guard.remove(&app_id) {
        artifacts::close_session(&session.dir);
    }
    drop(guard);
    registry::remove(&home, &app_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn app_history(state: tauri::State<'_, AppState>, app_id: String) -> Vec<registry::ArchivedVersion> {
    registry::history(&state.home, &app_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn rollback_app(
    state: tauri::State<'_, AppState>,
    app_id: String,
    version: Option<String>,
) -> Result<registry::InstalledApp> {
    let home = state.home.clone();
    if let Ok(mut engines) = state.engines.lock() {
        engines.remove(&app_id);
    }
    let mut guard = lock(&state)?;
    if let Some(session) = guard.remove(&app_id) {
        artifacts::close_session(&session.dir);
    }
    drop(guard);
    registry::rollback(&home, &app_id, version.as_deref())
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
pub fn probe_endpoint(url: String) -> Probe {
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
    SecretStore::new(&state.home).list()
}

#[tauri::command(rename_all = "snake_case")]
pub fn secret_store(state: tauri::State<'_, AppState>, name: String, value: String) -> Result<()> {
    SecretStore::new(&state.home).store(&name, &value)
}

#[tauri::command(rename_all = "snake_case")]
pub fn secret_delete(state: tauri::State<'_, AppState>, name: String) -> Result<()> {
    SecretStore::new(&state.home).delete(&name)
}

#[tauri::command(rename_all = "snake_case")]
pub fn open_app(state: tauri::State<'_, AppState>, app_id: String) -> Result<SessionInfo> {
    manifest::parse_id(&app_id, "app")?;
    let app_path = registry::find(&state.home, &app_id)
        .ok_or_else(|| Error::Msg(format!("app {app_id} is not installed")))?;

    let trusted = registry::load_trusted_key(&state.home);
    let report = manifest::verify_package(&app_path, trusted.as_ref());
    if !report.ok {
        return msg(format!(
            "app {app_id} failed verification: {}",
            report.problems.join("; ")
        ));
    }
    let app_manifest = manifest::load(&app_path)?;

    let policy = config::policy(&state.home);
    let root = artifacts::sessions_root(&state.home, &app_id);
    let _ = artifacts::sweep(&root, &policy);
    let dir = artifacts::open_session(&root, &policy)?;
    let docs = DocStore::new(dir.clone())?;

    for def in &app_manifest.docs {
        if docs.read(&def.id).is_err() {
            let _ = docs.create(&def.id, "");
        }
    }
    let doc_list = docs.list()?;
    let workspace = config::workspace(&state.home, &app_id);

    let mut guard = lock(&state)?;
    guard.insert(
        app_id.clone(),
        Session {
            dir: dir.clone(),
            docs,
            started_at: artifacts::now_secs(),
        },
    );

    Ok(SessionInfo {
        app_id,
        dir: dir.display().to_string(),
        workspace: workspace.display().to_string(),
        docs: doc_list,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn close_app(state: tauri::State<'_, AppState>, app_id: String) -> Result<()> {
    if let Ok(mut engines) = state.engines.lock() {
        engines.remove(&app_id);
    }
    let mut guard = lock(&state)?;
    if let Some(session) = guard.remove(&app_id) {
        artifacts::close_session(&session.dir);
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_docs(state: tauri::State<'_, AppState>, app_id: String) -> Result<Vec<DocMeta>> {
    with_session(&state, &app_id, |s| s.docs.list())
}

#[tauri::command(rename_all = "snake_case")]
pub fn read_doc(
    state: tauri::State<'_, AppState>,
    app_id: String,
    doc_id: String,
) -> Result<DocView> {
    with_session(&state, &app_id, |s| s.docs.read(&doc_id))
}

#[tauri::command(rename_all = "snake_case")]
pub fn create_doc(
    state: tauri::State<'_, AppState>,
    app_id: String,
    doc_id: String,
    markdown: String,
) -> Result<DocView> {
    with_session(&state, &app_id, |s| s.docs.create(&doc_id, &markdown))
}

#[tauri::command(rename_all = "snake_case")]
pub fn write_doc(
    state: tauri::State<'_, AppState>,
    app_id: String,
    doc_id: String,
    markdown: String,
    expected_revision: Option<String>,
) -> Result<DocView> {
    with_session(&state, &app_id, |s| {
        s.docs
            .write(&doc_id, &markdown, expected_revision.as_deref())
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn append_doc(
    state: tauri::State<'_, AppState>,
    app_id: String,
    doc_id: String,
    markdown: String,
    expected_revision: Option<String>,
) -> Result<DocView> {
    with_session(&state, &app_id, |s| {
        s.docs
            .append(&doc_id, &markdown, expected_revision.as_deref())
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn insert_after_doc(
    state: tauri::State<'_, AppState>,
    app_id: String,
    doc_id: String,
    anchor: String,
    markdown: String,
    expected_revision: Option<String>,
) -> Result<DocView> {
    with_session(&state, &app_id, |s| {
        s.docs.insert_after(
            &doc_id,
            &anchor,
            &markdown,
            expected_revision.as_deref(),
        )
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn replace_block_doc(
    state: tauri::State<'_, AppState>,
    app_id: String,
    doc_id: String,
    anchor: String,
    markdown: String,
    expected_revision: Option<String>,
) -> Result<DocView> {
    with_session(&state, &app_id, |s| {
        s.docs.replace_block(
            &doc_id,
            &anchor,
            &markdown,
            expected_revision.as_deref(),
        )
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn finalize_doc(
    state: tauri::State<'_, AppState>,
    app_id: String,
    doc_id: String,
) -> Result<DocView> {
    with_session(&state, &app_id, |s| s.docs.finalize(&doc_id))
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
        if let Ok(mut engines) = state.engines.lock() {
            engines.clear();
        }
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| Error::Msg(format!("workspace dialog failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub fn artifacts_usage(state: tauri::State<'_, AppState>, app_id: String) -> artifacts::Usage {
    let root = artifacts::sessions_root(&state.home, &app_id);
    artifacts::usage(&root)
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
    use super::parse_host_port;

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
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn mcp_server_save(
    state: tauri::State<'_, AppState>,
    id: String,
    url: String,
    token: Option<String>,
) -> Result<()> {
    let secrets = SecretStore::new(&state.home);
    let def = mcp::ServerDef {
        id,
        url: url.trim().to_string(),
        token: token.filter(|t| !t.trim().is_empty()),
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

#[tauri::command(rename_all = "snake_case")]
pub async fn mcp_probe(app: tauri::AppHandle, id: String) -> Result<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let def = mcp::get(&SecretStore::new(&state.home), &id)?;
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
    })
    .await
    .map_err(|e| Error::Msg(format!("mcp probe task failed: {e}")))?
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

fn chat_open_blocking(state: &AppState, app_id: String) -> Result<crate::engine::ChatInfo> {
    {
        let engines = engines_lock(state)?;
        if let Some(engine) = engines.get(&app_id) {
            return Ok(engine.info());
        }
    }

    // The agent works in the user-chosen workspace; the artifacts session is
    // still used for quotas and cleanup.
    let workspace = config::workspace(&state.home, &app_id);
    let engine = crate::engine::Engine::start(&app_id, &state.home, &workspace)?;

    let mut engines = engines_lock(state)?;
    if let Some(existing) = engines.get(&app_id) {
        return Ok(existing.info());
    }
    let info = engine.info();
    engines.insert(app_id, engine);
    Ok(info)
}
#[tauri::command(rename_all = "snake_case")]
pub fn chat_send(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    app_id: String,
    text: String,
    variant: Option<String>,
) -> Result<()> {
    let session = {
        let engines = engines_lock(&state)?;
        engines
            .get(&app_id)
            .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?
            .snapshot()
    };

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
        session.title_from_first_message(&text);
        let result = session.prompt(&text, variant.as_deref());
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
pub fn chat_list(
    state: tauri::State<'_, AppState>,
    app_id: String,
) -> Result<Vec<crate::engine::ChatSummary>> {
    let engines = engines_lock(&state)?;
    let engine = engines
        .get(&app_id)
        .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
    engine.sessions()
}

#[tauri::command(rename_all = "snake_case")]
pub fn chat_new(
    state: tauri::State<'_, AppState>,
    app_id: String,
) -> Result<crate::engine::ChatInfo> {

    let mut engines = engines_lock(&state)?;
    let engine = engines
        .get_mut(&app_id)
        .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
    engine.new_session("Новый чат")?;
    Ok(engine.info())
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
pub fn chat_delete(
    state: tauri::State<'_, AppState>,
    app_id: String,
    session_id: String,
) -> Result<crate::engine::ChatInfo> {
    let busy_key = format!("{}:{}", app_id, session_id);
    if let Ok(busy) = state.busy.lock() {
        if busy.contains(&busy_key) {
            return msg("stop this chat before deleting it");
        }
    }
    let mut engines = engines_lock(&state)?;
    let engine = engines
        .get_mut(&app_id)
        .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;

    let was_current = engine.session_id == session_id;
    engine.delete_session(&session_id)?;

    if was_current {
        let sessions = engine.sessions()?;
        if let Some(first) = sessions.first() {
            engine.select_session(&first.id)?;
        } else {
            engine.new_session("\u{041d}\u{043e}\u{0432}\u{044b}\u{0439} \u{0447}\u{0430}\u{0442}")?;
        }
    }
    Ok(engine.info())
}
#[tauri::command(rename_all = "snake_case")]
pub fn chat_history(
    state: tauri::State<'_, AppState>,
    app_id: String,
) -> Result<Vec<crate::engine::ChatItem>> {
    let engines = engines_lock(&state)?;
    let engine = engines
        .get(&app_id)
        .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
    engine.history()
}

#[tauri::command(rename_all = "snake_case")]
pub fn chat_reset(state: tauri::State<'_, AppState>, app_id: String) -> Result<()> {
    let mut engines = engines_lock(&state)?;
    let engine = engines
        .get_mut(&app_id)
        .ok_or_else(|| Error::Msg("chat is not open for this app".to_string()))?;
    engine.reset()
}

#[tauri::command(rename_all = "snake_case")]
pub fn chat_stop(state: tauri::State<'_, AppState>, app_id: String) -> Result<()> {
    let mut engines = engines_lock(&state)?;
    engines.remove(&app_id);
    Ok(())
}

