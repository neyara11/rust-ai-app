pub mod artifacts;
pub mod allowlist;
pub mod commands;
pub mod config;
pub mod engine;
pub mod error;
pub mod identity;
pub mod manifest;
pub mod mcp;
pub mod proxy;
pub mod registry;
pub mod secrets;
pub mod state;

use secrets::SecretStore;
use state::AppState;

pub fn run() {
    let state = AppState::new();
    startup_sweep(&state.home);

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::platform_info,
            commands::list_apps,
            commands::install_app,
            commands::remove_app,
            commands::app_history,
            commands::rollback_app,
            commands::get_config,
            commands::save_config,
            commands::probe_endpoint,
            commands::secrets_list,
            commands::secret_store,
            commands::secret_delete,
            commands::open_app,
            commands::close_app,
            commands::choose_workspace,
            commands::artifacts_usage,
            commands::cleanup_artifacts,
            commands::mcp_servers,
            commands::mcp_server_get,
            commands::mcp_server_save,
            commands::mcp_server_delete,
            commands::mcp_probe,
            commands::mcp_call,
            commands::audit_tail,
            commands::chat_open,
            commands::chat_send,
            commands::chat_list,
            commands::chat_new,
            commands::chat_select,
            commands::chat_abort,
            commands::chat_status,
            commands::chat_delete,
            commands::chat_history,
            commands::chat_reset,
            commands::chat_stop,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ECO AIOps platform");
}

/// Removes leftover staging directories and expired or orphaned session data.
pub fn startup_sweep(home: &std::path::Path) {
    identity::sweep(home);
    let policy = config::policy(home);
    for app in registry::list(home) {
        let root = artifacts::sessions_root(home, &app.app_id);
        let _ = artifacts::sweep(&root, &policy);
    }
    let _ = std::fs::remove_dir_all(home.join("staging"));
}

/// Headless verification of a package directory or zip. Used by CI and by
/// `--verify` on the built binary; no window is created.
pub fn cli_verify(arg: Option<&String>) -> i32 {
    let Some(path) = arg else {
        eprintln!("usage: eco-platform --verify <package-dir|package.zip>");
        return 2;
    };
    let home = registry::home();
    let src = std::path::PathBuf::from(path);
    let (dir, _temp) = match prepare_package(&home, &src) {
        Ok(v) => v,
        Err(e) => {
            println!("{{ \"ok\": false, \"problems\": [\"{e}\"] }}");
            return 1;
        }
    };
    let trusted = registry::load_trusted_keys(&home);
    let report = manifest::verify_package(&dir, &trusted);
    let out = serde_json::json!({ "ok": report.ok, "problems": report.problems });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    if report.ok {
        0
    } else {
        1
    }
}

fn prepare_package(
    home: &std::path::Path,
    src: &std::path::Path,
) -> Result<(std::path::PathBuf, Option<std::path::PathBuf>), String> {
    if src.is_dir() {
        return Ok((src.to_path_buf(), None));
    }
    let staging = std::env::temp_dir().join(format!("eco-verify-{}", artifacts::now_secs()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let _ = home;
    registry::extract_zip_public(src, &staging).map_err(|e| e.to_string())?;
    Ok((staging.clone(), Some(staging)))
}

/// Headless install, same fail-closed path as the UI. Used by CI and pilots.
pub fn cli_install(arg: Option<&String>) -> i32 {
    let Some(path) = arg else {
        eprintln!("usage: eco-platform --install <package-dir|package.zip>");
        return 2;
    };
    let home = registry::home();
    let src = std::path::PathBuf::from(path);
    match registry::install(&home, &src) {
        Ok(app) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&app).unwrap_or_default()
            );
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// Lists installed packages with verification status.
pub fn cli_list() -> i32 {
    let home = registry::home();
    let apps = registry::list(&home);
    println!(
        "{}",
        serde_json::to_string_pretty(&apps).unwrap_or_default()
    );
    0
}

/// `--history <app-id>`: archived package versions, newest first.
pub fn cli_history(arg: Option<&String>) -> i32 {
    let Some(app_id) = arg else {
        eprintln!("usage: eco-platform --history <app-id>");
        return 2;
    };
    let home = registry::home();
    let entries = registry::history(&home, app_id);
    println!(
        "{}",
        serde_json::to_string_pretty(&entries).unwrap_or_default()
    );
    0
}

/// `--rollback <app-id> [version]`: explicit downgrade to an archived version.
pub fn cli_rollback(app: Option<&String>, version: Option<&String>) -> i32 {
    let Some(app_id) = app else {
        eprintln!("usage: eco-platform --rollback <app-id> [version]");
        return 2;
    };
    let home = registry::home();
    match registry::rollback(&home, app_id, version.map(String::as_str)) {
        Ok(app) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&app).unwrap_or_default()
            );
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// `--check <package-dir|package.zip>`: compares a package with the installed
/// version without touching the profile.
pub fn cli_check(arg: Option<&String>) -> i32 {
    let Some(path) = arg else {
        eprintln!("usage: eco-platform --check <package-dir|package.zip>");
        return 2;
    };
    let home = registry::home();
    let src = std::path::PathBuf::from(path);
    let (dir, _temp) = match prepare_package(&home, &src) {
        Ok(v) => v,
        Err(e) => {
            println!("{{ \"ok\": false, \"problems\": [\"{e}\"] }}");
            return 1;
        }
    };
    let trusted = registry::load_trusted_keys(&home);
    let report = manifest::verify_package(&dir, &trusted);
    if !report.ok {
        let out = serde_json::json!({ "ok": false, "problems": report.problems });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return 1;
    }
    let package = match manifest::load(&dir) {
        Ok(m) => m,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            return 1;
        }
    };
    let installed_version = registry::find(&home, &package.app_id)
        .and_then(|p| manifest::load(&p).ok())
        .map(|m| m.version);
    let status = match &installed_version {
        None => "new",
        Some(current) => {
            let current = manifest::parse_version(current);
            let next = manifest::parse_version(&package.version);
            match (current, next) {
                (Some(a), Some(b)) if b > a => "update",
                (Some(a), Some(b)) if b < a => "downgrade",
                _ => "same",
            }
        }
    };
    let allow = allowlist::enforce(
        &home,
        &registry::load_trusted_keys(&home),
        &package.app_id,
        &package.version,
    );
    let out = serde_json::json!({
        "ok": true,
        "app_id": package.app_id,
        "name": package.name,
        "package_version": package.version,
        "installed_version": installed_version,
        "status": status,
        "allowlist_ok": allow.is_ok(),
        "allowlist_problem": allow.err().map(|e| e.to_string())
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    0
}



/// Headless MCP probe: `--mcp-probe <url>`.
pub fn cli_mcp_probe(url: Option<&String>) -> i32 {
    let Some(url) = url else {
        eprintln!("usage: eco-platform --mcp-probe <url>");
        return 2;
    };
    let def = mcp::ServerDef {
        id: "cli".to_string(),
        url: url.clone(),
        token: std::env::var("MCP_TOKEN").ok(),
    };
    match mcp::McpSession::connect(def).and_then(|s| {
        let tools = s.list_tools()?;
        Ok(serde_json::json!({
            "protocol": s.protocol,
            "server": s.server,
            "tools": tools.iter().map(|t| t.get("name").cloned().unwrap_or(serde_json::Value::Null)).collect::<Vec<_>>()
        }))
    }) {
        Ok(out) => {
            println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// Headless MCP call: `--mcp-call <url> <tool> <json-args>`.
pub fn cli_mcp_call(url: Option<&String>, tool: Option<&String>, args: Option<&String>) -> i32 {
    let (Some(url), Some(tool)) = (url, tool) else {
        eprintln!("usage: eco-platform --mcp-call <url> <tool> '<json-args>'");
        return 2;
    };
    let arguments: serde_json::Value = match args {
        Some(raw) => {
            let text = if let Some(path) = raw.strip_prefix('@') {
                std::fs::read_to_string(path).map_err(|e| format!("cannot read args file: {e}"))
            } else {
                Ok(raw.clone())
            };
            match text.and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string())) {
                Ok(value) => value,
                Err(e) => {
                    eprintln!("invalid json args: {e}");
                    return 2;
                }
            }
        }
        None => serde_json::json!({}),
    };

    let def = mcp::ServerDef {
        id: "cli".to_string(),
        url: url.clone(),
        token: std::env::var("MCP_TOKEN").ok(),
    };
    match mcp::McpSession::connect(def).and_then(|s| s.call(tool, arguments)) {
        Ok(value) => {
            let (text, truncated, bytes) = mcp::capped(&value);
            if truncated {
                eprintln!("note: response truncated, {bytes} bytes total");
            }
            println!("{text}");
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// Full platform path for a tool call: signed manifest allow-list, server
/// definition from DPAPI, size cap, audit record.
pub fn mcp_call_for_app(
    home: &std::path::Path,
    app_id: &str,
    server_id: &str,
    tool: &str,
    arguments: serde_json::Value,
) -> error::Result<(usize, bool, String)> {
    let app = registry::verified_manifest(home, app_id)?;
    let Some(definition) = manifest::find_tool(&app, tool) else {
        return error::msg(format!(
            "tool {tool} is not in the allow-list of {app_id}"
        ));
    };
    if definition.stream == "write" && !manifest::external_writes_allowed(&app) {
        return error::msg(format!(
            "tool {tool} is a write tool and {app_id} declares no external writes"
        ));
    }
    if definition.stream == "read" && manifest::external_writes_allowed(&app) {
        // write-capable apps still get read tools; nothing to do here
    }

    let started = std::time::Instant::now();
    let def = mcp::get(&SecretStore::new(home), server_id)?;
    let session = mcp::McpSession::connect(def)?;
    let value = session.call(tool, arguments)?;
    let (text, truncated, bytes) = mcp::capped(&value);

    commands::audit(
        home,
        app_id,
        serde_json::json!({
            "ts": artifacts::now_secs(),
            "app_id": app_id,
            "server_id": server_id,
            "tool": tool,
            "bytes": bytes,
            "truncated": truncated,
            "took_ms": started.elapsed().as_millis() as u64
        }),
    );

    Ok((bytes, truncated, text))
}

/// `--mcp-add <id> <url> [token]`: stores a server definition in DPAPI.
pub fn cli_mcp_add(id: Option<&String>, url: Option<&String>, token: Option<&String>) -> i32 {
    let (Some(id), Some(url)) = (id, url) else {
        eprintln!("usage: eco-platform --mcp-add <id> <url> [token]");
        return 2;
    };
    let home = registry::home();
    let def = mcp::ServerDef {
        id: id.clone(),
        url: url.clone(),
        token: token.cloned(),
    };
    match mcp::put(&SecretStore::new(&home), &def) {
        Ok(()) => {
            println!("stored mcp server {id} in DPAPI");
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// `--mcp-call-app <app-id> <server-id> <tool> <json-args>`.
pub fn cli_mcp_call_app(
    app: Option<&String>,
    server: Option<&String>,
    tool: Option<&String>,
    args: Option<&String>,
) -> i32 {
    let (Some(app), Some(server), Some(tool)) = (app, server, tool) else {
        eprintln!("usage: eco-platform --mcp-call-app <app-id> <server-id> <tool> '<json-args>'");
        return 2;
    };
    let arguments: serde_json::Value = match args {
        Some(raw) => {
            let text = if let Some(path) = raw.strip_prefix('@') {
                std::fs::read_to_string(path).map_err(|e| format!("cannot read args file: {e}"))
            } else {
                Ok(raw.clone())
            };
            match text.and_then(|t| {
                serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string())
            }) {
                Ok(value) => value,
                Err(e) => {
                    eprintln!("invalid json args: {e}");
                    return 2;
                }
            }
        }
        None => serde_json::json!({}),
    };

    match mcp_call_for_app(&registry::home(), app, server, tool, arguments) {
        Ok((bytes, truncated, text)) => {
            println!("{text}");
            if truncated {
                eprintln!("note: truncated, {bytes} bytes total");
            }
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// `--trust <trusted.pub>`: installs the package signing trust anchor.
/// Machine-wide with elevation; a release build refuses without admin rights.
pub fn cli_trust(arg: Option<&String>) -> i32 {
    let Some(path) = arg else {
        eprintln!("usage: eco-platform --trust <trusted.pub>");
        return 2;
    };
    let home = registry::home();
    match registry::install_trust_anchor(&home, &std::path::PathBuf::from(path)) {
        Ok(info) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&info).unwrap_or_default()
            );
            if info.source == "dev" {
                eprintln!("warning: dev anchor in the user profile; release builds ignore it");
            }
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// `--anchor`: shows which trust anchors the platform would use.
pub fn cli_anchor() -> i32 {
    let home = registry::home();
    let keys = registry::trust_key_info(&home);
    let anchor = registry::trusted_anchor(&home);
    let fingerprints = registry::anchor_fingerprints(&home);
    let allow = allowlist::info(&home, &registry::load_trusted_keys(&home));
    let out = if keys.is_empty() {
        serde_json::json!({
            "ok": false,
            "source": "none",
            "anchors": [],
            "allowlist": allow,
            "dev_mode": registry::dev_mode(),
            "elevated": registry::is_elevated()
        })
    } else {
        serde_json::json!({
            "ok": true,
            "path": anchor.as_ref().map(|(path, _)| path.clone()),
            "source": anchor.map(|(_, source)| source.as_str()),
            "fingerprint": fingerprints.first(),
            "fingerprints": fingerprints,
            "anchors": keys,
            "allowlist": allow,
            "dev_mode": registry::dev_mode(),
            "elevated": registry::is_elevated()
        })
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    if out["ok"].as_bool().unwrap_or(false) {
        0
    } else {
        1
    }
}

/// `--untrust <fingerprint|public-key|all> [--force]`: removes an active anchor.
pub fn cli_untrust(target: Option<&String>, force: Option<&String>) -> i32 {
    let Some(target) = target else {
        eprintln!("usage: eco-platform --untrust <fingerprint|public-key|all> [--force]");
        return 2;
    };
    let home = registry::home();
    let force = force.map(|value| value == "--force").unwrap_or(false);
    match registry::untrust_anchor(&home, target, force) {
        Ok(info) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&info).unwrap_or_default()
            );
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// `--allowlist`: shows the resolved allowlist document and its status.
pub fn cli_allowlist() -> i32 {
    let home = registry::home();
    let keys = registry::load_trusted_keys(&home);
    let info = allowlist::info(&home, &keys);
    println!(
        "{}",
        serde_json::to_string_pretty(&info).unwrap_or_default()
    );
    if info.valid() {
        0
    } else {
        1
    }
}

/// `--allowlist-verify <allowlist.json>`: checks a signed document before rollout.
pub fn cli_allowlist_verify(arg: Option<&String>) -> i32 {
    let Some(path) = arg else {
        eprintln!("usage: eco-platform --allowlist-verify <allowlist.json>");
        return 2;
    };
    let home = registry::home();
    let keys = registry::load_trusted_keys(&home);
    let info = allowlist::inspect_file(&std::path::PathBuf::from(path), "file", &keys);
    println!(
        "{}",
        serde_json::to_string_pretty(&info).unwrap_or_default()
    );
    if info.status == "valid" {
        0
    } else {
        1
    }
}

/// `--allowlist-check <app-id> [version]`: evaluates one install/open decision.
pub fn cli_allowlist_check(app: Option<&String>, version: Option<&String>) -> i32 {
    let Some(app_id) = app else {
        eprintln!("usage: eco-platform --allowlist-check <app-id> [version]");
        return 2;
    };
    let home = registry::home();
    let keys = registry::load_trusted_keys(&home);
    let installed = registry::find(&home, app_id)
        .and_then(|path| manifest::load(&path).ok())
        .map(|m| m.version);
    let version = version.cloned().or(installed).unwrap_or_default();
    match allowlist::enforce(&home, &keys, app_id, &version) {
        Ok(()) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "app_id": app_id,
                    "version": version,
                    "decision": "allowed"
                }))
                .unwrap_or_default()
            );
            0
        }
        Err(e) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": false,
                    "app_id": app_id,
                    "version": version,
                    "decision": "denied",
                    "problem": e.to_string()
                }))
                .unwrap_or_default()
            );
            1
        }
    }
}


/// `--chat <app-id> <message> [low|medium|high]`: headless smoke test.
pub fn cli_chat(
    app_id: Option<&String>,
    message: Option<&String>,
    level: Option<&String>,
) -> i32 {
    let (Some(app_id), Some(message)) = (app_id, message) else {
        eprintln!("usage: eco-platform --chat <app-id> <message> [low|medium|high]");
        return 2;
    };
    let home = registry::home();
    let policy = config::policy(&home);
    let root = artifacts::sessions_root(&home, app_id);
    let dir = match artifacts::open_session(&root, &policy) {
        Ok(d) => d,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            return 1;
        }
    };
    let engine = match engine::Engine::start(app_id, &home, &config::workspace(&home, app_id)) {
        Ok(e) => e,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            artifacts::close_session(&dir);
            return 1;
        }
    };
    println!("model: {}", engine.info().model);
    let session = engine.snapshot();
    let started = std::time::Instant::now();
    let result = session
        .prompt(message, level.map(|l| l.as_str()))
        .and_then(|_| session.history());
    drop(engine);
    artifacts::close_session(&dir);
    match result {
        Ok(items) => {
            for item in items {
                let stats = item
                    .tokens_out
                    .map(|out| {
                        let speed = item.tok_s.unwrap_or(0.0);
                        let ms = item.ms.unwrap_or(0);
                        format!(" (out {out}, {speed:.0} tok/s, {ms} ms)")
                    })
                    .unwrap_or_default();
                if item.kind == "reasoning" {
                    println!("[reasoning] {} chars", item.text.chars().count());
                } else {
                    println!("[{}]{stats} {}", item.role, item.text);
                }
            }
            println!("total: {} ms", started.elapsed().as_millis());
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}
/// `--secret-add <name> <value>`: stores a secret in DPAPI (dev helper).
pub fn cli_secret_add(name: Option<&String>, value: Option<&String>) -> i32 {
    let (Some(name), Some(value)) = (name, value) else {
        eprintln!("usage: eco-platform --secret-add <name> <value>");
        return 2;
    };
    let home = registry::home();
    match SecretStore::new(&home).store(name, value) {
        Ok(()) => {
            println!("secret {name} stored");
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// `--import-auth <secret-name> <auth.json> <provider>`: copies a provider key
/// from an existing local auth file into DPAPI without printing it.
pub fn cli_import_auth(
    name: Option<&String>,
    path: Option<&String>,
    provider: Option<&String>,
) -> i32 {
    let (Some(name), Some(path), Some(provider)) = (name, path, provider) else {
        eprintln!("usage: eco-platform --import-auth <secret-name> <auth.json> <provider>");
        return 2;
    };
    let parsed: Result<serde_json::Value, _> = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|raw| serde_json::from_str(&raw).map_err(|e| e.to_string()));
    let value = match parsed {
        Ok(v) => v,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            return 1;
        }
    };
    let key = value
        .get(provider)
        .and_then(|p| {
            p.get("key")
                .or_else(|| p.get("apiKey"))
                .or_else(|| p.get("access"))
        })
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let Some(key) = key else {
        println!("{{ \"ok\": false, \"problem\": \"provider {provider} has no key field\" }}");
        return 1;
    };
    match SecretStore::new(&registry::home()).store(name, &key) {
        Ok(()) => {
            println!("secret {name} imported from provider {provider} ({} chars)", key.len());
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// `--chat-async <app-id> <message>`: verifies live streaming through `/event`
/// and the final history handoff.
pub fn cli_chat_async(app_id: Option<&String>, message: Option<&String>) -> i32 {
    let (Some(app_id), Some(message)) = (app_id, message) else {
        eprintln!("usage: eco-platform --chat-async <app-id> <message>");
        return 2;
    };
    let home = registry::home();
    let policy = config::policy(&home);
    let root = artifacts::sessions_root(&home, app_id);
    let dir = match artifacts::open_session(&root, &policy) {
        Ok(d) => d,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            return 1;
        }
    };
    let engine = match engine::Engine::start(app_id, &home, &config::workspace(&home, app_id)) {
        Ok(e) => e,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            artifacts::close_session(&dir);
            return 1;
        }
    };
    let session = engine.snapshot();

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watcher = match session.event_stream() {
        Ok(stream) => {
            let stop = stop.clone();
            let reader = session.clone();
            Some(std::thread::spawn(move || {
                let mut last_text = 0usize;
                let result = reader.consume_events(stream, &stop, |items| {
                    let text = items
                        .iter()
                        .filter(|item| item.kind == "text")
                        .map(|item| item.text.as_str())
                        .collect::<Vec<_>>()
                        .join("");
                    let reasoning = items
                        .iter()
                        .filter(|item| item.kind == "reasoning")
                        .map(|item| item.text.as_str())
                        .collect::<Vec<_>>()
                        .join("");
                    if text.len() != last_text {
                        last_text = text.len();
                        println!(
                            "[stream] reasoning={} text={} chars: {}",
                            reasoning.chars().count(),
                            text.chars().count(),
                            text.chars().take(100).collect::<String>()
                        );
                    }
                });
                if let Err(e) = result {
                    println!("[stream] error: {e}");
                }
            }))
        }
        Err(e) => {
            println!("[stream] unavailable: {e}");
            None
        }
    };

    let started = std::time::Instant::now();
    let result = session
        .prompt(message, None)
        .and_then(|_| session.history());
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Some(watcher) = watcher {
        let _ = watcher.join();
    }
    println!("prompt finished in {} ms", started.elapsed().as_millis());

    drop(engine);
    artifacts::close_session(&dir);
    match result {
        Ok(items) => {
            for item in items {
                if item.kind == "reasoning" {
                    println!("[reasoning] {} chars", item.text.chars().count());
                } else {
                    println!("[{}] {}", item.role, item.text);
                }
            }
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// `--chat-sessions <app-id>`: lists chats and creates one new chat, proving
/// that starting a new chat does not delete the old ones.
pub fn cli_chat_sessions(app_id: Option<&String>) -> i32 {
    let Some(app_id) = app_id else {
        eprintln!("usage: eco-platform --chat-sessions <app-id>");
        return 2;
    };
    let home = registry::home();
    let policy = config::policy(&home);
    let root = artifacts::sessions_root(&home, app_id);
    let dir = match artifacts::open_session(&root, &policy) {
        Ok(d) => d,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            return 1;
        }
    };
    let mut engine = match engine::Engine::start(app_id, &home, &config::workspace(&home, app_id)) {
        Ok(e) => e,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            artifacts::close_session(&dir);
            return 1;
        }
    };
    match engine.sessions() {
        Ok(before) => {
            println!("before: {} chats", before.len());
            for chat in &before {
                println!("  {}  {}", chat.id, chat.title);
            }
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            drop(engine);
            artifacts::close_session(&dir);
            return 1;
        }
    }
    if let Err(e) = engine.new_session("Проверка нового чата") {
        println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
        drop(engine);
        artifacts::close_session(&dir);
        return 1;
    }
    match engine.sessions() {
        Ok(after) => {
            println!("after: {} chats", after.len());
            for chat in &after {
                println!("  {}  {}", chat.id, chat.title);
            }
        }
        Err(e) => println!("{{ \"ok\": false, \"problem\": \"{e}\" }}"),
    }
    drop(engine);
    artifacts::close_session(&dir);
    0
}

/// `--tokens [app-id]`: issued agent tokens (without hashes) and durable app
/// revocations.
pub fn cli_tokens(app_id: Option<&String>) -> i32 {
    let home = registry::home();
    let tokens = match identity::list(&home, app_id.map(String::as_str)) {
        Ok(tokens) => tokens,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            return 1;
        }
    };
    let revoked = match identity::revoked_apps(&home) {
        Ok(apps) => apps,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            return 1;
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "tokens": tokens,
            "revoked_apps": revoked
        }))
        .unwrap_or_default()
    );
    0
}

/// `--issue-token <app-id> [ttl-hours]`: issues a short-lived per-app token and
/// prints it once. The engine normally does this itself on start.
pub fn cli_issue_token(app_id: Option<&String>, ttl: Option<&String>) -> i32 {
    let Some(app_id) = app_id else {
        eprintln!("usage: eco-platform --issue-token <app-id> [ttl-hours]");
        return 2;
    };
    let home = registry::home();
    let app = match registry::verified_manifest(&home, app_id) {
        Ok(app) => app,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            return 1;
        }
    };
    let ttl_secs = match ttl {
        Some(value) => match value.parse::<u64>() {
            Ok(hours) if hours > 0 => hours.saturating_mul(3600),
            _ => {
                eprintln!("ttl-hours must be a positive integer");
                return 2;
            }
        },
        None => identity::DEFAULT_TTL_SECS,
    };
    match identity::issue(&home, &app.app_id, &app.version, ttl_secs, "admin") {
        Ok(issued) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&issued).unwrap_or_default()
            );
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// `--revoke <token-id|app-id>`: revokes one token, or durably revokes an app
/// and stops its engine. An app stays revoked across restarts until
/// `--unrevoke`.
pub fn cli_revoke(target: Option<&String>) -> i32 {
    let Some(target) = target else {
        eprintln!("usage: eco-platform --revoke <token-id|app-id>");
        return 2;
    };
    let home = registry::home();

    if target.starts_with("tok_") {
        let records = match identity::revoke_token(&home, target) {
            Ok(records) => records,
            Err(e) => {
                println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
                return 1;
            }
        };
        if records.is_empty() {
            println!(
                "{{ \"ok\": false, \"problem\": \"no active token {target}\" }}"
            );
            return 1;
        }
        let apps: std::collections::BTreeSet<&str> =
            records.iter().map(|record| record.app_id.as_str()).collect();
        let mut problems = Vec::new();
        for app in &apps {
            if let Err(e) = engine::kill_leftover(&home, app) {
                problems.push(format!("{app}: {e}"));
            }
        }
        let ok = problems.is_empty();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": ok,
                "revoked": records.len(),
                "apps": apps,
                "problems": problems
            }))
            .unwrap_or_default()
        );
        return if ok { 0 } else { 1 };
    }

    // Revocation must work even when the allowlist already denies the app, so
    // use the installed-package lookup instead of the runtime trust gate.
    if registry::find(&home, target).is_none() {
        println!(
            "{{ \"ok\": false, \"problem\": \"app {target} is not installed; nothing to revoke\" }}"
        );
        return 1;
    }
    let records = match identity::revoke_app(&home, target) {
        Ok(records) => records,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            return 1;
        }
    };
    match engine::kill_leftover(&home, target) {
        Ok(()) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "app_id": target,
                    "revoked_app": true,
                    "tokens_revoked": records.len()
                }))
                .unwrap_or_default()
            );
            0
        }
        Err(e) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": false,
                    "app_id": target,
                    "revoked_app": true,
                    "tokens_revoked": records.len(),
                    "problem": e.to_string()
                }))
                .unwrap_or_default()
            );
            1
        }
    }
}

/// `--unrevoke <app-id>`: removes the durable app revocation marker.
pub fn cli_unrevoke(app_id: Option<&String>) -> i32 {
    let Some(app_id) = app_id else {
        eprintln!("usage: eco-platform --unrevoke <app-id>");
        return 2;
    };
    let home = registry::home();
    match identity::unrevoke_app(&home, app_id) {
        Ok(true) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "app_id": app_id,
                    "unrevoked": true
                }))
                .unwrap_or_default()
            );
            0
        }
        Ok(false) => {
            println!(
                "{{ \"ok\": false, \"problem\": \"app {app_id} is not revoked\" }}"
            );
            1
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}
