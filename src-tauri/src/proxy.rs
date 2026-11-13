use crate::error::{msg, Error, Result};
use crate::manifest::{self, Manifest, ToolDef};
use crate::mcp::{self, McpSession};
use crate::registry;
use crate::secrets::SecretStore;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

/// Name of the single local MCP server that opencode sees. Every tool the
/// model can call is exposed by the platform gateway under this name.
pub const SERVER_NAME: &str = "platform";

const MAX_READ_BYTES: usize = 64 * 1024;
const MAX_WRITE_BYTES: usize = 1024 * 1024;
const MAX_LIST_ENTRIES: usize = 200;
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;

const FILE_TOOLS: &[&str] = &["list_files", "read_file", "write_file", "append_file"];

const INSTRUCTIONS: &str = "Все инструменты проходят через шлюз платформы: вне allow-list \
приложения вызов невозможен. Файловые инструменты работают только в рабочей папке \
приложения; абсолютные пути и выход за неё запрещены. Для Word используйте инструмент \
docx_export с относительными путями input и output.";

/// opencode normalizes tool names to letters, numbers, `_` and `-`. Do the same
/// so permission entries in the generated config match exactly.
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
    Package { tool: String, bin: String },
}

pub struct Proxy {
    home: PathBuf,
    app: Manifest,
    package_dir: PathBuf,
    workspace: PathBuf,
    servers: HashMap<String, McpSession>,
    routes: HashMap<String, Route>,
    tools: Vec<Value>,
    built: bool,
}

impl Proxy {
    pub fn new(app_id: &str, workspace: &Path) -> Result<Self> {
        let home = registry::home();
        let app = registry::verified_manifest(&home, app_id)?;
        let package_dir = registry::apps_dir(&home).join(app_id);
        if !workspace.is_dir() {
            fs::create_dir_all(workspace)?;
        }
        let workspace = fs::canonicalize(workspace)?;
        Ok(Self {
            home,
            app,
            package_dir,
            workspace,
            servers: HashMap::new(),
            routes: HashMap::new(),
            tools: Vec::new(),
            built: false,
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
            self.routes.insert(
                exposed,
                Route::Package {
                    tool: tool.tool.clone(),
                    bin,
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

    fn remote_call(&mut self, server: &str, tool: &str, arguments: Value) -> Result<Value> {
        let secrets = SecretStore::new(&self.home);
        if !self.servers.contains_key(server) {
            let def = mcp::get(&secrets, server)?;
            let session = McpSession::connect(def)?;
            self.servers.insert(server.to_string(), session);
        }
        let result = match self.servers.get(server) {
            Some(session) => session.call(tool, arguments),
            None => return msg(format!("mcp server {server} is not connected")),
        };
        if result.is_err() {
            self.servers.remove(server);
        }
        result
    }

    fn package_call(&self, tool: &str, bin: &str, arguments: &Value) -> Result<Value> {
        if tool != "docx_export" {
            return msg(format!("package tool {tool} is not supported"));
        }
        let input = text_arg(arguments, "input")?;
        let output = text_arg(arguments, "output")?;
        let input = resolve_path(&self.workspace, input, false)?;
        let output = resolve_path(&self.workspace, output, true)?;
        if !input.is_file() {
            return msg(format!("input file does not exist: {}", input.display()));
        }

        let converter = package_bin(&self.package_dir, bin)?;
        let mut command = Command::new(&converter);
        command
            .arg("--input")
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }

        let result = command
            .output()
            .map_err(|e| Error::Msg(format!("cannot run docx converter: {e}")))?;
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            let snippet: String = stderr.chars().take(400).collect();
            return msg(format!("docx converter failed: {snippet}"));
        }
        if !output.is_file() {
            return msg("docx converter finished without creating the output file");
        }
        Ok(json!({
            "input": input.display().to_string(),
            "output": output.display().to_string(),
            "bytes": fs::metadata(&output).map(|m| m.len()).unwrap_or(0)
        }))
    }

    /// Returns the MCP `tools/call` result object and the audit metadata.
    fn call(&mut self, name: &str, arguments: Value) -> Result<(Value, usize, bool)> {
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

        let started = Instant::now();
        let raw = match &route {
            Route::Files => file_call(&self.workspace, name, &arguments),
            Route::Remote { server, tool } => {
                self.remote_call(server, tool, arguments.clone())
            }
            Route::Package { tool, bin } => self.package_call(tool, bin, &arguments),
        };

        let (value, bytes, truncated) = match raw {
            Ok(value) => wrap_result(value),
            Err(e) => {
                self.audit(&route, name, 0, false, started.elapsed().as_millis() as u64, true);
                return Err(e);
            }
        };
        self.audit(
            &route,
            name,
            bytes,
            truncated,
            started.elapsed().as_millis() as u64,
            false,
        );
        Ok((value, bytes, truncated))
    }

    fn audit(
        &self,
        route: &Route,
        name: &str,
        bytes: usize,
        truncated: bool,
        took_ms: u64,
        failed: bool,
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
                "source": "agent",
                "server_id": server,
                "tool": tool,
                "bytes": bytes,
                "truncated": truncated,
                "failed": failed,
                "took_ms": took_ms
            }),
        );
    }
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
    if tool.tool == "docx_export" {
        return json!({
            "name": normalize_tool_name(&tool.tool),
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
        "name": normalize_tool_name(&tool.tool),
        "description": format!("Инструмент приложения {}.", tool.tool),
        "inputSchema": { "type": "object" }
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
pub fn serve(app_id: &str, workspace: &Path) -> i32 {
    let mut proxy = match Proxy::new(app_id, workspace) {
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
}
