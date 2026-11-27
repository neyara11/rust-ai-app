use crate::error::{msg, Error, Result};
use crate::state::AppState;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

const MAX_TEXT_BYTES: u64 = 256 * 1024;
const MAX_FILES: usize = 300;

/// `eco-pack` is a console binary; spawning it from the GUI must not flash a
/// console window. Other sidecars already use the same flag.
fn pack_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

#[derive(Debug, Serialize)]
pub struct PackToolInfo {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub default_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PackRun {
    pub ok: bool,
    pub output: String,
}

#[derive(Debug, Serialize)]
pub struct PackFile {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectedMcp {
    pub id: String,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct PackScan {
    pub has_manifest: bool,
    pub prompt: Option<String>,
    pub agents_md: bool,
    pub root_skill: bool,
    pub skills: Vec<String>,
    pub bins: Vec<String>,
    pub mcp: Vec<DetectedMcp>,
    pub env_files: Vec<String>,
}

fn candidates(home: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(value) = std::env::var("ECO_PACK_BIN") {
        if !value.trim().is_empty() {
            out.push(PathBuf::from(value));
        }
    }
    out.push(home.join("pack").join("eco-pack.exe"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join("pack").join("eco-pack.exe"));
            if let Some(repo) = dir.ancestors().nth(3) {
                out.push(
                    repo.join("tools")
                        .join("pack")
                        .join("target")
                        .join("release")
                        .join("eco-pack.exe"),
                );
                out.push(repo.join("vendor").join("pack").join("eco-pack.exe"));
                out.push(
                    repo.join("tools")
                        .join("pack")
                        .join("target")
                        .join("debug")
                        .join("eco-pack.exe"),
                );
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        out.push(
            cwd.join("tools")
                .join("pack")
                .join("target")
                .join("release")
                .join("eco-pack.exe"),
        );
        out.push(cwd.join("vendor").join("pack").join("eco-pack.exe"));
    }
    out
}

fn pack_bin(home: &Path) -> Option<PathBuf> {
    candidates(home).into_iter().find(|path| path.is_file())
}

fn run(home: &Path, args: &[String]) -> Result<PackRun> {
    let bin = pack_bin(home).ok_or_else(|| {
        Error::Msg(
            "eco-pack.exe not found; build it with tools\\build-pack.ps1 or set ECO_PACK_BIN"
                .to_string(),
        )
    })?;
    let output = pack_command(&bin)
        .args(args)
        .output()
        .map_err(|e| Error::Msg(format!("cannot run eco-pack: {e}")))?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    let err = String::from_utf8_lossy(&output.stderr);
    if !err.trim().is_empty() {
        if !text.trim().is_empty() {
            text.push('\n');
        }
        text.push_str(&err);
    }
    Ok(PackRun {
        ok: output.status.success(),
        output: text.trim_end().to_string(),
    })
}

fn package_dir(raw: &str) -> Result<PathBuf> {
    let path = PathBuf::from(raw.trim());
    if raw.trim().is_empty() {
        return msg("package folder is empty");
    }
    if !path.is_absolute() {
        return msg("package folder must be an absolute path");
    }
    let root = fs::canonicalize(&path)
        .map_err(|e| Error::Msg(format!("cannot resolve package folder: {e}")))?;
    if !root.is_dir() {
        return msg("package folder is not a directory");
    }
    Ok(root)
}

/// Same rule as eco-pack: relative, forward slashes, no `..`, no drive/colon.
fn safe_rel(rel: &str) -> bool {
    if rel.is_empty() || rel.starts_with('/') || rel.starts_with('\\') {
        return false;
    }
    if rel.contains('\\') || rel.contains(':') {
        return false;
    }
    Path::new(rel).components().all(|component| match component {
        Component::Normal(part) => !part.is_empty(),
        _ => false,
    })
}

fn editable(rel: &str) -> bool {
    rel == "manifest.json"
        || rel.starts_with("prompt/")
        || rel.starts_with("skills/")
        || (!rel.contains('/') && rel.ends_with(".md"))
}

fn ensure_inside(root: &Path, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        let canonical_parent = fs::canonicalize(parent)
            .map_err(|e| Error::Msg(format!("cannot resolve target folder: {e}")))?;
        if !canonical_parent.starts_with(root) {
            return msg("path must stay inside the package folder");
        }
    }
    if path.exists() {
        let canonical = fs::canonicalize(path)?;
        if !canonical.starts_with(root) {
            return msg("path must stay inside the package folder");
        }
    }
    Ok(())
}

fn root_file(root: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
            let path = entry.path();
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn strip_jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            continue;
        }
        if c == '/' {
            match chars.peek() {
                Some('/') => {
                    for next in chars.by_ref() {
                        if next == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    chars.next();
                    let mut prev = '\0';
                    for next in chars.by_ref() {
                        if prev == '*' && next == '/' {
                            break;
                        }
                        prev = next;
                    }
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}

fn mcp_sources() -> Vec<(&'static str, &'static str)> {
    vec![
        (".mcp.json", "mcpServers"),
        ("mcp.json", "mcpServers"),
        (".vscode/mcp.json", "servers"),
        ("opencode.json", "mcp"),
        ("opencode.jsonc", "mcp"),
    ]
}

/// Best-effort detection of existing MCP declarations. Only ids are reported:
/// URLs and tokens may contain secrets and belong to the platform DPAPI store.
fn detect_mcp(root: &Path) -> Vec<DetectedMcp> {
    let mut out: Vec<DetectedMcp> = Vec::new();
    for (rel, key) in mcp_sources() {
        let path = root.join(rel);
        if !path.is_file() {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let parsed = serde_json::from_str::<Value>(&raw)
            .or_else(|_| serde_json::from_str::<Value>(&strip_jsonc(&raw)));
        let Ok(value) = parsed else {
            continue;
        };
        let Some(servers) = value.get(key).and_then(Value::as_object) else {
            continue;
        };
        for (id, config) in servers {
            if id.trim().is_empty() || out.iter().any(|known| &known.id == id) {
                continue;
            }
            if config.get("command").is_some() && config.get("url").is_none() {
                continue;
            }
            out.push(DetectedMcp {
                id: id.clone(),
                source: rel.to_string(),
            });
        }
    }
    out
}

fn detect_env_files(root: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>, depth: usize) {
        if depth > 4 || out.len() >= 50 {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out, depth + 1);
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name == ".env" || name.starts_with(".env.") {
                if let Ok(rel) = path.strip_prefix(base) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out, 0);
    out.sort();
    out
}

fn scan_impl(root: &Path) -> Result<PackScan> {
    let has_manifest = root.join("manifest.json").is_file();
    let prompt = root
        .join("prompt")
        .join("system.md")
        .is_file()
        .then(|| "prompt/system.md".to_string());
    let agents_md = root_file(root, "AGENTS.md").is_some();
    let root_skill = root_file(root, "SKILL.md").is_some();

    let mut skills = Vec::new();
    let skills_dir = root.join("skills");
    if let Ok(entries) = fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("SKILL.md").is_file() {
                skills.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }
    skills.sort();

    let mut bins = Vec::new();
    let bin_dir = root.join("bin");
    if let Ok(entries) = fs::read_dir(&bin_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("exe"))
                    .unwrap_or(false)
            {
                bins.push(format!("bin/{}", entry.file_name().to_string_lossy()));
            }
        }
    }
    bins.sort();

    Ok(PackScan {
        has_manifest,
        prompt,
        agents_md,
        root_skill,
        skills,
        bins,
        mcp: detect_mcp(root),
        env_files: detect_env_files(root),
    })
}

fn app_id_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Generates a manifest from existing sources: `prompt/system.md` or `AGENTS.md`,
/// `skills/<name>/SKILL.md` or a root `SKILL.md`. Never overwrites an existing
/// manifest and leaves the author's original files in place.
fn import_impl(
    root: &Path,
    app_id: &str,
    name: &str,
    template: &str,
) -> Result<PackRun> {
    if root.join("manifest.json").exists() {
        return msg("manifest.json already exists; use validate or edit it in the wizard");
    }
    if !app_id_ok(app_id) {
        return msg("app_id must use letters, digits, '-' or '_' and be at most 64 chars");
    }
    if name.is_empty() || name.len() > 80 {
        return msg("name must be 1..80 characters");
    }
    if template != "minimal" && template != "files" {
        return msg("template must be minimal or files");
    }

    let mut notes: Vec<String> = Vec::new();
    let prompt_path = root.join("prompt").join("system.md");
    if !prompt_path.is_file() {
        let Some(agents) = root_file(root, "AGENTS.md") else {
            return msg("AGENTS.md or prompt/system.md is required to generate a manifest");
        };
        fs::create_dir_all(root.join("prompt"))?;
        fs::rename(&agents, &prompt_path)
            .map_err(|e| Error::Msg(format!("cannot move AGENTS.md: {e}")))?;
        notes.push("AGENTS.md -> prompt/system.md".to_string());
    }

    let skills_dir = root.join("skills");
    let has_skill = fs::read_dir(&skills_dir)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().is_dir() && entry.path().join("SKILL.md").is_file())
        })
        .unwrap_or(false);
    if !has_skill {
        if let Some(root_skill) = root_file(root, "SKILL.md") {
            fs::create_dir_all(skills_dir.join("main"))?;
            let target = skills_dir.join("main").join("SKILL.md");
            if !target.exists() {
                fs::rename(&root_skill, &target)
                    .map_err(|e| Error::Msg(format!("cannot move SKILL.md: {e}")))?;
                notes.push("SKILL.md -> skills/main/SKILL.md".to_string());
            }
        }
    }

    let file_scopes = if template == "files" {
        json!(["workspace"])
    } else {
        json!([])
    };
    let detected_mcp = detect_mcp(root);
    let mut connections = vec![json!({ "id": "llm", "type": "llm", "required": true })];
    for server in &detected_mcp {
        connections.push(json!({
            "id": server.id,
            "type": "http-mcp",
            "required": false
        }));
    }
    let manifest = json!({
        "schema_version": 2,
        "app_id": app_id,
        "name": name,
        "version": "0.1.0",
        "min_shell_version": "0.1.0",
        "publisher": "",
        "profile": {
            "system_prompt": "prompt/system.md",
            "model_hint": "gateway-default",
            "max_steps": 20
        },
        "tools": [],
        "connections": connections,
        "capabilities": {
            "index_allowlist": [],
            "file_scopes": file_scopes,
            "external_writes": []
        },
        "artifacts": { "quota_mb": 50, "ttl_hours": 72 },
        "hashes": {}
    });
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| Error::Msg(format!("cannot serialize manifest: {e}")))?;
    fs::write(root.join("manifest.json"), format!("{text}\n"))
        .map_err(|e| Error::Msg(format!("cannot write manifest.json: {e}")))?;
    notes.push("manifest.json created".to_string());
    if !notes.iter().any(|note| note.contains("AGENTS.md")) {
        notes.push("prompt/system.md kept".to_string());
    }
    for server in &detected_mcp {
        notes.push(format!(
            "MCP {} ({}): добавлен как http-mcp; URL/токен — в Настройках → Реестр MCP-серверов, запись выключена. Инструменты сервера нужно объявить в tools[] как {{\"tool\": \"<имя из tools/list>\", \"stream\": \"read\"}} — из connections они не появляются автоматически",
            server.id, server.source
        ));
    }
    let env_files = detect_env_files(root);
    if !env_files.is_empty() {
        notes.push(format!(
            "warning: найдены секреты ({}) — они не должны попадать в пакет; удалите их и используйте tools[].secrets с DPAPI",
            env_files.join(", ")
        ));
    }
    let bins = scan_impl(root)?.bins;
    if !bins.is_empty() {
        notes.push(format!(
            "warning: найдены exe, но они не объявлены в tools[]: {} — добавьте каждый как инструмент (bin, режим io: files для конвертера или io: json для stdin-скрипта, secrets) или validate/build завершится ошибкой",
            bins.join(", ")
        ));
    }

    if template == "minimal" {
        let mut probe = fs::read_to_string(&prompt_path).unwrap_or_default();
        let skills_dir = root.join("skills");
        if let Ok(entries) = fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                if entry.path().join("SKILL.md").is_file() {
                    probe.push_str(&fs::read_to_string(entry.path().join("SKILL.md")).unwrap_or_default());
                }
            }
        }
        if ["list_files", "read_file", "write_file", "append_file"]
            .iter()
            .any(|tool| probe.contains(tool))
        {
            notes.push(
                "warning: prompt/skills используют файловые инструменты, но шаблон minimal; выберите files и нажмите «Применить шаблон к манифесту»"
                    .to_string(),
            );
        }
    }

    Ok(PackRun {
        ok: true,
        output: notes.join("\n"),
    })
}

fn apply_template_impl(root: &Path, template: &str) -> Result<PackRun> {
    if template != "minimal" && template != "files" {
        return msg("template must be minimal or files");
    }
    let manifest_path = root.join("manifest.json");
    if !manifest_path.is_file() {
        return msg("manifest.json not found; import sources first");
    }
    let mut manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .map_err(|e| Error::Msg(format!("cannot read manifest.json: {e}")))?,
    )
    .map_err(|e| Error::Msg(format!("manifest.json is invalid: {e}")))?;
    if !manifest.get("capabilities").map(Value::is_object).unwrap_or(false) {
        manifest["capabilities"] = json!({});
    }
    manifest["capabilities"]["file_scopes"] = if template == "files" {
        json!(["workspace"])
    } else {
        json!([])
    };
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| Error::Msg(format!("cannot serialize manifest: {e}")))?;
    fs::write(&manifest_path, format!("{text}\n"))
        .map_err(|e| Error::Msg(format!("cannot write manifest.json: {e}")))?;
    Ok(PackRun {
        ok: true,
        output: format!(
            "file_scopes: {}\nmanifest updated; rebuild and re-sign the package",
            if template == "files" { "workspace" } else { "none" }
        ),
    })
}

fn target_path(root: &Path, rel: &str) -> Result<PathBuf> {
    if !safe_rel(rel) {
        return msg("invalid package file path");
    }
    if !editable(rel) {
        return msg("only manifest.json, prompt/** and skills/** are editable here");
    }
    let path = root.join(rel);
    ensure_inside(root, &path)?;
    Ok(path)
}

pub(crate) fn write_text(root: &Path, rel: &str, text: &str) -> Result<()> {
    if text.len() as u64 > MAX_TEXT_BYTES {
        return msg(format!("file is larger than {MAX_TEXT_BYTES} bytes"));
    }
    let path = target_path(root, rel)?;
    fs::write(&path, text).map_err(|e| Error::Msg(format!("cannot write {rel}: {e}")))
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PackFile>, depth: usize) {
    if depth > 4 || out.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if out.len() >= MAX_FILES {
            return;
        }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            walk(root, &path, out, depth + 1);
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        if editable(&rel) {
            out.push(PackFile {
                path: rel,
                bytes: meta.len(),
            });
        }
    }
}

fn default_dev_key(home: &Path) -> Option<PathBuf> {
    let mut candidates = vec![home.join("dev").join("keys").join("dev.key")];
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("dev").join("keys").join("dev.key"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(repo) = exe
            .parent()
            .and_then(|dir| dir.ancestors().nth(3))
        {
            candidates.push(repo.join("dev").join("keys").join("dev.key"));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn tool_info(home: &Path) -> PackToolInfo {
    let bin = pack_bin(home);
    let version = bin.as_ref().and_then(|path| {
        pack_command(path)
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
    });
    let default_key = default_dev_key(home).map(|path| path.display().to_string());
    PackToolInfo {
        available: bin.is_some(),
        path: bin.map(|path| path.display().to_string()),
        version,
        default_key,
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn pack_tool_info(state: tauri::State<'_, AppState>) -> Result<PackToolInfo> {
    let home = state.home.clone();
    tauri::async_runtime::spawn_blocking(move || tool_info(&home))
        .await
        .map_err(|e| Error::Msg(format!("eco-pack lookup failed: {e}")))
}

#[tauri::command(rename_all = "snake_case")]
pub fn pack_scan(dir: String) -> Result<PackScan> {
    let root = package_dir(&dir)?;
    scan_impl(&root)
}

#[tauri::command(rename_all = "snake_case")]
pub fn pack_import(
    dir: String,
    app_id: String,
    name: String,
    template: Option<String>,
) -> Result<PackRun> {
    let root = package_dir(&dir)?;
    let app_id = app_id.trim();
    let name = if name.trim().is_empty() {
        app_id
    } else {
        name.trim()
    };
    import_impl(
        &root,
        app_id,
        name,
        template.as_deref().unwrap_or("minimal"),
    )
}

fn connection_id_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn set_mcp_impl(root: &Path, id: &str, write: bool) -> Result<PackRun> {
    if !connection_id_ok(id) {
        return msg("MCP id must use letters, digits, '-' or '_' and be at most 64 chars");
    }
    let manifest_path = root.join("manifest.json");
    if !manifest_path.is_file() {
        return msg("manifest.json not found; import sources first");
    }
    let mut manifest: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .map_err(|e| Error::Msg(format!("cannot read manifest.json: {e}")))?,
    )
    .map_err(|e| Error::Msg(format!("manifest.json is invalid: {e}")))?;

    if !manifest.get("connections").map(Value::is_array).unwrap_or(false) {
        manifest["connections"] = json!([]);
    }
    let connections = manifest["connections"].as_array_mut().unwrap();
    if let Some(existing) = connections
        .iter()
        .find(|connection| connection.get("id").and_then(Value::as_str) == Some(id))
    {
        if existing.get("type").and_then(Value::as_str) != Some("http-mcp") {
            return msg(format!("connection '{id}' is already declared with another type"));
        }
    } else {
        connections.push(json!({ "id": id, "type": "http-mcp", "required": false }));
    }

    if !manifest
        .get("capabilities")
        .map(Value::is_object)
        .unwrap_or(false)
    {
        manifest["capabilities"] = json!({});
    }
    if !manifest["capabilities"]
        .get("external_writes")
        .map(Value::is_array)
        .unwrap_or(false)
    {
        manifest["capabilities"]["external_writes"] = json!([]);
    }
    let writes = manifest["capabilities"]["external_writes"]
        .as_array_mut()
        .unwrap();
    let position = writes.iter().position(|value| value.as_str() == Some(id));
    match (write, position) {
        (true, None) => writes.push(json!(id)),
        (false, Some(index)) => {
            writes.remove(index);
        }
        _ => {}
    }

    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| Error::Msg(format!("cannot serialize manifest: {e}")))?;
    fs::write(&manifest_path, format!("{text}\n"))
        .map_err(|e| Error::Msg(format!("cannot write manifest.json: {e}")))?;
    Ok(PackRun {
        ok: true,
        output: format!(
            "MCP '{id}': http-mcp, запись {}\nURL и токен задаются в Настройках → Реестр MCP-серверов; пересоберите и переподпишите пакет\nИнструменты сервера объявите в tools[] как {{\"tool\": \"<имя из tools/list>\", \"stream\": \"read\"}} — без этих записей remote-инструменты не появятся (автоподхвата из connections нет)",
            if write { "разрешена" } else { "выключена" }
        ),
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn pack_set_mcp(dir: String, id: String, write: Option<bool>) -> Result<PackRun> {
    let root = package_dir(&dir)?;
    set_mcp_impl(&root, id.trim(), write.unwrap_or(false))
}

#[tauri::command(rename_all = "snake_case")]
pub fn pack_apply_template(dir: String, template: Option<String>) -> Result<PackRun> {
    let root = package_dir(&dir)?;
    apply_template_impl(&root, template.as_deref().unwrap_or("files"))
}

#[tauri::command(rename_all = "snake_case")]
pub fn pack_init(
    state: tauri::State<'_, AppState>,
    dir: String,
    app_id: String,
    name: String,
    template: Option<String>,
) -> Result<PackRun> {
    if app_id.trim().is_empty() || name.trim().is_empty() {
        return msg("app_id and name are required");
    }
    let template = template.unwrap_or_else(|| "minimal".to_string());
    if template != "minimal" && template != "files" {
        return msg("template must be minimal or files");
    }
    let dir = PathBuf::from(dir.trim());
    if !dir.is_absolute() {
        return msg("package folder must be an absolute path");
    }
    run(
        &state.home,
        &[
            "init".to_string(),
            dir.display().to_string(),
            "--app-id".to_string(),
            app_id.trim().to_string(),
            "--name".to_string(),
            name.trim().to_string(),
            "--template".to_string(),
            template,
        ],
    )
}

#[tauri::command(rename_all = "snake_case")]
pub fn pack_validate(state: tauri::State<'_, AppState>, dir: String) -> Result<PackRun> {
    let root = package_dir(&dir)?;
    run(
        &state.home,
        &["validate".to_string(), root.display().to_string()],
    )
}

#[tauri::command(rename_all = "snake_case")]
pub fn pack_build(state: tauri::State<'_, AppState>, dir: String) -> Result<PackRun> {
    let root = package_dir(&dir)?;
    run(&state.home, &["build".to_string(), root.display().to_string()])
}

#[tauri::command(rename_all = "snake_case")]
pub fn pack_sign(
    state: tauri::State<'_, AppState>,
    dir: String,
    key: String,
) -> Result<PackRun> {
    let root = package_dir(&dir)?;
    let key = key.trim();
    if key.is_empty() {
        return msg("dev key path is required");
    }
    let key = fs::canonicalize(key)
        .map_err(|e| Error::Msg(format!("cannot resolve dev key: {e}")))?;
    run(
        &state.home,
        &[
            "sign".to_string(),
            root.display().to_string(),
            "--key".to_string(),
            key.display().to_string(),
        ],
    )
}

#[tauri::command(rename_all = "snake_case")]
pub fn pack_keygen(state: tauri::State<'_, AppState>, out_dir: String) -> Result<PackRun> {
    let out = out_dir.trim();
    if out.is_empty() {
        return msg("key folder is required");
    }
    let out = PathBuf::from(out);
    if !out.is_absolute() {
        return msg("key folder must be an absolute path");
    }
    run(
        &state.home,
        &[
            "keygen".to_string(),
            "--out".to_string(),
            out.display().to_string(),
        ],
    )
}

#[tauri::command(rename_all = "snake_case")]
pub async fn pack_files(dir: String) -> Result<Vec<PackFile>> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = package_dir(&dir)?;
        let mut out = Vec::new();
        walk(&root, &root, &mut out, 0);
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    })
    .await
    .map_err(|e| Error::Msg(format!("package file scan failed: {e}")))?
}

#[tauri::command(rename_all = "snake_case")]
pub fn pack_read_text(
    dir: String,
    path: String,
) -> Result<String> {
    let root = package_dir(&dir)?;
    let target = target_path(&root, &path)?;
    let meta = fs::metadata(&target).map_err(|e| Error::Msg(format!("cannot read {path}: {e}")))?;
    if !meta.is_file() {
        return msg("not a file");
    }
    if meta.len() > MAX_TEXT_BYTES {
        return msg(format!("file is larger than {MAX_TEXT_BYTES} bytes"));
    }
    let bytes = fs::read(&target).map_err(|e| Error::Msg(format!("cannot read {path}: {e}")))?;
    String::from_utf8(bytes).map_err(|_| Error::Msg(format!("{path} is not UTF-8 text")))
}

#[tauri::command(rename_all = "snake_case")]
pub fn pack_write_text(
    dir: String,
    path: String,
    text: String,
) -> Result<()> {
    let root = package_dir(&dir)?;
    write_text(&root, &path, &text)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn pack_pick_folder(title: Option<String>) -> Result<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_title(title.as_deref().unwrap_or("Папка мини-приложения"))
            .pick_folder()
            .map(|path| path.display().to_string())
    })
    .await
    .map_err(|e| Error::Msg(format!("folder dialog failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "eco-platform-pack-{tag}-{}-{}",
            std::process::id(),
            crate::artifacts::now_secs()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn safe_relative_paths_only() {
        assert!(safe_rel("manifest.json"));
        assert!(safe_rel("skills/main/SKILL.md"));
        assert!(!safe_rel("../secret.txt"));
        assert!(!safe_rel("prompt/../../x"));
        assert!(!safe_rel("C:/x"));
        assert!(!safe_rel("prompt\\system.md"));
        assert!(!safe_rel("/abs"));
    }

    #[test]
    fn import_builds_manifest_from_agents_and_skill() {
        let root = temp("import");
        fs::write(root.join("AGENTS.md"), "# Agent\n\nBe careful.\n").unwrap();
        fs::write(root.join("SKILL.md"), "# Skill\n\nDo the thing.\n").unwrap();

        let scan = scan_impl(&root).unwrap();
        assert!(!scan.has_manifest);
        assert!(scan.agents_md);
        assert!(scan.root_skill);
        assert!(scan.skills.is_empty());

        let run = import_impl(&root, "notes-demo", "Notes Demo", "files").unwrap();
        assert!(run.ok);
        assert!(root.join("prompt/system.md").is_file());
        assert!(root.join("skills/main/SKILL.md").is_file());
        assert!(!root.join("AGENTS.md").exists());
        assert!(!root.join("SKILL.md").exists());

        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["app_id"], "notes-demo");
        assert_eq!(manifest["capabilities"]["file_scopes"], json!(["workspace"]));
        assert_eq!(manifest["profile"]["system_prompt"], "prompt/system.md");
        assert_eq!(manifest["hashes"], json!({}));

        // A second import must not overwrite the generated manifest.
        assert!(import_impl(&root, "notes-demo", "Notes Demo", "files").is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn import_keeps_a_group_of_skills_and_their_resources() {
        let root = temp("skills-group");
        fs::write(root.join("AGENTS.md"), "# Agent\n\nUse the skills.\n").unwrap();
        fs::create_dir_all(root.join("skills/alpha")).unwrap();
        fs::create_dir_all(root.join("skills/beta/scripts")).unwrap();
        fs::write(root.join("skills/alpha/SKILL.md"), "# Alpha\n").unwrap();
        fs::write(root.join("skills/beta/SKILL.md"), "# Beta\n").unwrap();
        fs::write(root.join("skills/beta/scripts/run.py"), "print('ok')\n").unwrap();

        let scan = scan_impl(&root).unwrap();
        assert_eq!(scan.skills, vec!["alpha".to_string(), "beta".to_string()]);

        let run = import_impl(&root, "group-demo", "Group Demo", "files").unwrap();
        assert!(run.ok);
        assert!(root.join("skills/alpha/SKILL.md").is_file());
        assert!(root.join("skills/beta/SKILL.md").is_file());
        assert!(root.join("skills/beta/scripts/run.py").is_file());
        assert!(root.join("manifest.json").is_file());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_template_updates_file_scopes() {
        let root = temp("template");
        fs::write(root.join("AGENTS.md"), "# Agent\n").unwrap();
        import_impl(&root, "demo", "Demo", "minimal").unwrap();

        let run = apply_template_impl(&root, "files").unwrap();
        assert!(run.ok);
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["capabilities"]["file_scopes"], json!(["workspace"]));

        apply_template_impl(&root, "minimal").unwrap();
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["capabilities"]["file_scopes"], json!([]));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_and_declares_mcp_connections() {
        let root = temp("mcp");
        fs::write(
            root.join(".mcp.json"),
            r#"{"mcpServers":{"context7":{"type":"http","url":"https://mcp.example/mcp"},"local":{"command":"node"}}}"#,
        )
        .unwrap();
        fs::write(root.join("AGENTS.md"), "# Agent\n").unwrap();

        let scan = scan_impl(&root).unwrap();
        assert_eq!(scan.mcp.len(), 1);
        assert_eq!(scan.mcp[0].id, "context7");
        assert_eq!(scan.mcp[0].source, ".mcp.json");

        import_impl(&root, "demo", "Demo", "minimal").unwrap();
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
        let connections = manifest["connections"].as_array().unwrap();
        assert!(connections.iter().any(|connection| {
            connection["id"] == "context7" && connection["type"] == "http-mcp"
        }));
        assert_eq!(manifest["capabilities"]["external_writes"], json!([]));

        set_mcp_impl(&root, "context7", true).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(
            manifest["capabilities"]["external_writes"],
            json!(["context7"])
        );

        set_mcp_impl(&root, "context7", false).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["capabilities"]["external_writes"], json!([]));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn import_warns_about_undeclared_bins() {
        let root = temp("import-bins");
        fs::write(root.join("AGENTS.md"), "# Agent\n").unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("bin").join("docx-export.exe"), b"MZ").unwrap();

        let run = import_impl(&root, "demo", "Demo", "files").unwrap();
        assert!(
            run.output.contains("bin/docx-export.exe"),
            "import report must list the found exe: {}",
            run.output
        );
        assert!(
            run.output.contains("не объявлены в tools[]"),
            "import report must warn that bins are undeclared: {}",
            run.output
        );
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["tools"], json!([]));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_stays_inside_package() {
        let root = temp("write");
        let canonical = fs::canonicalize(&root).unwrap();
        write_text(&canonical, "skills/main/SKILL.md", "hello").unwrap();
        assert!(canonical.join("skills/main/SKILL.md").is_file());

        assert!(write_text(&canonical, "../escape.md", "x").is_err());
        assert!(write_text(&canonical, "bin/tool.exe", "x").is_err());
        assert!(!canonical.parent().unwrap().join("escape.md").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
