use crate::config::UserConfig;
use crate::manifest::Manifest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// How a tool stream is treated before a call is allowed to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    Allow,
    Ask,
    Deny,
}

impl Default for PermissionMode {
    fn default() -> Self {
        PermissionMode::Ask
    }
}

impl PermissionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionMode::Allow => "allow",
            PermissionMode::Ask => "ask",
            PermissionMode::Deny => "deny",
        }
    }
}

/// Default behaviour for effect classes. Reads are always allowed and are not
/// configurable here.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PermissionPolicy {
    #[serde(default = "ask")]
    pub write: PermissionMode,
    #[serde(default = "ask")]
    pub exec: PermissionMode,
}

fn ask() -> PermissionMode {
    PermissionMode::Ask
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self {
            write: PermissionMode::Ask,
            exec: PermissionMode::Ask,
        }
    }
}

/// A remembered "always allow" grant. Grants are intentionally owned by the
/// platform, not by the engine: revoking one takes effect on the next call
/// without restarting the sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGrant {
    pub id: String,
    pub app_id: String,
    /// Engine permission action, for example `platform_docx_export`.
    pub action: String,
    /// Human-readable tool name from the manifest.
    pub tool: String,
    /// `write` or `exec`.
    pub stream: String,
    pub app_version: String,
    pub workspace: String,
    pub mode: String,
    #[serde(default)]
    pub bin: Option<String>,
    #[serde(default)]
    pub bin_sha256: Option<String>,
    pub created: u64,
    #[serde(default)]
    pub expires: Option<u64>,
}

/// Per-app override. A missing field keeps following the global default, so
/// "как в профиле" survives later changes to `defaults`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PermissionOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<PermissionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exec: Option<PermissionMode>,
}

impl PermissionOverride {
    pub fn is_empty(&self) -> bool {
        self.write.is_none() && self.exec.is_none()
    }

    pub fn resolve(&self, defaults: &PermissionPolicy) -> PermissionPolicy {
        PermissionPolicy {
            write: self.write.unwrap_or(defaults.write),
            exec: self.exec.unwrap_or(defaults.exec),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionsConfig {
    #[serde(default)]
    pub defaults: PermissionPolicy,
    #[serde(default)]
    pub apps: BTreeMap<String, PermissionOverride>,
    #[serde(default)]
    pub grants: Vec<PermissionGrant>,
}

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub action: String,
    pub tool: String,
    pub stream: String,
    pub package_exec: bool,
    pub bin: Option<String>,
    pub bin_sha256: Option<String>,
}

impl ToolInfo {
    fn unknown(action: &str) -> Self {
        Self {
            action: action.to_string(),
            tool: action.strip_prefix("platform_").unwrap_or(action).to_string(),
            stream: String::new(),
            package_exec: false,
            bin: None,
            bin_sha256: None,
        }
    }

    pub fn can_always(&self) -> bool {
        self.stream == "write" || (self.stream == "exec" && self.package_exec)
    }

    pub fn can_session(&self) -> bool {
        self.stream == "write" || self.stream == "exec"
    }
}

/// Everything the broker needs for one app while a turn is running.
#[derive(Debug, Clone)]
pub struct PermissionContext {
    pub app_id: String,
    pub app_version: String,
    pub workspace: String,
    pub mode: String,
    pub policy: PermissionPolicy,
    pub tools: BTreeMap<String, ToolInfo>,
}

impl PermissionContext {
    pub fn load(home: &std::path::Path, app_id: &str) -> crate::error::Result<Self> {
        let cfg = crate::config::load(home);
        let app = crate::registry::verified_manifest(home, app_id)?;
        let workspace = crate::config::workspace(home, app_id)
            .display()
            .to_string();
        Ok(Self::from_parts(&app, &cfg, workspace))
    }

    pub fn from_parts(app: &Manifest, cfg: &UserConfig, workspace: String) -> Self {
        Self {
            app_id: app.app_id.clone(),
            app_version: app.version.clone(),
            workspace,
            mode: cfg.environment.as_str().to_string(),
            policy: policy_for(cfg, &app.app_id),
            tools: tool_map_with_memory(
                app,
                cfg.memory_apps.iter().any(|id| id == &app.app_id),
            ),
        }
    }

    pub fn info(&self, action: &str) -> ToolInfo {
        self.tools
            .get(action)
            .cloned()
            .unwrap_or_else(|| ToolInfo::unknown(action))
    }
}

pub fn policy_for(cfg: &UserConfig, app_id: &str) -> PermissionPolicy {
    cfg.permissions
        .apps
        .get(app_id)
        .map(|over| over.resolve(&cfg.permissions.defaults))
        .unwrap_or(cfg.permissions.defaults)
}

/// Headless runs (`--chat`, `--chat-async`) have no operator to answer an ask.
/// They default to the pre-broker behaviour, allowing tool calls; set
/// `ECO_PLATFORM_TOOL_POLICY=ask` to use the configured policy instead (the CLI
/// then rejects the asks, so an unattended run fails loudly instead of hanging).
pub fn headless_policy_override() -> Option<PermissionPolicy> {
    match std::env::var("ECO_PLATFORM_TOOL_POLICY").ok().as_deref() {
        Some("ask") | Some("default") => None,
        _ => Some(PermissionPolicy {
            write: PermissionMode::Allow,
            exec: PermissionMode::Allow,
        }),
    }
}

/// Resolves the policy for a concrete tool. A blanket `exec = allow` only
/// covers signed package tools; workspace exec still asks.
pub fn mode_for_tool(policy: &PermissionPolicy, info: &ToolInfo) -> PermissionMode {
    match info.stream.as_str() {
        "write" => policy.write,
        "exec" => {
            if policy.exec == PermissionMode::Allow && !info.package_exec {
                PermissionMode::Ask
            } else {
                policy.exec
            }
        }
        "read" | "skill" => PermissionMode::Allow,
        _ => PermissionMode::Ask,
    }
}

fn insert_tool(map: &mut BTreeMap<String, ToolInfo>, info: ToolInfo) {
    map.entry(info.action.clone()).or_insert(info);
}

/// Permission actions exposed by the platform MCP proxy. Mirrors
/// `proxy::gateway_tool_names`, with the effect class attached.
pub fn tool_map(app: &Manifest) -> BTreeMap<String, ToolInfo> {
    tool_map_with_memory(app, false)
}

pub fn tool_map_with_memory(app: &Manifest, memory_enabled: bool) -> BTreeMap<String, ToolInfo> {
    let mut map = BTreeMap::new();

    for (name, stream) in [
        ("artifact_list", "read"),
        ("artifact_search", "read"),
        ("artifact_read", "read"),
    ] {
        insert_tool(
            &mut map,
            ToolInfo {
                action: format!("platform_{name}"),
                tool: name.to_string(),
                stream: stream.to_string(),
                package_exec: false,
                bin: None,
                bin_sha256: None,
            },
        );
    }

    if memory_enabled {
        for (name, stream) in [
            ("memory_save", "write"),
            ("memory_search", "read"),
            ("memory_recent", "read"),
            ("memory_forget", "write"),
        ] {
            insert_tool(
                &mut map,
                ToolInfo {
                    action: format!("platform_{name}"),
                    tool: name.to_string(),
                    stream: stream.to_string(),
                    package_exec: false,
                    bin: None,
                    bin_sha256: None,
                },
            );
        }
    }

    if app
        .capabilities
        .file_scopes
        .iter()
        .any(|scope| scope == "workspace")
    {
        for (name, stream) in [
            ("list_files", "read"),
            ("read_file", "read"),
            ("write_file", "write"),
            ("append_file", "write"),
        ] {
            insert_tool(
                &mut map,
                ToolInfo {
                    action: format!("platform_{name}"),
                    tool: name.to_string(),
                    stream: stream.to_string(),
                    package_exec: false,
                    bin: None,
                    bin_sha256: None,
                },
            );
        }
    }

    for tool in &app.tools {
        let stream = if tool.stream.is_empty() {
            "read".to_string()
        } else {
            tool.stream.clone()
        };
        if let Some(bin) = &tool.bin {
            let name = crate::proxy::normalize_tool_name(&tool.tool);
            // Running a packaged binary is always an exec effect, whatever the
            // manifest declared: a "write" label must not hand an executable a
            // persistent grant or skip the exec confirmation. Hashes are keyed
            // with forward slashes (see `manifest::verify`).
            let bin_sha256 = app.hashes.get(&bin.replace('\\', "/")).cloned();
            insert_tool(
                &mut map,
                ToolInfo {
                    action: format!("platform_{name}"),
                    tool: tool.tool.clone(),
                    stream: "exec".to_string(),
                    package_exec: bin_sha256.is_some(),
                    bin: Some(bin.clone()),
                    bin_sha256,
                },
            );
            continue;
        }
        for connection in &app.connections {
            if connection.kind != "http-mcp" {
                continue;
            }
            if stream == "write" && !crate::manifest::external_write_allowed(app, &connection.id) {
                continue;
            }
            let name =
                crate::proxy::normalize_tool_name(&format!("{}_{}", connection.id, tool.tool));
            insert_tool(
                &mut map,
                ToolInfo {
                    action: format!("platform_{name}"),
                    tool: tool.tool.clone(),
                    stream: stream.clone(),
                    package_exec: false,
                    bin: None,
                    bin_sha256: None,
                },
            );
        }
    }

    insert_tool(
        &mut map,
        ToolInfo {
            action: "platform_skill".to_string(),
            tool: "skill".to_string(),
            stream: "read".to_string(),
            package_exec: false,
            bin: None,
            bin_sha256: None,
        },
    );

    map
}

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub id: String,
    pub app_id: String,
    pub session_id: String,
    pub action: String,
    pub patterns: Vec<String>,
    pub always: Vec<String>,
    pub metadata: Value,
    pub message_id: Option<String>,
    pub call_id: Option<String>,
    pub created: u64,
}

#[derive(Debug, Clone)]
pub enum PermissionBusEvent {
    Asked(PermissionRequest),
    /// The same request again, after the tool part revealed its target (the part
    /// can arrive after `permission.asked`). The card is refreshed; policy is
    /// not re-evaluated and nothing is audited.
    Updated(PermissionRequest),
    Replied(String),
}

/// Short human-readable target of a tool call (`metadata.target`), if present.
pub fn target_of(metadata: &Value) -> Option<&str> {
    metadata
        .get("target")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|target| !target.is_empty())
}

/// Adds `metadata.target` without touching other keys. An existing target wins,
/// so a value reported by the engine is never overwritten.
pub fn merge_target(metadata: &mut Value, target: &str) {
    let target = target.trim();
    if target.is_empty() || target_of(metadata).is_some() {
        return;
    }
    if !metadata.is_object() {
        *metadata = Value::Object(serde_json::Map::new());
    }
    if let Value::Object(map) = metadata {
        map.insert("target".to_string(), Value::String(target.to_string()));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionVerdict {
    Once,
    Reject,
    Pending,
}
pub fn parse_request(app_id: &str, session_id: &str, props: &Value) -> Option<PermissionRequest> {
    let id = props.get("id").and_then(Value::as_str)?.to_string();
    let action = props.get("permission").and_then(Value::as_str)?.to_string();
    let strings = |key: &str| {
        props
            .get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let tool = props.get("tool").unwrap_or(&Value::Null);
    Some(PermissionRequest {
        id,
        app_id: app_id.to_string(),
        session_id: session_id.to_string(),
        action,
        patterns: strings("patterns"),
        always: strings("always"),
        metadata: props.get("metadata").cloned().unwrap_or(Value::Null),
        message_id: tool
            .get("messageID")
            .and_then(Value::as_str)
            .map(str::to_string),
        call_id: tool
            .get("callID")
            .and_then(Value::as_str)
            .map(str::to_string),
        created: crate::artifacts::now_secs(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingPermission {
    pub id: String,
    pub app_id: String,
    pub session_id: String,
    pub action: String,
    pub tool: String,
    pub stream: String,
    pub patterns: Vec<String>,
    pub always: Vec<String>,
    pub metadata: Value,
    pub message_id: Option<String>,
    pub call_id: Option<String>,
    pub created: u64,
    pub can_session: bool,
    pub can_always: bool,
    pub package_exec: bool,
    pub bin: Option<String>,
    pub bin_sha256: Option<String>,
}

impl PendingPermission {
    pub fn enrich(req: PermissionRequest, ctx: &PermissionContext) -> Self {
        let info = ctx.info(&req.action);
        let can_session = info.can_session();
        let can_always = info.can_always();
        Self {
            id: req.id,
            app_id: req.app_id,
            session_id: req.session_id,
            action: req.action,
            tool: info.tool,
            stream: info.stream,
            patterns: req.patterns,
            always: req.always,
            metadata: req.metadata,
            message_id: req.message_id,
            call_id: req.call_id,
            created: req.created,
            can_session,
            can_always,
            package_exec: info.package_exec,
            bin: info.bin,
            bin_sha256: info.bin_sha256,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionGrant {
    pub app_id: String,
    pub session_id: String,
    pub action: String,
    pub created: u64,
}

pub fn session_grant_key(app_id: &str, session_id: &str, action: &str) -> String {
    format!("{app_id}:{session_id}:{action}")
}

static GRANT_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn new_grant(pending: &PendingPermission, ctx: &PermissionContext) -> Option<PermissionGrant> {
    if !pending.can_always {
        return None;
    }
    let now = crate::artifacts::now_secs();
    Some(PermissionGrant {
        id: format!(
            "pg_{:x}{:02x}",
            now,
            GRANT_SEQ.fetch_add(1, Ordering::Relaxed) & 0xff
        ),
        app_id: pending.app_id.clone(),
        action: pending.action.clone(),
        tool: pending.tool.clone(),
        stream: pending.stream.clone(),
        app_version: ctx.app_version.clone(),
        workspace: ctx.workspace.clone(),
        mode: ctx.mode.clone(),
        bin: pending.bin.clone(),
        bin_sha256: pending.bin_sha256.clone(),
        created: now,
        expires: None,
    })
}

pub fn grant_matches(
    grant: &PermissionGrant,
    pending: &PendingPermission,
    ctx: &PermissionContext,
    now: u64,
) -> bool {
    if grant.app_id != pending.app_id
        || grant.action != pending.action
        || grant.stream != pending.stream
        || grant.app_version != ctx.app_version
        || grant.workspace != ctx.workspace
        || grant.mode != ctx.mode
    {
        return false;
    }
    if grant.expires.map(|expires| now >= expires).unwrap_or(false) {
        return false;
    }
    if grant.bin_sha256 != pending.bin_sha256 {
        return false;
    }
    pending.can_always
}

pub fn session_grant_matches(grant: &SessionGrant, pending: &PendingPermission) -> bool {
    grant.app_id == pending.app_id
        && grant.session_id == pending.session_id
        && grant.action == pending.action
        && pending.can_session
}

pub fn upsert_grant(cfg: &mut UserConfig, grant: PermissionGrant) {
    cfg.permissions
        .grants
        .retain(|existing| !(existing.app_id == grant.app_id && existing.action == grant.action));
    cfg.permissions.grants.push(grant);
}

pub fn revoke_app_grants(home: &std::path::Path, app_id: &str) {
    let mut cfg = crate::config::load(home);
    let before = cfg.permissions.grants.len();
    cfg.permissions
        .grants
        .retain(|grant| grant.app_id != app_id);
    if cfg.permissions.grants.len() != before {
        let _ = crate::config::save(home, &cfg);
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Manifest;

    fn manifest() -> Manifest {
        serde_json::from_value(serde_json::json!({
            "schema_version": 2,
            "app_id": "doc-writer",
            "name": "Docs",
            "version": "1.2.3",
            "profile": { "system_prompt": "prompt/system.md" },
            "hashes": { "bin/docx-export.exe": "sha256:abc" },
            "capabilities": { "file_scopes": ["workspace"], "external_writes": ["ctx"] },
            "connections": [
                { "id": "ctx", "type": "http-mcp" }
            ],
            "tools": [
                { "tool": "docx_export", "bin": "bin/docx-export.exe", "stream": "exec" },
                { "tool": "search", "stream": "read" },
                { "tool": "write", "stream": "write" }
            ]
        }))
        .unwrap()
    }

    fn cfg() -> UserConfig {
        UserConfig::default()
    }

    fn pending(action: &str) -> PendingPermission {
        PendingPermission {
            id: "per_1".into(),
            app_id: "doc-writer".into(),
            session_id: "ses_1".into(),
            action: action.into(),
            tool: "docx_export".into(),
            stream: "exec".into(),
            patterns: vec!["*".into()],
            always: vec!["*".into()],
            metadata: Value::Null,
            message_id: None,
            call_id: None,
            created: 10,
            can_session: true,
            can_always: true,
            package_exec: true,
            bin: Some("bin/docx-export.exe".into()),
            bin_sha256: Some("sha256:abc".into()),
        }
    }

    #[test]
    fn defaults_are_ask_and_app_override_wins() {
        let mut cfg = cfg();
        assert_eq!(cfg.permissions.defaults.write, PermissionMode::Ask);
        assert_eq!(cfg.permissions.defaults.exec, PermissionMode::Ask);
        cfg.permissions.apps.insert(
            "doc-writer".into(),
            PermissionOverride {
                write: Some(PermissionMode::Allow),
                exec: Some(PermissionMode::Deny),
            },
        );
        let policy = policy_for(&cfg, "doc-writer");
        assert_eq!(policy.write, PermissionMode::Allow);
        assert_eq!(policy.exec, PermissionMode::Deny);
    }

    #[test]
    fn partial_override_follows_the_global_default() {
        let mut cfg = cfg();
        cfg.permissions.defaults.exec = PermissionMode::Deny;
        cfg.permissions.apps.insert(
            "doc-writer".into(),
            PermissionOverride {
                write: Some(PermissionMode::Allow),
                exec: None,
            },
        );
        let policy = policy_for(&cfg, "doc-writer");
        assert_eq!(policy.write, PermissionMode::Allow);
        assert_eq!(
            policy.exec,
            PermissionMode::Deny,
            "an unset field must keep inheriting the profile"
        );

        cfg.permissions.defaults.exec = PermissionMode::Allow;
        assert_eq!(policy_for(&cfg, "doc-writer").exec, PermissionMode::Allow);
    }

    #[test]
    fn binary_tool_is_exec_even_when_manifest_says_write() {
        let mut app = manifest();
        app.tools[0].stream = "write".into();
        let map = tool_map(&app);
        let tool = map.get("platform_docx_export").unwrap();
        assert_eq!(tool.stream, "exec");
        assert!(tool.package_exec, "a hash-covered binary is a package tool");
        assert_eq!(
            mode_for_tool(
                &PermissionPolicy {
                    write: PermissionMode::Allow,
                    exec: PermissionMode::Ask,
                },
                tool
            ),
            PermissionMode::Ask,
            "write=allow must not run an executable without the exec policy"
        );
    }

    #[test]
    fn binary_hash_lookup_normalizes_windows_paths() {
        let mut app = manifest();
        app.tools[0].bin = Some("bin\\docx-export.exe".into());
        let map = tool_map(&app);
        let tool = map.get("platform_docx_export").unwrap();
        assert!(
            tool.package_exec,
            "a backslash manifest path must still match its forward-slash hash"
        );
        assert_eq!(tool.bin_sha256.as_deref(), Some("sha256:abc"));
    }

    #[test]
    fn package_exec_is_always_allowed_but_workspace_exec_is_not() {
        let map = tool_map(&manifest());
        let package = map.get("platform_docx_export").unwrap();
        assert!(package.package_exec);
        assert!(package.can_always());

        let file_write = map.get("platform_write_file").unwrap();
        assert!(file_write.can_always());
        assert!(!file_write.package_exec);

        let read = map.get("platform_read_file").unwrap();
        assert!(!read.can_always());
    }

    #[test]
    fn grant_is_bound_to_version_workspace_mode_and_binary_hash() {
        let app = manifest();
        let mut cfg = cfg();
        let ctx = PermissionContext::from_parts(&app, &cfg, "D:\\ws".into());
        let grant = new_grant(&pending("platform_docx_export"), &ctx).unwrap();
        assert!(grant_matches(&grant, &pending("platform_docx_export"), &ctx, 11));

        let mut other = ctx.clone();
        other.workspace = "D:\\other".into();
        assert!(!grant_matches(&grant, &pending("platform_docx_export"), &other, 11));

        other = ctx.clone();
        other.app_version = "9.9.9".into();
        assert!(!grant_matches(&grant, &pending("platform_docx_export"), &other, 11));

        other = ctx.clone();
        other.mode = "local".into();
        assert!(!grant_matches(&grant, &pending("platform_docx_export"), &other, 11));

        let mut stale = grant.clone();
        stale.bin_sha256 = Some("sha256:other".into());
        assert!(!grant_matches(&stale, &pending("platform_docx_export"), &ctx, 11));

        let mut expired = grant.clone();
        expired.expires = Some(11);
        assert!(!grant_matches(&expired, &pending("platform_docx_export"), &ctx, 11));

        // Avoid an unused warning in this test; the helper is exercised below.
        cfg.permissions.defaults.write = PermissionMode::Ask;
    }

    #[test]
    fn mcp_tool_actions_are_namespaced() {
        let map = tool_map(&manifest());
        assert_eq!(map.get("platform_ctx_search").unwrap().stream, "read");
        assert_eq!(map.get("platform_ctx_write").unwrap().stream, "write");
        assert_eq!(
            map.get("platform_ctx_write").unwrap().tool, "write",
            "display name comes from the manifest tool"
        );
    }

    #[test]
    fn blanket_exec_allow_still_asks_for_workspace_exec() {
        let map = tool_map(&manifest());
        let mut policy = PermissionPolicy {
            write: PermissionMode::Ask,
            exec: PermissionMode::Allow,
        };
        let package = map.get("platform_docx_export").unwrap();
        assert_eq!(mode_for_tool(&policy, package), PermissionMode::Allow);

        let mut workspace_exec = package.clone();
        workspace_exec.package_exec = false;
        workspace_exec.stream = "exec".into();
        assert_eq!(
            mode_for_tool(&policy, &workspace_exec),
            PermissionMode::Ask,
            "a blanket allow must not cover workspace executables"
        );

        policy.exec = PermissionMode::Deny;
        assert_eq!(mode_for_tool(&policy, package), PermissionMode::Deny);
    }

    #[test]
    fn parses_permission_asked_payload() {
        let props = serde_json::json!({
            "id": "per_1",
            "sessionID": "ses_1",
            "permission": "platform_docx_export",
            "patterns": ["*"],
            "always": ["*"],
            "metadata": { "path": "report.docx" },
            "tool": { "messageID": "msg_1", "callID": "prt_1" }
        });
        let req = parse_request("doc-writer", "ses_1", &props).unwrap();
        assert_eq!(req.action, "platform_docx_export");
        assert_eq!(req.patterns, vec!["*"]);
        assert_eq!(req.message_id.as_deref(), Some("msg_1"));
        assert_eq!(req.call_id.as_deref(), Some("prt_1"));
    }

    #[test]
    fn unknown_tool_has_no_persistent_grant() {
        let app = manifest();
        let cfg = cfg();
        let ctx = PermissionContext::from_parts(&app, &cfg, "D:\\ws".into());
        let mut req = pending("platform_no_such");
        req.can_always = false;
        req.can_session = false;
        let pending = PendingPermission::enrich(
            PermissionRequest {
                id: "per_1".into(),
                app_id: "doc-writer".into(),
                session_id: "ses_1".into(),
                action: "platform_no_such".into(),
                patterns: vec![],
                always: vec![],
                metadata: Value::Null,
                message_id: None,
                call_id: None,
                created: 1,
            },
            &ctx,
        );
        assert!(!pending.can_always);
        assert!(new_grant(&pending, &ctx).is_none());
    }
}



