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
}

fn key(id: &str) -> String {
    format!("mcp.{id}")
}

fn validate_id(id: &str) -> Result<()> {
    crate::manifest::parse_id(id, "mcp server")
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

pub fn put(secrets: &SecretStore, def: &ServerDef) -> Result<()> {
    validate_id(&def.id)?;
    let raw = serde_json::to_string(def)?;
    secrets.store(&key(&def.id), &raw)
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
        // Native TLS (Schannel on Windows) instead of rustls: corporate TLS
        // inspection in a closed network is handled by the system store.
        let tls = ureq::native_tls::TlsConnector::new()
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
    let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
    let bytes = pretty.len();
    if bytes <= MAX_RESPONSE_BYTES {
        return (pretty, false, bytes);
    }
    let mut cut = String::new();
    for ch in pretty.chars() {
        if cut.len() + ch.len_utf8() > MAX_RESPONSE_BYTES {
            break;
        }
        cut.push(ch);
    }
    cut.push_str("\n… truncated by platform (64 KiB limit)");
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
}
