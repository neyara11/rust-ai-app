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
    #[serde(default = "stream_read")]
    pub stream: String,
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
pub struct DocDef {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub export: Vec<String>,
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
    pub docs: Vec<DocDef>,
    #[serde(default)]
    pub artifacts: ArtifactCfg,
    #[serde(default)]
    pub hashes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub problems: Vec<String>,
}

impl VerifyReport {
    fn fail(problems: Vec<String>) -> Self {
        Self {
            ok: problems.is_empty(),
            problems,
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

/// Verifies the package in `dir`: schema version, detached signature over
/// manifest.json and per-file hashes. Fail-closed: any problem makes the
/// package unusable.
pub fn verify_package(dir: &Path, trusted: Option<&VerifyingKey>) -> VerifyReport {
    let manifest_bytes = match fs::read(dir.join("manifest.json")) {
        Ok(b) => b,
        Err(e) => return VerifyReport::fail(vec![format!("manifest.json unreadable: {e}")]),
    };
    let manifest: Manifest = match serde_json::from_slice(&manifest_bytes) {
        Ok(m) => m,
        Err(e) => return VerifyReport::fail(vec![format!("manifest.json invalid: {e}")]),
    };

    let mut problems = Vec::new();

    if manifest.schema_version > SUPPORTED_SCHEMA {
        problems.push(format!(
            "schema_version {} is newer than supported {}",
            manifest.schema_version, SUPPORTED_SCHEMA
        ));
    }

    match trusted {
        None => problems.push("trust anchor missing: trusted.pub not installed".to_string()),
        Some(key) => match fs::read_to_string(dir.join("manifest.sig")) {
            Err(e) => problems.push(format!("manifest.sig unreadable: {e}")),
            Ok(raw) => match hex::decode(raw.trim()) {
                Err(e) => problems.push(format!("manifest.sig is not hex: {e}")),
                Ok(bytes) => match Signature::from_slice(&bytes) {
                    Err(e) => problems.push(format!("manifest.sig invalid: {e}")),
                    Ok(sig) => {
                        if let Err(e) = key.verify_strict(&manifest_bytes, &sig) {
                            problems.push(format!("signature mismatch: {e}"));
                        }
                    }
                },
            },
        },
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

    VerifyReport::fail(problems)
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
        let report = verify_package(&dir, Some(&trusted));
        assert!(report.ok, "{:?}", report.problems);

        assert!(!verify_package(&dir, None).ok);

        std::fs::write(dir.join("prompt.md"), b"tampered").unwrap();
        let report = verify_package(&dir, Some(&trusted));
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
        let report = verify_package(&dir, Some(&signing.verifying_key()));
        assert!(!report.ok);
        assert!(report.problems.iter().any(|p| p.contains("schema_version")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
