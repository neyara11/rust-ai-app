use crate::docs::DocStore;
use crate::error::{msg, Error, Result};
use crate::manifest::{self, DocDef, Manifest};
use crate::mcp::{self, McpSession};
use crate::registry;
use crate::secrets::SecretStore;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Name of the single local MCP server that opencode sees. Every tool the
/// model can call is exposed by the platform gateway under this name.
pub const SERVER_NAME: &str = "platform";

const MAX_DOC_BYTES: usize = 256 * 1024;
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;

const DOC_TOOLS: &[&str] = &[
    "doc_list",
    "doc_read",
    "doc_create",
    "doc_append",
    "doc_insert_after",
    "doc_replace_block",
    "doc_finalize",
];

const INSTRUCTIONS: &str = "Все инструменты проходят через шлюз платформы: вне allow-list \
приложения вызов невозможен. Файловых путей нет. Документы адресуются только id из \
манифеста. Перед изменением документа прочитайте его через doc_read и передавайте \
полученную ревизию в expected_revision: вызов с устаревшей ревизией будет отклонён.";

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

/// Names (without the `platform_` prefix) that the gateway exposes for an app.
/// This is the single source of truth shared by the engine config (permission
/// entries) and the proxy (tools/list). It is pure and never touches the
/// network, so config generation stays fast.
pub fn gateway_tool_names(app: &Manifest) -> Vec<String> {
    let mut out = Vec::new();
    let writes_allowed = manifest::external_writes_allowed(app);

    for connection in &app.connections {
        if connection.kind != "http-mcp" {
            continue;
        }
        for tool in &app.tools {
            if tool.stream == "write" && !writes_allowed {
                continue;
            }
            out.push(normalize_tool_name(&format!(
                "{}_{}",
                connection.id, tool.tool
            )));
        }
    }

    if !app.docs.is_empty() {
        for name in DOC_TOOLS {
            out.push((*name).to_string());
        }
    }

    out.sort();
    out.dedup();
    out
}

struct Route {
    /// Empty for platform-owned document tools.
    server: String,
    tool: String,
}

pub struct Proxy {
    home: PathBuf,
    app: Manifest,
    docs: DocStore,
    servers: HashMap<String, McpSession>,
    routes: HashMap<String, Route>,
    tools: Vec<Value>,
    built: bool,
}

impl Proxy {
    pub fn new(app_id: &str, session_dir: &Path) -> Result<Self> {
        let home = registry::home();
        let app = registry::verified_manifest(&home, app_id)?;
        let docs = DocStore::new(session_dir.to_path_buf())?;
        Ok(Self {
            home,
            app,
            docs,
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

        let secrets = SecretStore::new(&self.home);
        let writes_allowed = manifest::external_writes_allowed(&self.app);
        let mut tools = Vec::new();

        // Platform document tools are reserved first: a remote tool must never
        // shadow doc_read/doc_append and friends.
        if !self.app.docs.is_empty() {
            for name in DOC_TOOLS {
                self.routes.insert(
                    (*name).to_string(),
                    Route {
                        server: String::new(),
                        tool: (*name).to_string(),
                    },
                );
            }
            tools.extend(doc_specs(&self.app.docs));
        }

        for connection in self.app.connections.clone() {
            if connection.kind != "http-mcp" {
                continue;
            }
            let schemas = self.load_schemas(&secrets, &connection.id);
            for tool in self.app.tools.clone() {
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
                    Route {
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
            // Drop the cached session so a transient transport failure does not
            // poison every later call.
            self.servers.remove(server);
        }
        result
    }

    /// Returns the MCP `tools/call` result object and the audit metadata.
    fn call(&mut self, name: &str, arguments: Value) -> Result<(Value, usize, bool)> {
        self.ensure_tools();
        let route = match self.routes.get(name) {
            Some(route) => Route {
                server: route.server.clone(),
                tool: route.tool.clone(),
            },
            None => {
                return msg(format!(
                    "tool {name} is not allowed for {}",
                    self.app.app_id
                ))
            }
        };

        let started = Instant::now();
        let raw = if route.server.is_empty() {
            doc_call(&self.docs, &self.app.docs, &route.tool, &arguments)
        } else {
            self.remote_call(&route.server, &route.tool, arguments)
        };

        let (value, bytes, truncated) = match raw {
            Ok(value) => wrap_result(value),
            Err(e) => {
                self.audit(&route, 0, false, started.elapsed().as_millis() as u64, true);
                return Err(e);
            }
        };
        self.audit(
            &route,
            bytes,
            truncated,
            started.elapsed().as_millis() as u64,
            false,
        );
        Ok((value, bytes, truncated))
    }

    fn audit(&self, route: &Route, bytes: usize, truncated: bool, took_ms: u64, failed: bool) {
        let (server, tool) = if route.server.is_empty() {
            ("docs".to_string(), route.tool.clone())
        } else {
            (route.server.clone(), route.tool.clone())
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

fn doc_specs(docs: &[DocDef]) -> Vec<Value> {
    let ids = docs
        .iter()
        .map(|d| d.id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    vec![
        json!({
            "name": "doc_list",
            "description": format!("Список документов сессии с ревизиями. Допустимые id: {ids}."),
            "inputSchema": schema(json!({}), &[])
        }),
        json!({
            "name": "doc_read",
            "description": "Прочитать документ: markdown и текущая ревизия. Всегда читайте перед изменением.",
            "inputSchema": schema(json!({ "id": string_prop() }), &["id"])
        }),
        json!({
            "name": "doc_create",
            "description": "Создать документ с начальным markdown. Пустой документ, созданный платформой, будет заполнен.",
            "inputSchema": schema(json!({ "id": string_prop(), "markdown": string_prop() }), &["id", "markdown"])
        }),
        json!({
            "name": "doc_append",
            "description": "Дописать markdown в конец документа. Передайте expected_revision из doc_read, чтобы не потерять ручные правки.",
            "inputSchema": schema(json!({
                "id": string_prop(),
                "markdown": string_prop(),
                "expected_revision": string_prop()
            }), &["id", "markdown"])
        }),
        json!({
            "name": "doc_insert_after",
            "description": "Вставить markdown после якоря <!-- id: anchor -->, до следующего якоря.",
            "inputSchema": schema(json!({
                "id": string_prop(),
                "anchor": string_prop(),
                "markdown": string_prop(),
                "expected_revision": string_prop()
            }), &["id", "anchor", "markdown"])
        }),
        json!({
            "name": "doc_replace_block",
            "description": "Заменить блок между якорем <!-- id: anchor --> и следующим якорем.",
            "inputSchema": schema(json!({
                "id": string_prop(),
                "anchor": string_prop(),
                "markdown": string_prop(),
                "expected_revision": string_prop()
            }), &["id", "anchor", "markdown"])
        }),
        json!({
            "name": "doc_finalize",
            "description": "Пометить документ готовым. Пользователь сам решит, куда его сохранить.",
            "inputSchema": schema(json!({ "id": string_prop() }), &["id"])
        }),
    ]
}

fn string_prop() -> Value {
    json!({ "type": "string" })
}

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn check_doc_size(markdown: &str) -> Result<()> {
    if markdown.len() > MAX_DOC_BYTES {
        return msg(format!(
            "document fragment is too large: {} bytes (limit {})",
            markdown.len(),
            MAX_DOC_BYTES
        ));
    }
    Ok(())
}

fn check_declared(declared: &[DocDef], id: &str) -> Result<()> {
    manifest::parse_id(id, "document")?;
    if declared.iter().any(|d| d.id == id) {
        Ok(())
    } else {
        msg(format!("document {id} is not declared by this app"))
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

/// Document results are returned as a text item with an explicit field order:
/// the revision must survive the 64 KiB response cap, which may cut the
/// markdown tail.
fn doc_content(view: crate::docs::DocView) -> Value {
    let text = format!(
        "{{\"id\":{},\"revision\":{},\"finalized\":{},\"markdown\":{}}}",
        json_string(&view.id),
        json_string(&view.revision),
        view.finalized,
        json_string(&view.markdown)
    );
    json!({ "content": [{ "type": "text", "text": text }] })
}

/// Dispatches the platform-owned document tools. Kept free of `Proxy` state so
/// it can be unit-tested without a package registry.
pub fn doc_call(docs: &DocStore, declared: &[DocDef], name: &str, args: &Value) -> Result<Value> {
    let text = |key: &str| -> Result<&str> {
        args.get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Msg(format!("missing string argument {key}")))
    };
    let revision = || args.get("expected_revision").and_then(Value::as_str);

    match name {
        "doc_list" => Ok(serde_json::to_value(docs.list()?)?),
        "doc_read" => {
            let id = text("id")?;
            check_declared(declared, id)?;
            Ok(doc_content(docs.read(id)?))
        }
        "doc_create" => {
            let id = text("id")?;
            check_declared(declared, id)?;
            let markdown = text("markdown")?;
            check_doc_size(markdown)?;
            // The UI pre-creates declared documents as empty files, so create
            // must also initialize an empty document instead of failing.
            let view = match docs.create(id, markdown) {
                Ok(view) => view,
                Err(original) => {
                    let current = match docs.read(id) {
                        Ok(current) => current,
                        Err(_) => return Err(original),
                    };
                    if !current.markdown.trim().is_empty() {
                        return Err(original);
                    }
                    docs.write(id, markdown, Some(&current.revision))?
                }
            };
            Ok(doc_content(view))
        }
        "doc_append" => {
            let id = text("id")?;
            check_declared(declared, id)?;
            let markdown = text("markdown")?;
            check_doc_size(markdown)?;
            Ok(doc_content(docs.append(id, markdown, revision())?))
        }
        "doc_insert_after" => {
            let id = text("id")?;
            check_declared(declared, id)?;
            let anchor = text("anchor")?;
            let markdown = text("markdown")?;
            check_doc_size(markdown)?;
            Ok(doc_content(docs.insert_after(
                id,
                anchor,
                markdown,
                revision(),
            )?))
        }
        "doc_replace_block" => {
            let id = text("id")?;
            check_declared(declared, id)?;
            let anchor = text("anchor")?;
            let markdown = text("markdown")?;
            check_doc_size(markdown)?;
            Ok(doc_content(docs.replace_block(
                id,
                anchor,
                markdown,
                revision(),
            )?))
        }
        "doc_finalize" => {
            let id = text("id")?;
            check_declared(declared, id)?;
            Ok(doc_content(docs.finalize(id)?))
        }
        _ => msg(format!("unknown document tool {name}")),
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
pub fn serve(app_id: &str, session_dir: &Path) -> i32 {
    let mut proxy = match Proxy::new(app_id, session_dir) {
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
            "docs": [
                { "id": "report", "label": "Report" }
            ],
            "capabilities": { "external_writes": [] }
        }))
        .unwrap()
    }

    #[test]
    fn tool_names_are_normalized_and_write_gated() {
        assert_eq!(
            normalize_tool_name("resolve.library/id"),
            "resolve_library_id"
        );

        let names = gateway_tool_names(&test_manifest());
        assert!(names.contains(&"ctx_search".to_string()));
        assert!(!names.contains(&"ctx_dump".to_string()));
        assert!(names.contains(&"doc_read".to_string()));
    }

    #[test]
    fn write_tools_need_external_write_capability() {
        let mut app = test_manifest();
        app.capabilities.external_writes = vec!["sentry".to_string()];
        let names = gateway_tool_names(&app);
        assert!(names.contains(&"ctx_dump".to_string()));
    }

    #[test]
    fn doc_tools_are_absent_without_declared_documents() {
        let mut app = test_manifest();
        app.docs.clear();
        let names = gateway_tool_names(&app);
        assert!(!names.iter().any(|n| n.starts_with("doc_")));
    }

    fn doc_json(value: &Value) -> Value {
        let text = value["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn doc_tools_enforce_declared_ids_and_revisions() {
        let dir = std::env::temp_dir().join(format!("eco-proxy-docs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let docs = DocStore::new(dir.clone()).unwrap();
        let declared = vec![DocDef {
            id: "report".to_string(),
            label: "Report".to_string(),
            export: vec![],
        }];

        assert!(doc_call(
            &docs,
            &declared,
            "doc_create",
            &json!({ "id": "other", "markdown": "x" })
        )
        .is_err());

        let created = doc_json(
            &doc_call(
                &docs,
                &declared,
                "doc_create",
                &json!({ "id": "report", "markdown": "<!-- id: sym -->\nhello\n" }),
            )
            .unwrap(),
        );
        let revision = created["revision"].as_str().unwrap().to_string();

        let appended = doc_json(
            &doc_call(
                &docs,
                &declared,
                "doc_append",
                &json!({ "id": "report", "markdown": "more", "expected_revision": revision }),
            )
            .unwrap(),
        );
        assert!(appended["markdown"].as_str().unwrap().contains("more"));

        assert!(doc_call(
            &docs,
            &declared,
            "doc_append",
            &json!({ "id": "report", "markdown": "stale", "expected_revision": revision })
        )
        .is_err());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn doc_create_fills_empty_platform_document() {
        let dir = std::env::temp_dir().join(format!("eco-proxy-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let docs = DocStore::new(dir.clone()).unwrap();
        let declared = vec![DocDef {
            id: "report".to_string(),
            label: "Report".to_string(),
            export: vec![],
        }];

        // The UI creates declared documents as empty files on app open.
        docs.create("report", "").unwrap();

        let filled = doc_json(
            &doc_call(
                &docs,
                &declared,
                "doc_create",
                &json!({ "id": "report", "markdown": "# Отчёт\n" }),
            )
            .unwrap(),
        );
        assert!(filled["markdown"].as_str().unwrap().contains("# Отчёт"));
        assert!(!filled["revision"].as_str().unwrap().is_empty());

        // A non-empty document is not overwritten.
        assert!(doc_call(
            &docs,
            &declared,
            "doc_create",
            &json!({ "id": "report", "markdown": "other" })
        )
        .is_err());

        let _ = std::fs::remove_dir_all(dir);
    }
}
