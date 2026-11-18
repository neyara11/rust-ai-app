use crate::error::{msg, Error, Result};
use crate::manifest::{self, Manifest, ToolDef};
use crate::mcp::{self, McpSession};
use crate::registry;
use crate::secrets::SecretStore;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Name of the single local MCP server that opencode sees. Every tool the
/// model can call is exposed by the platform gateway under this name.
pub const SERVER_NAME: &str = "platform";

const MAX_READ_BYTES: usize = 64 * 1024;
const MAX_WRITE_BYTES: usize = 1024 * 1024;
const MAX_LIST_ENTRIES: usize = 200;
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;
const MAX_PACKAGE_OUTPUT: usize = 64 * 1024;
/// Platform default for one package tool call. A real converter or API client
/// often needs minutes, so the floor is 5 minutes; longer tools declare
/// `tools[].timeout_secs` instead of relying on this value.
const PACKAGE_TOOL_TIMEOUT_SECS: u64 = 300;
/// Hard ceiling for a manifest-declared tool timeout (2 hours).
const MAX_PACKAGE_TOOL_TIMEOUT_SECS: u64 = 7200;
/// Results at least this large are also dumped to the artifact NDJSON.
const DUMP_MIN_BYTES: usize = 8 * 1024;

const FILE_TOOLS: &[&str] = &["list_files", "read_file", "write_file", "append_file"];
/// Platform-owned, read-only access to the app's own artifact dumps, so the
/// agent can read a truncated tool result instead of asking the user to grep
/// the Artifacts panel by hand.
const ARTIFACT_TOOLS: &[&str] = &["artifact_list", "artifact_search", "artifact_read"];

const INSTRUCTIONS: &str = "Р’СЃРµ РёРЅСЃС‚СЂСѓРјРµРЅС‚С‹ РїСЂРѕС…РѕРґСЏС‚ С‡РµСЂРµР· С€Р»СЋР· РїР»Р°С‚С„РѕСЂРјС‹: РІРЅРµ allow-list \
РїСЂРёР»РѕР¶РµРЅРёСЏ РІС‹Р·РѕРІ РЅРµРІРѕР·РјРѕР¶РµРЅ. Р¤Р°Р№Р»РѕРІС‹Рµ РёРЅСЃС‚СЂСѓРјРµРЅС‚С‹ СЂР°Р±РѕС‚Р°СЋС‚ С‚РѕР»СЊРєРѕ РІ СЂР°Р±РѕС‡РµР№ РїР°РїРєРµ \
РїСЂРёР»РѕР¶РµРЅРёСЏ; Р°Р±СЃРѕР»СЋС‚РЅС‹Рµ РїСѓС‚Рё Рё РІС‹С…РѕРґ Р·Р° РЅРµС‘ Р·Р°РїСЂРµС‰РµРЅС‹. Р”Р»СЏ Word РёСЃРїРѕР»СЊР·СѓР№С‚Рµ РёРЅСЃС‚СЂСѓРјРµРЅС‚ \
docx_export СЃ РѕС‚РЅРѕСЃРёС‚РµР»СЊРЅС‹РјРё РїСѓС‚СЏРјРё input Рё output.";

/// opencode normalizes tool names to letters, numbers, `_` and `-`. Do the same
/// so permission entries in the generated config match exactly.
fn env_name_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Platform secrets (the LLM gateway key and remote MCP definitions, which
/// contain bearer tokens) must never be handed to package code.
fn secret_allowed_for_package(value: &str) -> bool {
    !manifest::is_reserved_secret(value)
}

fn read_capped_file(path: &Path) -> Result<(Vec<u8>, u64)> {
    let file = fs::File::open(path)?;
    let total = file.metadata()?.len();
    let mut bytes = Vec::new();
    file.take(MAX_READ_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    Ok((bytes, total))
}

fn secret_name_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub fn normalize_tool_name(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn has_file_scope(app: &Manifest) -> bool {
    app.capabilities
        .file_scopes
        .iter()
        .any(|scope| scope == "workspace")
}

/// True when the package ships one or more `skills/<name>/SKILL.md`. The
/// gateway then exposes the platform-owned `skill` tool for progressive
/// loading instead of pasting every skill into the system prompt.
pub fn has_skills(package_dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(package_dir.join("skills")) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| entry.path().join("SKILL.md").is_file())
}

/// Names (without the `platform_` prefix) that the gateway exposes for an app.
/// Shared by the engine config (permission entries) and the proxy.
pub fn gateway_tool_names(app: &Manifest, package_dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    if has_skills(package_dir) {
        out.push("skill".to_string());
    }

    for connection in &app.connections {
        if connection.kind != "http-mcp" {
            continue;
        }
        let writes_allowed = manifest::external_write_allowed(app, &connection.id);
        for tool in &app.tools {
            if tool.bin.is_some() {
                continue;
            }
            if tool.stream == "write" && !writes_allowed {
                continue;
            }
            out.push(normalize_tool_name(&format!(
                "{}_{}",
                connection.id, tool.tool
            )));
        }
    }

    if has_file_scope(app) {
        for name in FILE_TOOLS {
            out.push((*name).to_string());
        }
    }

    // Dumps are the app's own tool output: available regardless of file scope.
    for name in ARTIFACT_TOOLS {
        out.push((*name).to_string());
    }

    for tool in &app.tools {
        if tool.bin.is_some() {
            out.push(normalize_tool_name(&tool.tool));
        }
    }

    out.sort();
    out.dedup();
    out
}

#[derive(Clone)]
enum Route {
    Files,
    Artifacts,
    /// Platform-owned skill loader: reads `skills/<name>/SKILL.md` from the
    /// signed package. Exposed only when the package ships skills.
    Skill,
    Remote { server: String, tool: String },
    Package {
        tool: String,
        bin: String,
        argv: Vec<String>,
        io: String,
        secrets: std::collections::BTreeMap<String, String>,
        timeout_secs: Option<u64>,
    },
}

#[derive(Clone)]
struct ProxyIdentity {
    agent_id: String,
    user: String,
    token_id: String,
    /// Raw token, kept only in memory to re-check revocation on every call.
    raw: String,
}

pub struct Proxy {
    home: PathBuf,
    app: Manifest,
    package_dir: PathBuf,
    workspace: PathBuf,
    artifact_dir: Option<PathBuf>,
    servers: HashMap<String, McpSession>,
    routes: HashMap<String, Route>,
    tools: Vec<Value>,
    built: bool,
    identity: ProxyIdentity,
    call_seq: u64,
    /// Last successful allowlist check, cached briefly to keep the per-call
    /// path from re-reading and re-verifying the signed document every time.
    allow_checked: Option<(Instant, String)>,
}

/// Resolves the agent identity passed by the engine. A missing token is a hard
/// error outside dev mode: the gateway never runs unauthenticated.
fn resolve_identity(home: &Path, app: &Manifest) -> Result<ProxyIdentity> {
    let agent_id = format!("{}/{}", app.app_id, app.version);
    let token = std::env::var("ECO_AGENT_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| {
            crate::error::Error::Msg(
                "agent token is missing: the tool gateway refuses to start".to_string(),
            )
        })?;
    let record = crate::identity::validate(home, &token)?;
    if record.app_id != app.app_id {
        return msg(format!(
            "agent token belongs to {}, not {}",
            record.app_id, app.app_id
        ));
    }
    if record.app_version != app.version {
        return msg(format!(
            "agent token was issued for version {}, not {}",
            record.app_version, app.version
        ));
    }
    let user = std::env::var("ECO_AGENT_USER")
        .ok()
        .map(|user| user.trim().to_string())
        .filter(|user| !user.is_empty())
        .unwrap_or_else(|| crate::identity::user_pseudonym(home));
    Ok(ProxyIdentity {
        agent_id,
        user,
        token_id: record.token_id,
        raw: token,
    })
}

impl Proxy {
    pub fn new(app_id: &str, workspace: &Path, artifact_dir: Option<&Path>) -> Result<Self> {
        let home = registry::home();
        let app = registry::verified_manifest(&home, app_id)?;
        let package_dir = registry::apps_dir(&home).join(app_id);
        if !workspace.is_dir() {
            fs::create_dir_all(workspace)?;
        }
        let workspace = fs::canonicalize(workspace)?;
        let artifact_dir = artifact_dir.filter(|dir| dir.is_dir());
        let identity = resolve_identity(&home, &app)?;
        Ok(Self {
            home,
            app,
            package_dir,
            workspace,
            artifact_dir: artifact_dir.map(Path::to_path_buf),
            servers: HashMap::new(),
            routes: HashMap::new(),
            tools: Vec::new(),
            built: false,
            identity,
            call_seq: 0,
            allow_checked: None,
        })
    }

    /// Builds routes and tool metadata on first tools/list. Remote schemas are
    /// best effort: a server that is down does not hide its declared tools,
    /// the call simply fails with a clear error.
    fn ensure_tools(&mut self) {
        if self.built {
            return;
        }
        self.built = true;

        let mut tools = Vec::new();

        // Platform-owned skill loader is reserved before app tools.
        if has_skills(&self.package_dir) {
            self.routes.insert("skill".to_string(), Route::Skill);
            tools.push(json!({
                "name": "skill",
                "description": "РќР°РІС‹РєРё (СЃРєРёР»С‹) РїСЂРёР»РѕР¶РµРЅРёСЏ. action=list РІРѕР·РІСЂР°С‰Р°РµС‚ СЃРїРёСЃРѕРє, action=read СЃ РёРјРµРЅРµРј РІРѕР·РІСЂР°С‰Р°РµС‚ РїРѕР»РЅС‹Р№ SKILL.md. Р—Р°РіСЂСѓР·Рё РЅСѓР¶РЅС‹Р№ СЃРєРёР» РїРµСЂРµРґ РІС‹РїРѕР»РЅРµРЅРёРµРј Р·Р°РґР°С‡Рё.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "enum": ["list", "read"] },
                        "name": { "type": "string", "description": "РРјСЏ СЃРєРёР»Р° РёР· action=list" }
                    },
                    "required": ["action"],
                    "additionalProperties": false
                }
            }));
        }

        // Platform-owned file tools are reserved next.
        if has_file_scope(&self.app) {
            for name in FILE_TOOLS {
                self.routes.insert((*name).to_string(), Route::Files);
            }
            tools.extend(file_specs());
        }

        // Artifact dumps are always available: they hold this app's own tool
        // output, including results too large to inline.
        for name in ARTIFACT_TOOLS {
            self.routes.insert((*name).to_string(), Route::Artifacts);
        }
        tools.extend(artifact_specs());

        // App-provided executables (the package owns them).
        for tool in self.app.tools.clone() {
            let Some(bin) = tool.bin.clone() else {
                continue;
            };
            let exposed = normalize_tool_name(&tool.tool);
            if self.routes.contains_key(&exposed) {
                eprintln!("eco-platform mcp proxy: duplicate tool {exposed}, skipped");
                continue;
            }
            if tool.argv.len() > 16 || tool.argv.iter().any(|arg| arg.is_empty() || arg.len() > 512) {
                eprintln!("eco-platform mcp proxy: invalid argv for {exposed}, skipped");
                continue;
            }
            if tool.secrets.len() > 16
                || tool.secrets.iter().any(|(env, secret)| {
                    !env_name_ok(env)
                        || !secret_name_ok(secret)
                        || !secret_allowed_for_package(secret)
                })
            {
                eprintln!("eco-platform mcp proxy: invalid secrets mapping for {exposed}, skipped");
                continue;
            }
            let io = tool.io.to_lowercase();
            if io != "files" && io != "json" {
                eprintln!("eco-platform mcp proxy: invalid io mode for {exposed}, skipped");
                continue;
            }
            if io == "json" && tool.argv.iter().any(|arg| arg.contains("{input}") || arg.contains("{output}")) {
                eprintln!("eco-platform mcp proxy: json tool {exposed} must not use {{input}}/{{output}} argv");
                continue;
            }
            self.routes.insert(
                exposed,
                Route::Package {
                    tool: tool.tool.clone(),
                    bin,
                    argv: tool.argv.clone(),
                    io,
                    secrets: tool.secrets.clone(),
                    timeout_secs: tool.timeout_secs,
                },
            );
            tools.push(package_spec(&tool));
        }

        let secrets = SecretStore::new(&self.home);
        for connection in self.app.connections.clone() {
            if connection.kind != "http-mcp" {
                continue;
            }
            let writes_allowed = manifest::external_write_allowed(&self.app, &connection.id);
            let schemas = self.load_schemas(&secrets, &connection.id);
            for tool in self.app.tools.clone() {
                if tool.bin.is_some() {
                    continue;
                }
                if tool.stream == "write" && !writes_allowed {
                    continue;
                }
                let exposed = normalize_tool_name(&format!("{}_{}", connection.id, tool.tool));
                if self.routes.contains_key(&exposed) {
                    eprintln!("eco-platform mcp proxy: duplicate tool {exposed}, skipped");
                    continue;
                }
                let spec = match schemas.get(&tool.tool) {
                    Some(remote) => {
                        let mut spec = remote.clone();
                        spec["name"] = json!(exposed);
                        spec
                    }
                    None => json!({
                        "name": exposed,
                        "description": format!(
                            "РРЅСЃС‚СЂСѓРјРµРЅС‚ {} СЃРµСЂРІРµСЂР° {}.",
                            tool.tool, connection.id
                        ),
                        "inputSchema": { "type": "object" }
                    }),
                };
                self.routes.insert(
                    exposed,
                    Route::Remote {
                        server: connection.id.clone(),
                        tool: tool.tool.clone(),
                    },
                );
                tools.push(spec);
            }
        }

        self.tools = tools;
    }

    fn load_schemas(&mut self, secrets: &SecretStore, server: &str) -> HashMap<String, Value> {
        let def = match mcp::get_for_app(secrets, &self.app.app_id, server) {
            Ok(def) => def,
            Err(e) => {
                eprintln!("eco-platform mcp proxy: {server} is not configured: {e}");
                return HashMap::new();
            }
        };
        if !self.servers.contains_key(server) {
            match McpSession::connect(def) {
                Ok(session) => {
                    self.servers.insert(server.to_string(), session);
                }
                Err(e) => {
                    eprintln!("eco-platform mcp proxy: cannot connect {server}: {e}");
                    return HashMap::new();
                }
            }
        }
        let list = match self.servers.get(server) {
            Some(session) => session.list_tools(),
            None => return HashMap::new(),
        };
        match list {
            Ok(items) => items
                .into_iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.to_string();
                    Some((name, item))
                })
                .collect(),
            Err(e) => {
                eprintln!("eco-platform mcp proxy: cannot list tools of {server}: {e}");
                HashMap::new()
            }
        }
    }

    fn identity_headers(&self, request_id: &str) -> Vec<(String, String)> {
        vec![
            ("X-Agent-Id".to_string(), self.identity.agent_id.clone()),
            ("X-Agent-Version".to_string(), self.app.version.clone()),
            ("X-User".to_string(), self.identity.user.clone()),
            ("X-Request-Id".to_string(), request_id.to_string()),
            ("X-Agent-Token-Id".to_string(), self.identity.token_id.clone()),
        ]
    }

    fn remote_call(
        &mut self,
        server: &str,
        tool: &str,
        arguments: Value,
        request_id: &str,
    ) -> Result<Value> {
        let secrets = SecretStore::new(&self.home);
        if !self.servers.contains_key(server) {
            let def = mcp::get_for_app(&secrets, &self.app.app_id, server)?;
            let session = McpSession::connect(def)?;
            self.servers.insert(server.to_string(), session);
        }
        let headers = self.identity_headers(request_id);
        let result = match self.servers.get(server) {
            Some(session) => session.call_with_headers(tool, arguments, &headers),
            None => return msg(format!("mcp server {server} is not connected")),
        };
        if result.is_err() {
            self.servers.remove(server);
        }
        result
    }

    fn package_call(
        &self,
        tool: &str,
        bin: &str,
        argv: &[String],
        io: &str,
        secrets: &std::collections::BTreeMap<String, String>,
        timeout_secs: Option<u64>,
        request_id: &str,
        arguments: &Value,
    ) -> Result<Value> {
        let files_mode = io != "json";
        let (input, output) = if files_mode {
            let input = text_arg(arguments, "input")?;
            let output = text_arg(arguments, "output")?;
            let input = resolve_path(&self.workspace, input, false)?;
            let output = resolve_path(&self.workspace, output, true)?;
            if !input.is_file() {
                return msg(format!("input file does not exist: {}", input.display()));
            }
            (Some(input), Some(output))
        } else {
            (None, None)
        };

        let converter = package_bin(&self.package_dir, bin)?;
        let mut command = Command::new(&converter);
        let mut stdin_payload: Option<Vec<u8>> = None;

        if files_mode {
            let input = input.as_ref().unwrap();
            let output = output.as_ref().unwrap();
            if argv.is_empty() {
                command
                    .arg("--input")
                    .arg(input)
                    .arg("--output")
                    .arg(output);
            } else {
                let mut uses_input = false;
                let mut uses_output = false;
                for raw in argv {
                    if raw.contains("{input}") {
                        uses_input = true;
                    }
                    if raw.contains("{output}") {
                        uses_output = true;
                    }
                    command.arg(
                        raw.replace("{input}", &input.to_string_lossy())
                            .replace("{output}", &output.to_string_lossy()),
                    );
                }
                if !uses_input || !uses_output {
                    return msg(format!(
                        "package tool {tool} argv must reference both {{input}} and {{output}}"
                    ));
                }
            }
            command.stdin(Stdio::null());
        } else {
            for raw in argv {
                command.arg(raw);
            }
            let payload = serde_json::to_vec(arguments)
                .map_err(|e| Error::Msg(format!("cannot serialize tool arguments: {e}")))?;
            if payload.len() > 256 * 1024 {
                return msg(format!(
                    "package tool {tool} arguments exceed 256 KiB"
                ));
            }
            stdin_payload = Some(payload);
            command.stdin(Stdio::piped());
        }

        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // The package tool is untrusted app code: it must not inherit the
            // engine env (LLM key, agent token) or run in an unexpected cwd.
            .env_clear()
            .current_dir(&self.workspace);
        for key in ["SystemRoot", "windir", "TEMP", "TMP"] {
            if let Ok(value) = std::env::var(key) {
                command.env(key, value);
            }
        }
        if !secrets.is_empty() {
            let store = SecretStore::new(&self.home);
            for (env_name, secret_name) in secrets {
                match store.load_for_app(&self.app.app_id, secret_name)? {
                    Some(value) if !value.is_empty() => {
                        command.env(env_name, value);
                    }
                    _ => {
                        return msg(format!(
                            "package tool {tool} needs secret '{secret_name}' for {env_name}; add it in the application settings"
                        ));
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }

        let timeout = timeout_secs
            .map(|secs| Duration::from_secs(secs.clamp(1, MAX_PACKAGE_TOOL_TIMEOUT_SECS)))
            .unwrap_or_else(package_timeout);
        let cancel_path = self
            .home
            .join("apps")
            .join(&self.app.app_id)
            .join("engine")
            .join("tool.pid");
        let spool_dir = self
            .artifact_dir
            .as_ref()
            .map(|dir| dir.join(crate::artifacts::TOOL_OUTPUT_DIR));
        let run = run_with_limits(
            &mut command,
            timeout,
            MAX_PACKAGE_OUTPUT,
            stdin_payload.as_deref(),
            Some(&cancel_path),
            spool_dir
                .as_deref()
                .map(|dir| (dir, request_id)),
        )?;
        let spill = self.record_spill(tool, request_id, &run);
        if run.timed_out {
            let mut message = format!(
                "package tool {tool} timed out after {}s; process killed",
                timeout.as_secs()
            );
            if let Some((stdout, _)) = &spill {
                message.push_str(&format!(
                    ". Output so far: artifact_read file={stdout}"
                ));
            }
            return msg(message);
        }
        if run.overflowed {
            let spill_hint = spill
                .as_ref()
                .map(|(stdout, _)| format!(" Full output: artifact_read file={stdout}"))
                .unwrap_or_default();
            if files_mode {
                // A killed converter may have produced a partial file; never
                // report success for it.
                return msg(format!(
                    "package tool {tool} output exceeded {} KiB per stream; process killed.{spill_hint}",
                    MAX_PACKAGE_OUTPUT / 1024
                ));
            }
            let preview = String::from_utf8_lossy(&run.stdout)
                .chars()
                .take(2000)
                .collect::<String>();
            let mut result = json!({
                "ok": true,
                "truncated": true,
                "preview": preview,
                "hint": "Вывод инструмента усечён. Полный текст доступен через artifact_read (file и offset), поиск — artifact_search."
            });
            if let Some((stdout, stderr)) = &spill {
                result["stdout_file"] = json!(stdout);
                result["stderr_file"] = json!(stderr);
            }
            return Ok(result);
        }
        if !run.status.success() {
            let stderr = String::from_utf8_lossy(&run.stderr);
            let stdout = String::from_utf8_lossy(&run.stdout);
            let detail = if stderr.trim().is_empty() {
                stdout
            } else {
                stderr
            };
            let snippet: String = detail.trim().chars().take(400).collect();
            let code = run
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "?".to_string());
            let tail = spill
                .as_ref()
                .map(|(stdout, _)| format!(" (full output: artifact_read file={stdout})"))
                .unwrap_or_default();
            if snippet.is_empty() {
                return msg(format!("package tool {tool} failed (exit {code}){tail}"));
            }
            return msg(format!(
                "package tool {tool} failed (exit {code}): {snippet}{tail}"
            ));
        }

        if !files_mode {
            let text = String::from_utf8_lossy(&run.stdout).trim().to_string();
            if text.is_empty() {
                return Ok(json!({ "ok": true }));
            }
            return Ok(serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!(text)));
        }

        let input = input.unwrap();
        let output = output.unwrap();
        if !output.is_file() {
            return msg(format!("package tool {tool} finished without creating the output file"));
        }
        Ok(json!({
            "input": input.display().to_string(),
            "output": output.display().to_string(),
            "bytes": fs::metadata(&output).map(|m| m.len()).unwrap_or(0)
        }))
    }

    /// Read-only access to the app's artifact dumps and saved full tool output.
    fn artifact_call(&self, name: &str, args: &Value) -> Result<Value> {
        let Some(current) = &self.artifact_dir else {
            return msg("artifact dumps are not enabled for this app");
        };
        let root = current
            .parent()
            .ok_or_else(|| Error::Msg("invalid artifact session path".to_string()))?;
        let current_session = current
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        match name {
            "artifact_list" => {
                let dumps: Vec<Value> = crate::artifacts::list_dumps(root)
                    .into_iter()
                    .map(|dump| {
                        json!({
                            "session": dump.session,
                            "bytes": dump.bytes,
                            "modified": dump.modified
                        })
                    })
                    .collect();
                Ok(json!({ "dumps": dumps, "current_session": current_session }))
            }
            "artifact_search" => {
                let query = text_arg(args, "query")?;
                let session = args.get("session").and_then(Value::as_str);
                if let Some(session) = session {
                    crate::artifacts::session_dir(root, session)?;
                }
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(20)
                    .clamp(1, 50) as usize;
                let hits: Vec<Value> = crate::artifacts::grep(root, query, session, limit)?
                    .into_iter()
                    .map(|hit| {
                        json!({
                            "session": hit.session,
                            "line": hit.line,
                            "text": hit.text
                        })
                    })
                    .collect();
                Ok(json!({ "query": query, "hits": hits }))
            }
            "artifact_read" => {
                let session = args
                    .get("session")
                    .and_then(Value::as_str)
                    .unwrap_or(&current_session);
                let session_path = crate::artifacts::session_dir(root, session)?;
                let file = args
                    .get("file")
                    .and_then(Value::as_str)
                    .unwrap_or(crate::artifacts::DUMP_FILE);
                if !artifact_file_ok(file) {
                    return msg("file must be dumps.ndjson or tool-output/<name>");
                }
                let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0);
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(32 * 1024)
                    .clamp(1, crate::artifacts::MAX_READ_BYTES as u64)
                    as usize;
                let path = session_path.join(file);
                if !path.is_file() {
                    return msg(format!("artifact file not found: {file}"));
                }
                let mut value =
                    crate::artifacts::read_range(&path, offset, limit)?;
                value["session"] = json!(session);
                value["file"] = json!(file);
                Ok(value)
            }
            other => msg(format!("unknown artifact tool {other}")),
        }
    }

    /// Returns the MCP `tools/call` result object and the audit metadata.
    fn call(&mut self, name: &str, arguments: Value) -> Result<(Value, usize, bool)> {
        // Long-lived gateway: a token revoked or expired after start must not
        // keep working. Re-check the raw token on every tool call.
        if let Err(e) = crate::identity::validate(&self.home, &self.identity.raw) {
            crate::commands::audit(
                &self.home,
                &self.app.app_id,
                json!({
                    "ts": crate::artifacts::now_secs(),
                    "app_id": self.app.app_id,
                    "app_version": self.app.version,
                    "source": "agent",
                    "action": "tool_denied",
                    "token_id": self.identity.token_id,
                    "reason": e.to_string()
                }),
            );
            return Err(e);
        }
        // Allowlist/CRL may change while the gateway is running; re-check on
        // every call instead of trusting the decision made at startup.
        let policy = crate::config::policy_for(&self.home);
        if policy.enforce_allowlist() {
            let fresh = self
                .allow_checked
                .as_ref()
                .map(|(checked, version)| {
                    version == &self.app.version
                        && checked.elapsed() < Duration::from_secs(5)
                })
                .unwrap_or(false);
            if !fresh {
                let trusted = registry::load_trusted_keys(&self.home);
                let allow = crate::allowlist::guard(&self.home, &trusted);
                if let Err(e) = allow.decision(&self.app.app_id, &self.app.version) {
                    crate::commands::audit(
                        &self.home,
                        &self.app.app_id,
                        json!({
                            "ts": crate::artifacts::now_secs(),
                            "app_id": self.app.app_id,
                            "app_version": self.app.version,
                            "source": "agent",
                            "action": "tool_denied",
                            "token_id": self.identity.token_id,
                            "reason": e.to_string()
                        }),
                    );
                    return Err(e);
                }
                self.allow_checked = Some((Instant::now(), self.app.version.clone()));
            }
        }
        self.ensure_tools();
        let route = match self.routes.get(name).cloned() {
            Some(route) => route,
            None => {
                return msg(format!(
                    "tool {name} is not allowed for {}",
                    self.app.app_id
                ))
            }
        };

        self.call_seq = self.call_seq.saturating_add(1);
        let request_id = format!("{}-{}", self.identity.token_id, self.call_seq);
        let started = Instant::now();
        let raw = match &route {
            Route::Files => file_call(&self.workspace, name, &arguments),
            Route::Artifacts => self.artifact_call(name, &arguments),
            Route::Skill => skill_call(&self.package_dir, &arguments),
            Route::Remote { server, tool } => {
                self.remote_call(server, tool, arguments.clone(), &request_id)
            }
            Route::Package {
                tool,
                bin,
                argv,
                io,
                secrets,
                timeout_secs,
            } => self.package_call(tool, bin, argv, io, secrets, *timeout_secs, &request_id, &arguments),
        };

        let (value, bytes, truncated) = match raw {
            Ok(value) => wrap_result(value),
            Err(e) => {
                let problem = e.to_string();
                self.audit(
                    &route,
                    name,
                    &request_id,
                    0,
                    false,
                    started.elapsed().as_millis() as u64,
                    true,
                    Some(&problem),
                );
                return Err(e);
            }
        };
        self.audit(
            &route,
            name,
            &request_id,
            bytes,
            truncated,
            started.elapsed().as_millis() as u64,
            false,
            None,
        );
        self.dump_result(&route, name, &request_id, &value, bytes, truncated);
        Ok((value, bytes, truncated))
    }

    /// Keeps large tool results for later grep/query. Quota overflow is not a
    /// tool failure: the model already has the capped response.
    /// Registers spooled stdout/stderr in the session dump and returns their
    /// names relative to the session directory, as accepted by `artifact_read`.
    fn record_spill(
        &self,
        tool: &str,
        request_id: &str,
        run: &BoundedRun,
    ) -> Option<(String, String)> {
        let dir = self.artifact_dir.as_ref()?;
        let stdout_path = run.stdout_path.as_ref()?;
        let stderr_path = run.stderr_path.as_ref()?;
        let rel = |path: &PathBuf| {
            format!(
                "{}/{}",
                crate::artifacts::TOOL_OUTPUT_DIR,
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default()
            )
        };
        let stdout_file = rel(stdout_path);
        let stderr_file = rel(stderr_path);
        let record = json!({
            "ts": crate::artifacts::now_secs(),
            "app_id": self.app.app_id,
            "app_version": self.app.version,
            "request_id": request_id,
            "source": "package",
            "tool": tool,
            "truncated": run.overflowed,
            "timed_out": run.timed_out,
            "exit": run.status.code(),
            "stdout_file": stdout_file,
            "stderr_file": stderr_file,
            "stdout_bytes": fs::metadata(stdout_path).map(|m| m.len()).unwrap_or(0),
            "stderr_bytes": fs::metadata(stderr_path).map(|m| m.len()).unwrap_or(0),
            "preview": String::from_utf8_lossy(&run.stdout).chars().take(500).collect::<String>()
        });
        let policy = crate::config::policy(&self.home);
        if let Err(e) = crate::artifacts::append_dump(dir, &record, &policy) {
            eprintln!("eco-platform mcp proxy: cannot write spill record: {e}");
        }
        Some((stdout_file, stderr_file))
    }

    fn dump_result(
        &self,
        route: &Route,
        name: &str,
        request_id: &str,
        value: &Value,
        bytes: usize,
        truncated: bool,
    ) {
        let Some(dir) = &self.artifact_dir else {
            return;
        };
        if bytes < DUMP_MIN_BYTES && !truncated {
            return;
        }
        let (source, tool) = match route {
            Route::Files => ("files".to_string(), name.to_string()),
            Route::Skill => ("skills".to_string(), name.to_string()),
            Route::Artifacts => ("artifacts".to_string(), name.to_string()),
            Route::Package { tool, .. } => ("package".to_string(), tool.clone()),
            Route::Remote { server, tool } => (server.clone(), tool.clone()),
        };
        let record = json!({
            "ts": crate::artifacts::now_secs(),
            "app_id": self.app.app_id,
            "app_version": self.app.version,
            "request_id": request_id,
            "source": source,
            "tool": tool,
            "bytes": bytes,
            "truncated": truncated,
            "result": value
        });
        let policy = crate::config::policy(&self.home);
        match crate::artifacts::append_dump(dir, &record, &policy) {
            Ok(true) => {}
            Ok(false) => eprintln!(
                "eco-platform mcp proxy: artifact quota exceeded, dump for {name} skipped"
            ),
            Err(e) => eprintln!("eco-platform mcp proxy: cannot write artifact dump: {e}"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn audit(
        &self,
        route: &Route,
        name: &str,
        request_id: &str,
        bytes: usize,
        truncated: bool,
        took_ms: u64,
        failed: bool,
        problem: Option<&str>,
    ) {
        let (server, tool) = match route {
            Route::Files => ("files".to_string(), name.to_string()),
            Route::Skill => ("skills".to_string(), name.to_string()),
            Route::Artifacts => ("artifacts".to_string(), name.to_string()),
            Route::Package { tool, .. } => ("package".to_string(), tool.clone()),
            Route::Remote { server, tool } => (server.clone(), tool.clone()),
        };
        crate::commands::audit(
            &self.home,
            &self.app.app_id,
            json!({
                "ts": crate::artifacts::now_secs(),
                "app_id": self.app.app_id,
                "app_version": self.app.version,
                "source": "agent",
                "action": "tool",
                "request_id": request_id,
                "token_id": self.identity.token_id,
                "agent_id": self.identity.agent_id,
                "server_id": server,
                "tool": tool,
                "bytes": bytes,
                "truncated": truncated,
                "failed": failed,
                "problem": problem.map(|value| value.chars().take(400).collect::<String>()),
                "took_ms": took_ms
            }),
        );
    }
}

struct BoundedRun {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    timed_out: bool,
    overflowed: bool,
    /// Kept only when the full output is worth reading later (overflow or a
    /// failed/timed-out run): the caller exposes them through artifact_read.
    stdout_path: Option<PathBuf>,
    stderr_path: Option<PathBuf>,
}

fn package_timeout() -> Duration {
    std::env::var("ECO_PACKAGE_TOOL_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(|secs| Duration::from_secs(secs.clamp(1, MAX_PACKAGE_TOOL_TIMEOUT_SECS)))
        .unwrap_or_else(|| Duration::from_secs(PACKAGE_TOOL_TIMEOUT_SECS))
}

/// Reads a pipe while keeping at most `cap` bytes and draining the rest so a
/// chatty child cannot deadlock on a full pipe.
fn read_capped<R: Read>(mut reader: R, cap: usize, overflow: Arc<AtomicBool>) -> (Vec<u8>, usize) {
    let mut stored = Vec::new();
    let mut total = 0usize;
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                total = total.saturating_add(n);
                if stored.len() < cap {
                    let take = (cap - stored.len()).min(n);
                    stored.extend_from_slice(&buf[..take]);
                }
                if total > cap {
                    overflow.store(true, Ordering::Relaxed);
                }
            }
            Err(_) => break,
        }
    }
    (stored, total)
}

fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut kill = Command::new("taskkill");
        kill.args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        kill.creation_flags(0x0800_0000);
        let _ = kill.status();
    }
    let _ = child.kill();
}

/// Runs a package-provided executable with a hard timeout and bounded output.
/// The caller interprets `timed_out` / `overflowed` and `status`.
/// Marks the package tool that is currently running, so `chat_abort` can kill
/// its process tree even though the proxy lives in another process. The marker
/// is removed as soon as the call returns to avoid killing an unrelated reuse
/// of the same pid later.
struct ToolPidFile(Option<PathBuf>);

impl ToolPidFile {
    fn create(path: Option<&Path>, pid: u32) -> Self {
        let Some(path) = path else {
            return Self(None);
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(path, pid.to_string()) {
            Ok(()) => Self(Some(path.to_path_buf())),
            Err(_) => Self(None),
        }
    }
}

impl Drop for ToolPidFile {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn read_capped_path(path: &Path, cap: usize) -> (Vec<u8>, u64) {
    let Ok(file) = fs::File::open(path) else {
        return (Vec::new(), 0);
    };
    let size = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let mut bytes = Vec::new();
    let _ = file.take(cap as u64).read_to_end(&mut bytes);
    (bytes, size)
}

fn run_with_limits(
    command: &mut Command,
    timeout: Duration,
    cap: usize,
    input: Option<&[u8]>,
    cancel_path: Option<&Path>,
    spool: Option<(&Path, &str)>,
) -> Result<BoundedRun> {
    // Oversized output is spooled to the artifact session instead of being
    // discarded: the model can then read the full text through artifact_read.
    let spool_files = match spool {
        Some((dir, request_id)) => {
            fs::create_dir_all(dir)?;
            let stdout_path = dir.join(format!("{request_id}-stdout.txt"));
            let stderr_path = dir.join(format!("{request_id}-stderr.txt"));
            command.stdout(Stdio::from(fs::File::create(&stdout_path)?));
            command.stderr(Stdio::from(fs::File::create(&stderr_path)?));
            Some((stdout_path, stderr_path))
        }
        None => None,
    };

    let mut child = command
        .spawn()
        .map_err(|e| Error::Msg(format!("cannot start package tool: {e}")))?;
    // Record the running tool so the platform can kill it when the user presses
    // Stop: the proxy is a separate process from the UI command handler.
    let _cancel_file = ToolPidFile::create(cancel_path, child.id());
    let overflow = Arc::new(AtomicBool::new(false));

    let stdin_handle = match (input, child.stdin.take()) {
        (Some(data), Some(mut stdin)) => {
            let data = data.to_vec();
            Some(std::thread::spawn(move || {
                let _ = stdin.write_all(&data);
                let _ = stdin.flush();
            }))
        }
        _ => None,
    };

    let stdout_handle = child.stdout.take().map(|stdout| {
        let flag = overflow.clone();
        std::thread::spawn(move || read_capped(stdout, cap, flag))
    });
    let stderr_handle = child.stderr.take().map(|stderr| {
        let flag = overflow.clone();
        std::thread::spawn(move || read_capped(stderr, cap, flag))
    });

    let timeout = timeout.max(Duration::from_millis(100));
    let deadline = Instant::now() + timeout;
    let (status, timed_out, mut overflowed) = loop {
        match child.try_wait()? {
            Some(status) => break (status, false, overflow.load(Ordering::Relaxed)),
            None => {
                let spooled_too_large = spool_files.as_ref().map_or(false, |(out, err)| {
                    fs::metadata(out).map(|m| m.len() > cap as u64).unwrap_or(false)
                        || fs::metadata(err)
                            .map(|m| m.len() > cap as u64)
                            .unwrap_or(false)
                });
                if overflow.load(Ordering::Relaxed) || spooled_too_large {
                    kill_tree(&mut child);
                    let status = child.wait()?;
                    break (status, false, true);
                }
                if Instant::now() >= deadline {
                    kill_tree(&mut child);
                    let status = child.wait()?;
                    break (status, true, false);
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    };

    if let Some(handle) = stdin_handle {
        let _ = handle.join();
    }

    let (stdout, stderr, stdout_path, stderr_path) = if let Some((out_path, err_path)) = spool_files
    {
        let (stdout, out_size) = read_capped_path(&out_path, cap);
        let (stderr, err_size) = read_capped_path(&err_path, cap);
        overflowed = overflowed || out_size > cap as u64 || err_size > cap as u64;
        let keep = overflowed || timed_out || !status.success();
        if keep {
            (stdout, stderr, Some(out_path), Some(err_path))
        } else {
            let _ = fs::remove_file(&out_path);
            let _ = fs::remove_file(&err_path);
            (stdout, stderr, None, None)
        }
    } else {
        let (stdout, _) = stdout_handle
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        let (stderr, _) = stderr_handle
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        (stdout, stderr, None, None)
    };

    Ok(BoundedRun {
        status,
        stdout,
        stderr,
        timed_out,
        overflowed,
        stdout_path,
        stderr_path,
    })
}

fn text_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Msg(format!("missing string argument {key}")))
}

fn file_specs() -> Vec<Value> {
    vec![
        json!({
            "name": "list_files",
            "description": "РЎРїРёСЃРѕРє С„Р°Р№Р»РѕРІ Рё РїР°РїРѕРє РІ СЂР°Р±РѕС‡РµР№ РїР°РїРєРµ. path вЂ” РѕС‚РЅРѕСЃРёС‚РµР»СЊРЅС‹Р№ РїСѓС‚СЊ, РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ РєРѕСЂРµРЅСЊ.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "read_file",
            "description": "РџСЂРѕС‡РёС‚Р°С‚СЊ С‚РµРєСЃС‚РѕРІС‹Р№ С„Р°Р№Р» РёР· СЂР°Р±РѕС‡РµР№ РїР°РїРєРё. РђР±СЃРѕР»СЋС‚РЅС‹Рµ РїСѓС‚Рё Рё РІС‹С…РѕРґ Р·Р° РїР°РїРєСѓ Р·Р°РїСЂРµС‰РµРЅС‹.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "write_file",
            "description": "РЎРѕР·РґР°С‚СЊ РёР»Рё РїРµСЂРµР·Р°РїРёСЃР°С‚СЊ С‚РµРєСЃС‚РѕРІС‹Р№ С„Р°Р№Р» РІ СЂР°Р±РѕС‡РµР№ РїР°РїРєРµ. РџСѓС‚СЊ РѕС‚РЅРѕСЃРёС‚РµР»СЊРЅС‹Р№.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "append_file",
            "description": "Р”РѕРїРёСЃР°С‚СЊ С‚РµРєСЃС‚ РІ РєРѕРЅРµС† С„Р°Р№Р»Р° РІ СЂР°Р±РѕС‡РµР№ РїР°РїРєРµ; СЃРѕР·РґР°С‘С‚ С„Р°Р№Р», РµСЃР»Рё РµРіРѕ РЅРµС‚.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }
        }),
    ]
}

fn artifact_specs() -> Vec<Value> {
    vec![
        json!({
            "name": "artifact_list",
            "description": "Список артефакт-дампов сессий приложения (NDJSON с крупными результатами инструментов и полным выводом).",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "artifact_search",
            "description": "Поиск по содержимому дампов артефактов (без учёта регистра). Возвращает сессию, файл, строку и фрагмент. Используйте, когда результат инструмента был усечён или нужно найти прошлый вывод.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "session": { "type": "string", "description": "Ограничить одной сессией из artifact_list" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                },
                "required": ["query"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "artifact_read",
            "description": "Чтение дампа или сохранённого полного вывода инструмента по диапазону байт. Возвращает text, next_offset, size, eof — продолжайте с next_offset, пока eof не станет true.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": { "type": "string", "description": "Сессия из artifact_list; по умолчанию текущая" },
                    "file": { "type": "string", "description": "dumps.ndjson или tool-output/<имя>.txt; по умолчанию dumps.ndjson" },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 131072 }
                },
                "additionalProperties": false
            }
        }),
    ]
}

fn package_spec(tool: &ToolDef) -> Value {
    let name = normalize_tool_name(&tool.tool);
    if tool.io.eq_ignore_ascii_case("json") {
        let schema = tool
            .parameters
            .clone()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({ "type": "object", "additionalProperties": true }));
        return json!({
            "name": name,
            "description": format!(
                "РРЅСЃС‚СЂСѓРјРµРЅС‚ РїСЂРёР»РѕР¶РµРЅРёСЏ {}. РђСЂРіСѓРјРµРЅС‚С‹ РїРµСЂРµРґР°СЋС‚СЃСЏ JSON-РѕР±СЉРµРєС‚РѕРј РїРѕ СЃС…РµРјРµ РёРЅСЃС‚СЂСѓРјРµРЅС‚Р°; СЃРµРєСЂРµС‚С‹ Рё СЃРµС‚СЊ РЅР°СЃС‚СЂР°РёРІР°СЋС‚СЃСЏ РїР»Р°С‚С„РѕСЂРјРѕР№.",
                tool.tool
            ),
            "inputSchema": schema
        });
    }
    if tool.tool == "docx_export" {
        return json!({
            "name": name,
            "description": "РљРѕРЅРІРµСЂС‚РёСЂРѕРІР°С‚СЊ markdown-С„Р°Р№Р» РёР· СЂР°Р±РѕС‡РµР№ РїР°РїРєРё РІ Word .docx. РџСѓС‚Рё input Рё output РѕС‚РЅРѕСЃРёС‚РµР»СЊРЅС‹Рµ, РІРЅРµ СЂР°Р±РѕС‡РµР№ РїР°РїРєРё Р·Р°РїСЂРµС‰РµРЅС‹.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "input": { "type": "string" },
                    "output": { "type": "string" }
                },
                "required": ["input", "output"],
                "additionalProperties": false
            }
        });
    }
    json!({
        "name": name,
        "description": format!(
            "РРЅСЃС‚СЂСѓРјРµРЅС‚ РїСЂРёР»РѕР¶РµРЅРёСЏ {}. РџСѓС‚Рё input Рё output РѕС‚РЅРѕСЃРёС‚РµР»СЊРЅС‹Рµ Рё РЅРµ РјРѕРіСѓС‚ РІС‹С…РѕРґРёС‚СЊ Р·Р° СЂР°Р±РѕС‡СѓСЋ РїР°РїРєСѓ.",
            tool.tool
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "input": { "type": "string" },
                "output": { "type": "string" }
            },
            "required": ["input", "output"],
            "additionalProperties": false
        }
    })
}

/// Resolves a model-supplied path against the workspace. Absolute paths,
/// parent components and symlink escapes are rejected.
fn resolve_path(workspace: &Path, raw: &str, for_write: bool) -> Result<PathBuf> {    let raw = raw.trim();
    if raw.is_empty() {
        return msg("path is empty");
    }
    let rel = Path::new(raw);
    if rel.is_absolute() {
        return msg("absolute paths are not allowed");
    }
    for component in rel.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => return msg("path must stay inside the workspace"),
        }
    }

    let root = fs::canonicalize(workspace)?;
    let candidate = root.join(rel);
    if for_write {
        if let Some(parent) = candidate.parent() {
            fs::create_dir_all(parent)?;
            let parent = fs::canonicalize(parent)?;
            if !inside(&root, &parent) {
                return msg("path must stay inside the workspace");
            }
        }
        if candidate.exists() {
            let canon = fs::canonicalize(&candidate)?;
            if !inside(&root, &canon) {
                return msg("path must stay inside the workspace");
            }
        }
        Ok(candidate)
    } else {
        let canon = fs::canonicalize(&candidate)?;
        if !inside(&root, &canon) {
            return msg("path must stay inside the workspace");
        }
        Ok(canon)
    }
}

/// Only the dump itself and flat files under `tool-output/` are readable: a
/// model must not be able to walk the engine profile through an artifact path.
fn artifact_file_ok(file: &str) -> bool {
    if file == crate::artifacts::DUMP_FILE {
        return true;
    }
    let Some(rest) = file.strip_prefix("tool-output/") else {
        return false;
    };
    !rest.is_empty()
        && !rest.contains(['/', '\\'])
        && rest
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "._-".contains(ch))
}

fn inside(root: &Path, path: &Path) -> bool {
    if path.starts_with(root) {
        return true;
    }
    #[cfg(windows)]
    {
        let root = root.to_string_lossy().to_lowercase();
        let path = path.to_string_lossy().to_lowercase();
        path == root || path.starts_with(&format!("{root}\\"))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Shared with `workspace`: opening or exporting an artifact obeys the same
/// containment rules as the agent's own file tools.
pub(crate) fn resolve_workspace_path(workspace: &Path, raw: &str) -> Result<PathBuf> {
    resolve_path(workspace, raw, false)
}

/// Resolves an executable inside the signed package.
fn package_bin(package_dir: &Path, raw: &str) -> Result<PathBuf> {
    let rel = Path::new(raw);
    if rel.is_absolute() {
        return msg("package binary path must be relative");
    }
    for component in rel.components() {
        match component {
            Component::Normal(_) => {}
            _ => return msg("package binary path escapes the package"),
        }
    }
    let root = fs::canonicalize(package_dir)?;
    let path = fs::canonicalize(root.join(rel))?;
    if !inside(&root, &path) {
        return msg("package binary path escapes the package");
    }
    if !path.is_file() {
        return msg(format!("package binary not found: {raw}"));
    }
    Ok(path)
}

fn skill_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// A short human-readable title for `skill list`: frontmatter `description:`
/// when present, otherwise the first non-heading line.
fn skill_description(text: &str) -> String {
    let mut in_frontmatter = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            in_frontmatter = !in_frontmatter;
            continue;
        }
        if in_frontmatter {
            if let Some(rest) = trimmed.strip_prefix("description:") {
                return rest
                    .trim()
                    .trim_matches('"')
                    .chars()
                    .take(160)
                    .collect();
            }
        }
    }
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        return trimmed.chars().take(160).collect();
    }
    String::new()
}

/// Lists or reads packaged skills. Skills stay separate files, so a package
/// can ship a whole group and the model loads only the one it needs.
fn skill_call(package_dir: &Path, args: &Value) -> Result<Value> {
    let action = text_arg(args, "action")?;
    let root = fs::canonicalize(package_dir.join("skills"))
        .map_err(|_| Error::Msg("package has no skills directory".to_string()))?;

    match action {
        "list" => {
            let mut skills = Vec::new();
            for entry in fs::read_dir(&root)?.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let skill_md = path.join("SKILL.md");
                if !skill_md.is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                let text = fs::read_to_string(&skill_md).unwrap_or_default();
                skills.push(json!({
                    "name": name,
                    "description": skill_description(&text)
                }));
                if skills.len() >= MAX_LIST_ENTRIES {
                    break;
                }
            }
            skills.sort_by(|a, b| {
                a["name"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["name"].as_str().unwrap_or(""))
            });
            Ok(json!({ "skills": skills, "root": "skills" }))
        }
        "read" => {
            let name = text_arg(args, "name")?;
            if !skill_name_ok(name) {
                return msg("invalid skill name: use letters, digits, '-', '_' or '.'");
            }
            let path = fs::canonicalize(root.join(name).join("SKILL.md"))?;
            if !inside(&root, &path) {
                return msg("skill path escapes the package");
            }
            if !path.is_file() {
                return msg(format!("skill '{name}' not found"));
            }
            let (bytes, total) = read_capped_file(&path)?;
            let truncated = bytes.len() > MAX_READ_BYTES;
            let bytes = if truncated {
                &bytes[..MAX_READ_BYTES]
            } else {
                &bytes[..]
            };
            let text = String::from_utf8(bytes.to_vec())
                .map_err(|_| Error::Msg("skill is not UTF-8 text".to_string()))?;
            Ok(json!({
                "name": name,
                "text": text,
                "truncated": truncated,
                "bytes": total
            }))
        }
        _ => msg("skill action must be 'list' or 'read'"),
    }
}

fn file_call(workspace: &Path, name: &str, args: &Value) -> Result<Value> {
    match name {
        "list_files" => {
            let raw = args
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or(".");
            let dir = resolve_path(workspace, raw, false)?;
            if !dir.is_dir() {
                return msg(format!("not a directory: {}", dir.display()));
            }
            let mut entries = Vec::new();
            for entry in fs::read_dir(&dir)?.flatten().take(MAX_LIST_ENTRIES) {
                let file_type = entry.file_type()?;
                let kind = if file_type.is_dir() {
                    "dir"
                } else if file_type.is_file() {
                    "file"
                } else {
                    "other"
                };
                entries.push(json!({
                    "name": entry.file_name().to_string_lossy(),
                    "type": kind,
                    "bytes": entry.metadata().map(|m| m.len()).unwrap_or(0)
                }));
            }
            entries.sort_by(|a, b| {
                a["name"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(b["name"].as_str().unwrap_or(""))
            });
            Ok(json!({ "path": raw, "entries": entries }))
        }
        "read_file" => {
            let raw = text_arg(args, "path")?;
            let path = resolve_path(workspace, raw, false)?;
            if !path.is_file() {
                return msg(format!("not a file: {}", path.display()));
            }
            let (bytes, total) = read_capped_file(&path)?;
            let truncated = bytes.len() > MAX_READ_BYTES;
            let bytes = if truncated {
                &bytes[..MAX_READ_BYTES]
            } else {
                &bytes[..]
            };
            let text = String::from_utf8(bytes.to_vec())
                .map_err(|_| Error::Msg("file is not UTF-8 text".to_string()))?;
            Ok(json!({
                "path": raw,
                "text": text,
                "truncated": truncated,
                "bytes": total
            }))
        }
        "write_file" => {
            let raw = text_arg(args, "path")?;
            let content = text_arg(args, "content")?;
            if content.len() > MAX_WRITE_BYTES {
                return msg(format!(
                    "content is too large: {} bytes (limit {})",
                    content.len(),
                    MAX_WRITE_BYTES
                ));
            }
            let path = resolve_path(workspace, raw, true)?;
            fs::write(&path, content)?;
            Ok(json!({ "path": raw, "bytes": content.len() }))
        }
        "append_file" => {
            let raw = text_arg(args, "path")?;
            let content = text_arg(args, "content")?;
            if content.len() > MAX_WRITE_BYTES {
                return msg(format!(
                    "content is too large: {} bytes (limit {})",
                    content.len(),
                    MAX_WRITE_BYTES
                ));
            }
            let path = resolve_path(workspace, raw, true)?;
            use std::io::Write as _;
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            file.write_all(content.as_bytes())?;
            Ok(json!({ "path": raw, "bytes": content.len() }))
        }
        _ => msg(format!("unknown file tool {name}")),
    }
}

/// Passes a well-formed remote MCP result through unchanged, otherwise wraps
/// the capped JSON in a single text content item.
fn wrap_result(value: Value) -> (Value, usize, bool) {
    let compact_len = serde_json::to_string(&value)
        .map(|s| s.len())
        .unwrap_or(usize::MAX);
    let has_content = value.get("content").and_then(Value::as_array).is_some();
    if has_content && compact_len <= mcp::MAX_RESPONSE_BYTES {
        return (value, compact_len, false);
    }
    let is_error = value
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (text, truncated, bytes) = mcp::capped(&value);
    (
        json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }),
        bytes,
        truncated,
    )
}

fn response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn handle(proxy: &mut Proxy, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");

    match method {
        "initialize" => {
            let protocol = request
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(mcp::PROTOCOL_VERSION);
            Some(response(
                id,
                json!({
                    "protocolVersion": protocol,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": {
                        "name": "eco-platform-gateway",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "instructions": INSTRUCTIONS
                }),
            ))
        }
        "ping" => Some(response(id, json!({}))),
        "tools/list" => {
            proxy.ensure_tools();
            Some(response(id, json!({ "tools": proxy.tools })))
        }
        "tools/call" => {
            let name = request
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let arguments = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = match proxy.call(name, arguments) {
                Ok((value, _bytes, _truncated)) => value,
                Err(e) => json!({
                    "content": [{ "type": "text", "text": e.to_string() }],
                    "isError": true
                }),
            };
            Some(response(id, result))
        }
        m if m.starts_with("notifications/") => None,
        _ => {
            if request.get("id").is_some() {
                Some(error_response(id, -32601, "method not found"))
            } else {
                None
            }
        }
    }
}

/// Runs the stdio MCP gateway until the parent closes stdin. Never writes
/// anything but JSON-RPC to stdout; diagnostics go to stderr.
pub fn serve(app_id: &str, workspace: &Path, artifacts: Option<&Path>) -> i32 {
    let mut proxy = match Proxy::new(app_id, workspace, artifacts) {
        Ok(proxy) => proxy,
        Err(e) => {
            eprintln!("eco-platform mcp proxy: {e}");
            return 1;
        }
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("eco-platform mcp proxy: stdin read failed: {e}");
                break;
            }
        }
        if line.len() > MAX_LINE_BYTES {
            let _ = serde_json::to_writer(
                &mut writer,
                &error_response(Value::Null, -32600, "request line too large"),
            );
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(e) => {
                let _ = serde_json::to_writer(
                    &mut writer,
                    &error_response(Value::Null, -32700, &format!("parse error: {e}")),
                );
                let _ = writer.write_all(b"\n");
                let _ = writer.flush();
                continue;
            }
        };
        if let Some(answer) = handle(&mut proxy, &request) {
            let _ = serde_json::to_writer(&mut writer, &answer);
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_manifest() -> Manifest {
        serde_json::from_value(json!({
            "schema_version": 2,
            "app_id": "demo",
            "name": "Demo",
            "version": "0.1.0",
            "profile": { "system_prompt": "" },
            "tools": [
                { "tool": "search", "stream": "read" },
                { "tool": "dump", "stream": "write" }
            ],
            "connections": [
                { "id": "ctx", "type": "http-mcp" }
            ],
            "capabilities": {
                "file_scopes": ["workspace"],
                "external_writes": []
            }
        }))
        .unwrap()
    }

    fn temp_workspace(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eco-proxy-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn tool_names_are_normalized_and_scoped() {
        assert_eq!(
            normalize_tool_name("resolve.library/id"),
            "resolve_library_id"
        );

        let empty = std::env::temp_dir().join(format!("eco-proxy-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&empty);
        let names = gateway_tool_names(&test_manifest(), &empty);
        assert!(names.contains(&"ctx_search".to_string()));
        assert!(!names.contains(&"ctx_dump".to_string()));
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"write_file".to_string()));
        assert!(names.contains(&"artifact_read".to_string()));
        assert!(names.contains(&"artifact_search".to_string()));

        let mut app = test_manifest();
        app.capabilities.file_scopes.clear();
        let names = gateway_tool_names(&app, &empty);
        assert!(!names.contains(&"read_file".to_string()));
        // Dumps are the app's own output, so they stay available without files.
        assert!(names.contains(&"artifact_read".to_string()));
        let _ = fs::remove_dir_all(&empty);
    }

    #[test]
    fn skills_are_exposed_and_loaded_by_name() {
        let dir = std::env::temp_dir().join(format!("eco-proxy-skills-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("skills/alpha")).unwrap();
        fs::create_dir_all(dir.join("skills/beta")).unwrap();
        fs::write(
            dir.join("skills/alpha/SKILL.md"),
            "---\ndescription: Alpha skill\n---\n# Alpha\n",
        )
        .unwrap();
        fs::write(dir.join("skills/beta/SKILL.md"), "# Beta\nBeta body\n").unwrap();

        let names = gateway_tool_names(&test_manifest(), &dir);
        assert!(names.contains(&"skill".to_string()));

        let listed = skill_call(&dir, &json!({ "action": "list" })).unwrap();
        assert_eq!(listed["skills"][0]["name"], "alpha");
        assert_eq!(listed["skills"][0]["description"], "Alpha skill");
        assert_eq!(listed["skills"][1]["name"], "beta");

        let read = skill_call(&dir, &json!({ "action": "read", "name": "beta" })).unwrap();
        assert!(read["text"].as_str().unwrap().contains("Beta body"));
        assert!(skill_call(&dir, &json!({ "action": "read", "name": "../x" })).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_tools_are_exposed() {
        let mut app = test_manifest();
        app.tools.push(serde_json::from_value(json!({
            "tool": "docx_export",
            "stream": "write",
            "bin": "bin/docx-export.exe"
        }))
        .unwrap());
        let names = gateway_tool_names(&app, &std::env::temp_dir());
        assert!(names.contains(&"docx_export".to_string()));
    }

    #[test]
    fn package_tool_specs_expose_input_and_output() {
        let tool: crate::manifest::ToolDef = serde_json::from_value(json!({
            "tool": "convert",
            "stream": "write",
            "bin": "bin/convert.exe",
            "argv": ["--in", "{input}", "--out", "{output}"]
        }))
        .unwrap();
        let spec = package_spec(&tool);
        assert_eq!(spec["inputSchema"]["required"], json!(["input", "output"]));
        assert_eq!(spec["name"], "convert");
    }

    #[test]
    fn resolve_path_blocks_escapes() {
        let root = temp_workspace("resolve");
        assert!(resolve_path(&root, "docs/a.md", true).is_ok());
        assert!(resolve_path(&root, "../secret.txt", true).is_err());
        assert!(resolve_path(&root, "a/../../secret.txt", true).is_err());
        assert!(resolve_path(&root, "C:\\Windows\\win.ini", true).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_tools_read_write_append() {
        let root = temp_workspace("files");
        file_call(
            &root,
            "write_file",
            &json!({ "path": "docs/a.md", "content": "hello" }),
        )
        .unwrap();
        file_call(
            &root,
            "append_file",
            &json!({ "path": "docs/a.md", "content": " world" }),
        )
        .unwrap();
        let read = file_call(&root, "read_file", &json!({ "path": "docs/a.md" })).unwrap();
        assert_eq!(read["text"], "hello world");
        let listed = file_call(&root, "list_files", &json!({ "path": "docs" })).unwrap();
        assert_eq!(listed["entries"][0]["name"], "a.md");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn secret_env_mapping_rules() {
        assert!(env_name_ok("SERVICE_TOKEN"));
        assert!(env_name_ok("API_URL_2"));
        assert!(!env_name_ok("service_token"));
        assert!(!env_name_ok("SERVICE-TOKEN"));
        assert!(!env_name_ok(""));
        assert!(secret_name_ok("svc_token"));
        assert!(secret_name_ok("svc.token"));
        assert!(!secret_name_ok("svc token"));
        assert!(!secret_name_ok(""));
    }

    #[test]
    fn run_with_limits_feeds_stdin() {
        let mut command = Command::new("cmd");
        command
            .args(["/C", "findstr ."])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let run = run_with_limits(
            &mut command,
            Duration::from_secs(5),
            4096,
            Some(b"{\"query\":\"hello\"}\n"),
            None,
            None,
        )
        .unwrap();
        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(stdout.contains("hello"), "stdout was: {stdout}");
    }

    #[test]
    fn package_spec_uses_json_parameters() {
        let tool: ToolDef = serde_json::from_value(json!({
            "tool": "crm_lookup",
            "bin": "bin/crm.exe",
            "io": "json",
            "parameters": {
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }
        }))
        .unwrap();
        let spec = package_spec(&tool);
        assert_eq!(spec["inputSchema"]["properties"]["query"]["type"], "string");
        assert_eq!(spec["inputSchema"]["required"], json!(["query"]));

        let files: ToolDef = serde_json::from_value(json!({
            "tool": "docx_export",
            "bin": "bin/docx-export.exe"
        }))
        .unwrap();
        let spec = package_spec(&files);
        assert_eq!(spec["inputSchema"]["required"], json!(["input", "output"]));
    }

    #[test]
    fn cancel_marker_is_removed_after_the_call() {
        let dir = std::env::temp_dir().join(format!(
            "eco-proxy-cancel-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("tool.pid");
        let mut command = Command::new("cmd");
        command
            .args(["/C", "exit", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let run = run_with_limits(&mut command, Duration::from_secs(5), 1024, None, Some(&marker), None)
            .unwrap();
        assert!(run.status.success());
        assert!(!marker.exists(), "cancel marker must not outlive the tool call");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversized_output_is_spooled_for_later_reading() {
        let dir = std::env::temp_dir().join(format!("eco-spool-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut command = Command::new("cmd");
        command
            .args([
                "/C",
                "for /L %i in (1,1,4000) do @echo 0123456789012345678901234567890123456789",
            ])
            .stdin(Stdio::null());
        let run = run_with_limits(
            &mut command,
            Duration::from_secs(20),
            1024,
            None,
            None,
            Some((&dir, "req-1")),
        )
        .unwrap();
        assert!(run.overflowed, "output above the cap must be reported");
        let stdout_path = run.stdout_path.expect("spooled stdout must be kept");
        let size = fs::metadata(&stdout_path).unwrap().len();
        assert!(size > 1024, "full output must stay on disk: {size}");
        let value = crate::artifacts::read_range(&stdout_path, 0, 64).unwrap();
        assert_eq!(value["text"].as_str().unwrap().len(), 64);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_runner_enforces_timeout_and_captures_output() {
        let mut ok = Command::new("cmd");
        ok.args(["/C", "echo", "hi"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let run = run_with_limits(&mut ok, Duration::from_secs(5), 1024, None, None, None).unwrap();
        assert!(run.status.success());
        assert!(String::from_utf8_lossy(&run.stdout).contains("hi"));

        let mut slow = Command::new("cmd");
        slow.args(["/C", "ping -n 20 127.0.0.1 >nul"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let run = run_with_limits(&mut slow, Duration::from_millis(200), 1024, None, None, None).unwrap();
        assert!(run.timed_out);
    }
}
