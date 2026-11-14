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
    #[serde(default)]
    pub max_steps: Option<u32>,
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

#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub problems: Vec<String>,
    /// Fingerprint of the active anchor key that validated the signature.
    pub signer_fingerprint: Option<String>,
}

impl VerifyReport {
    fn fail(problems: Vec<String>) -> Self {
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

/// Verifies the package in `dir`: schema version, detached signature over
/// manifest.json and per-file hashes. Fail-closed: any problem makes the
/// package unusable. The signature may be made by any of the active anchors,
/// which allows staged key rotation.
pub fn verify_package(dir: &Path, trusted: &[VerifyingKey]) -> VerifyReport {
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
                            None => problems
                                .push("signature mismatch: no active anchor matches".to_string()),
                        }
                    }
                },
            },
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

    VerifyReport {
        ok: problems.is_empty(),
        problems,
        signer_fingerprint,
    }
}

pub fn find_tool<'a>(manifest: &'a Manifest, tool: &str) -> Option<&'a ToolDef> {
    manifest.tools.iter().find(|t| t.tool == tool)
}

pub fn external_writes_allowed(manifest: &Manifest) -> bool {
    !manifest.capabilities.external_writes.is_empty()
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
}
