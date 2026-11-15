use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub type Result<T> = std::result::Result<T, String>;

pub const TEMPLATE_MINIMAL: &str = "minimal";
pub const TEMPLATE_FILES: &str = "files";

#[derive(Debug, Default)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn manifest_path(dir: &Path) -> PathBuf {
    dir.join("manifest.json")
}

pub fn read_manifest_text(dir: &Path) -> Result<String> {
    let path = manifest_path(dir);
    fs::read_to_string(&path).map_err(|e| format!("manifest.json unreadable: {e}"))
}

pub fn read_manifest(dir: &Path) -> Result<Value> {
    let text = read_manifest_text(dir)?;
    serde_json::from_str(&text).map_err(|e| format!("manifest.json invalid: {e}"))
}

pub fn write_manifest_text(dir: &Path, text: &str) -> Result<()> {
    fs::write(manifest_path(dir), text).map_err(|e| format!("cannot write manifest.json: {e}"))
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn app_id_ok(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn semver_ok(value: &str) -> bool {
    let core = value.trim().split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| p.parse::<u64>().is_ok())
}

pub fn rel_path_ok(value: &str) -> bool {
    if value.is_empty() || value.starts_with('/') || value.starts_with('\\') {
        return false;
    }
    if value.contains('\\') || value.contains(':') {
        return false;
    }
    let path = Path::new(value);
    path.components().all(|component| match component {
        Component::Normal(part) => !part.is_empty(),
        Component::CurDir => false,
        _ => false,
    })
}

fn file_for(dir: &Path, rel: &str) -> PathBuf {
    let mut out = dir.to_path_buf();
    for part in rel.split('/') {
        out.push(part);
    }
    out
}

fn manifest_string<'a>(manifest: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = manifest;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

fn existing_hash_keys(manifest: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(map) = manifest.get("hashes").and_then(Value::as_object) {
        for key in map.keys() {
            out.push(key.clone());
        }
    }
    out
}

fn skills_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let root = dir.join("skills");
    if !root.is_dir() {
        return out;
    }
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else if let Ok(rel) = path.strip_prefix(base) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    walk(dir, &root, &mut out);
    out.sort();
    out
}

fn tool_bins(manifest: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(tools) = manifest.get("tools").and_then(Value::as_array) {
        for tool in tools {
            if let Some(bin) = tool.get("bin").and_then(Value::as_str) {
                if !bin.is_empty() {
                    out.push(bin.to_string());
                }
            }
        }
    }
    out
}

/// Every path that must be covered by `hashes`: prompt, skills, tool binaries,
/// then any keys already present in the manifest.
fn targets_with_skills(manifest: &Value, dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |value: &str| {
        if value.is_empty() {
            return;
        }
        if !out.iter().any(|existing| existing == value) {
            out.push(value.to_string());
        }
    };
    if let Some(prompt) = manifest_string(manifest, &["profile", "system_prompt"]) {
        add(prompt);
    }
    for skill in skills_files(dir) {
        add(&skill);
    }
    for bin in tool_bins(manifest) {
        add(&bin);
    }
    for key in existing_hash_keys(manifest) {
        add(&key);
    }
    out
}

/// Recomputes `hashes` from the files on disk and rewrites only that block in
/// `manifest.json`, keeping the rest of the file byte-identical.
pub fn recompute_hashes(dir: &Path) -> Result<Vec<(String, String)>> {
    let manifest = read_manifest(dir)?;
    let targets = targets_with_skills(&manifest, dir);
    let mut entries = Vec::new();
    for rel in targets {
        if !rel_path_ok(&rel) {
            return Err(format!("hashed path is not a safe relative path: {rel}"));
        }
        let path = file_for(dir, &rel);
        if !path.is_file() {
            return Err(format!("hashed file is missing: {rel}"));
        }
        entries.push((rel.clone(), hash_file(&path)?));
    }
    let text = read_manifest_text(dir)?;
    let updated = replace_hashes_block(&text, &entries)?;
    write_manifest_text(dir, &updated)?;
    Ok(entries)
}

fn find_matching_brace(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (offset, &byte) in bytes.iter().enumerate().skip(open) {
        if in_string {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Replaces the value of the top-level `hashes` object, preserving the rest of
/// the manifest text and the file's newline style.
pub fn replace_hashes_block(text: &str, entries: &[(String, String)]) -> Result<String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let key_pos = text
        .find("\"hashes\"")
        .ok_or("manifest has no hashes object")?;
    let open = text[key_pos..]
        .find('{')
        .map(|offset| key_pos + offset)
        .ok_or("manifest has no hashes object")?;
    let close = find_matching_brace(text, open).ok_or("unbalanced hashes object")?;

    let line_start = text[..key_pos]
        .rfind(|c| c == '\n' || c == '\r')
        .map(|i| i + 1)
        .unwrap_or(0);
    let indent: String = text[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let entry_indent = format!("{indent}  ");

    let mut body = String::new();
    for (index, (rel, hash)) in entries.iter().enumerate() {
        if index > 0 {
            body.push(',');
            body.push_str(newline);
        }
        body.push_str(&entry_indent);
        body.push('"');
        body.push_str(rel);
        body.push_str("\": \"sha256:");
        body.push_str(hash);
        body.push('"');
    }

    let replacement = if entries.is_empty() {
        "{}".to_string()
    } else {
        format!("{{{newline}{body}{newline}{indent}}}")
    };
    let mut out = String::with_capacity(text.len() + replacement.len());
    out.push_str(&text[..open]);
    out.push_str(&replacement);
    out.push_str(&text[close + 1..]);
    Ok(out)
}

fn check_text_secrets(path: &Path, report: &mut Report) {
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    if meta.len() > 1024 * 1024 {
        return;
    }
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let lowered = text.to_lowercase();
    for marker in [
        "begin private key",
        "password=",
        "api_key=",
        "secret_key=",
        "sk-proj-",
        "sk-ant-",
        "xoxb-",
        "ghp_",
    ] {
        if lowered.contains(marker) {
            report.warnings.push(format!(
                "{} contains a possible secret marker: {marker}",
                path.display()
            ));
        }
    }
}

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

fn is_secret_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || name == "credentials"
        || name == "credentials.json"
}

fn walk_content_files(dir: &Path, output: &mut Vec<PathBuf>) {
    let excluded_dirs = ["engine", "sessions", ".git", "target"];
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if excluded_dirs.contains(&name.as_str()) {
                continue;
            }
            walk_content_files(&path, output);
        } else {
            output.push(path);
        }
    }
}

pub fn validate(dir: &Path) -> Result<Report> {
    let mut report = Report::default();
    let text = match read_manifest_text(dir) {
        Ok(text) => text,
        Err(e) => {
            report.errors.push(e);
            return Ok(report);
        }
    };
    let manifest: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            report.errors.push(format!("manifest.json invalid: {e}"));
            return Ok(report);
        }
    };

    match manifest.get("schema_version").and_then(Value::as_u64) {
        Some(2) => {}
        Some(other) => report
            .errors
            .push(format!("schema_version must be 2, found {other}")),
        None => report.errors.push("schema_version is missing".to_string()),
    }
    match manifest.get("app_id").and_then(Value::as_str) {
        Some(id) if app_id_ok(id) => {}
        Some(id) => report.errors.push(format!(
            "app_id must use letters, digits, '-' or '_': {id}"
        )),
        None => report.errors.push("app_id is missing".to_string()),
    }
    match manifest.get("name").and_then(Value::as_str) {
        Some(name) if !name.trim().is_empty() => {}
        _ => report.errors.push("name is missing or empty".to_string()),
    }
    match manifest.get("version").and_then(Value::as_str) {
        Some(version) if semver_ok(version) => {}
        Some(version) => report
            .errors
            .push(format!("version is not x.y.z: {version}")),
        None => report.errors.push("version is missing".to_string()),
    }
    if let Some(min) = manifest.get("min_shell_version").and_then(Value::as_str) {
        if !min.is_empty() && !semver_ok(min) {
            report
                .errors
                .push(format!("min_shell_version is not x.y.z: {min}"));
        }
    }

    match manifest_string(&manifest, &["profile", "system_prompt"]) {
        Some(prompt) if !prompt.is_empty() => {
            if !rel_path_ok(prompt) {
                report
                    .errors
                    .push(format!("system_prompt is not a safe path: {prompt}"));
            } else {
                let path = file_for(dir, prompt);
                if !path.is_file() {
                    report
                        .errors
                        .push(format!("system prompt file is missing: {prompt}"));
                } else {
                    if fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > 262_144 {
                        report
                            .warnings
                            .push(format!("system prompt is large: {prompt}"));
                    }
                    check_text_secrets(&path, &mut report);
                }
            }
        }
        _ => report
            .errors
            .push("profile.system_prompt is missing".to_string()),
    }

    if let Some(tools) = manifest.get("tools").and_then(Value::as_array) {
        for tool in tools {
            let name = tool.get("tool").and_then(Value::as_str).unwrap_or("");
            if name.is_empty() {
                report.errors.push("tools[] entry has no name".to_string());
            }
            if let Some(bin) = tool.get("bin").and_then(Value::as_str) {
                if !rel_path_ok(bin) {
                    report
                        .errors
                        .push(format!("tool {name}: bin is not a safe path: {bin}"));
                } else if !file_for(dir, bin).is_file() {
                    report
                        .errors
                        .push(format!("tool {name}: bin is missing: {bin}"));
                }
            }
            let io = tool
                .get("io")
                .and_then(Value::as_str)
                .unwrap_or("files")
                .to_lowercase();
            if io != "files" && io != "json" {
                report
                    .errors
                    .push(format!("tool {name}: io must be files or json"));
            }
            if io == "json" {
                if let Some(parameters) = tool.get("parameters") {
                    if !parameters.is_object() {
                        report
                            .errors
                            .push(format!("tool {name}: parameters must be a JSON object"));
                    }
                }
                if let Some(argv) = tool.get("argv").and_then(Value::as_array) {
                    if argv.iter().any(|arg| {
                        arg.as_str()
                            .map(|arg| arg.contains("{input}") || arg.contains("{output}"))
                            .unwrap_or(false)
                    }) {
                        report.errors.push(format!(
                            "tool {name}: io=json must not use {{input}}/{{output}} in argv"
                        ));
                    }
                }
            }
            if let Some(secrets) = tool.get("secrets") {
                let Some(secrets) = secrets.as_object() else {
                    report
                        .errors
                        .push(format!("tool {name}: secrets must be an object"));
                    continue;
                };
                if secrets.len() > 16 {
                    report
                        .errors
                        .push(format!("tool {name}: too many secrets (max 16)"));
                }
                for (env, secret) in secrets {
                    if !env_name_ok(env) {
                        report.errors.push(format!(
                            "tool {name}: invalid env name in secrets: {env}"
                        ));
                    }
                    match secret.as_str() {
                        Some(secret) if secret_name_ok(secret) => {}
                        Some(_) => report.errors.push(format!(
                            "tool {name}: invalid secret name for {env}"
                        )),
                        None => report
                            .errors
                            .push(format!("tool {name}: secret for {env} must be a string")),
                    }
                }
            }
        }
    } else if manifest.get("tools").is_some() {
        report.errors.push("tools must be an array".to_string());
    }

    if let Some(caps) = manifest.get("capabilities") {
        if let Some(scopes) = caps.get("file_scopes").and_then(Value::as_array) {
            for scope in scopes {
                match scope.as_str() {
                    Some("workspace") => {}
                    Some(other) => report
                        .errors
                        .push(format!("unknown file scope: {other}")),
                    None => report.errors.push("file_scopes must be strings".to_string()),
                }
            }
        }
        if let Some(writes) = caps.get("external_writes").and_then(Value::as_array) {
            if !writes.is_empty() {
                report
                    .warnings
                    .push("external_writes is not empty".to_string());
            }
        }
    }

    let mut declared_hashes = BTreeSet::new();
    if let Some(hashes) = manifest.get("hashes").and_then(Value::as_object) {
        for (rel, expected) in hashes {
            declared_hashes.insert(rel.clone());
            if !rel_path_ok(rel) {
                report
                    .errors
                    .push(format!("hashed path is not safe: {rel}"));
                continue;
            }
            let path = file_for(dir, rel);
            if !path.is_file() {
                report
                    .errors
                    .push(format!("hashed file is missing: {rel}"));
                continue;
            }
            let Some(expected_hex) = expected
                .as_str()
                .and_then(|value| value.strip_prefix("sha256:"))
            else {
                report
                    .errors
                    .push(format!("hash for {rel} is not sha256:..."));
                continue;
            };
            if expected_hex.len() != 64 || !expected_hex.chars().all(|c| c.is_ascii_hexdigit()) {
                report
                    .errors
                    .push(format!("hash for {rel} is not a sha256 hex digest"));
                continue;
            }
            match hash_file(&path) {
                Ok(actual) if actual == expected_hex.to_lowercase() => {}
                Ok(_) => report.errors.push(format!(
                    "stale hash for {rel}: run `eco-pack build`"
                )),
                Err(e) => report.errors.push(e),
            }
        }
    } else {
        report.errors.push("hashes object is missing".to_string());
    }

    let declared: BTreeSet<String> = targets_with_skills(&manifest, dir).into_iter().collect();
    for rel in &declared {
        if !declared_hashes.contains(rel) {
            report.errors.push(format!(
                "file is not covered by hashes: {rel} (run `eco-pack build`)"
            ));
        }
    }
    let mut files = Vec::new();
    walk_content_files(dir, &mut files);
    let mut total = 0u64;
    for path in &files {
        total += fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if path.file_name().map(|n| n == "manifest.sig").unwrap_or(false) {
            continue;
        }
        if path.file_name().map(|n| n == "manifest.json").unwrap_or(false) {
            continue;
        }
        let Ok(rel) = path.strip_prefix(dir) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        if is_secret_file(path) {
            report.errors.push(format!(
                "secret material must not be packaged: {rel}; use tools[].secrets with platform DPAPI"
            ));
            continue;
        }
        if declared.contains(&rel) {
            continue;
        }
        if path.extension().map(|e| e == "md").unwrap_or(false) {
            check_text_secrets(path, &mut report);
        }
        report
            .warnings
            .push(format!("file is not covered by hashes: {rel}"));
    }
    if total > 512 * 1024 * 1024 {
        report
            .warnings
            .push(format!("package is large: {} MiB", total / (1024 * 1024)));
    }
    Ok(report)
}

pub fn build_zip(dir: &Path, output: &Path) -> Result<()> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let canonical_dir = fs::canonicalize(dir).map_err(|e| format!("cannot resolve package: {e}"))?;
    let output_abs = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(output)
    };
    let output_abs = fs::canonicalize(&output_abs).unwrap_or(output_abs);

    let file = fs::File::create(&output_abs)
        .map_err(|e| format!("cannot create {}: {e}", output_abs.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut files = Vec::new();
    walk_content_files(&canonical_dir, &mut files);
    files.sort();
    for path in files {
        if path.file_name().map(|n| n == "manifest.sig").unwrap_or(false) {
            continue;
        }
        let canonical = fs::canonicalize(&path).map_err(|e| e.to_string())?;
        if canonical == output_abs {
            continue;
        }
        let rel = path
            .strip_prefix(&canonical_dir)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(rel, options)
            .map_err(|e| format!("zip: {e}"))?;
        let mut input = fs::File::open(&path).map_err(|e| e.to_string())?;
        std::io::copy(&mut input, &mut zip).map_err(|e| format!("zip: {e}"))?;
    }
    zip.finish()
        .map_err(|e| format!("zip: {e}"))?
        .flush()
        .map_err(|e| format!("zip: {e}"))?;
    Ok(())
}

pub fn init(
    dir: &Path,
    app_id: &str,
    name: &str,
    template: &str,
) -> Result<()> {
    if !app_id_ok(app_id) {
        return Err(format!(
            "app_id must use letters, digits, '-' or '_': {app_id}"
        ));
    }
    if manifest_path(dir).exists() {
        return Err(format!("{} already exists", manifest_path(dir).display()));
    }
    fs::create_dir_all(dir.join("prompt")).map_err(|e| e.to_string())?;
    fs::create_dir_all(dir.join("skills").join("main")).map_err(|e| e.to_string())?;

    let files_scope = if template == TEMPLATE_FILES {
        "[\"workspace\"]"
    } else {
        "[]"
    };
    let manifest = format!(
        r#"{{
  "schema_version": 2,
  "app_id": "{app_id}",
  "name": "{name}",
  "version": "0.1.0",
  "min_shell_version": "0.1.0",
  "publisher": "",
  "profile": {{
    "system_prompt": "prompt/system.md",
    "model_hint": "",
    "max_steps": 12
  }},
  "tools": [],
  "connections": [],
  "capabilities": {{
    "index_allowlist": [],
    "file_scopes": {files_scope},
    "external_writes": []
  }},
  "docs": [],
  "artifacts": {{
    "quota_mb": 50,
    "ttl_hours": 72
  }},
  "hashes": {{}}
}}
"#
    );
    fs::write(manifest_path(dir), manifest).map_err(|e| e.to_string())?;

    let prompt = format!(
        "# {name}\n\nТы — ассистент мини-приложения `{app_id}`. Опиши здесь задачу, тон и ограничения.\n"
    );
    fs::write(dir.join("prompt").join("system.md"), prompt).map_err(|e| e.to_string())?;

    let skill = format!(
        "# {name}\n\n## Когда применять\n\nОпиши триггеры навыка.\n\n## Шаги\n\n1. Уточни входные данные.\n2. Выполни задачу.\n3. Проверь результат.\n"
    );
    fs::write(
        dir.join("skills").join("main").join("SKILL.md"),
        skill,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eco-pack-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_package(tag: &str) -> PathBuf {
        let dir = temp(tag);
        init(&dir.join("app"), "demo-app", "Demo App", TEMPLATE_MINIMAL).unwrap();
        dir.join("app")
    }

    #[test]
    fn secret_names_follow_env_rules() {
        assert!(env_name_ok("SERVICE_TOKEN"));
        assert!(!env_name_ok("service_token"));
        assert!(secret_name_ok("svc.token"));
        assert!(!secret_name_ok("svc token"));
    }

    #[test]
    fn secret_files_are_rejected() {
        let app = make_package("secret-file");
        fs::write(app.join(".env"), "SERVICE_TOKEN=abc\n").unwrap();
        recompute_hashes(&app).unwrap();
        let report = validate(&app).unwrap();
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("secret material must not be packaged")));
        let _ = fs::remove_dir_all(app.parent().unwrap());
    }

    #[test]
    fn hashes_block_keeps_manifest_layout() {
        let text = "{\n  \"a\": 1,\n  \"hashes\": {\n    \"prompt/system.md\": \"sha256:aa\"\n  },\n  \"b\": 2\n}\n";
        let updated = replace_hashes_block(
            text,
            &[("prompt/system.md".into(), "01".repeat(32))],
        )
        .unwrap();
        assert!(updated.starts_with("{\n  \"a\": 1,\n  \"hashes\": {\n"));
        assert!(updated.contains("\"prompt/system.md\": \"sha256:"));
        assert!(updated.ends_with("},\n  \"b\": 2\n}\n"));
        assert!(updated.contains(&"01".repeat(32)));
    }

    #[test]
    fn hashes_block_preserves_crlf() {
        let text = "{\r\n  \"hashes\": {\r\n    \"x\": \"sha256:aa\"\r\n  }\r\n}\r\n";
        let updated = replace_hashes_block(text, &[("x".into(), "02".repeat(32))]).unwrap();
        assert!(!updated.contains("}\r\n  \"x\""));
        assert!(updated.contains("\r\n"));
        assert!(updated.contains(&"02".repeat(32)));
    }

    #[test]
    fn init_build_validate_and_zip_roundtrip() {
        let app = make_package("roundtrip");
        recompute_hashes(&app).unwrap();
        let report = validate(&app).unwrap();
        assert!(report.ok(), "{:?}", report.errors);

        let zip_path = app.parent().unwrap().join("out.zip");
        build_zip(&app, &zip_path).unwrap();

        let file = fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut names = Vec::new();
        for index in 0..archive.len() {
            names.push(archive.by_index(index).unwrap().name().to_string());
        }
        assert!(names.contains(&"manifest.json".to_string()));
        assert!(names.contains(&"prompt/system.md".to_string()));
        assert!(names.contains(&"skills/main/SKILL.md".to_string()));
        assert!(!names.iter().any(|name| name.ends_with(".sig")));

        let mut manifest = String::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut manifest)
            .unwrap();
        assert!(manifest.contains("\"hashes\""));
        assert!(manifest.contains("prompt/system.md"));
    }

    #[test]
    fn fresh_package_requires_build() {
        let app = make_package("fresh");
        let report = validate(&app).unwrap();
        assert!(!report.ok());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("not covered by hashes")));
        recompute_hashes(&app).unwrap();
        assert!(validate(&app).unwrap().ok());
    }

    #[test]
    fn stale_hash_is_reported() {
        let app = make_package("stale");
        recompute_hashes(&app).unwrap();
        fs::write(app.join("prompt/system.md"), "changed prompt").unwrap();
        let report = validate(&app).unwrap();
        assert!(!report.ok());
        assert!(report.errors.iter().any(|e| e.contains("stale hash")));
    }

    #[test]
    fn unsafe_hash_path_is_an_error() {
        let app = make_package("unsafe");
        let text = read_manifest_text(&app).unwrap().replace(
            "\"hashes\": {}",
            "\"hashes\": { \"../secret\": \"sha256:00\" }",
        );
        write_manifest_text(&app, &text).unwrap();
        let report = validate(&app).unwrap();
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("not safe") || e.contains("not a safe")));
    }

    #[test]
    fn missing_bin_is_an_error() {
        let app = make_package("missing-bin");
        let text = read_manifest_text(&app).unwrap().replace(
            "\"tools\": []",
            "\"tools\": [ { \"tool\": \"run\", \"bin\": \"bin/run.exe\" } ]",
        );
        write_manifest_text(&app, &text).unwrap();
        let report = validate(&app).unwrap();
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("bin is missing")));
    }
}

