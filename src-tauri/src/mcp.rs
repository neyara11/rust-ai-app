use crate::error::{msg, Error, Result};
use crate::secrets::SecretStore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

pub const PROTOCOL_VERSION: &str = "2024-11-05";
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// Remote MCP server definition. The URL itself can contain a secret path, so
/// the whole definition is stored in DPAPI, not in config.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerDef {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub token: Option<String>,
    /// Closed-contour escape hatch: an internal MCP server may use a private
    /// CA or a self-signed certificate. Off by default and audited in the UI.
    #[serde(default)]
    pub allow_invalid_certs: bool,
}

fn key(id: &str) -> String {
    format!("mcp.{id}")
}

fn scoped_key(app_id: &str, id: &str) -> String {
    format!("app.{app_id}.mcp.{id}")
}

fn validate_id(id: &str) -> Result<()> {
    crate::manifest::parse_id(id, "mcp server")
}

fn validate_app_id(app_id: &str) -> Result<()> {
    crate::manifest::parse_id(app_id, "app")
}

pub fn list(secrets: &SecretStore) -> Result<Vec<String>> {
    let mut out: Vec<String> = secrets
        .list()?
        .into_iter()
        .filter_map(|n| n.strip_prefix("mcp.").map(str::to_string))
        .collect();
    out.sort();
    Ok(out)
}

pub fn get(secrets: &SecretStore, id: &str) -> Result<ServerDef> {
    validate_id(id)?;
    let raw = secrets
        .load(&key(id))?
        .ok_or_else(|| Error::Msg(format!("mcp server {id} is not configured")))?;
    serde_json::from_str(&raw).map_err(Error::Json)
}

/// App-scoped definition first, then the global registry. This is what one
/// application uses: the same MCP id can have different URLs, tokens and TLS
/// policy in different apps.
pub fn get_for_app(secrets: &SecretStore, app_id: &str, id: &str) -> Result<ServerDef> {
    validate_app_id(app_id)?;
    validate_id(id)?;
    if let Some(raw) = secrets.load(&scoped_key(app_id, id))? {
        return serde_json::from_str(&raw).map_err(Error::Json);
    }
    get(secrets, id)
}

/// Only the app-scoped definition, if the user created an override.
pub fn get_scoped(secrets: &SecretStore, app_id: &str, id: &str) -> Result<Option<ServerDef>> {
    validate_app_id(app_id)?;
    validate_id(id)?;
    match secrets.load(&scoped_key(app_id, id))? {
        Some(raw) => Ok(Some(serde_json::from_str(&raw).map_err(Error::Json)?)),
        None => Ok(None),
    }
}

pub fn put_for_app(secrets: &SecretStore, app_id: &str, def: &ServerDef) -> Result<()> {
    validate_app_id(app_id)?;
    put_to(secrets, &scoped_key(app_id, &def.id), def)
}

pub fn delete_for_app(secrets: &SecretStore, app_id: &str, id: &str) -> Result<()> {
    validate_app_id(app_id)?;
    validate_id(id)?;
    secrets.delete(&scoped_key(app_id, id))
}

/// Remote MCP endpoints must use TLS: the bearer token and every tool call
/// payload travel to this URL. Plain HTTP is allowed only for loopback.
fn validate_url(raw: &str) -> Result<()> {
    let trimmed = raw.trim();
    if trimmed.to_ascii_lowercase().starts_with("https://") {
        return Ok(());
    }
    let Some(rest) = trimmed.strip_prefix("http://") else {
        return msg("mcp server url must use https:// (http:// is allowed only for loopback)");
    };
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    // Userinfo before '@' must not be able to masquerade as the host.
    if authority.contains('@') {
        return msg("mcp server url must not contain userinfo");
    }
    let host = if let Some(after) = authority.strip_prefix('[') {
        after.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "::1";
    if loopback {
        Ok(())
    } else {
        msg("mcp server url must use https:// (http:// is allowed only for loopback)")
    }
}

fn put_to(secrets: &SecretStore, key: &str, def: &ServerDef) -> Result<()> {
    validate_id(&def.id)?;
    validate_url(&def.url)?;
    let raw = serde_json::to_string(def)?;
    secrets.store(key, &raw)
}

pub fn put(secrets: &SecretStore, def: &ServerDef) -> Result<()> {
    put_to(secrets, &key(&def.id), def)
}

pub fn delete(secrets: &SecretStore, id: &str) -> Result<()> {
    validate_id(id)?;
    secrets.delete(&key(id))
}

/// Minimal MCP client over streamable HTTP. Blocking on purpose: calls are
/// short, bounded, and the tool host must be able to cut them off.
pub struct McpSession {
    agent: ureq::Agent,
    def: ServerDef,
    request_id: AtomicU64,
    session_id: Mutex<Option<String>>,
    pub protocol: String,
    pub server: Value,
}

impl McpSession {
    pub fn connect(def: ServerDef) -> Result<Self> {
        validate_url(&def.url)?;
        // Native TLS (Schannel on Windows) instead of rustls: corporate TLS
        // inspection in a closed network is handled by the system store.
        let mut tls_builder = ureq::native_tls::TlsConnector::builder();
        if def.allow_invalid_certs {
            tls_builder.danger_accept_invalid_certs(true);
            tls_builder.danger_accept_invalid_hostnames(true);
        }
        let tls = tls_builder
            .build()
            .map_err(|e| Error::Msg(format!("TLS init failed: {e}")))?;
        let agent = ureq::AgentBuilder::new()
            .tls_connector(std::sync::Arc::new(tls))
            .timeout_connect(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build();
        let mut session = Self {
            agent,
            def,
            request_id: AtomicU64::new(1),
            session_id: Mutex::new(None),
            protocol: String::new(),
            server: Value::Null,
        };

        let init = session.rpc(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "eco-platform",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )?;

        session.protocol = init
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or(PROTOCOL_VERSION)
            .to_string();
        session.server = init.get("serverInfo").cloned().unwrap_or(Value::Null);
        session.notify("notifications/initialized", json!({}));
        Ok(session)
    }

    pub fn list_tools(&self) -> Result<Vec<Value>> {
        let result = self.rpc("tools/list", json!({}))?;
        Ok(result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    pub fn call(&self, tool: &str, arguments: Value) -> Result<Value> {
        self.call_with_headers(tool, arguments, &[])
    }

    /// Same as `call`, with per-request identity headers (agent id, user,
    /// request id). Used by the platform gateway for remote MCP calls.
    pub fn call_with_headers(
        &self,
        tool: &str,
        arguments: Value,
        headers: &[(String, String)],
    ) -> Result<Value> {
        self.rpc_with_headers(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
            headers,
        )
    }

    fn rpc(&self, method: &str, params: Value) -> Result<Value> {
        self.rpc_with_headers(method, params, &[])
    }

    fn rpc_with_headers(
        &self,
        method: &str,
        params: Value,
        headers: &[(String, String)],
    ) -> Result<Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        let text = self.post(&payload, headers)?;
        let value = parse_rpc_body(&text, id)?;
        if let Some(err) = value.get("error") {
            return msg(format!("MCP error: {err}"));
        }
        Ok(value.get("result").cloned().unwrap_or(Value::Null))
    }

    fn notify(&self, method: &str, params: Value) {
        let payload = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let _ = self.post(&payload, &[]);
    }

    fn post(&self, payload: &Value, headers: &[(String, String)]) -> Result<String> {
        let mut request = self
            .agent
            .post(&self.def.url)
            .set("Content-Type", "application/json")
            .set("Accept", "application/json, text/event-stream")
            .set("MCP-Protocol-Version", PROTOCOL_VERSION);

        for (name, value) in headers {
            request = request.set(name, value);
        }

        if let Some(token) = &self.def.token {
            request = request.set("Authorization", &format!("Bearer {token}"));
        }
        if let Some(session_id) = self.session_id.lock().ok().and_then(|g| g.clone()) {
            request = request.set("MCP-Session-Id", &session_id);
        }

        let body = serde_json::to_string(payload)?;
        match request.send_string(&body) {
            Ok(response) => {
                if let Some(session_id) = response.header("mcp-session-id") {
                    if let Ok(mut guard) = self.session_id.lock() {
                        *guard = Some(session_id.to_string());
                    }
                }
                response
                    .into_string()
                    .map_err(|e| Error::Msg(format!("MCP read failed: {e}")))
            }
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                let snippet: String = body.chars().take(400).collect();
                msg(format!("MCP HTTP {code}: {snippet}"))
            }
            Err(e) => msg(format!("MCP transport error: {e}")),
        }
    }
}

fn parse_rpc_body(text: &str, id: u64) -> Result<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return msg("empty MCP response");
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if value.get("id").and_then(Value::as_u64) == Some(id)
            || value.get("result").is_some()
            || value.get("error").is_some()
        {
            return Ok(value);
        }
    }

    for line in trimmed.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(data) {
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return Ok(value);
            }
        }
    }

    let snippet: String = trimmed.chars().take(200).collect();
    msg(format!("cannot parse MCP response: {snippet}"))
}

/// Caps a tool result so it can never blow up the model context or the audit
/// log. The full response stays on the server side.
pub fn capped(value: &Value) -> (String, bool, usize) {
    capped_with(value, MAX_RESPONSE_BYTES)
}

/// Same as `capped`, with an explicit limit. File and artifact results use a
/// higher limit than remote MCP responses: they are already bounded by the
/// read cap, and the JSON envelope must not cut them a second time.
pub fn capped_with(value: &Value, limit: usize) -> (String, bool, usize) {
    let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
    let bytes = pretty.len();
    if bytes <= limit {
        return (pretty, false, bytes);
    }
    let mut cut = String::new();
    for ch in pretty.chars() {
        if cut.len() + ch.len_utf8() > limit {
            break;
        }
        cut.push(ch);
    }
    cut.push_str(&format!(
        "\n… truncated by platform ({} KiB limit)",
        limit / 1024
    ));
    (cut, true, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_sse() {
        let plain = r#"{"jsonrpc":"2.0","id":3,"result":{"ok":true}}"#;
        let v = parse_rpc_body(plain, 3).unwrap();
        assert_eq!(v["result"]["ok"], true);

        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":4,\"result\":{\"ok\":1}}\n\n";
        let v = parse_rpc_body(sse, 4).unwrap();
        assert_eq!(v["result"]["ok"], 1);

        assert!(parse_rpc_body("nonsense", 1).is_err());
    }

    #[test]
    fn caps_large_results() {
        let big = serde_json::json!({ "text": "x".repeat(MAX_RESPONSE_BYTES * 2) });
        let (text, truncated, bytes) = capped(&big);
        assert!(truncated);
        assert!(bytes > MAX_RESPONSE_BYTES);
        assert!(text.len() < MAX_RESPONSE_BYTES + 100);
    }

    #[test]
    fn app_scoped_definition_overrides_global_and_falls_back() {
        let home = std::env::temp_dir().join(format!(
            "eco-mcp-scope-{}-{}",
            std::process::id(),
            crate::artifacts::now_secs()
        ));
        let secrets = SecretStore::new(&home);
        let global = ServerDef {
            id: "shared".into(),
            url: "https://global.example/mcp".into(),
            token: Some("global-token".into()),
            allow_invalid_certs: false,
        };
        put(&secrets, &global).unwrap();
        let app = ServerDef {
            id: "shared".into(),
            url: "https://app.example/mcp".into(),
            token: Some("app-token".into()),
            allow_invalid_certs: true,
        };
        put_for_app(&secrets, "doc-writer", &app).unwrap();

        let resolved = get_for_app(&secrets, "doc-writer", "shared").unwrap();
        assert_eq!(resolved.url, "https://app.example/mcp");
        assert_eq!(resolved.token.as_deref(), Some("app-token"));
        assert!(resolved.allow_invalid_certs);
        assert_eq!(
            get_for_app(&secrets, "other-app", "shared").unwrap().url,
            "https://global.example/mcp"
        );

        delete_for_app(&secrets, "doc-writer", "shared").unwrap();
        assert_eq!(
            get_for_app(&secrets, "doc-writer", "shared").unwrap().url,
            "https://global.example/mcp"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn remote_mcp_urls_require_tls_outside_loopback() {
        assert!(validate_url("https://example.com/mcp").is_ok());
        assert!(validate_url("https://example.com/mcp?x=1").is_ok());
        assert!(validate_url("http://127.0.0.1:8080/mcp").is_ok());
        assert!(validate_url("http://localhost/mcp").is_ok());
        assert!(validate_url("http://[::1]:8080/mcp").is_ok());
        assert!(validate_url("http://evil.example/mcp").is_err());
        assert!(validate_url("http://localhost:80@evil.example/mcp").is_err());
        assert!(validate_url("ftp://example.com/mcp").is_err());
    }
}
