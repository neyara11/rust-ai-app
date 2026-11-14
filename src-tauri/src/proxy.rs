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
const PACKAGE_TOOL_TIMEOUT_SECS: u64 = 60;
/// Results at least this large are also dumped to the artifact NDJSON.
const DUMP_MIN_BYTES: usize = 8 * 1024;

const FILE_TOOLS: &[&str] = &["list_files", "read_file", "write_file", "append_file"];

const INSTRUCTIONS: &str = "Все инструменты проходят через шлюз платформы: вне allow-list \
приложения вызов невозможен. Файловые инструменты работают только в рабочей папке \
приложения; абсолютные пути и выход за неё запрещены. Для Word используйте инструмент \
docx_export с относительными путями input и output.";

/// opencode normalizes tool names to letters, numbers, `_` and `-`. Do the same
/// so permission entries in the generated config match exactly.
fn env_name_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
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

/// Names (without the `platform_` prefix) that the gateway exposes for an app.
/// Shared by the engine config (permission entries) and the proxy.
pub fn gateway_tool_names(app: &Manifest) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let writes_allowed = manifest::external_writes_allowed(app);

    for connection in &app.connections {
        if connection.kind != "http-mcp" {
            continue;
        }
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
    Remote { server: String, tool: String },
    Package {
        tool: String,
        bin: String,
        argv: Vec<String>,
        io: String,
        parameters: Value,
        secrets: std::collections::BTreeMap<String, String>,
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

        // Platform-owned file tools are reserved first.
        if has_file_scope(&self.app) {
            for name in FILE_TOOLS {
                self.routes.insert((*name).to_string(), Route::Files);
            }
            tools.extend(file_specs());
        }

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
                || tool
                    .secrets
                    .iter()
                    .any(|(env, secret)| !env_name_ok(env) || !secret_name_ok(secret))
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
            let parameters = tool
                .parameters
                .clone()
                .filter(Value::is_object)
                .unwrap_or_else(|| json!({ "type": "object", "additionalProperties": true }));
            self.routes.insert(
                exposed,
                Route::Package {
                    tool: tool.tool.clone(),
                    bin,
                    argv: tool.argv.clone(),
                    io,
                    parameters,
                    secrets: tool.secrets.clone(),
                },
            );
            tools.push(package_spec(&tool));
        }

        let secrets = SecretStore::new(&self.home);
        let writes_allowed = manifest::external_writes_allowed(&self.app);
        for connection in self.app.connections.clone() {
            if connection.kind != "http-mcp" {
                continue;
            }
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
                            "Инструмент {} сервера {}.",
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
        let def = match mcp::get(secrets, server) {
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
            let def = mcp::get(&secrets, server)?;
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
                match store.load(secret_name)? {
                    Some(value) if !value.is_empty() => {
                        command.env(env_name, value);
                    }
                    _ => {
                        return msg(format!(
                            "package tool {tool} needs secret '{secret_name}' for {env_name}; add it in Settings → Secrets"
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

        let timeout = package_timeout();
        let run = run_with_limits(
            &mut command,
            timeout,
            MAX_PACKAGE_OUTPUT,
            stdin_payload.as_deref(),
        )?;
        if run.timed_out {
            return msg(format!(
                "package tool {tool} timed out after {}s; process killed",
                timeout.as_secs()
            ));
        }
        if run.overflowed {
            return msg(format!(
                "package tool {tool} output exceeded {} KiB per stream; process killed",
                MAX_PACKAGE_OUTPUT / 1024
            ));
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
            if snippet.is_empty() {
                return msg(format!("package tool {tool} failed (exit {code})"));
            }
            return msg(format!("package tool {tool} failed (exit {code}): {snippet}"));
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
            Route::Remote { server, tool } => {
                self.remote_call(server, tool, arguments.clone(), &request_id)
            }
            Route::Package {
                tool,
                bin,
                argv,
                io,
                secrets,
                ..
            } => self.package_call(tool, bin, argv, io, secrets, &arguments),
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
}

fn package_timeout() -> Duration {
    std::env::var("ECO_PACKAGE_TOOL_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
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
fn run_with_limits(
    command: &mut Command,
    timeout: Duration,
    cap: usize,
    input: Option<&[u8]>,
) -> Result<BoundedRun> {
    let mut child = command
        .spawn()
        .map_err(|e| Error::Msg(format!("cannot start package tool: {e}")))?;
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
    let (status, timed_out, overflowed) = loop {
        match child.try_wait()? {
            Some(status) => break (status, false, false),
            None => {
                if overflow.load(Ordering::Relaxed) {
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

    let (stdout, _bytes_out) = stdout_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let (stderr, _bytes_err) = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    Ok(BoundedRun {
        status,
        stdout,
        stderr,
        timed_out,
        overflowed,
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
            "description": "Список файлов и папок в рабочей папке. path — относительный путь, по умолчанию корень.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "read_file",
            "description": "Прочитать текстовый файл из рабочей папки. Абсолютные пути и выход за папку запрещены.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "write_file",
            "description": "Создать или перезаписать текстовый файл в рабочей папке. Путь относительный.",
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
            "description": "Дописать текст в конец файла в рабочей папке; создаёт файл, если его нет.",
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
                "Инструмент приложения {}. Аргументы передаются JSON-объектом по схеме инструмента; секреты и сеть настраиваются платформой.",
                tool.tool
            ),
            "inputSchema": schema
        });
    }
    if tool.tool == "docx_export" {
        return json!({
            "name": name,
            "description": "Конвертировать markdown-файл из рабочей папки в Word .docx. Пути input и output относительные, вне рабочей папки запрещены.",
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
            "Инструмент приложения {}. Пути input и output относительные и не могут выходить за рабочую папку.",
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
fn resolve_path(workspace: &Path, raw: &str, for_write: bool) -> Result<PathBuf> {
    let raw = raw.trim();
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
            let bytes = fs::read(&path)?;
            let total = bytes.len();
            let bytes = if total > MAX_READ_BYTES {
                &bytes[..MAX_READ_BYTES]
            } else {
                &bytes[..]
            };
            let text = String::from_utf8(bytes.to_vec())
                .map_err(|_| Error::Msg("file is not UTF-8 text".to_string()))?;
            Ok(json!({
                "path": raw,
                "text": text,
                "truncated": total > MAX_READ_BYTES,
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

        let names = gateway_tool_names(&test_manifest());
        assert!(names.contains(&"ctx_search".to_string()));
        assert!(!names.contains(&"ctx_dump".to_string()));
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"write_file".to_string()));

        let mut app = test_manifest();
        app.capabilities.file_scopes.clear();
        let names = gateway_tool_names(&app);
        assert!(!names.contains(&"read_file".to_string()));
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
        let names = gateway_tool_names(&app);
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
    fn package_runner_enforces_timeout_and_captures_output() {
        let mut ok = Command::new("cmd");
        ok.args(["/C", "echo", "hi"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let run = run_with_limits(&mut ok, Duration::from_secs(5), 1024, None).unwrap();
        assert!(run.status.success());
        assert!(String::from_utf8_lossy(&run.stdout).contains("hi"));

        let mut slow = Command::new("cmd");
        slow.args(["/C", "ping -n 20 127.0.0.1 >nul"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let run = run_with_limits(&mut slow, Duration::from_millis(200), 1024, None).unwrap();
        assert!(run.timed_out);
    }
}
