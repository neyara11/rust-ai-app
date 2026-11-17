use crate::config;
use crate::error::{msg, Error, Result};
use crate::identity;
use crate::manifest;
use crate::proxy;
use crate::secrets::SecretStore;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
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
    pub mime: Option<String>,
    /// Content-addressed reference to a saved attachment image. The history
    /// itself never carries base64: the picture is fetched on demand by the UI
    /// through `chat_image`, so a long chat does not re-send every screenshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// Prompt tokens served from / written to the provider cache. Together with
    /// `tokens_in` this is the context that was actually occupied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tok_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Uploaded prompt content. `data` is standard base64 without a `data:` prefix;
/// the engine builds the data URL and never writes the payload to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInput {
    pub mime: String,
    #[serde(default)]
    pub filename: String,
    pub data: String,
}

pub const MAX_ATTACHMENTS: usize = 3;
pub const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TEXT_BYTES: usize = 64 * 1024;

fn normalized_mime(raw: &str) -> String {
    let mime = raw.trim().to_lowercase();
    if mime == "image/jpg" {
        "image/jpeg".to_string()
    } else {
        mime
    }
}

fn is_text_mime(mime: &str) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/json"
                | "application/xml"
                | "application/x-yaml"
                | "application/yaml"
                | "application/toml"
                | "application/javascript"
                | "application/x-sh"
        )
}

fn image_magic_matches(mime: &str, bytes: &[u8]) -> bool {
    match mime {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "image/webp" => {
            bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
        }
        _ => false,
    }
}

fn safe_filename(raw: &str, mime: &str) -> String {
    let name = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(raw)
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect::<String>();
    let name = name.trim();
    if name.is_empty() {
        let ext = mime.rsplit('/').next().unwrap_or("bin");
        format!("attachment.{ext}")
    } else {
        name.to_string()
    }
}

/// Validates uploaded content before any engine call. Images require `vision`;
/// text files are inlined as text parts and work with any model.
pub fn validate_attachments(attachments: &[AttachmentInput], vision: bool) -> Result<()> {
    if attachments.len() > MAX_ATTACHMENTS {
        return msg(format!(
            "too many attachments: {} > {MAX_ATTACHMENTS}",
            attachments.len()
        ));
    }
    for attachment in attachments {
        let mime = normalized_mime(&attachment.mime);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(attachment.data.trim())
            .map_err(|_| Error::Msg(format!("attachment '{}' is not valid base64", attachment.filename)))?;
        if is_text_mime(&mime) {
            if bytes.len() > MAX_TEXT_BYTES {
                return msg(format!(
                    "attachment '{}' exceeds {MAX_TEXT_BYTES} bytes",
                    attachment.filename
                ));
            }
            std::str::from_utf8(&bytes).map_err(|_| {
                Error::Msg(format!(
                    "attachment '{}' is not valid UTF-8 text",
                    attachment.filename
                ))
            })?;
        } else if mime.starts_with("image/") {
            if !vision {
                return msg("model is configured without image support");
            }
            if bytes.len() > MAX_IMAGE_BYTES {
                return msg(format!(
                    "attachment '{}' exceeds {MAX_IMAGE_BYTES} bytes",
                    attachment.filename
                ));
            }
            if !image_magic_matches(&mime, &bytes) {
                return msg(format!(
                    "attachment '{}' does not match declared type {mime}",
                    attachment.filename
                ));
            }
        } else {
            return msg(format!(
                "attachment '{}' has unsupported type {mime}",
                attachment.filename
            ));
        }
    }
    Ok(())
}

fn build_parts(text: &str, attachments: &[AttachmentInput]) -> Vec<Value> {
    let mut parts = Vec::new();
    if !text.is_empty() {
        parts.push(json!({ "type": "text", "text": text }));
    }
    for attachment in attachments {
        let mime = normalized_mime(&attachment.mime);
        let name = safe_filename(&attachment.filename, &mime);
        if is_text_mime(&mime) {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(attachment.data.trim())
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_default();
            parts.push(json!({
                "type": "text",
                "text": format!("[attachment: {name}]\n\n{decoded}")
            }));
        } else {
            parts.push(json!({
                "type": "file",
                "mime": mime,
                "url": format!("data:{mime};base64,{}", attachment.data.trim()),
                "filename": name
            }));
        }
    }
    parts
}

impl ChatItem {
    fn new(role: &str, kind: &str, text: String) -> Self {
        Self {
            role: role.to_string(),
            kind: kind.to_string(),
            text,
            mime: None,
            image_ref: None,
            tokens_in: None,
            tokens_out: None,
            reasoning_tokens: None,
            cache_read: None,
            cache_write: None,
            ms: None,
            tok_s: None,
            model: None,
        }
    }
}

#[derive(Default)]
struct StreamPart {
    id: String,
    kind: String,
    text: String,
    tool: String,
    status: String,
    /// Engine call id (`part.callID`), used to join a permission request with
    /// the tool part that carries its arguments.
    call_id: String,
    /// Short, human-readable target of the call (a path, or `in -> out`).
    target: Option<String>,
}

/// Live parts of the assistant message currently being generated. Built from
/// `message.part.updated` / `message.part.delta` events on `/event`.
#[derive(Default)]
struct StreamState {
    message_id: Option<String>,
    parts: Vec<StreamPart>,
    /// Items from earlier assistant messages of the same turn. Without this a
    /// new message (for example after a tool call) would erase the reasoning
    /// and tool calls the user already saw.
    done: Vec<ChatItem>,
    changed: bool,
}

/// Bounded queue of asked-but-unanswered requests that still lack a target; a
/// tool part arriving later fills it in and refreshes the card.
const MAX_WAITING_ASKS: usize = 16;
/// Per-value and whole-target caps keep the card readable and metadata free of
/// file bodies.
const TARGET_VALUE_MAX_CHARS: usize = 160;
const TARGET_MAX_CHARS: usize = 240;

const TOOL_MARKUP_NOTE: &str = "Модель попыталась вызвать недоступный инструмент; служебная разметка скрыта. Проверьте, что пакету разрешены нужные tools/file_scopes.";

fn has_tool_markup(text: &str) -> bool {
    let lowered = text.to_lowercase();
    lowered.contains("<||dsml||") || lowered.contains("<|dsml|")
}

fn clean_text(text: String) -> String {
    if has_tool_markup(&text) {
        TOOL_MARKUP_NOTE.to_string()
    } else {
        text
    }
}

fn squash_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A path-like metadata string: one line, no JSON braces, sane length. This is
/// what keeps file bodies out of the permission card.
fn looks_like_path(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= TARGET_VALUE_MAX_CHARS
        && !value.contains('\n')
        && !value.contains('\r')
        && !value.contains('{')
        && !value.contains('}')
}

fn short_target_value(value: Option<&Value>) -> Option<String> {
    let raw = value?.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    let capped: String = squash_ws(raw)
        .chars()
        .take(TARGET_VALUE_MAX_CHARS)
        .collect();
    if capped.is_empty() {
        None
    } else {
        Some(capped)
    }
}

/// Picks a path-like value: single-path tools first, then the two-path
/// `docx_export` shape. Only known path keys are read, never `content`.
fn pick_target(input: &Value) -> Option<String> {
    let Some(object) = input.as_object() else {
        let text = input.as_str()?;
        return looks_like_path(text).then(|| squash_ws(text));
    };
    for key in ["path", "filePath", "file", "target"] {
        if let Some(found) = short_target_value(object.get(key)) {
            return Some(found);
        }
    }
    let from = short_target_value(object.get("input"));
    let to = short_target_value(object.get("output"));
    match (from, to) {
        (Some(from), Some(to)) => Some(format!("{from} \u{2192} {to}")),
        (Some(from), None) => Some(from),
        (None, Some(to)) => Some(to),
        (None, None) => None,
    }
}

/// Derives the short target of a tool part from `state.input`, falling back to
/// `state.metadata` and `state.title`.
fn tool_target(part: &Value) -> Option<String> {
    let state = part.get("state").unwrap_or(&Value::Null);
    let target = pick_target(state.get("input").unwrap_or(&Value::Null))
        .or_else(|| pick_target(state.get("metadata").unwrap_or(&Value::Null)))
        .or_else(|| short_target_value(state.get("title")))?;
    if target.is_empty() {
        return None;
    }
    Some(target.chars().take(TARGET_MAX_CHARS).collect())
}

impl StreamState {
    fn part(&mut self, id: &str) -> &mut StreamPart {
        if let Some(index) = self.parts.iter().position(|part| part.id == id) {
            return &mut self.parts[index];
        }
        self.parts.push(StreamPart {
            id: id.to_string(),
            ..Default::default()
        });
        self.parts.last_mut().unwrap()
    }

    fn part_by_call(&self, call_id: &str) -> Option<&StreamPart> {
        if call_id.is_empty() {
            return None;
        }
        self.parts
            .iter()
            .find(|part| part.call_id == call_id && part.target.is_some())
    }

    fn items(&self) -> Vec<ChatItem> {
        let mut items = self.done.clone();
        items.extend(self.current_items());
        items
    }

    fn current_items(&self) -> Vec<ChatItem> {
        let mut items = Vec::new();
        let reasoning = self
            .parts
            .iter()
            .filter(|part| part.kind == "reasoning")
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let text = self
            .parts
            .iter()
            .filter(|part| part.kind == "text")
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        if !reasoning.trim().is_empty() {
            items.push(ChatItem::new("assistant", "reasoning", reasoning));
        }
        if !text.is_empty() {
            items.push(ChatItem::new("assistant", "text", clean_text(text)));
        }
        for part in self.parts.iter().filter(|part| part.kind == "tool") {
            let status = if part.status.is_empty() {
                "called"
            } else {
                part.status.as_str()
            };
            items.push(ChatItem::new(
                "tool",
                "tool",
                format!("{} -> {}", part.tool, status),
            ));
        }
        items
    }

    fn flush(&mut self) {
        let current = self.current_items();
        if !current.is_empty() {
            self.done.extend(current);
        }
        self.parts.clear();
    }
}

/// An asked request whose tool part has not arrived yet.
struct WaitingAsk {
    call_id: String,
    request: crate::permissions::PermissionRequest,
}

/// Copies the target of the matching tool part into the request metadata when
/// the part is already in the live state. Returns true when it was added.
fn enrich_from_state(
    request: &mut crate::permissions::PermissionRequest,
    state: &StreamState,
) -> bool {
    if crate::permissions::target_of(&request.metadata).is_some() {
        return false;
    }
    let Some(call_id) = request.call_id.as_deref() else {
        return false;
    };
    let Some(target) = state
        .part_by_call(call_id)
        .and_then(|part| part.target.clone())
    else {
        return false;
    };
    crate::permissions::merge_target(&mut request.metadata, &target);
    true
}

/// Resolves waiting asks against the current live state, returning requests
/// that just learned their target. Resolved entries leave the queue.
fn resolve_waiting_targets(
    waiting: &mut Vec<WaitingAsk>,
    state: &StreamState,
) -> Vec<crate::permissions::PermissionRequest> {
    let mut ready = Vec::new();
    let mut index = 0;
    while index < waiting.len() {
        let found = state
            .part_by_call(&waiting[index].call_id)
            .and_then(|part| part.target.clone());
        match found {
            Some(target) => {
                let mut ask = waiting.remove(index);
                if crate::permissions::target_of(&ask.request.metadata).is_none() {
                    crate::permissions::merge_target(&mut ask.request.metadata, &target);
                    ready.push(ask.request);
                }
            }
            None => index += 1,
        }
    }
    ready
}

/// Applies one bus event to the live state. Returns true when the turn is
/// finished.
fn apply_stream_event(state: &mut StreamState, event: &Value) -> bool {
    let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
    let props = event.get("properties").unwrap_or(&Value::Null);

    match kind {
        "message.updated" => {
            let info = props.get("info").unwrap_or(&Value::Null);
            if info.get("role").and_then(Value::as_str) != Some("assistant") {
                return false;
            }
            if let Some(id) = info.get("id").and_then(Value::as_str) {
                if state.message_id.as_deref() != Some(id) {
                    state.flush();
                    state.message_id = Some(id.to_string());
                }
            }
        }
        "message.part.updated" => {
            let Some(part) = props.get("part") else {
                return false;
            };
            if part.get("messageID").and_then(Value::as_str)
                != state.message_id.as_deref()
            {
                return false;
            }
            let Some(id) = part.get("id").and_then(Value::as_str) else {
                return false;
            };
            let part_kind = part.get("type").and_then(Value::as_str).unwrap_or("");
            if !matches!(part_kind, "text" | "reasoning" | "tool") {
                return false;
            }
            let entry = state.part(id);
            entry.kind = part_kind.to_string();
            match part_kind {
                "text" | "reasoning" => {
                    entry.text = part
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                }
                "tool" => {
                    entry.tool = part
                        .get("tool")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string();
                    entry.status = part
                        .pointer("/state/status")
                        .and_then(Value::as_str)
                        .unwrap_or("called")
                        .to_string();
                    if let Some(call_id) = part.get("callID").and_then(Value::as_str) {
                        entry.call_id = call_id.to_string();
                    }
                    if let Some(target) = tool_target(part) {
                        entry.target = Some(target);
                    }
                }
                _ => {}
            }
            state.changed = true;
        }
        "message.part.delta" => {
            if props.get("messageID").and_then(Value::as_str)
                != state.message_id.as_deref()
            {
                return false;
            }
            let Some(id) = props.get("partID").and_then(Value::as_str) else {
                return false;
            };
            let delta = props.get("delta").and_then(Value::as_str).unwrap_or("");
            if delta.is_empty() {
                return false;
            }
            let entry = state.part(id);
            entry.text.push_str(delta);
            state.changed = true;
        }
        _ => {}
    }
    false
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
    pub home: PathBuf,
    pub app_id: String,
    pub app_name: String,
    pub app_version: String,
    pub token_id: String,
    artifact_dir: PathBuf,
    pid_file: PathBuf,
}

/// Removes the `.active` marker if engine startup fails after the artifact
/// session was created, so a failed start does not leave an unsweepable session.
struct ArtifactGuard {
    dir: PathBuf,
    armed: bool,
}

impl Drop for ArtifactGuard {
    fn drop(&mut self) {
        if self.armed {
            crate::artifacts::close_session(&self.dir);
        }
    }
}

/// Recorded engine process identity: a stale pid file must not be used to kill
/// an unrelated process after Windows reuses the pid.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeInfo {
    pid: u32,
    #[serde(default)]
    image: String,
    #[serde(default)]
    created: u64,
}

/// Kills a just-spawned sidecar if startup fails before `Engine` is built.
struct ChildGuard {
    child: Option<Child>,
    pid_file: PathBuf,
    pid: u32,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = child.kill();
        let _ = child.wait();
        if recorded_pid(&self.pid_file) == Some(self.pid) {
            let _ = std::fs::remove_file(&self.pid_file);
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Only remove the runtime file if it still belongs to this engine: a
        // newer engine (for example started by the CLI) may have replaced it.
        if recorded_pid(&self.pid_file) == Some(self.child.id()) {
            let _ = std::fs::remove_file(&self.pid_file);
        }
        crate::artifacts::close_session(&self.artifact_dir);
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
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidate in [dir.join("engine").join("opencode.exe"), dir.join("opencode.exe")] {
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
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
    msg("opencode binary not found: set ECO_OPENCODE_BIN, place it in <home>/engine/opencode.exe, or ship it in the install dir as engine/opencode.exe")
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

/// The event stream stays open for the whole turn, so it must not inherit the
/// 300 s request timeout used for regular calls.
fn stream_agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        // The stream is long-lived, but a silent socket must not pin a reader
        // thread forever. A generator/tool run longer than this can outlive the
        // stream: completion still arrives through the turn POST, but the UI
        // stops receiving live events until the next turn.
        .timeout_read(Duration::from_secs(120));
    if let Ok(tls) = ureq::native_tls::TlsConnector::new() {
        builder = builder.tls_connector(std::sync::Arc::new(tls));
    }
    builder.build()
}

/// The turn POST returns only when the model has finished the whole turn, so
/// it must not carry the 300 s cap used by regular calls: runs can last hours.
/// A generous ceiling still exists, otherwise a wedged engine would pin the
/// worker (and the chat busy flag) forever.
const MAX_TURN_SECS: u64 = 4 * 60 * 60;

fn turn_agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(MAX_TURN_SECS));
    if let Ok(tls) = ureq::native_tls::TlsConnector::new() {
        builder = builder.tls_connector(std::sync::Arc::new(tls));
    }
    builder.build()
}

/// Short-timeout agent for readiness probes: `wait_ready` must honour its own
/// budget instead of inheriting the 300 s request timeout.
fn probe_agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout(Duration::from_secs(2));
    if let Ok(tls) = ureq::native_tls::TlsConnector::new() {
        builder = builder.tls_connector(std::sync::Arc::new(tls));
    }
    builder.build()
}

fn health_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(5))
        .build()
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

/// Model metadata for the generated provider config. Opencode does not know
/// capabilities of a custom gateway model, so they come from the platform
/// config. Fields match the models.dev schema that opencode already consumes.
/// Fallbacks for a partially filled model capability block: the engine schema
/// rejects a `limit` object that misses either number.
const DEFAULT_CONTEXT_TOKENS: u64 = 128_000;
const DEFAULT_OUTPUT_TOKENS: u64 = 8_192;

fn model_entry(model_name: &str, caps: &config::ModelCapabilities) -> Value {
    let mut model = serde_json::Map::new();
    model.insert("name".to_string(), json!(model_name));
    model.insert("attachment".to_string(), json!(caps.vision));
    model.insert("reasoning".to_string(), json!(caps.reasoning));
    model.insert("tool_call".to_string(), json!(true));
    model.insert(
        "modalities".to_string(),
        json!({
            "input": if caps.vision { vec!["text", "image"] } else { vec!["text"] },
            "output": vec!["text"]
        }),
    );
    if caps.context_tokens.is_some() || caps.output_tokens.is_some() {
        // OpenCode's config schema requires both numbers once `limit` exists:
        // an empty output/context entry makes the whole engine config invalid
        // and the engine exits before it can serve `/config`.
        let mut limit = serde_json::Map::new();
        limit.insert(
            "context".to_string(),
            json!(caps.context_tokens.unwrap_or(DEFAULT_CONTEXT_TOKENS)),
        );
        limit.insert(
            "output".to_string(),
            json!(caps.output_tokens.unwrap_or(DEFAULT_OUTPUT_TOKENS)),
        );
        model.insert("limit".to_string(), Value::Object(limit));
    }
    let mut variants = serde_json::Map::new();
    if caps.reasoning {
        for effort in caps.efforts() {
            variants.insert(effort.clone(), json!({ "reasoningEffort": effort }));
        }
    }
    model.insert("variants".to_string(), Value::Object(variants));
    Value::Object(model)
}

fn provider_config(
    cfg: &config::UserConfig,
    secrets: &SecretStore,
    headers: &Value,
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

    // The engine is a Node process. A private CA/self-signed gateway can be
    // accepted explicitly per profile; the default stays strict.
    if cfg.llm_allow_invalid_certs {
        envs.push(("NODE_TLS_REJECT_UNAUTHORIZED".to_string(), "0".to_string()));
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
                "apiKey": api_key,
                // Agent identity markers for every model call. The raw token
                // stays in the engine env; the config only references it.
                "headers": headers
            },
            "models": {
                (model_name.clone()): model_entry(&model_name, &cfg.llm_capabilities)
            }
        }
    });

    Ok((provider, "gw".to_string(), model_name, envs))
}

/// Everything the gateway process needs. Paths are absolute because opencode
/// starts the proxy with its own working directory and a replaced HOME.
pub struct GatewayConfig {
    pub exe: PathBuf,
    pub home: PathBuf,
    pub workspace: PathBuf,
    /// Active artifact session; large tool results are dumped there.
    pub artifacts: PathBuf,
}

fn build_config(
    app: &manifest::Manifest,
    cfg: &config::UserConfig,
    secrets: &SecretStore,
    gateway: &GatewayConfig,
    package_dir: &std::path::Path,
    token: &identity::IssuedToken,
    user: &str,
    policy_override: Option<crate::permissions::PermissionPolicy>,
) -> Result<(Value, String, String, Vec<(String, String)>)> {
    let headers = json!({
        "X-Agent-Id": format!("{}/{}", app.app_id, app.version),
        "X-Agent-Version": app.version,
        "X-User": user,
        // One engine instance is one request scope; per-turn ids go to audit.
        "X-Request-Id": token.token_id,
        "X-Agent-Token-Id": token.token_id,
        "X-Agent-Token-Expires": token.expires_at.to_string()
    });
    let (provider, provider_id, model_id, envs) = provider_config(cfg, secrets, &headers)?;

    let names = proxy::gateway_tool_names(app, package_dir);
    let policy = policy_override
        .unwrap_or_else(|| crate::permissions::policy_for(cfg, &app.app_id));
    let tool_map = crate::permissions::tool_map(app);
    let mut permissions = serde_json::Map::new();
    permissions.insert("*".to_string(), json!("deny"));
    for name in &names {
        let action = format!("{}_{}", proxy::SERVER_NAME, name);
        let mode = tool_map
            .get(&action)
            .map(|info| crate::permissions::mode_for_tool(&policy, info))
            .unwrap_or(crate::permissions::PermissionMode::Ask);
        permissions.insert(action, json!(mode.as_str()));
    }

    let mut servers = serde_json::Map::new();
    if !names.is_empty() {
        servers.insert(
            proxy::SERVER_NAME.to_string(),
            json!({
                "type": "local",
                "command": [
                    gateway.exe.to_string_lossy(),
                    "--mcp-proxy",
                    app.app_id,
                    gateway.workspace.to_string_lossy(),
                    gateway.artifacts.to_string_lossy()
                ],
                "environment": {
                    "ECO_PLATFORM_HOME": gateway.home.to_string_lossy(),
                    // Passed through the engine env; never written as a raw
                    // value into this config file.
                    "ECO_AGENT_TOKEN": "{env:ECO_AGENT_TOKEN}",
                    "ECO_AGENT_USER": "{env:ECO_AGENT_USER}"
                },
                // Keep tools on the provider's native list: the platform owns
                // naming, permissions and audit.
                "codemode": false
            }),
        );
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

fn skill_texts(package_dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let dir = package_dir.join("skills");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path().join("SKILL.md");
        if let Ok(text) = std::fs::read_to_string(path) {
            out.push((
                entry.file_name().to_string_lossy().to_string(),
                text,
            ));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn recorded_pid(pid_file: &Path) -> Option<u32> {
    let raw = std::fs::read_to_string(pid_file).ok()?;
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        serde_json::from_str::<RuntimeInfo>(trimmed)
            .ok()
            .map(|info| info.pid)
    } else {
        trimmed.parse::<u32>().ok()
    }
}

fn parse_runtime(raw: &str) -> Result<RuntimeInfo> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        serde_json::from_str(trimmed)
            .map_err(|e| Error::Msg(format!("engine pid file is corrupt: {e}")))
    } else {
        let pid = trimmed
            .parse::<u32>()
            .map_err(|_| Error::Msg("engine pid file is corrupt".to_string()))?;
        Ok(RuntimeInfo {
            pid,
            image: String::new(),
            created: 0,
        })
    }
}

enum ProcessState {
    Missing,
    Unknown,
    Found(RuntimeInfo),
}

#[cfg(windows)]
fn process_state(pid: u32) -> ProcessState {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            let code = std::io::Error::last_os_error().raw_os_error();
            return if code == Some(87) {
                ProcessState::Missing
            } else {
                ProcessState::Unknown
            };
        }

        let mut buf = vec![0u16; 32 * 1024];
        let mut size = buf.len() as u32;
        let image = if QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) != 0 {
            std::ffi::OsString::from_wide(&buf[..size as usize])
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        };

        let mut created = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exited = created;
        let mut kernel = created;
        let mut user = created;
        let created_ticks =
            if GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0 {
                ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64
            } else {
                0
            };
        CloseHandle(handle);

        ProcessState::Found(RuntimeInfo {
            pid,
            image,
            created: created_ticks,
        })
    }
}

#[cfg(not(windows))]
fn process_state(_pid: u32) -> ProcessState {
    ProcessState::Unknown
}

fn same_process(recorded: &RuntimeInfo, current: &RuntimeInfo) -> bool {
    if recorded.created != 0 && current.created != 0 {
        return recorded.created == current.created;
    }
    if !recorded.image.is_empty() {
        return recorded.image.eq_ignore_ascii_case(&current.image);
    }
    current
        .image
        .to_ascii_lowercase()
        .ends_with("opencode.exe")
}

fn runtime_info(pid: u32, image: &Path) -> RuntimeInfo {
    match process_state(pid) {
        ProcessState::Found(info) => info,
        _ => RuntimeInfo {
            pid,
            image: image.display().to_string(),
            created: 0,
        },
    }
}

/// Stops a sidecar left behind by a crash, a force-killed app, or an explicit
/// token revocation. Refuses to kill when the pid no longer belongs to the
/// recorded engine image. Safe to call when no engine is running.
/// Keeps the engine child inside a job object that terminates it when the
/// platform exits. Without this a crash or a force-killed window leaves an
/// orphan `opencode serve`, which locks the app's engine home and makes the
/// next start fail with "engine did not become ready in time".
#[cfg(windows)]
fn keep_engine_with_platform(child: &std::process::Child) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if configured == 0 {
            let _ = CloseHandle(job);
            return;
        }
        let process = child.as_raw_handle() as HANDLE;
        if AssignProcessToJobObject(job, process) == 0 {
            let _ = CloseHandle(job);
            return;
        }
        // The handle is intentionally not closed: closing it would terminate
        // the engine immediately. Windows closes it when the platform exits,
        // and KILL_ON_JOB_CLOSE then takes the engine down with us.
    }
}

#[cfg(not(windows))]
fn keep_engine_with_platform(_child: &std::process::Child) {}

pub fn kill_leftover(home: &Path, app_id: &str) -> Result<()> {
    let pid_file = home
        .join("apps")
        .join(app_id)
        .join("engine")
        .join("engine.pid");
    let raw = match std::fs::read_to_string(&pid_file) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(Error::Io(e)),
    };
    let recorded = parse_runtime(&raw)?;
    match process_state(recorded.pid) {
        ProcessState::Missing => {
            let _ = std::fs::remove_file(&pid_file);
            return Ok(());
        }
        ProcessState::Unknown => {
            return msg(format!(
                "cannot verify engine process {}; refusing to kill it",
                recorded.pid
            ))
        }
        ProcessState::Found(current) => {
            if !same_process(&recorded, &current) {
                return msg(format!(
                    "pid {} is not the recorded engine process; refusing to kill it",
                    recorded.pid
                ));
            }
        }
    }

    let mut kill = Command::new("taskkill");
    kill.args(["/PID", &recorded.pid.to_string(), "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        kill.creation_flags(0x0800_0000);
    }
    let status = kill.status().map_err(|e| {
        Error::Msg(format!("cannot run taskkill for pid {}: {e}", recorded.pid))
    })?;
    if !status.success() {
        return msg(format!(
            "taskkill failed for pid {}; engine may still be running",
            recorded.pid
        ));
    }
    let _ = std::fs::remove_file(&pid_file);
    Ok(())
}

impl Engine {
    pub fn start(app_id: &str, home: &Path, workdir: &Path) -> Result<Self> {
        Self::start_with(app_id, home, workdir, None)
    }

    /// Same as [`Engine::start`], but a headless caller can supply the effect
    /// policy instead of reading one from the profile.
    pub fn start_with(
        app_id: &str,
        home: &Path,
        workdir: &Path,
        policy_override: Option<crate::permissions::PermissionPolicy>,
    ) -> Result<Self> {
        let package_dir = crate::registry::apps_dir(home).join(app_id);
        let app = crate::registry::verified_manifest(home, app_id)?;
        let cfg = config::load(home);
        let secrets = SecretStore::new(home);

        // One artifact session per engine run: large tool results are dumped
        // as NDJSON there and swept by TTL/quota.
        let artifact_dir = crate::artifacts::open_session(
            &crate::artifacts::sessions_root(home, app_id),
            &config::policy(home),
        )?;
        let mut artifact_guard = ArtifactGuard {
            dir: artifact_dir.clone(),
            armed: true,
        };

        // Short-lived per-app token: the proxy validates it on every call and
        // `--revoke` can stop a running agent without reinstalling the app.
        let token = identity::issue(
            home,
            &app.app_id,
            &app.version,
            identity::DEFAULT_TTL_SECS,
            "platform",
        )?;
        let user = identity::user_pseudonym(home);

        let gateway = GatewayConfig {
            exe: std::env::current_exe()
                .map_err(|e| Error::Msg(format!("cannot resolve platform binary: {e}")))?,
            home: std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf()),
            workspace: std::fs::canonicalize(workdir)
                .unwrap_or_else(|_| workdir.to_path_buf()),
            artifacts: artifact_dir.clone(),
        };

        let (engine_config, provider_id, model_id, mut engine_envs) =
            build_config(
                &app,
                &cfg,
                &secrets,
                &gateway,
                &package_dir,
                &token,
                &user,
                policy_override,
            )?;
        engine_envs.push(("ECO_AGENT_TOKEN".to_string(), token.token.clone()));
        engine_envs.push(("ECO_AGENT_USER".to_string(), user.clone()));

        let engine_home = home.join("apps").join(app_id).join("engine");
        std::fs::create_dir_all(&engine_home)?;
        write_config(&engine_home, &engine_config)?;

        // Kill an engine left behind by a previous crash or a force-killed app.
        // A process that cannot be verified or stopped aborts the start: two
        // engines must never share the same session store.
        kill_leftover(home, app_id)?;
        let pid_file = engine_home.join("engine.pid");

        let binary = find_binary(home)?;
        let models = binary
            .parent()
            .map(|dir| dir.join("models.json"))
            .filter(|path| path.is_file());
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
        // A start that never becomes ready is otherwise invisible: keep the
        // engine's stderr next to the pid file so the reason can be read.
        let stderr_log = engine_home.join("engine.stderr.log");
        let stderr = std::fs::File::create(&stderr_log)
            .map(Stdio::from)
            .unwrap_or_else(|_| Stdio::null());
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
            // Closed contour: the engine must not fetch models, npm plugins or
            // LSP servers at runtime. The provider packages it needs are
            // bundled in the opencode binary; models.json is vendored next to it.
            .env("OPENCODE_DISABLE_MODELS_FETCH", "1")
            .env("OPENCODE_DISABLE_DEFAULT_PLUGINS", "1")
            .env("OPENCODE_DISABLE_LSP_DOWNLOAD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(stderr);
        if let Some(models) = &models {
            command.env("OPENCODE_MODELS_PATH", models);
        }
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
        keep_engine_with_platform(&child);
        let runtime = runtime_info(child.id(), &binary);
        let mut child_guard = ChildGuard {
            child: Some(child),
            pid_file: pid_file.clone(),
            pid: runtime.pid,
        };
        // Write the pid immediately so a failed start can always be cleaned
        // up by `kill_leftover`, even after an early return.
        std::fs::write(&pid_file, serde_json::to_string(&runtime)?)?;

        let probe = probe_agent();
        wait_ready(&probe, &base, &auth, Duration::from_secs(25))?;
        let agent = agent();

        let session_id = match existing_session(&agent, &base, &auth)? {
            Some(id) => id,
            None => create_session(&agent, &base, &auth, &app.name)?,
        };

        let mut system = read_system_prompt(&package_dir, &app)?;
        let skills = skill_texts(&package_dir);
        match skills.len() {
            0 => {}
            // One skill is the app's whole behaviour: keep it inline for
            // compatibility with existing single-skill packages.
            1 => {
                system.push_str("\n\n---\n\n");
                system.push_str(&skills[0].1);
            }
            // A group of skills is loaded on demand so the prompt stays small
            // and each skill keeps its own scope.
            _ => {
                let names = skills
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                system.push_str(
                    "\n\n---\n\n## Скилы приложения\n\nДоступны скилы: ",
                );
                system.push_str(&names);
                system.push_str(
                    ".\nПеред выполнением задачи загрузи нужный скил инструментом platform_skill: {\"action\":\"read\",\"name\":\"<имя>\"}. Список скилов: {\"action\":\"list\"}. Работай строго по загруженному скилу; зависимые скилы загружай тем же инструментом по мере необходимости.\n",
                );
            }
        }
        let has_file_tools = app
            .capabilities
            .file_scopes
            .iter()
            .any(|scope| scope == "workspace");
        let has_package_tools = app
            .tools
            .iter()
            .any(|tool| tool.bin.as_deref().map(|bin| !bin.trim().is_empty()).unwrap_or(false));
        let has_remote_tools = app
            .connections
            .iter()
            .any(|connection| connection.kind == "http-mcp");
        if !has_file_tools && !has_package_tools && !has_remote_tools {
            system.push_str(
                "\n\n---\n\n## Ограничения среды\n\nУ пакета нет доступных инструментов. Не вызывай инструменты и не выводи служебную разметку вызовов (tool call, DSML). Если задача требует файлов или команд, объясни ограничение и попроси включить нужные возможности в манифесте (file_scopes, tools, http-mcp).\n",
            );
        }

        artifact_guard.armed = false;
        let child = child_guard
            .child
            .take()
            .ok_or_else(|| Error::Msg("engine child disappeared during startup".to_string()))?;
        Ok(Self {
            child,
            base,
            auth,
            provider_id,
            model_id,
            session_id,
            system,
            workdir: workdir.to_path_buf(),
            home: home.to_path_buf(),
            app_id: app.app_id.clone(),
            app_name: app.name.clone(),
            app_version: app.version.clone(),
            token_id: token.token_id.clone(),
            artifact_dir,
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
        self.snapshot().sessions()
    }

    pub fn new_session(&mut self, title: &str) -> Result<()> {
        self.session_id = self.snapshot().new_session(title)?;
        Ok(())
    }

    pub fn select_session(&mut self, id: &str) -> Result<()> {
        crate::manifest::parse_id(id, "session")?;
        self.session_id = id.to_string();
        Ok(())
    }

    pub fn delete_session(&mut self, id: &str) -> Result<()> {
        self.snapshot().delete_session(id)
    }

    pub fn snapshot(&self) -> ChatSession {
        ChatSession {
            base: self.base.clone(),
            auth: self.auth.clone(),
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
            session_id: self.session_id.clone(),
            system: self.system.clone(),
            home: self.home.clone(),
            app_id: self.app_id.clone(),
            app_name: self.app_name.clone(),
            app_version: self.app_version.clone(),
            token_id: self.token_id.clone(),
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
        self.session_id = self.snapshot().new_session("chat")?;
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
    pub home: PathBuf,
    pub app_id: String,
    pub app_name: String,
    pub app_version: String,
    pub token_id: String,
}

impl ChatSession {
    /// Session operations that only need the connection details, so callers
    /// can release the engine map lock before doing HTTP.
    pub fn sessions(&self) -> Result<Vec<ChatSummary>> {
        let agent = agent();
        let raw = http(&agent, &self.base, &self.auth, "GET", "/session", None)?;
        Ok(parse_sessions(&raw))
    }

    pub fn new_session(&self, title: &str) -> Result<String> {
        let agent = agent();
        create_session(&agent, &self.base, &self.auth, title)
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
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

    /// Renames a session in the engine. The title was shown to the operator, so
    /// only whitespace/length are enforced here.
    pub fn rename_session(&self, id: &str, title: &str) -> Result<()> {
        crate::manifest::parse_id(id, "session")?;
        let title = title.trim();
        if title.is_empty() {
            return msg("session title is empty");
        }
        if title.chars().count() > 120 {
            return msg("session title is longer than 120 characters");
        }
        let agent = agent();
        http(
            &agent,
            &self.base,
            &self.auth,
            "PATCH",
            &format!("/session/{id}"),
            Some(&json!({ "title": title })),
        )?;
        Ok(())
    }

    /// Synchronous prompt: returns when the agent finishes the turn. Callers
    /// run it on a worker thread so the UI stays responsive.
    pub fn prompt(&self, text: &str, variant: Option<&str>) -> Result<()> {
        self.prompt_with_attachments(text, variant, &[])
    }

    /// Same as `prompt`, with prompt attachments. Callers must validate them
    /// with `validate_attachments` before starting the turn.
    pub fn prompt_with_attachments(
        &self,
        text: &str,
        variant: Option<&str>,
        attachments: &[AttachmentInput],
    ) -> Result<()> {
        let started = Instant::now();
        let request_id = identity::new_request_id();
        let result = self.prompt_raw(text, variant, attachments);
        let took_ms = started.elapsed().as_millis() as u64;
        match &result {
            Ok(response) => self.audit_llm(&request_id, variant, took_ms, response, None),
            Err(e) => self.audit_llm(
                &request_id,
                variant,
                took_ms,
                &Value::Null,
                Some(&e.to_string()),
            ),
        }
        result.map(|_| ())
    }

    fn prompt_raw(
        &self,
        text: &str,
        variant: Option<&str>,
        attachments: &[AttachmentInput],
    ) -> Result<Value> {
        let agent = turn_agent();
        let mut payload = json!({
            "model": { "providerID": self.provider_id, "modelID": self.model_id },
            "system": self.system,
            "parts": build_parts(text, attachments)
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
        )
    }

    /// Writes one audit record per model turn: app/version, token, model,
    /// tokens and duration. Message content is never logged.
    fn audit_llm(
        &self,
        request_id: &str,
        variant: Option<&str>,
        took_ms: u64,
        response: &Value,
        problem: Option<&str>,
    ) {
        let info = response.get("info").unwrap_or(response);
        let tokens = info.get("tokens");
        let model = info
            .get("modelID")
            .and_then(Value::as_str)
            .unwrap_or(self.model_id.as_str());
        crate::commands::audit(
            &self.home,
            &self.app_id,
            json!({
                "ts": crate::artifacts::now_secs(),
                "app_id": self.app_id,
                "app_version": self.app_version,
                "source": "agent",
                "action": "llm",
                "request_id": request_id,
                "token_id": self.token_id,
                "session": self.session_id,
                "provider_id": self.provider_id,
                "model_id": model,
                "variant": variant,
                "tokens_in": tokens.and_then(|t| t.get("input")).and_then(Value::as_u64),
                "tokens_out": tokens.and_then(|t| t.get("output")).and_then(Value::as_u64),
                "reasoning_tokens": tokens
                    .and_then(|t| t.get("reasoning"))
                    .and_then(Value::as_u64),
                "took_ms": took_ms,
                "ok": problem.is_none(),
                "problem": problem.map(|p| p.chars().take(200).collect::<String>())
            }),
        );
    }

    /// Opens the server-sent event stream. Connect before sending the prompt,
    /// otherwise the first deltas (reasoning start) are lost.
    pub fn event_stream(&self) -> Result<ureq::Response> {
        self.open_event_stream(None)
    }

    /// Re-opens the bus stream, resuming from the last seen SSE id when the
    /// engine supports it. Used when a long, silent tool outlives the read
    /// timeout: a dropped stream must not end the turn's live updates.
    fn open_event_stream(&self, last_event_id: Option<&str>) -> Result<ureq::Response> {
        let agent = stream_agent();
        let url = format!("{}/global/event", self.base);
        let mut request = agent
            .get(&url)
            .set("Authorization", &self.auth)
            .set("Accept", "text/event-stream");
        if let Some(id) = last_event_id {
            request = request.set("Last-Event-ID", id);
        }
        match request.call() {
            Ok(response) => Ok(response),
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                msg(format!(
                    "event stream HTTP {code}: {}",
                    body.chars().take(200).collect::<String>()
                ))
            }
            Err(e) => msg(format!("event stream transport error: {e}")),
        }
    }

    /// Consumes `/event` until the session turns idle. Only the assistant
    /// message of this session is tracked; other sessions and bus metadata are
    /// ignored. Emits a live snapshot at most every 80 ms.
    pub fn consume_events<F, P>(
        &self,
        response: ureq::Response,
        stop: &AtomicBool,
        mut emit: F,
        mut on_permission: P,
    ) -> Result<()>
    where
        F: FnMut(Vec<ChatItem>),
        P: FnMut(crate::permissions::PermissionBusEvent) -> crate::permissions::PermissionVerdict,
    {
        let mut reader = BufReader::new(response.into_reader());
        let mut state = StreamState::default();
        let mut data = String::new();
        let mut line = String::new();
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut waiting: Vec<WaitingAsk> = Vec::new();
        let mut last_event_id: Option<String> = None;
        let mut reconnect_attempts = 0u32;

        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            line.clear();
            let read = reader.read_line(&mut line).unwrap_or(0);
            if read == 0 {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                // The read timeout also fires during a legitimately silent tool.
                // Re-open the stream (resuming by SSE id when supported) instead
                // of ending live updates for the rest of the turn.
                if reconnect_attempts >= 10 {
                    return Err(Error::Msg(
                        "event stream was lost and did not resume".to_string(),
                    ));
                }
                reconnect_attempts += 1;
                let wait = Duration::from_millis(250 * reconnect_attempts as u64).min(Duration::from_secs(3));
                let until = Instant::now() + wait;
                while Instant::now() < until && !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                }
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                if let Ok(next) = self.open_event_stream(last_event_id.as_deref()) {
                    reader = BufReader::new(next.into_reader());
                }
                continue;
            }
            reconnect_attempts = 0;
            let trimmed = line.trim_end_matches(|c| c == '\r' || c == '\n');
            if !trimmed.is_empty() {
                if let Some(rest) = trimmed.strip_prefix("id:") {
                    last_event_id = Some(rest.trim().to_string());
                } else if let Some(rest) = trimmed.strip_prefix("data:") {
                    data.push_str(rest.trim_start());
                }
                continue;
            }
            if data.is_empty() {
                continue;
            }
            let envelope: Value = match serde_json::from_str(&data) {
                Ok(value) => value,
                Err(_) => {
                    data.clear();
                    continue;
                }
            };
            data.clear();

            // `/global/event` wraps the bus event in
            // `{ directory, project, payload: { type, properties } }`.
            let event = envelope.get("payload").unwrap_or(&envelope);
            let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
            let props = event.get("properties").unwrap_or(&Value::Null);
            if props.get("sessionID").and_then(Value::as_str)
                != Some(self.session_id.as_str())
            {
                continue;
            }

            if kind == "permission.asked" {
                if let Some(mut request) =
                    crate::permissions::parse_request(&self.app_id, &self.session_id, props)
                {
                    enrich_from_state(&mut request, &state);
                    let id = request.id.clone();
                    let call_id = request.call_id.clone();
                    let wants_target = crate::permissions::target_of(&request.metadata).is_none();
                    let waiting_request = request.clone();
                    let verdict =
                        on_permission(crate::permissions::PermissionBusEvent::Asked(request));
                    match verdict {
                        crate::permissions::PermissionVerdict::Once => {
                            if let Err(e) = self.reply_permission(&id, "once") {
                                eprintln!("permission auto-reply failed: {e}");
                            }
                        }
                        crate::permissions::PermissionVerdict::Reject => {
                            if let Err(e) = self.reply_permission(&id, "reject") {
                                eprintln!("permission reject failed: {e}");
                            }
                        }
                        crate::permissions::PermissionVerdict::Pending => {}
                    }
                    // The card is shown but the target is unknown: remember the
                    // ask so a later tool part can refresh it.
                    if verdict == crate::permissions::PermissionVerdict::Pending && wants_target {
                        if let Some(call_id) = call_id {
                            if waiting.len() >= MAX_WAITING_ASKS {
                                waiting.remove(0);
                            }
                            waiting.push(WaitingAsk {
                                call_id,
                                request: waiting_request,
                            });
                        }
                    }
                }
                continue;
            }
            if kind == "permission.replied" {
                if let Some(id) = props.get("requestID").and_then(Value::as_str) {
                    waiting.retain(|ask| ask.request.id != id);
                    on_permission(crate::permissions::PermissionBusEvent::Replied(
                        id.to_string(),
                    ));
                }
                continue;
            }

            let finished = match kind {
                "session.idle" => true,
                "session.status" => {
                    props.pointer("/status/type").and_then(Value::as_str) == Some("idle")
                }
                "session.error" => true,
                _ => apply_stream_event(&mut state, event),
            };

            if kind == "message.part.updated" && !waiting.is_empty() {
                for request in resolve_waiting_targets(&mut waiting, &state) {
                    on_permission(crate::permissions::PermissionBusEvent::Updated(request));
                }
            }

            if state.changed
                && (finished || last_emit.elapsed() >= Duration::from_millis(80))
            {
                emit(state.items());
                state.changed = false;
                last_emit = Instant::now();
            }
            if finished {
                break;
            }
        }
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

    /// Answers a pending permission request. `response` is `once`, `always` or
    /// `reject`; the platform broker only uses `once` and `reject` so grants
    /// stay revocable.
    pub fn reply_permission(&self, permission_id: &str, response: &str) -> Result<()> {
        let agent = agent();
        http(
            &agent,
            &self.base,
            &self.auth,
            "POST",
            &format!("/session/{}/permissions/{}", self.session_id, permission_id),
            Some(&json!({ "response": response })),
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
        let title = title_from_text(text);
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

    /// Repairs chats created before titles came from the first message: the
    /// old title is the app name, `chat`, or an empty string. Best-effort and
    /// runs on a worker thread; failures leave the existing title untouched.
    pub fn repair_generic_titles(&self) -> usize {
        let agent = agent();
        let Ok(raw) = http(&agent, &self.base, &self.auth, "GET", "/session", None) else {
            return 0;
        };
        let mut repaired = 0;
        for chat in parse_sessions(&raw) {
            if !is_generic_title(&chat.title, &self.app_id, &self.app_name) {
                continue;
            }
            let Ok(raw) = http(
                &agent,
                &self.base,
                &self.auth,
                "GET",
                &format!("/session/{}/message", chat.id),
                None,
            ) else {
                continue;
            };
            let items = parse_history(&raw);
            let Some(first) = items
                .iter()
                .find(|item| item.role == "user" && item.kind == "text" && !item.text.trim().is_empty())
            else {
                continue;
            };
            let title = title_from_text(&first.text);
            if title.is_empty() {
                continue;
            }
            if http(
                &agent,
                &self.base,
                &self.auth,
                "PATCH",
                &format!("/session/{}", chat.id),
                Some(&json!({ "title": title })),
            )
            .is_ok()
            {
                repaired += 1;
            }
        }
        repaired
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

    /// Cheap liveness probe: `/config` answers even while a tool runs, so a
    /// wedged event loop (no answers for several probes) can be told apart from
    /// a long but healthy turn.
    pub fn healthy(&self) -> bool {
        http(&health_agent(), &self.base, &self.auth, "GET", "/config", None).is_ok()
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

fn inlined_attachment_name(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("[attachment: ")?;
    let end = rest.find(']')?;
    let name = rest[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// `data:image/png;base64,AAAA` -> `AAAA`. Non-base64 or non-data URLs yield
/// `None`: the UI must not receive arbitrary remote references.
fn data_uri_base64(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("data:")?;
    let (meta, payload) = rest.split_once(',')?;
    if !meta.to_ascii_lowercase().contains(";base64") {
        return None;
    }
    if !meta.to_ascii_lowercase().starts_with("image/") {
        return None;
    }
    Some(payload)
}

/// Content address of an uploaded image. The history stores only this hash.
pub fn attachment_ref(payload: &str) -> String {
    hex::encode(Sha256::digest(payload.as_bytes()))
}

fn attachments_dir(home: &Path, app_id: &str) -> PathBuf {
    home.join("apps").join(app_id).join("attachments")
}

/// Saves an image attachment once per content hash. Returns `None` for
/// non-images and for payloads above the UI limit; callers treat it as best
/// effort and never fail the turn because the cache could not be written.
pub fn save_attachment_image(
    home: &Path,
    app_id: &str,
    mime: &str,
    payload: &str,
) -> Result<Option<String>> {
    const MAX_CACHED_IMAGE: usize = 8 * 1024 * 1024;
    if !mime.starts_with("image/") || payload.is_empty() || payload.len() > MAX_CACHED_IMAGE {
        return Ok(None);
    }
    let image_ref = attachment_ref(payload);
    let dir = attachments_dir(home, app_id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{image_ref}.json"));
    if !path.exists() {
        let body = json!({ "mime": mime, "data": payload });
        std::fs::write(&path, serde_json::to_vec(&body)?)?;
        prune_attachments(&dir);
    }
    Ok(Some(image_ref))
}

/// Reads a cached attachment image by its content hash.
pub fn load_attachment_image(home: &Path, app_id: &str, image_ref: &str) -> Result<Value> {
    let valid = image_ref.len() == 64 && image_ref.chars().all(|ch| ch.is_ascii_hexdigit());
    if !valid {
        return msg("invalid image reference");
    }
    let path = attachments_dir(home, app_id).join(format!("{image_ref}.json"));
    let raw = std::fs::read(&path)
        .map_err(|_| Error::Msg("attachment image is not cached anymore".to_string()))?;
    let value: Value = serde_json::from_slice(&raw)?;
    let mime = value.get("mime").and_then(Value::as_str).unwrap_or("");
    let data = value.get("data").and_then(Value::as_str).unwrap_or("");
    if !mime.starts_with("image/") || data.is_empty() {
        return msg("attachment image is corrupt");
    }
    Ok(value)
}

/// Keeps the attachment cache bounded; old pictures are dropped in favour of
/// recent ones and simply fall back to a text chip in the history.
fn prune_attachments(dir: &Path) {
    const MAX_CACHED_FILES: usize = 200;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect();
    if files.len() <= MAX_CACHED_FILES {
        return;
    }
    files.sort_by_key(|(modified, _)| *modified);
    let excess = files.len() - MAX_CACHED_FILES;
    for (_, path) in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

pub fn parse_history(raw: &Value) -> Vec<ChatItem> {
    let mut items = Vec::new();
    let Some(messages) = raw.as_array() else {
        return items;
    };
    // The history never carries image bytes: pictures are stored once in the
    // app's attachment cache and fetched on demand by the UI.
    for message in messages.iter() {
        let info = message.get("info");
        let role = info
            .and_then(|i| i.get("role"))
            .and_then(Value::as_str)
            .unwrap_or("assistant")
            .to_string();

        let mut text = String::new();
        let mut reasoning = String::new();
        let mut tools = Vec::new();
        let mut files: Vec<(String, String, Option<String>)> = Vec::new();
        if let Some(parts) = message.get("parts").and_then(Value::as_array) {
            for part in parts {
                match part.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(t) = part.get("text").and_then(Value::as_str) {
                            if let Some(name) = inlined_attachment_name(t) {
                                files.push((name.to_string(), "text/plain".to_string(), None));
                                continue;
                            }
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(t);
                        }
                    }
                    Some("file") => {
                        let filename = part
                            .get("filename")
                            .and_then(Value::as_str)
                            .filter(|name| !name.trim().is_empty())
                            .unwrap_or("вложение")
                            .to_string();
                        let mime = part
                            .get("mime")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        // OpenCode keeps the uploaded payload in `url` as a data
                        // URI. Store only a content hash: the UI fetches the
                        // picture through `chat_image` when it renders the item.
                        let image_ref = part
                            .get("url")
                            .and_then(Value::as_str)
                            .and_then(data_uri_base64)
                            .map(attachment_ref);
                        files.push((filename, mime, image_ref));
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
        let cache_read = tokens
            .and_then(|t| t.get("cache"))
            .and_then(|c| c.get("read"))
            .and_then(Value::as_u64);
        let cache_write = tokens
            .and_then(|t| t.get("cache"))
            .and_then(|c| c.get("write"))
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
            let mut item = ChatItem::new(&role, "text", clean_text(text));
            item.tokens_in = tokens_in;
            item.tokens_out = tokens_out;
            item.reasoning_tokens = reasoning_tokens;
            item.cache_read = cache_read;
            item.cache_write = cache_write;
            item.ms = ms;
            item.tok_s = tok_s;
            item.model = model;
            items.push(item);
        }
        for tool in tools {
            items.push(ChatItem::new("tool", "tool", tool));
        }
        for (filename, mime, image_ref) in files {
            let mut item = ChatItem::new(&role, "file", filename);
            item.mime = Some(mime).filter(|mime| !mime.is_empty());
            item.image_ref = image_ref;
            items.push(item);
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

fn is_generic_title(title: &str, app_id: &str, app_name: &str) -> bool {
    let title = title.trim();
    title.is_empty()
        || matches!(title, "chat" | "Диалог" | "Новый диалог")
        || title == app_id
        || title == app_name
}

fn title_from_text(text: &str) -> String {
    let mut title: String = text
        .lines()
        .next()
        .unwrap_or(text)
        .trim()
        .chars()
        .take(60)
        .collect();
    if title.is_empty() {
        title = "Новый диалог".to_string();
    }
    title
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn skills_keep_their_names_and_order() {
        let dir = std::env::temp_dir().join(format!("eco-engine-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("skills/beta")).unwrap();
        std::fs::create_dir_all(dir.join("skills/alpha")).unwrap();
        std::fs::write(dir.join("skills/beta/SKILL.md"), "beta body").unwrap();
        std::fs::write(dir.join("skills/alpha/SKILL.md"), "alpha body").unwrap();

        let skills = skill_texts(&dir);
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].0, "alpha");
        assert_eq!(skills[0].1, "alpha body");
        assert_eq!(skills[1].0, "beta");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stream_state_accumulates_parts() {
        let mut state = StreamState::default();
        let mut feed = |value: Value| apply_stream_event(&mut state, &value);

        feed(json!({
            "type": "message.updated",
            "properties": { "sessionID": "s", "info": { "id": "m", "role": "assistant" } }
        }));
        feed(json!({
            "type": "message.part.updated",
            "properties": { "sessionID": "s", "part": {
                "id": "r", "messageID": "m", "type": "reasoning", "text": "think"
            }}
        }));
        feed(json!({
            "type": "message.part.updated",
            "properties": { "sessionID": "s", "part": {
                "id": "t", "messageID": "m", "type": "text", "text": ""
            }}
        }));
        feed(json!({
            "type": "message.part.delta",
            "properties": { "sessionID": "s", "messageID": "m", "partID": "t", "field": "text", "delta": "hel" }
        }));
        feed(json!({
            "type": "message.part.delta",
            "properties": { "sessionID": "s", "messageID": "m", "partID": "t", "field": "text", "delta": "lo" }
        }));
        feed(json!({
            "type": "message.part.updated",
            "properties": { "sessionID": "s", "part": {
                "id": "x", "messageID": "m", "type": "tool", "tool": "doc_read",
                "state": { "status": "running" }
            }}
        }));

        let items = state.items();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, "reasoning");
        assert_eq!(items[0].text, "think");
        assert_eq!(items[1].kind, "text");
        assert_eq!(items[1].text, "hello");
        assert_eq!(items[2].text, "doc_read -> running");
    }

    #[test]
    fn parts_of_other_messages_are_ignored() {
        let mut state = StreamState::default();
        apply_stream_event(
            &mut state,
            &json!({
                "type": "message.updated",
                "properties": { "info": { "id": "m1", "role": "assistant" } }
            }),
        );
        apply_stream_event(
            &mut state,
            &json!({
                "type": "message.part.updated",
                "properties": { "part": {
                    "id": "p", "messageID": "m1", "type": "text", "text": "mine"
                }}
            }),
        );
        apply_stream_event(
            &mut state,
            &json!({
                "type": "message.part.updated",
                "properties": { "part": {
                    "id": "q", "messageID": "other", "type": "text", "text": "not mine"
                }}
            }),
        );
        let items = state.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "mine");
    }

    #[test]
    fn previous_messages_are_kept_in_live_items() {
        let mut state = StreamState::default();
        let mut feed = |value: Value| apply_stream_event(&mut state, &value);

        feed(json!({
            "type": "message.updated",
            "properties": { "info": { "id": "m1", "role": "assistant" } }
        }));
        feed(json!({
            "type": "message.part.updated",
            "properties": { "part": {
                "id": "r", "messageID": "m1", "type": "reasoning", "text": "first"
            }}
        }));
        feed(json!({
            "type": "message.part.updated",
            "properties": { "part": {
                "id": "x", "messageID": "m1", "type": "tool", "tool": "agg",
                "state": { "status": "running" }
            }}
        }));

        // A tool call starts a new assistant message; the earlier reasoning and
        // tool call must stay visible while the next answer streams.
        feed(json!({
            "type": "message.updated",
            "properties": { "info": { "id": "m2", "role": "assistant" } }
        }));
        feed(json!({
            "type": "message.part.updated",
            "properties": { "part": {
                "id": "t", "messageID": "m2", "type": "text", "text": "answer"
            }}
        }));

        let items = state.items();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, "reasoning");
        assert_eq!(items[0].text, "first");
        assert_eq!(items[1].text, "agg -> running");
        assert_eq!(items[2].kind, "text");
        assert_eq!(items[2].text, "answer");
    }

    fn start_assistant_message(state: &mut StreamState) {
        apply_stream_event(
            state,
            &json!({
                "type": "message.updated",
                "properties": { "sessionID": "s", "info": { "id": "m", "role": "assistant" } }
            }),
        );
    }

    fn tool_part(call_id: &str, input: Value) -> Value {
        json!({
            "id": "p1", "messageID": "m", "type": "tool", "tool": "write_file",
            "callID": call_id,
            "state": { "status": "running", "input": input }
        })
    }

    fn feed_tool_part(state: &mut StreamState, part: Value) {
        apply_stream_event(
            state,
            &json!({
                "type": "message.part.updated",
                "properties": { "sessionID": "s", "part": part }
            }),
        );
    }

    fn asked(request_id: &str, call_id: &str) -> crate::permissions::PermissionRequest {
        crate::permissions::parse_request(
            "doc-writer",
            "s",
            &json!({
                "id": request_id,
                "sessionID": "s",
                "permission": "platform_write_file",
                "patterns": ["*"],
                "always": ["*"],
                "tool": { "messageID": "m", "callID": call_id }
            }),
        )
        .unwrap()
    }

    #[test]
    fn permission_target_comes_from_the_tool_part() {
        let mut state = StreamState::default();
        start_assistant_message(&mut state);
        feed_tool_part(
            &mut state,
            tool_part(
                "call_1",
                json!({ "path": "about-elephants.md", "content": "SECRET BODY" }),
            ),
        );

        let mut request = asked("per_1", "call_1");
        assert_eq!(crate::permissions::target_of(&request.metadata), None);
        assert!(enrich_from_state(&mut request, &state));
        assert_eq!(
            crate::permissions::target_of(&request.metadata),
            Some("about-elephants.md")
        );
        assert_eq!(request.metadata["target"], json!("about-elephants.md"));
        assert!(!request.metadata.to_string().contains("SECRET BODY"));
    }

    #[test]
    fn two_path_tool_renders_input_to_output() {
        let part = tool_part(
            "call_2",
            json!({ "input": "report.md", "output": "report.docx" }),
        );
        assert_eq!(
            tool_target(&part).as_deref(),
            Some("report.md \u{2192} report.docx")
        );
    }

    #[test]
    fn target_falls_back_to_metadata_and_title() {
        let metadata_part = json!({
            "type": "tool",
            "state": { "metadata": { "filePath": "notes\\draft.md" } }
        });
        assert_eq!(tool_target(&metadata_part).as_deref(), Some("notes\\draft.md"));
        let title_part = json!({ "type": "tool", "state": { "title": "record  main.rs" } });
        assert_eq!(tool_target(&title_part).as_deref(), Some("record main.rs"));
    }

    #[test]
    fn tool_target_ignores_content_fields() {
        let part = tool_part(
            "call_4",
            json!({ "content": "big body", "newString": "x", "oldString": "y" }),
        );
        assert_eq!(tool_target(&part), None);
    }

    #[test]
    fn late_tool_part_refreshes_the_pending_request() {
        let mut state = StreamState::default();
        start_assistant_message(&mut state);
        let mut waiting = vec![WaitingAsk {
            call_id: "call_3".to_string(),
            request: asked("per_2", "call_3"),
        }];
        assert!(resolve_waiting_targets(&mut waiting, &state).is_empty());
        assert_eq!(waiting.len(), 1);

        feed_tool_part(&mut state, tool_part("call_3", json!({ "filePath": "doc.docx" })));
        let updated = resolve_waiting_targets(&mut waiting, &state);
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].id, "per_2");
        assert_eq!(
            crate::permissions::target_of(&updated[0].metadata),
            Some("doc.docx")
        );
        assert!(waiting.is_empty());
    }

    #[test]
    fn waiting_list_is_bounded() {
        let mut waiting: Vec<WaitingAsk> = Vec::new();
        for index in 0..(MAX_WAITING_ASKS + 2) {
            if waiting.len() >= MAX_WAITING_ASKS {
                waiting.remove(0);
            }
            let call_id = format!("call_{index}");
            waiting.push(WaitingAsk {
                call_id: call_id.clone(),
                request: asked(&format!("per_{index}"), &call_id),
            });
        }
        assert_eq!(waiting.len(), MAX_WAITING_ASKS);
        assert_eq!(waiting[0].request.id, "per_2");
    }

    #[test]
    fn model_entry_declares_capabilities() {
        let caps = crate::config::ModelCapabilities {
            vision: true,
            reasoning: true,
            reasoning_efforts: vec!["low".to_string(), "max".to_string()],
            context_tokens: Some(128_000),
            output_tokens: Some(32_000),
        };
        let entry = model_entry("vision-model", &caps);
        assert_eq!(entry["attachment"], json!(true));
        assert_eq!(entry["reasoning"], json!(true));
        assert_eq!(entry["tool_call"], json!(true));
        assert_eq!(entry["modalities"]["input"], json!(["text", "image"]));
        assert_eq!(entry["limit"]["context"], json!(128_000));
        assert_eq!(entry["limit"]["output"], json!(32_000));
        assert_eq!(entry["variants"]["max"]["reasoningEffort"], json!("max"));
        assert_eq!(entry["variants"]["low"]["reasoningEffort"], json!("low"));
    }

    #[test]
    fn partial_model_limits_are_completed() {
        let caps = crate::config::ModelCapabilities {
            context_tokens: Some(1_000_000),
            output_tokens: None,
            ..Default::default()
        };
        let entry = model_entry("gateway-model", &caps);
        assert_eq!(entry["limit"]["context"], json!(1_000_000));
        assert_eq!(entry["limit"]["output"], json!(DEFAULT_OUTPUT_TOKENS));

        let caps = crate::config::ModelCapabilities {
            context_tokens: None,
            output_tokens: Some(32_000),
            ..Default::default()
        };
        let entry = model_entry("gateway-model", &caps);
        assert_eq!(entry["limit"]["context"], json!(DEFAULT_CONTEXT_TOKENS));
        assert_eq!(entry["limit"]["output"], json!(32_000));
    }

    #[test]
    fn text_only_model_has_no_variants_or_limits() {
        let caps = crate::config::ModelCapabilities {
            reasoning: false,
            ..Default::default()
        };
        let entry = model_entry("text-model", &caps);
        assert_eq!(entry["attachment"], json!(false));
        assert_eq!(entry["reasoning"], json!(false));
        assert_eq!(entry["modalities"]["input"], json!(["text"]));
        assert_eq!(entry["variants"], json!({}));
        assert!(entry.get("limit").is_none());
    }

    #[test]
    fn image_attachment_requires_vision_and_checks_magic() {
        use base64::Engine as _;
        let png = base64::engine::general_purpose::STANDARD
            .encode(b"\x89PNG\r\n\x1a\nrest-of-file");
        let input = AttachmentInput {
            mime: "image/png".to_string(),
            filename: "shot.png".to_string(),
            data: png,
        };
        assert!(validate_attachments(std::slice::from_ref(&input), false).is_err());
        assert!(validate_attachments(std::slice::from_ref(&input), true).is_ok());

        let parts = build_parts("look", std::slice::from_ref(&input));
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1]["type"], json!("file"));
        assert!(parts[1]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));

        let fake = AttachmentInput {
            mime: "image/png".to_string(),
            filename: "fake.png".to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(b"not a png"),
        };
        assert!(validate_attachments(&[fake], true).is_err());
    }

    #[test]
    fn text_attachment_is_inlined_without_vision() {
        use base64::Engine as _;
        let input = AttachmentInput {
            mime: "text/plain".to_string(),
            filename: "notes.txt".to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(b"hello"),
        };
        assert!(validate_attachments(std::slice::from_ref(&input), false).is_ok());
        let parts = build_parts("", std::slice::from_ref(&input));
        assert_eq!(parts.len(), 1);
        let text = parts[0]["text"].as_str().unwrap();
        assert!(text.contains("[attachment: notes.txt]"));
        assert!(text.contains("hello"));
    }

    #[test]
    fn attachment_images_are_cached_by_hash() {
        let home = std::env::temp_dir().join(format!("eco-attach-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let payload = "iVBORw0KGgo=";
        let image_ref = save_attachment_image(&home, "demo", "image/png", payload)
            .unwrap()
            .expect("image should be cached");
        assert_eq!(image_ref, attachment_ref(payload));

        let loaded = load_attachment_image(&home, "demo", &image_ref).unwrap();
        assert_eq!(loaded["mime"], json!("image/png"));
        assert_eq!(loaded["data"], json!(payload));
        assert!(load_attachment_image(&home, "demo", "../../secrets").is_err());
        assert!(save_attachment_image(&home, "demo", "text/plain", payload)
            .unwrap()
            .is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn history_uses_image_refs_instead_of_payloads() {
        let raw = json!([{
            "info": { "role": "user" },
            "parts": [
                { "type": "file", "mime": "image/png", "filename": "shot.png",
                  "url": "data:image/png;base64,iVBORw0KGgo=" }
            ]
        }]);
        let items = parse_history(&raw);
        let file = items.iter().find(|item| item.kind == "file").unwrap();
        let expected = attachment_ref("iVBORw0KGgo=");
        assert_eq!(file.image_ref.as_deref(), Some(expected.as_str()));
        let serialized = serde_json::to_value(file).unwrap();
        assert!(serialized.get("data").is_none());
    }

    #[test]
    fn history_parses_file_parts() {
        let raw = json!([{
            "info": { "role": "user" },
            "parts": [
                { "type": "text", "text": "what is this" },
                { "type": "file", "mime": "image/png", "filename": "shot.png", "url": "data:..." }
            ]
        }]);
        let items = parse_history(&raw);
        assert!(items
            .iter()
            .any(|item| item.kind == "file"
                && item.text == "shot.png"
                && item.mime.as_deref() == Some("image/png")));
    }

    #[test]
    fn history_hides_inlined_attachment_body() {
        let raw = json!([{
            "info": { "role": "user" },
            "parts": [{ "type": "text", "text": "[attachment: notes.txt]\n\nsecret body" }]
        }]);
        let items = parse_history(&raw);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "file");
        assert_eq!(items[0].text, "notes.txt");
        assert!(!items.iter().any(|item| item.kind == "text"));
    }

    #[test]
    fn raw_tool_markup_is_replaced_with_a_notice() {
        assert_eq!(clean_text("обычный ответ".to_string()), "обычный ответ");
        let cleaned = clean_text("<||DSML|| calls><||DSML|| invoke name=\"bash\">".to_string());
        assert!(cleaned.contains("недоступный инструмент"));
        assert!(!cleaned.contains("DSML"));
    }

    #[test]
    fn legacy_chat_titles_are_recognized_and_derived() {
        assert!(is_generic_title("", "doc-writer", "Документы и Word"));
        assert!(is_generic_title("chat", "doc-writer", "Документы и Word"));
        assert!(is_generic_title("Диалог", "doc-writer", "Документы и Word"));
        assert!(is_generic_title("doc-writer", "doc-writer", "Документы и Word"));
        assert!(is_generic_title("Документы и Word", "doc-writer", "Документы и Word"));
        assert!(!is_generic_title("report.md → Word", "doc-writer", "Документы и Word"));
        assert_eq!(
            title_from_text("  Первая строка\nвторая"),
            "Первая строка"
        );
        assert_eq!(title_from_text("   \n"), "Новый диалог");
    }
}

