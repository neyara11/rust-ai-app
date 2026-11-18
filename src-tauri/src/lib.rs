pub mod artifacts;
pub mod allowlist;
pub mod commands;
pub mod config;
pub mod engine;
pub mod error;
pub mod identity;
pub mod manifest;
pub mod mcp;
pub mod pack;
pub mod permissions;
pub mod proxy;
pub mod registry;
pub mod seal;
pub mod secrets;
pub mod state;
pub mod svg_icon;
pub mod workspace;

use secrets::SecretStore;
use state::AppState;
use tauri::Manager;

/// Editor terminals export `ELECTRON_RUN_AS_NODE=1` for their own child
/// processes. Inherited by the platform it makes every Electron-based handler
/// (VS Code and friends) run as Node, so "open file" would execute the document
/// instead of showing it. Drop the launcher flags before the UI starts.
fn scrub_launcher_env() {
    for key in ["ELECTRON_RUN_AS_NODE", "ELECTRON_NO_ATTACH_CONSOLE"] {
        std::env::remove_var(key);
    }
}

pub fn run() {
    scrub_launcher_env();
    let state = AppState::new();
    // Restore runtime state parked by an interrupted install/rollback.
    registry::recover_runtime(&state.home);
    // Reap engines left by a previous crash before the window is interactive:
    // in a background thread `kill_leftover` could kill a freshly started one.
    for app_id in registry::installed_ids(&state.home) {
        let _ = engine::kill_leftover(&state.home, &app_id);
    }

    tauri::Builder::default()
        .manage(state)
        .setup(|app| {
            // The sweep hashes every installed package and kills leftover
            // engines; do it after the window exists so startup stays fast.
            let home = app.state::<AppState>().home.clone();
            std::thread::spawn(move || startup_sweep(&home));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::platform_info,
            commands::set_mode,
            commands::set_role,
            commands::list_apps,
            commands::install_app,
            commands::pick_package,
            commands::save_diagram,
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
            commands::close_all_apps,
            commands::choose_workspace,
            commands::workspace_file_open,
            commands::workspace_file_reveal,
            commands::workspace_file_save_copy,
            commands::open_external,
            commands::workspace_open_folder,
            commands::artifacts_usage,
            commands::artifact_dumps,
            commands::artifact_grep,
            commands::export_artifacts,
            commands::cleanup_artifacts,
            pack::pack_tool_info,
            pack::pack_scan,
            pack::pack_import,
            pack::pack_apply_template,
            pack::pack_set_mcp,
            pack::pack_init,
            pack::pack_validate,
            pack::pack_build,
            pack::pack_sign,
            pack::pack_keygen,
            pack::pack_files,
            pack::pack_read_text,
            pack::pack_write_text,
            pack::pack_pick_folder,
            commands::mcp_servers,
            commands::mcp_server_get,
            commands::mcp_server_save,
            commands::mcp_server_delete,
            commands::mcp_app_servers,
            commands::mcp_app_server_save,
            commands::mcp_app_server_delete,
            commands::mcp_probe,
            commands::mcp_app_probe,
            commands::mcp_call,
            commands::app_secrets,
            commands::app_secret_set,
            commands::app_secret_delete,
            commands::audit_tail,
            commands::chat_open,
            commands::chat_send,
            commands::chat_list,
            commands::chat_rename,
            commands::chat_new,
            commands::chat_select,
            commands::chat_abort,
            commands::chat_status,
            commands::chat_delete,
            commands::chat_history,
            commands::chat_image,
            commands::chat_reset,
            commands::chat_stop,
            commands::chat_permission,
            commands::permission_grants,
            commands::permission_revoke_grant,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ECO AIOps platform");
}

/// Removes leftover staging directories, orphaned engines from a previous
/// crash, and expired or orphaned session data.
pub fn startup_sweep(home: &std::path::Path) {
    identity::sweep(home);
    let policy = config::policy(home);
    for app in registry::list(home) {
        let root = artifacts::sessions_root(home, &app.app_id);
        let _ = artifacts::sweep(&root, &policy);
    }
    sweep_staging(home);
}

/// Removes only old staging directories. A concurrent CLI install/rollback may
/// be using a fresh one, so never wipe the whole tree.
fn sweep_staging(home: &std::path::Path) {
    let root = home.join("staging");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Keep-* holds runtime state awaiting recovery; never sweep it.
        if name.starts_with("keep-") {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let old = meta
            .modified()
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .map(|age| age.as_secs() > 6 * 3600)
            .unwrap_or(false);
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Headless verification of a package directory or zip. Used by CI and by
fn cli_package_path(raw: &str) -> std::path::PathBuf {
    let src = std::path::PathBuf::from(raw);
    if src.exists() {
        return src;
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
    {
        for candidate in [dir.join(&src), dir.join("dev-apps").join(&src)] {
            if candidate.exists() {
                return candidate;
            }
        }
    }
    src
}

/// `--verify` on the built binary; no window is created.
pub fn cli_verify(arg: Option<&String>) -> i32 {
    let Some(path) = arg else {
        eprintln!("usage: eco-platform --verify <package-dir|package.zip>");
        return 2;
    };
    let home = registry::home();
    let src = cli_package_path(path);
    let (dir, _temp) = match prepare_package(&home, &src) {
        Ok(v) => v,
        Err(e) => {
            println!("{{ \"ok\": false, \"problems\": [\"{e}\"] }}");
            return 1;
        }
    };
    let report = verify_by_policy(&home, &dir);
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
    let src = cli_package_path(path);
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
/// Local mode checks integrity only; corporate mode keeps the signature gate.
fn verify_by_policy(home: &std::path::Path, dir: &std::path::Path) -> manifest::VerifyReport {
    let policy = config::policy_for(home);
    let app = match manifest::load(dir) {
        Ok(app) => app,
        Err(e) => return manifest::VerifyReport::fail(vec![e.to_string()]),
    };
    // Only an installed directory has a seal: for a package being reviewed
    // before install, integrity/signature checks are the right gate.
    let installed = registry::find(home, &app.app_id)
        .and_then(|installed| std::fs::canonicalize(installed).ok())
        .zip(std::fs::canonicalize(dir).ok())
        .map(|(installed, dir)| installed == dir)
        .unwrap_or(false);
    if installed {
        return registry::verify_for_readonly(
            home,
            &app.app_id,
            dir,
            &registry::load_trusted_keys(home),
            policy,
        );
    }
    if policy.local() {
        manifest::verify_integrity(dir)
    } else {
        manifest::verify_package(dir, &registry::load_trusted_keys(home))
    }
}

/// `--mode <local|corporate> [developer|user]`: stores the first-run choice
/// without opening the UI.
pub fn cli_mode(environment: Option<&String>, role: Option<&String>) -> i32 {
    let Some(environment) = environment else {
        eprintln!("usage: eco-platform --mode <local|corporate> [developer|user]");
        return 2;
    };
    let environment = match environment.as_str() {
        "local" => config::Environment::Local,
        "corporate" => config::Environment::Corporate,
        other => {
            eprintln!("unknown environment '{other}': use local or corporate");
            return 2;
        }
    };
    let role = match role.map(String::as_str) {
        Some("developer") => config::Role::Developer,
        Some("user") | None => config::Role::User,
        Some(other) => {
            eprintln!("unknown role '{other}': use developer or user");
            return 2;
        }
    };
    let home = registry::home();
    match config::set_mode(&home, environment, role) {
        Ok(cfg) => {
            println!(
                "mode: {} · {}",
                cfg.environment.as_str(),
                cfg.role.as_str()
            );
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

pub fn cli_role(role: Option<&String>) -> i32 {
    let Some(role) = role else {
        eprintln!("usage: eco-platform --role <developer|user>");
        return 2;
    };
    let role = match role.as_str() {
        "developer" => config::Role::Developer,
        "user" => config::Role::User,
        other => {
            eprintln!("unknown role '{other}': use developer or user");
            return 2;
        }
    };
    let home = registry::home();
    match config::set_role(&home, role) {
        Ok(cfg) => {
            println!(
                "role: {} · environment: {}",
                cfg.role.as_str(),
                cfg.environment.as_str()
            );
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

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
    let report = verify_by_policy(&home, &dir);
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



/// `--artifacts <app-id>`: artifact usage and NDJSON dump files.
pub fn cli_artifacts(app: Option<&String>) -> i32 {
    let Some(app_id) = app else {
        eprintln!("usage: eco-platform --artifacts <app-id>");
        return 2;
    };
    let home = registry::home();
    let root = artifacts::sessions_root(&home, app_id);
    let out = serde_json::json!({
        "app_id": app_id,
        "usage": artifacts::usage(&root),
        "dumps": artifacts::list_dumps(&root)
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    0
}

/// `--grep-artifacts <app-id> <query> [session]`: case-insensitive search over
/// the session NDJSON dumps.
pub fn cli_grep_artifacts(
    app: Option<&String>,
    query: Option<&String>,
    session: Option<&String>,
) -> i32 {
    let (Some(app_id), Some(query)) = (app, query) else {
        eprintln!("usage: eco-platform --grep-artifacts <app-id> <query> [session]");
        return 2;
    };
    let home = registry::home();
    let root = artifacts::sessions_root(&home, app_id);
    match artifacts::grep(&root, query, session.map(String::as_str), 100) {
        Ok(hits) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "app_id": app_id,
                    "hits": hits
                }))
                .unwrap_or_default()
            );
            0
        }
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
            1
        }
    }
}

/// CLI-only TLS escape hatch for private CAs: `MCP_INSECURE=1`.
fn cli_mcp_insecure() -> bool {
    std::env::var("MCP_INSECURE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
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
        allow_invalid_certs: cli_mcp_insecure(),
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
        allow_invalid_certs: cli_mcp_insecure(),
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
    if definition.stream == "write" && !manifest::external_write_allowed(&app, server_id) {
        return error::msg(format!(
            "tool {tool} is a write tool and {app_id} does not grant writes to {server_id}"
        ));
    }

    let started = std::time::Instant::now();
    let def = mcp::get_for_app(&SecretStore::new(home), app_id, server_id)?;
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
        allow_invalid_certs: cli_mcp_insecure(),
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
/// Tool calls are allowed by default because nobody can answer an approval;
/// `ECO_PLATFORM_TOOL_POLICY=ask` switches to the profile policy instead.
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
    if let Some(level) = level {
        let caps = config::load(&home).llm_capabilities;
        if !caps.allows_variant(level) {
            eprintln!("reasoning variant '{level}' is not enabled for the configured model");
            return 2;
        }
    }
    let engine = match engine::Engine::start_with(
        app_id,
        &home,
        &config::workspace(&home, app_id),
        crate::permissions::headless_policy_override(),
    ) {
        Ok(e) => e,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
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
/// and the final history handoff. Like `--chat`, tool calls are allowed unless
/// `ECO_PLATFORM_TOOL_POLICY=ask`.
pub fn cli_chat_async(app_id: Option<&String>, message: Option<&String>) -> i32 {
    let (Some(app_id), Some(message)) = (app_id, message) else {
        eprintln!("usage: eco-platform --chat-async <app-id> <message>");
        return 2;
    };
    let home = registry::home();
    let engine = match engine::Engine::start_with(
        app_id,
        &home,
        &config::workspace(&home, app_id),
        crate::permissions::headless_policy_override(),
    ) {
        Ok(e) => e,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
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
                let result = reader.consume_events(
                    stream,
                    &stop,
                    |items| {
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
                    },
                    // Headless runs have no operator: fail closed.
                    |_event| crate::permissions::PermissionVerdict::Reject,
                );
                if let Err(e) = result {
                    println!("[stream] error: {e}");
                }
            }))
        }
        Err(e) => {
            println!("[stream] unavailable: {e}");
            return 1;
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

/// `--system-prompt <app-id>`: prints the exact system prompt the engine gets,
/// including the generated platform tool guide. Useful to verify what the model
/// is told without starting a chat.
pub fn cli_system_prompt(app_id: Option<&String>) -> i32 {
    let Some(app_id) = app_id else {
        eprintln!("usage: eco-platform --system-prompt <app-id>");
        return 2;
    };
    let home = registry::home();
    let result = (|| -> error::Result<()> {
        let app = registry::verified_manifest(&home, app_id)?;
        let dir = registry::apps_dir(&home).join(app_id);
        let prompt = engine::compose_system_prompt(&dir, &app)?;
        println!("{prompt}");
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// `--artifact-tool <app> <tool> <json>`: runs a platform tool through the same
/// proxy path the agent uses (token check, allowlist, routing) and prints the
/// result. Intended for verifying artifact_list/search/read without a chat.
pub fn cli_artifact_tool(
    app_id: Option<&String>,
    tool: Option<&String>,
    arguments: Option<&String>,
) -> i32 {
    let (Some(app_id), Some(tool)) = (app_id, tool) else {
        eprintln!("usage: eco-platform --artifact-tool <app-id> <tool> <json>");
        return 2;
    };
    let home = registry::home();
    let result = (|| -> error::Result<()> {
        let app = registry::verified_manifest(&home, app_id)?;
        let root = artifacts::sessions_root(&home, app_id);
        std::fs::create_dir_all(&root)?;
        let policy = config::policy(&home);
        let session_dir = match artifacts::list_dumps(&root).first() {
            Some(dump) => std::path::Path::new(&dump.path)
                .parent()
                .map(|path| path.to_path_buf())
                .ok_or_else(|| error::Error::Msg("invalid dump path".to_string()))?,
            None => artifacts::open_session(&root, &policy)?,
        };
        let workspace = config::workspace(&home, app_id);
        std::fs::create_dir_all(&workspace)?;
        let token = identity::issue(&home, app_id, &app.version, 120, "cli")?;
        std::env::set_var("ECO_AGENT_TOKEN", &token.token);
        std::env::set_var("ECO_AGENT_USER", "cli");
        let mut proxy = proxy::Proxy::new(app_id, &workspace, Some(&session_dir))?;
        let raw = match arguments.map(String::as_str) {
            Some("-") | None => {
                use std::io::Read;
                let mut buffer = String::new();
                std::io::stdin().read_to_string(&mut buffer)?;
                buffer
            }
            Some(raw) => raw.to_string(),
        };
        let value: serde_json::Value =
            serde_json::from_str(if raw.trim().is_empty() { "{}" } else { &raw })?;
        let result = proxy.call_tool(tool, value)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
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
    let mut engine = match engine::Engine::start(app_id, &home, &config::workspace(&home, app_id)) {
        Ok(e) => e,
        Err(e) => {
            println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
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
            return 1;
        }
    }
    if let Err(e) = engine.new_session("Проверка нового чата") {
        println!("{{ \"ok\": false, \"problem\": \"{e}\" }}");
        drop(engine);
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

#[cfg(test)]
mod launcher_env_tests {
    use super::scrub_launcher_env;

    #[test]
    fn launcher_flags_do_not_leak_into_the_platform() {
        std::env::set_var("ELECTRON_RUN_AS_NODE", "1");
        std::env::set_var("ELECTRON_NO_ATTACH_CONSOLE", "1");
        scrub_launcher_env();
        assert!(std::env::var("ELECTRON_RUN_AS_NODE").is_err());
        assert!(std::env::var("ELECTRON_NO_ATTACH_CONSOLE").is_err());
    }
}
