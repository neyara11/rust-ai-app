use crate::config;
use crate::error::{msg, Error, Result};
use crate::manifest;
use crate::mcp;
use crate::secrets::SecretStore;
use base64::Engine as _;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const DISABLED_BUILTINS: &[&str] = &[
    "bash",
    "edit",
    "write",
    "apply_patch",
    "read",
    "glob",
    "grep",
    "webfetch",
    "task",
    "todowrite",
    "websearch",
    "question",
];

#[derive(Debug, Clone, Serialize)]
pub struct ChatItem {
    pub role: String,
    pub kind: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tok_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl ChatItem {
    fn new(role: &str, kind: &str, text: String) -> Self {
        Self {
            role: role.to_string(),
            kind: kind.to_string(),
            text,
            tokens_in: None,
            tokens_out: None,
            reasoning_tokens: None,
            ms: None,
            tok_s: None,
            model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatSummary {
    pub id: String,
    pub title: String,
    pub updated: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatInfo {
    pub engine: bool,
    pub model: String,
    pub session: String,
    pub workdir: String,
}

/// A running opencode sidecar for one application. One engine per app, started
/// on demand and stopped when the app is closed.
pub struct Engine {
    child: Child,
    base: String,
    auth: String,
    pub provider_id: String,
    pub model_id: String,
    pub session_id: String,
    pub system: String,
    pub workdir: PathBuf,
    pid_file: PathBuf,
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.pid_file);
    }
}

fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

fn find_binary(home: &Path) -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("ECO_OPENCODE_BIN") {
        let p = PathBuf::from(custom);
        if p.is_file() {
            return Ok(p);
        }
    }
    let bundled = home.join("engine").join("opencode.exe");
    if bundled.is_file() {
        return Ok(bundled);
    }
    let dev = PathBuf::from("tools")
        .join("oc")
        .join("node_modules")
        .join("opencode-ai")
        .join("bin")
        .join("opencode.exe");
    if dev.is_file() {
        return Ok(dev);
    }
    msg("opencode binary not found: set ECO_OPENCODE_BIN or place it in <home>/engine/opencode.exe")
}

fn basic_auth(password: &str) -> String {
    let raw = format!("opencode:{password}");
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw.as_bytes())
    )
}

fn agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(300));
    if let Ok(tls) = ureq::native_tls::TlsConnector::new() {
        builder = builder.tls_connector(std::sync::Arc::new(tls));
    }
    builder.build()
}

fn http(
    agent: &ureq::Agent,
    base: &str,
    auth: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value> {
    let url = format!("{base}{path}");
    let mut request = match method {
        "GET" => agent.get(&url),
        "PATCH" => agent.patch(&url),
        "DELETE" => agent.delete(&url),
        _ => agent.post(&url),
    }
    .set("Authorization", auth)
    .set("Accept", "application/json");

    let response = if let Some(value) = body {
        request = request.set("Content-Type", "application/json");
        request.send_string(&serde_json::to_string(value)?)
    } else {
        request.call()
    };

    match response {
        Ok(resp) => {
            let text = resp
                .into_string()
                .map_err(|e| Error::Msg(format!("engine read failed: {e}")))?;
            if text.trim().is_empty() {
                Ok(Value::Null)
            } else {
                Ok(serde_json::from_str(&text)?)
            }
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            msg(format!("engine HTTP {code}: {}", body.chars().take(400).collect::<String>()))
        }
        Err(e) => msg(format!("engine transport error: {e}")),
    }
}

fn provider_config(
    cfg: &config::UserConfig,
    secrets: &SecretStore,
) -> Result<(Value, String, String, Vec<(String, String)>)> {
    let endpoint = cfg.llm_endpoint.clone().unwrap_or_default();
    let mut envs: Vec<(String, String)> = Vec::new();
    let model_name = cfg
        .llm_model
        .clone()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "stub-model".to_string());

    if endpoint.trim().is_empty() {
        // Demo path: opencode's built-in free model.
        let provider = json!({});
        return Ok((
            provider,
            "opencode".to_string(),
            "glm-5.3-flash-free".to_string(),
            envs,
        ));
    }

    // The key never goes to disk: opencode substitutes {env:...} at runtime.
    let api_key = match secrets.load("llm_api_key")? {
        Some(key) if !key.trim().is_empty() => {
            envs.push(("ECO_LLM_KEY".to_string(), key));
            "{env:ECO_LLM_KEY}".to_string()
        }
        _ => "none".to_string(),
    };

    let provider = json!({
        "gw": {
            "npm": "@ai-sdk/openai-compatible",
            "name": "LLM Gateway",
            "options": {
                "baseURL": endpoint,
                "apiKey": api_key
            },
            "models": {
                (model_name.clone()): {
                    "name": model_name.clone(),
                    "variants": {
                        "low": { "reasoningEffort": "low" },
                        "medium": { "reasoningEffort": "medium" },
                        "high": { "reasoningEffort": "high" }
                    }
                }
            }
        }
    });

    Ok((provider, "gw".to_string(), model_name, envs))
}

fn build_config(
    app: &manifest::Manifest,
    cfg: &config::UserConfig,
    secrets: &SecretStore,
) -> Result<(Value, String, String, Vec<(String, String)>)> {
    let (provider, provider_id, model_id, mut envs) = provider_config(cfg, secrets)?;

    let mut servers = serde_json::Map::new();
    let mut permissions = serde_json::Map::new();
    permissions.insert("*".to_string(), json!("deny"));

    for connection in &app.connections {
        if connection.kind != "http-mcp" {
            continue;
        }
        let def = mcp::get(secrets, &connection.id)?;
        let mut headers = serde_json::Map::new();
        if let Some(token) = &def.token {
            let var = format!(
                "ECO_MCP_{}_TOKEN",
                connection.id.to_uppercase().replace('-', "_")
            );
            headers.insert(
                "Authorization".to_string(),
                json!(format!("Bearer {{env:{var}}}")),
            );
            envs.push((var, token.clone()));
        }
        servers.insert(
            connection.id.clone(),
            json!({
                "type": "remote",
                "url": def.url,
                "headers": Value::Object(headers),
                "timeout": 30000
            }),
        );
        for tool in &app.tools {
            permissions.insert(
                format!("{}_{}", connection.id, tool.tool),
                json!("allow"),
            );
        }
    }

    let mut tools = serde_json::Map::new();
    for name in DISABLED_BUILTINS {
        tools.insert((*name).to_string(), json!(false));
    }

    let mut root = serde_json::Map::new();
    root.insert("$schema".to_string(), json!("https://opencode.ai/config.json"));
    root.insert("model".to_string(), json!(format!("{provider_id}/{model_id}")));
    root.insert("autoupdate".to_string(), json!(false));
    root.insert("share".to_string(), json!("disabled"));
    if !provider.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        root.insert("provider".to_string(), provider);
    }
    if !servers.is_empty() {
        root.insert("mcp".to_string(), Value::Object(servers));
    }
    root.insert("permission".to_string(), Value::Object(permissions));
    root.insert("tools".to_string(), Value::Object(tools));

    Ok((Value::Object(root), provider_id, model_id, envs))
}

fn write_config(engine_home: &Path, config: &Value) -> Result<()> {
    let dir = engine_home.join("config").join("opencode");
    std::fs::create_dir_all(&dir)?;
    let mut file = std::fs::File::create(dir.join("opencode.json"))?;
    file.write_all(serde_json::to_string_pretty(config)?.as_bytes())?;
    Ok(())
}

fn wait_ready(agent: &ureq::Agent, base: &str, auth: &str, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    loop {
        if http(agent, base, auth, "GET", "/config", None).is_ok() {
            return Ok(());
        }
        if started.elapsed() > timeout {
            return msg("engine did not become ready in time");
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn read_system_prompt(package_dir: &Path, app: &manifest::Manifest) -> Result<String> {
    if app.profile.system_prompt.is_empty() {
        return Ok(String::new());
    }
    let path = package_dir.join(&app.profile.system_prompt);
    Ok(std::fs::read_to_string(path)?)
}

fn skill_texts(package_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let dir = package_dir.join("skills");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path().join("SKILL.md");
        if let Ok(text) = std::fs::read_to_string(path) {
            out.push(text);
        }
    }
    out
}

impl Engine {
    pub fn start(app_id: &str, home: &Path, workdir: &Path) -> Result<Self> {
        let package_dir = crate::registry::apps_dir(home).join(app_id);
        let app = crate::registry::verified_manifest(home, app_id)?;
        let cfg = config::load(home);
        let secrets = SecretStore::new(home);

        let (engine_config, provider_id, model_id, engine_envs) =
            build_config(&app, &cfg, &secrets)?;

        let engine_home = home.join("apps").join(app_id).join("engine");
        std::fs::create_dir_all(&engine_home)?;
        write_config(&engine_home, &engine_config)?;

        // Kill an engine left behind by a previous crash or a force-killed app.
        let pid_file = engine_home.join("engine.pid");
        if let Ok(raw) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = raw.trim().parse::<u32>() {
                let mut kill = Command::new("taskkill");
                kill.args(["/PID", &pid.to_string(), "/F"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    kill.creation_flags(0x0800_0000);
                }
                let _ = kill.status();
            }
            let _ = std::fs::remove_file(&pid_file);
        }

        let binary = find_binary(home)?;
        let port = free_port()?;
        let base = format!("http://127.0.0.1:{port}");
        let password = format!(
            "{:x}",
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0))
        );
        let auth = basic_auth(&password);

        let mut command = Command::new(&binary);
        command
            .args(["serve", "--port", &port.to_string(), "--hostname", "127.0.0.1"])
            .current_dir(workdir)
            .env("HOME", &engine_home)
            .env("USERPROFILE", &engine_home)
            .env("XDG_CONFIG_HOME", engine_home.join("config"))
            .env("XDG_DATA_HOME", engine_home.join("data"))
            .env("XDG_CACHE_HOME", engine_home.join("cache"))
            .env("OPENCODE_SERVER_PASSWORD", &password)
            .env("OPENCODE_DISABLE_AUTOUPDATE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        for (key, value) in &engine_envs {
            command.env(key, value);
        }
        let child = command
            .spawn()
            .map_err(|e| Error::Msg(format!("cannot start opencode: {e}")))?;

        let agent = agent();
        wait_ready(&agent, &base, &auth, Duration::from_secs(25))?;
        std::fs::write(&pid_file, child.id().to_string())?;

        let session_id = match existing_session(&agent, &base, &auth)? {
            Some(id) => id,
            None => create_session(&agent, &base, &auth, &app.name)?,
        };

        let mut system = read_system_prompt(&package_dir, &app)?;
        for skill in skill_texts(&package_dir) {
            system.push_str("\n\n---\n\n");
            system.push_str(&skill);
        }

        Ok(Self {
            child,
            base,
            auth,
            provider_id,
            model_id,
            session_id,
            system,
            workdir: workdir.to_path_buf(),
            pid_file,
        })
    }

    pub fn info(&self) -> ChatInfo {
        ChatInfo {
            engine: true,
            model: format!("{}/{}", self.provider_id, self.model_id),
            session: self.session_id.clone(),
            workdir: self.workdir.display().to_string(),
        }
    }

    pub fn sessions(&self) -> Result<Vec<ChatSummary>> {
        let agent = agent();
        let raw = http(&agent, &self.base, &self.auth, "GET", "/session", None)?;
        Ok(parse_sessions(&raw))
    }

    pub fn new_session(&mut self, title: &str) -> Result<()> {
        let agent = agent();
        self.session_id = create_session(&agent, &self.base, &self.auth, title)?;
        Ok(())
    }

    pub fn select_session(&mut self, id: &str) -> Result<()> {
        crate::manifest::parse_id(id, "session")?;
        self.session_id = id.to_string();
        Ok(())
    }

    pub fn delete_session(&mut self, id: &str) -> Result<()> {
        crate::manifest::parse_id(id, "session")?;
        let agent = agent();
        http(
            &agent,
            &self.base,
            &self.auth,
            "DELETE",
            &format!("/session/{id}"),
            None,
        )?;
        Ok(())
    }

    pub fn snapshot(&self) -> ChatSession {
        ChatSession {
            base: self.base.clone(),
            auth: self.auth.clone(),
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
            session_id: self.session_id.clone(),
            system: self.system.clone(),
        }
    }

    pub fn history(&self) -> Result<Vec<ChatItem>> {
        self.snapshot().history()
    }

    pub fn send(&self, text: &str) -> Result<Vec<ChatItem>> {
        let session = self.snapshot();
        session.prompt(text, None)?;
        session.history()
    }

    pub fn reset(&mut self) -> Result<()> {
        let agent = agent();
        let session = http(
            &agent,
            &self.base,
            &self.auth,
            "POST",
            "/session",
            Some(&json!({ "title": "chat" })),
        )?;
        self.session_id = session
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Msg("engine did not return a session id".to_string()))?
            .to_string();
        Ok(())
    }
}

/// Lightweight handle to one chat session: safe to move into a worker thread
/// so the UI never blocks on a model call.
#[derive(Clone)]
pub struct ChatSession {
    pub base: String,
    pub auth: String,
    pub provider_id: String,
    pub model_id: String,
    pub session_id: String,
    pub system: String,
}

impl ChatSession {
    /// Synchronous prompt: returns when the agent finishes the turn. Callers
    /// run it on a worker thread so the UI stays responsive.
    pub fn prompt(&self, text: &str, variant: Option<&str>) -> Result<()> {
        let agent = agent();
        let mut payload = json!({
            "model": { "providerID": self.provider_id, "modelID": self.model_id },
            "system": self.system,
            "parts": [{ "type": "text", "text": text }]
        });
        if let Some(variant) = variant {
            if let Some(object) = payload.as_object_mut() {
                object.insert("variant".to_string(), json!(variant));
            }
        }
        http(
            &agent,
            &self.base,
            &self.auth,
            "POST",
            &format!("/session/{}/message", self.session_id),
            Some(&payload),
        )?;
        Ok(())
    }

    /// Interrupts the current turn, if any.
    pub fn abort(&self) -> Result<()> {
        let agent = agent();
        http(
            &agent,
            &self.base,
            &self.auth,
            "POST",
            &format!("/session/{}/abort", self.session_id),
            Some(&json!({})),
        )?;
        Ok(())
    }

    /// Sets the chat title from the first user message, if the chat is empty.
    pub fn title_from_first_message(&self, text: &str) {
        let empty = self
            .history_raw()
            .map(|raw| raw.as_array().map(|a| a.is_empty()).unwrap_or(true))
            .unwrap_or(false);
        if !empty {
            return;
        }
        let mut title: String = text.lines().next().unwrap_or(text).trim().chars().take(60).collect();
        if title.is_empty() {
            title = "Новый диалог".to_string();
        }
        let agent = agent();
        let _ = http(
            &agent,
            &self.base,
            &self.auth,
            "PATCH",
            &format!("/session/{}", self.session_id),
            Some(&json!({ "title": title })),
        );
    }

    pub fn history_raw(&self) -> Result<Value> {
        let agent = agent();
        http(
            &agent,
            &self.base,
            &self.auth,
            "GET",
            &format!("/session/{}/message", self.session_id),
            None,
        )
    }

    pub fn history(&self) -> Result<Vec<ChatItem>> {
        Ok(parse_history(&self.history_raw()?))
    }

    /// True while the last assistant message has no completion timestamp.
    pub fn busy(&self, raw: &Value) -> bool {
        let Some(messages) = raw.as_array() else {
            return true;
        };
        let mut last: Option<&Value> = None;
        for message in messages {
            let role = message
                .get("info")
                .and_then(|i| i.get("role"))
                .and_then(Value::as_str);
            if role == Some("assistant") {
                last = Some(message);
            }
        }
        match last {
            None => true,
            Some(message) => message
                .get("info")
                .and_then(|i| i.get("completed"))
                .is_none(),
        }
    }
}

pub fn parse_history(raw: &Value) -> Vec<ChatItem> {
    let mut items = Vec::new();
    let Some(messages) = raw.as_array() else {
        return items;
    };
    for message in messages {
        let info = message.get("info");
        let role = info
            .and_then(|i| i.get("role"))
            .and_then(Value::as_str)
            .unwrap_or("assistant")
            .to_string();

        let mut text = String::new();
        let mut reasoning = String::new();
        let mut tools = Vec::new();
        if let Some(parts) = message.get("parts").and_then(Value::as_array) {
            for part in parts {
                match part.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(t) = part.get("text").and_then(Value::as_str) {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(t);
                        }
                    }
                    Some("reasoning") => {
                        if let Some(t) = part.get("text").and_then(Value::as_str) {
                            if !reasoning.is_empty() {
                                reasoning.push('\n');
                            }
                            reasoning.push_str(t);
                        }
                    }
                    Some("tool") => {
                        let name = part
                            .get("tool")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string();
                        let status = part
                            .get("state")
                            .and_then(|s| s.get("status"))
                            .and_then(Value::as_str)
                            .unwrap_or("called");
                        tools.push(format!("{name} -> {status}"));
                    }
                    _ => {}
                }
            }
        }

        let tokens = info.and_then(|i| i.get("tokens"));
        let tokens_in = tokens.and_then(|t| t.get("input")).and_then(Value::as_u64);
        let tokens_out = tokens.and_then(|t| t.get("output")).and_then(Value::as_u64);
        let reasoning_tokens = tokens
            .and_then(|t| t.get("reasoning"))
            .and_then(Value::as_u64);
        let model = info
            .and_then(|i| i.get("modelID"))
            .and_then(Value::as_str)
            .map(str::to_string);

        let time = info.and_then(|i| i.get("time"));
        let created = time.and_then(|t| t.get("created")).and_then(Value::as_u64);
        let completed = time
            .and_then(|t| t.get("completed"))
            .and_then(Value::as_u64);
        let ms = match (created, completed) {
            (Some(start), Some(end)) if end >= start => Some(end - start),
            _ => None,
        };
        // Reasoning tokens are generated too, so they count towards speed.
        let generated = tokens_out.unwrap_or(0) + reasoning_tokens.unwrap_or(0);
        let tok_s = match ms {
            Some(ms) if ms > 0 && generated > 0 => {
                Some(generated as f64 / (ms as f64 / 1000.0))
            }
            _ => None,
        };

        if !reasoning.is_empty() {
            items.push(ChatItem::new(&role, "reasoning", reasoning));
        }
        if !text.is_empty() {
            let mut item = ChatItem::new(&role, "text", text);
            item.tokens_in = tokens_in;
            item.tokens_out = tokens_out;
            item.reasoning_tokens = reasoning_tokens;
            item.ms = ms;
            item.tok_s = tok_s;
            item.model = model;
            items.push(item);
        }
        for tool in tools {
            items.push(ChatItem::new("tool", "tool", tool));
        }
    }
    items
}
fn create_session(agent: &ureq::Agent, base: &str, auth: &str, title: &str) -> Result<String> {
    let session = http(
        agent,
        base,
        auth,
        "POST",
        "/session",
        Some(&json!({ "title": title })),
    )?;
    session
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::Msg("engine did not return a session id".to_string()))
}

fn existing_session(agent: &ureq::Agent, base: &str, auth: &str) -> Result<Option<String>> {
    let raw = http(agent, base, auth, "GET", "/session", None)?;
    let sessions = parse_sessions(&raw);
    Ok(sessions.first().map(|s| s.id.clone()))
}

pub fn parse_sessions(raw: &Value) -> Vec<ChatSummary> {
    let mut out = Vec::new();
    let Some(items) = raw.as_array() else {
        return out;
    };
    for item in items {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
            .unwrap_or("\u{0414}\u{0438}\u{0430}\u{043b}\u{043e}\u{0433}")
            .to_string();
        let updated = item
            .get("time")
            .and_then(|t| t.get("updated").or_else(|| t.get("created")))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        out.push(ChatSummary {
            id: id.to_string(),
            title,
            updated,
        });
    }
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    out
}
pub type EngineMap = HashMap<String, Engine>;

