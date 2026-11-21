use crate::error::{Error, Result};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const SUPPORTED_SCHEMA: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub system_prompt: String,
    #[serde(default)]
    pub model_hint: Option<String>,
    /// Maximum model steps in one turn. Enforced by the engine (`agent.steps`),
    /// which removes tools and asks for a summary on the final step.
    #[serde(default)]
    pub max_steps: Option<u32>,
    /// Maximum tool calls the gateway accepts in one turn. The gateway is the
    /// only path to tools, so this also bounds tool-call storms.
    #[serde(default)]
    pub max_tool_calls: Option<u32>,
    /// Maximum tool calls per rolling minute for this app.
    #[serde(default)]
    pub rate_limit_per_min: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub tool: String,
    #[serde(default)]
    pub argv: Vec<String>,
    /// Executable inside the package, relative to the package root. Used by
    /// app-provided tools such as the docx converter.
    #[serde(default)]
    pub bin: Option<String>,
    /// `files` (default): input/output paths in the workspace.
    /// `json`: the model's arguments are sent to stdin as JSON and stdout is
    /// returned as the tool result.
    #[serde(default = "io_files")]
    pub io: String,
    /// JSON Schema for `io: json`. Falls back to a free-form object.
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
    /// Env var -> DPAPI secret name. The platform injects values only into the
    /// package process for this call; secrets never live in the package.
    #[serde(default)]
    pub secrets: BTreeMap<String, String>,
    /// Wall-clock limit for one call, in seconds. Absent keeps the platform
    /// default; long-running tools declare their budget explicitly (cap 2 h).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default = "stream_read")]
    pub stream: String,
}

fn io_files() -> String {
    "files".to_string()
}

fn stream_read() -> String {
    "read".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Capabilities {
    #[serde(default)]
    pub index_allowlist: Vec<String>,
    #[serde(default)]
    pub file_scopes: Vec<String>,
    #[serde(default)]
    pub external_writes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactCfg {
    #[serde(default = "default_quota_mb")]
    pub quota_mb: u64,
    #[serde(default = "default_ttl_hours")]
    pub ttl_hours: u64,
}

fn default_quota_mb() -> u64 {
    500
}

fn default_ttl_hours() -> u64 {
    168
}

impl Default for ArtifactCfg {
    fn default() -> Self {
        Self {
            quota_mb: default_quota_mb(),
            ttl_hours: default_ttl_hours(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub app_id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub min_shell_version: String,
    #[serde(default)]
    pub publisher: String,
    /// One line shown under the app name in the catalog. Optional so packages
    /// built before this field existed keep working unchanged.
    #[serde(default)]
    pub description: Option<String>,
    /// `icon.svg` inside the package. Validated and sanitized before rendering.
    #[serde(default)]
    pub icon: Option<String>,
    pub profile: Profile,
    #[serde(default)]
    pub tools: Vec<ToolDef>,
    #[serde(default)]
    pub connections: Vec<Connection>,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default)]
    pub artifacts: ArtifactCfg,
    #[serde(default)]
    pub hashes: BTreeMap<String, String>,
}

pub const MAX_DESCRIPTION_CHARS: usize = 160;

impl Manifest {
    /// Trimmed, control-character-free description for the catalog card.
    pub fn clean_description(&self) -> Option<String> {
        let raw = self.description.as_deref()?.trim();
        let spaced: String = raw
            .chars()
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .collect();
        let clean = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
        if clean.is_empty() {
            None
        } else {
            Some(clean.chars().take(MAX_DESCRIPTION_CHARS).collect())
        }
    }

    /// Sanitized icon as a data URL, or `None` when the manifest has no icon or
    /// the file is unsafe/missing. A bad icon only loses the picture; the app
    /// itself keeps working.
    pub fn icon_data_url(&self, dir: &Path) -> Option<String> {
        let rel = self.icon.as_deref()?.trim();
        if !icon_path_ok(rel) {
            return None;
        }
        let path = dir.join(rel);
        let meta = fs::metadata(&path).ok()?;
        if !meta.is_file() || meta.len() > crate::svg_icon::MAX_ICON_BYTES {
            return None;
        }
        let bytes = fs::read(&path).ok()?;
        let svg = crate::svg_icon::sanitize(&bytes).ok()?;
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
        Some(format!("data:image/svg+xml;base64,{encoded}"))
    }
}

/// Icons are SVG-only, stay inside the package and must not use drive/stream
/// syntax or traversal segments.
fn icon_path_ok(rel: &str) -> bool {
    let rel = rel.replace('\\', "/");
    if !rel.to_ascii_lowercase().ends_with(".svg") {
        return false;
    }
    if rel.starts_with('/') || rel.contains(':') {
        return false;
    }
    !rel
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub problems: Vec<String>,
    /// Fingerprint of the active anchor key that validated the signature.
    pub signer_fingerprint: Option<String>,
}

impl VerifyReport {
    pub(crate) fn fail(problems: Vec<String>) -> Self {
        Self {
            ok: problems.is_empty(),
            problems,
            signer_fingerprint: None,
        }
    }
}

pub fn load(dir: &Path) -> Result<Manifest> {
    let raw = fs::read_to_string(dir.join("manifest.json"))?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Version format is `major.minor.patch`; a `-suffix` or `+suffix` is ignored
/// for ordering. Used for `min_shell_version` and downgrade protection.
pub fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let core = raw.trim().split(|c| c == '-' || c == '+').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn shell_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Rejects a lower version than the installed one. Equal versions are allowed:
/// re-installing a rebuilt package is a content update, not a downgrade.
pub fn ensure_not_downgrade(current: &str, next: &str) -> Result<()> {
    let current = parse_version(current)
        .ok_or_else(|| Error::Msg(format!("installed version is not x.y.z: {current}")))?;
    let next = parse_version(next)
        .ok_or_else(|| Error::Msg(format!("package version is not x.y.z: {next}")))?;
    if next < current {
        return Err(Error::Msg(format!(
            "downgrade rejected: installed version is higher than package version"
        )));
    }
    Ok(())
}

/// Verifies package integrity only: schema version, version format, the
/// system prompt file and every declared sha256. It does not require a trust
/// anchor. Local mode pairs this with a local seal; corporate mode uses
/// [`verify_package`].
pub fn verify_integrity(dir: &Path) -> VerifyReport {
    verify_impl(dir, None, false, true)
}

/// Everything `verify_integrity` checks except file hashes. Used in local mode
/// when the seal (and its verification cache) already vouches for the file
/// contents, so listing apps does not re-hash every package file.
pub fn verify_integrity_meta(dir: &Path) -> VerifyReport {
    verify_impl(dir, None, false, false)
}

/// Install-time verification: additionally requires the signed hash list to
/// cover every file the engine will read or execute.
pub fn verify_integrity_install(dir: &Path) -> VerifyReport {
    verify_impl(dir, None, true, true)
}

/// Verifies the package in `dir`: schema version, detached signature over
/// manifest.json and per-file hashes. Fail-closed: any problem makes the
/// package unusable. The signature may be made by any of the active anchors,
/// which allows staged key rotation.
pub fn verify_package(dir: &Path, trusted: &[VerifyingKey]) -> VerifyReport {
    verify_impl(dir, Some(trusted), false, true)
}

pub fn verify_package_install(dir: &Path, trusted: &[VerifyingKey]) -> VerifyReport {
    verify_impl(dir, Some(trusted), true, true)
}

fn verify_impl(
    dir: &Path,
    trusted: Option<&[VerifyingKey]>,
    require_coverage: bool,
    hash_files: bool,
) -> VerifyReport {
    let manifest_bytes = match fs::read(dir.join("manifest.json")) {
        Ok(b) => b,
        Err(e) => return VerifyReport::fail(vec![format!("manifest.json unreadable: {e}")]),
    };
    let manifest: Manifest = match serde_json::from_slice(&manifest_bytes) {
        Ok(m) => m,
        Err(e) => return VerifyReport::fail(vec![format!("manifest.json invalid: {e}")]),
    };

    let mut problems = Vec::new();
    let mut signer_fingerprint = None;

    if manifest.schema_version > SUPPORTED_SCHEMA {
        problems.push(format!(
            "schema_version {} is newer than supported {}",
            manifest.schema_version, SUPPORTED_SCHEMA
        ));
    }

    if parse_version(&manifest.version).is_none() {
        problems.push(format!(
            "version {} is not in major.minor.patch form",
            manifest.version
        ));
    }

    if !manifest.min_shell_version.trim().is_empty() {
        match parse_version(&manifest.min_shell_version) {
            Some(required) => {
                if let Some(shell) = parse_version(shell_version()) {
                    if required > shell {
                        problems.push(format!(
                            "min_shell_version {} is newer than shell {}",
                            manifest.min_shell_version,
                            shell_version()
                        ));
                    }
                }
            }
            None => problems.push(format!(
                "min_shell_version {} is not in major.minor.patch form",
                manifest.min_shell_version
            )),
        }
    }

    if let Some(trusted) = trusted {
        if trusted.is_empty() {
            problems.push("trust anchor missing: trusted.pub not installed".to_string());
        } else {
            match fs::read_to_string(dir.join("manifest.sig")) {
                Err(e) => problems.push(format!("manifest.sig unreadable: {e}")),
                Ok(raw) => match hex::decode(raw.trim()) {
                    Err(e) => problems.push(format!("manifest.sig is not hex: {e}")),
                    Ok(bytes) => match Signature::from_slice(&bytes) {
                        Err(e) => problems.push(format!("manifest.sig invalid: {e}")),
                        Ok(sig) => {
                            let signer = trusted
                                .iter()
                                .find(|key| key.verify_strict(&manifest_bytes, &sig).is_ok());
                            match signer {
                                Some(key) => {
                                    let digest = sha256_hex(&key.to_bytes());
                                    signer_fingerprint = Some(digest[..16].to_string());
                                }
                                None => problems.push(
                                    "signature mismatch: no active anchor matches".to_string(),
                                ),
                            }
                        }
                    },
                },
            }
        }
    }

    if !manifest.profile.system_prompt.is_empty() {
        let prompt_path = dir.join(&manifest.profile.system_prompt);
        if !prompt_path.exists() {
            problems.push(format!(
                "system prompt file missing: {}",
                manifest.profile.system_prompt
            ));
        }
    }

    if hash_files {
        for (rel, expected) in &manifest.hashes {
            let Some(expected_hex) = expected.strip_prefix("sha256:") else {
                problems.push(format!("hash for {rel} is not sha256:..."));
                continue;
            };
            match fs::read(dir.join(rel)) {
                Err(e) => problems.push(format!("hashed file {rel} unreadable: {e}")),
                Ok(bytes) => {
                    let actual = sha256_hex(&bytes);
                    if actual != expected_hex {
                        problems.push(format!("hash mismatch for {rel}"));
                    }
                }
            }
        }
    }

    // Install-time only: a package must cover every file the engine will read
    // or execute. Already-installed packages stay openable after an upgrade.
    if require_coverage {
        if !manifest.profile.system_prompt.is_empty() {
            let rel = manifest.profile.system_prompt.replace('\\', "/");
            if !manifest.hashes.contains_key(&rel) {
                problems.push(format!("system prompt is not covered by hashes: {rel}"));
            }
        }
        if let Ok(entries) = fs::read_dir(dir.join("skills")) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() || !entry.path().join("SKILL.md").is_file() {
                    continue;
                }
                let rel = format!("skills/{}/SKILL.md", entry.file_name().to_string_lossy());
                if !manifest.hashes.contains_key(&rel) {
                    problems.push(format!("skill is not covered by hashes: {rel}"));
                }
            }
        }
        for tool in &manifest.tools {
            if let Some(bin) = &tool.bin {
                let rel = bin.replace('\\', "/");
                if !manifest.hashes.contains_key(&rel) {
                    problems.push(format!("tool binary is not covered by hashes: {rel}"));
                }
            }
        }
        if let Some(icon) = &manifest.icon {
            let rel = icon.replace('\\', "/");
            if !manifest.hashes.contains_key(&rel) {
                problems.push(format!("icon is not covered by hashes: {rel}"));
            }
        }
    }

    // Reserved platform secrets are never valid for package code, including
    // for already-installed packages (the proxy also filters them at runtime).
    for tool in &manifest.tools {
        for secret in tool.secrets.values() {
            if is_reserved_secret(secret) {
                problems.push(format!(
                    "tool {} requests reserved platform secret '{secret}'",
                    tool.tool
                ));
            }
        }
    }

    VerifyReport {
        ok: problems.is_empty(),
        problems,
        signer_fingerprint,
    }
}

pub fn find_tool<'a>(manifest: &'a Manifest, tool: &str) -> Option<&'a ToolDef> {
    manifest.tools.iter().find(|t| t.tool == tool)
}

/// Platform secrets that must never be handed to package code. Windows file
/// names are case-insensitive, so the comparison is case-folded.
pub fn is_reserved_secret(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    name == "llm_api_key"
        || name.starts_with("mcp.")
        || name.starts_with("llm.")
        || name.starts_with("app.")
}

pub fn external_write_allowed(manifest: &Manifest, connection_id: &str) -> bool {
    manifest
        .capabilities
        .external_writes
        .iter()
        .any(|id| id == connection_id)
}

pub fn parse_id(raw: &str, what: &str) -> Result<()> {
    if raw.is_empty() {
        return Err(Error::Msg(format!("{what} id is empty")));
    }
    if !raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::Msg(format!("{what} id has invalid characters: {raw}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("eco-manifest-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const TEMPLATE: &str = r#"{
      "schema_version": 2,
      "app_id": "demo",
      "name": "Demo",
      "version": "1.0.0",
      "profile": { "system_prompt": "prompt.md" },
      "hashes": { "prompt.md": "sha256:HASH" }
    }"#;

    #[test]
    fn signature_and_hashes_are_enforced() {
        let dir = tmp("sig");
        std::fs::write(dir.join("prompt.md"), b"hello").unwrap();
        let body = TEMPLATE.replace("HASH", &sha256_hex(b"hello"));
        std::fs::write(dir.join("manifest.json"), &body).unwrap();

        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let sig = signing.sign(body.as_bytes());
        std::fs::write(dir.join("manifest.sig"), hex::encode(sig.to_bytes())).unwrap();

        let trusted = signing.verifying_key();
        let report = verify_package(&dir, &[trusted]);
        assert!(report.ok, "{:?}", report.problems);

        assert!(!verify_package(&dir, &[]).ok);

        std::fs::write(dir.join("prompt.md"), b"tampered").unwrap();
        let report = verify_package(&dir, &[trusted]);
        assert!(!report.ok);
        assert!(report.problems.iter().any(|p| p.contains("hash mismatch")));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn newer_schema_is_rejected() {
        let dir = tmp("schema");
        let body = TEMPLATE
            .replace("HASH", "00")
            .replace("\"schema_version\": 2", "\"schema_version\": 99");
        std::fs::write(dir.join("manifest.json"), &body).unwrap();
        std::fs::write(dir.join("prompt.md"), b"hello").unwrap();
        let signing = SigningKey::from_bytes(&[9u8; 32]);
        let sig = signing.sign(body.as_bytes());
        std::fs::write(dir.join("manifest.sig"), hex::encode(sig.to_bytes())).unwrap();
        let report = verify_package(&dir, &[signing.verifying_key()]);
        assert!(!report.ok);
        assert!(report.problems.iter().any(|p| p.contains("schema_version")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn versions_are_compared_and_shell_constraint_is_enforced() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
        assert!(parse_version("1.2").is_none());
        assert!(parse_version("x.y.z").is_none());

        assert!(ensure_not_downgrade("1.2.0", "1.1.9").is_err());
        assert!(ensure_not_downgrade("1.2.0", "1.2.0").is_ok());
        assert!(ensure_not_downgrade("1.2.0", "1.3.0").is_ok());

        let dir = tmp("shell");
        std::fs::write(dir.join("prompt.md"), b"hello").unwrap();
        let base = TEMPLATE.replace("HASH", &sha256_hex(b"hello"));
        let body = base.replace(
            "\"version\": \"1.0.0\",",
            "\"version\": \"1.0.0\",\n      \"min_shell_version\": \"99.0.0\",",
        );
        std::fs::write(dir.join("manifest.json"), &body).unwrap();
        let signing = SigningKey::from_bytes(&[5u8; 32]);
        let sig = signing.sign(body.as_bytes());
        std::fs::write(dir.join("manifest.sig"), hex::encode(sig.to_bytes())).unwrap();

        let report = verify_package(&dir, &[signing.verifying_key()]);
        assert!(!report.ok);
        assert!(report
            .problems
            .iter()
            .any(|p| p.contains("min_shell_version")));

        let body = base.replace(
            "\"version\": \"1.0.0\",",
            "\"version\": \"1.0.0\",\n      \"min_shell_version\": \"0.0.1\",",
        );
        std::fs::write(dir.join("manifest.json"), &body).unwrap();
        let sig = signing.sign(body.as_bytes());
        std::fs::write(dir.join("manifest.sig"), hex::encode(sig.to_bytes())).unwrap();
        let report = verify_package(&dir, &[signing.verifying_key()]);
        assert!(report.ok, "{:?}", report.problems);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn external_writes_are_scoped_per_connection() {
        let app: Manifest = serde_json::from_str(
            r#"{"schema_version":2,"app_id":"a","name":"A","version":"1.0.0",
                "profile":{"system_prompt":""},
                "capabilities":{"external_writes":["alpha"]}}"#,
        )
        .unwrap();
        assert!(external_write_allowed(&app, "alpha"));
        assert!(!external_write_allowed(&app, "beta"));
    }

    #[test]
    fn skills_and_bins_must_be_covered_by_hashes() {
        // Install-time gate only; opening an already-installed package must
        // not be blocked by a legacy manifest without full coverage.
        let dir = tmp("coverage");
        std::fs::create_dir_all(dir.join("prompt")).unwrap();
        std::fs::create_dir_all(dir.join("skills/extra")).unwrap();
        std::fs::write(dir.join("prompt/system.md"), b"p").unwrap();
        std::fs::write(dir.join("skills/extra/SKILL.md"), b"s").unwrap();
        let body = format!(
            r#"{{"schema_version":2,"app_id":"a","name":"A","version":"1.0.0","profile":{{"system_prompt":"prompt/system.md"}},"hashes":{{"prompt/system.md":"sha256:{}"}}}}"#,
            sha256_hex(b"p")
        );
        std::fs::write(dir.join("manifest.json"), &body).unwrap();
        let report = verify_integrity_install(&dir);
        assert!(!report.ok);
        assert!(
            report.problems.iter().any(|p| p.contains("skill is not covered")),
            "{:?}",
            report.problems
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn meta_integrity_skips_file_hash_checks() {
        let dir = tmp("meta");
        std::fs::create_dir_all(dir.join("prompt")).unwrap();
        std::fs::write(dir.join("prompt/system.md"), b"p").unwrap();
        let body = format!(
            r#"{{"schema_version":2,"app_id":"a","name":"A","version":"1.0.0","profile":{{"system_prompt":"prompt/system.md"}},"hashes":{{"prompt/system.md":"sha256:{}"}}}}"#,
            sha256_hex(b"p")
        );
        std::fs::write(dir.join("manifest.json"), &body).unwrap();

        // A changed file is caught by the hash pass...
        std::fs::write(dir.join("prompt/system.md"), b"tampered").unwrap();
        assert!(!verify_integrity(&dir).ok);
        // ...but the metadata-only pass leaves content to the seal.
        assert!(verify_integrity_meta(&dir).ok);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn package_tools_cannot_request_platform_secrets() {
        let dir = tmp("tools-secret");
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin/x.exe"), b"x").unwrap();
        let body = format!(
            r#"{{"schema_version":2,"app_id":"a","name":"A","version":"1.0.0",
                "profile":{{"system_prompt":""}},
                "tools":[{{"tool":"x","bin":"bin/x.exe","secrets":{{"TOKEN":"mcp.1"}}}}],
                "hashes":{{"bin/x.exe":"sha256:{}"}}}}"#,
            sha256_hex(b"x")
        );
        std::fs::write(dir.join("manifest.json"), &body).unwrap();
        let report = verify_integrity(&dir);
        assert!(!report.ok);
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.contains("reserved platform secret")),
            "{:?}",
            report.problems
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reserved_secrets_are_case_insensitive() {
        let dir = tmp("secret-case");
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin/x.exe"), b"x").unwrap();
        let body = format!(
            r#"{{"schema_version":2,"app_id":"a","name":"A","version":"1.0.0",
                "profile":{{"system_prompt":""}},
                "tools":[{{"tool":"x","bin":"bin/x.exe","secrets":{{"TOKEN":"MCP.1"}}}}],
                "hashes":{{"bin/x.exe":"sha256:{}"}}}}"#,
            sha256_hex(b"x")
        );
        std::fs::write(dir.join("manifest.json"), &body).unwrap();
        let report = verify_integrity(&dir);
        assert!(!report.ok);
        assert!(
            report
                .problems
                .iter()
                .any(|p| p.contains("reserved platform secret")),
            "{:?}",
            report.problems
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn old_manifest_without_description_and_icon_still_parses() {
        let dir = tmp("icon-old");
        std::fs::write(dir.join("prompt.md"), b"p").unwrap();
        let plain = r#"{"schema_version":2,"app_id":"demo","name":"Demo","version":"1.0.0",
            "profile":{"system_prompt":"prompt.md"},"hashes":{}}"#;
        std::fs::write(dir.join("manifest.json"), plain).unwrap();
        let manifest = load(&dir).unwrap();
        assert!(manifest.description.is_none());
        assert!(manifest.icon.is_none());
        assert!(manifest.clean_description().is_none());
        assert!(manifest.icon_data_url(&dir).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn icon_is_sanitized_and_unsafe_paths_are_ignored() {
        let dir = tmp("icon-load");
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8" fill="#123"/></svg>"##;
        std::fs::write(dir.join("icon.svg"), svg).unwrap();
        let body = r#"{"schema_version":2,"app_id":"demo","name":"Demo","version":"1.0.0",
            "description":"  Line one\nline two  ",
            "icon":"icon.svg",
            "profile":{"system_prompt":""},"hashes":{}}"#;
        std::fs::write(dir.join("manifest.json"), body).unwrap();
        let manifest = load(&dir).unwrap();
        assert_eq!(
            manifest.clean_description().as_deref(),
            Some("Line one line two")
        );
        let url = manifest.icon_data_url(&dir).unwrap();
        assert!(url.starts_with("data:image/svg+xml;base64,"));

        let mut unsafe_manifest = manifest.clone();
        unsafe_manifest.icon = Some("../icon.svg".to_string());
        assert!(unsafe_manifest.icon_data_url(&dir).is_none());
        unsafe_manifest.icon = Some("icon.png".to_string());
        assert!(unsafe_manifest.icon_data_url(&dir).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
