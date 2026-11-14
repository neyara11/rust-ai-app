//! Signed allowlist of approved mini-apps.
//!
//! The security team signs `allowlist.json` (detached `allowlist.sig`) with the
//! same key family as packages. The platform trusts the allowlist only if the
//! signature verifies against an active trust anchor. A present but invalid or
//! expired document is fail-closed: nothing can be installed or opened.
//!
//! Layout:
//! `%ProgramData%\EcoAIOps\allowlist.json` + `allowlist.sig` for the machine
//! scope (delivered by GPO/Intune). Development builds fall back to
//! `<home>\allowlist.json` only when `dev_mode` is on. No file means "no
//! enterprise allowlist configured", so package signature verification alone
//! decides.

use crate::error::{msg, Result};
use crate::manifest;
use crate::registry;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const ALLOWLIST_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedApp {
    pub app_id: String,
    /// Exact versions approved for this app. Empty means any signed version.
    #[serde(default)]
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowlistDoc {
    pub schema_version: u32,
    #[serde(default)]
    pub issued_at: Option<u64>,
    /// Reject the whole document after this time. Optional, but recommended.
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub apps: Vec<AllowedApp>,
    /// Apps revoked regardless of version; opening them is denied as well.
    #[serde(default)]
    pub revoked: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowlistSource {
    Machine,
    Dev,
}

impl AllowlistSource {
    pub fn as_str(self) -> &'static str {
        match self {
            AllowlistSource::Machine => "machine",
            AllowlistSource::Dev => "dev",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AllowlistInfo {
    pub status: String,
    pub source: String,
    pub path: String,
    pub apps: usize,
    pub revoked: usize,
    pub issued_at: Option<u64>,
    pub expires_at: Option<u64>,
    pub fingerprint: Option<String>,
    pub problems: Vec<String>,
}

impl AllowlistInfo {
    fn absent() -> Self {
        Self {
            status: "absent".into(),
            source: "none".into(),
            path: String::new(),
            apps: 0,
            revoked: 0,
            issued_at: None,
            expires_at: None,
            fingerprint: None,
            problems: Vec::new(),
        }
    }

    pub fn valid(&self) -> bool {
        self.status == "valid" || self.status == "absent"
    }
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Reads the machine ProgramData path from HKLM instead of the process
/// environment: a standard user can override `ProgramData` in their own
/// environment, which would otherwise let them point the machine allowlist at
/// an empty directory and silently disable it.
#[cfg(windows)]
fn program_data_from_registry() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
        REG_EXPAND_SZ, REG_SZ,
    };

    let sub = wide("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment");
    let name = wide("ProgramData");
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, sub.as_ptr(), 0, KEY_READ, &mut key) != 0 {
            return None;
        }
        let mut kind: u32 = 0;
        let mut len: u32 = 0;
        if RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null(),
            &mut kind,
            std::ptr::null_mut(),
            &mut len,
        ) != 0
            || (kind != REG_SZ && kind != REG_EXPAND_SZ)
            || len == 0
            || len > 4096
        {
            RegCloseKey(key);
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        if RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null(),
            &mut kind,
            buf.as_mut_ptr(),
            &mut len,
        ) != 0
        {
            RegCloseKey(key);
            return None;
        }
        RegCloseKey(key);
        let units: Vec<u16> = buf
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        let text = String::from_utf16_lossy(&units);
        let text = text.trim_end_matches('\0').trim();
        if text.is_empty() {
            None
        } else {
            Some(PathBuf::from(text))
        }
    }
}

#[cfg(windows)]
fn program_data() -> PathBuf {
    program_data_from_registry().unwrap_or_else(|| PathBuf::from("C:\\ProgramData"))
}

#[cfg(not(windows))]
fn program_data() -> PathBuf {
    std::env::var("ProgramData")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\ProgramData"))
}

pub fn machine_allowlist_path() -> PathBuf {
    program_data().join("EcoAIOps").join("allowlist.json")
}

pub fn dev_allowlist_path(home: &Path) -> PathBuf {
    home.join("allowlist.json")
}

/// Machine scope wins. The env override is debug-only and never shadows a
/// machine document, so a user cannot substitute a stale signed allowlist in a
/// release build. Tests use the profile copy.
pub fn resolve_path(home: &Path) -> Option<(PathBuf, AllowlistSource)> {
    if cfg!(test) {
        let path = dev_allowlist_path(home);
        return path.is_file().then_some((path, AllowlistSource::Dev));
    }
    let machine = machine_allowlist_path();
    if machine.is_file() {
        return Some((machine, AllowlistSource::Machine));
    }
    if cfg!(debug_assertions) {
        if let Some(path) = std::env::var("ECO_ALLOWLIST_PATH")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
        {
            if path.is_file() {
                return Some((path, AllowlistSource::Dev));
            }
        }
    }
    if registry::dev_mode() {
        let dev = dev_allowlist_path(home);
        if dev.is_file() {
            return Some((dev, AllowlistSource::Dev));
        }
    }
    None
}

pub fn signature_path(document: &Path) -> PathBuf {
    document.with_extension("sig")
}

fn read_signatures(raw: &str) -> Vec<Signature> {
    raw.split_whitespace()
        .filter_map(|token| hex::decode(token).ok())
        .filter_map(|bytes| Signature::from_slice(&bytes).ok())
        .collect()
}

/// True if any signature in `allowlist.sig` verifies under any active anchor.
fn verify_signatures(
    document: &Path,
    bytes: &[u8],
    keys: &[VerifyingKey],
) -> std::result::Result<Option<String>, Vec<String>> {
    if keys.is_empty() {
        return Err(vec![
            "trust anchor missing: cannot verify the allowlist".to_string()
        ]);
    }
    let sig_path = signature_path(document);
    let raw = match fs::read_to_string(&sig_path) {
        Ok(raw) => raw,
        Err(e) => return Err(vec![format!("{} is unreadable: {e}", sig_path.display())]),
    };
    let signatures = read_signatures(&raw);
    if signatures.is_empty() {
        return Err(vec![format!(
            "{} contains no valid Ed25519 signature",
            sig_path.display()
        )]);
    }
    for key in keys {
        for signature in &signatures {
            if key.verify_strict(bytes, signature).is_ok() {
                let digest = manifest::sha256_hex(&key.to_bytes());
                return Ok(Some(digest[..16].to_string()));
            }
        }
    }
    Err(vec![
        "allowlist signature does not match any active trust anchor".to_string(),
    ])
}

fn validate(doc: &AllowlistDoc) -> Vec<String> {
    let mut problems = Vec::new();
    if doc.schema_version > ALLOWLIST_SCHEMA {
        problems.push(format!(
            "schema_version {} is newer than supported {}",
            doc.schema_version, ALLOWLIST_SCHEMA
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for app in &doc.apps {
        if let Err(e) = manifest::parse_id(&app.app_id, "app") {
            problems.push(format!("allowlist app entry: {e}"));
        }
        if !seen.insert(app.app_id.clone()) {
            problems.push(format!("allowlist lists {} more than once", app.app_id));
        }
        for version in &app.versions {
            if manifest::parse_version(version).is_none() {
                problems.push(format!(
                    "allowlist version {version} for {} is not x.y.z",
                    app.app_id
                ));
            }
        }
    }
    for app_id in &doc.revoked {
        if let Err(e) = manifest::parse_id(app_id, "app") {
            problems.push(format!("allowlist revoked entry: {e}"));
        }
    }
    problems
}

fn decision(doc: &AllowlistDoc, app_id: &str, version: &str) -> Result<()> {
    if doc.revoked.iter().any(|revoked| revoked == app_id) {
        return msg(format!("app {app_id} is revoked by the allowlist"));
    }
    let Some(entry) = doc.apps.iter().find(|entry| entry.app_id == app_id) else {
        return msg(format!("app {app_id} is not approved by the allowlist"));
    };
    if entry.versions.is_empty() {
        return Ok(());
    }
    if entry.versions.iter().any(|allowed| allowed == version) {
        Ok(())
    } else {
        msg(format!(
            "version {version} of {app_id} is not approved by the allowlist"
        ))
    }
}

/// Result of a single allowlist load: reporting fields plus the document when
/// it is valid. Install/open/rollback and the UI share this one code path, so
/// validation and expiry semantics cannot drift.
pub struct AllowlistGuard {
    info: AllowlistInfo,
    doc: Option<AllowlistDoc>,
}

impl AllowlistGuard {
    pub fn info(&self) -> &AllowlistInfo {
        &self.info
    }

    pub fn decision(&self, app_id: &str, version: &str) -> Result<()> {
        if self.info.status == "absent" {
            return Ok(());
        }
        let Some(doc) = &self.doc else {
            let detail = if self.info.problems.is_empty() {
                format!("allowlist is {}", self.info.status)
            } else {
                self.info.problems.join("; ")
            };
            return msg(format!("allowlist rejected: {detail}"));
        };
        decision(doc, app_id, version)
    }
}

fn inspect(
    path: &Path,
    source: &str,
    keys: &[VerifyingKey],
) -> (AllowlistInfo, Option<AllowlistDoc>) {
    let mut out = AllowlistInfo {
        status: "invalid".into(),
        source: source.into(),
        path: path.display().to_string(),
        apps: 0,
        revoked: 0,
        issued_at: None,
        expires_at: None,
        fingerprint: None,
        problems: Vec::new(),
    };
    if !signature_path(path).is_file() {
        out.status = "incomplete".into();
        out.problems.push(format!(
            "{} is missing: deliver allowlist.sig before allowlist.json",
            signature_path(path).display()
        ));
        return (out, None);
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            out.problems
                .push(format!("{} is unreadable: {e}", path.display()));
            return (out, None);
        }
    };
    let doc: AllowlistDoc = match serde_json::from_slice(&bytes) {
        Ok(doc) => doc,
        Err(e) => {
            out.problems
                .push(format!("{} is invalid JSON: {e}", path.display()));
            return (out, None);
        }
    };
    out.apps = doc.apps.len();
    out.revoked = doc.revoked.len();
    out.issued_at = doc.issued_at;
    out.expires_at = doc.expires_at;

    let mut problems = validate(&doc);
    match verify_signatures(path, &bytes, keys) {
        Ok(fingerprint) => out.fingerprint = fingerprint,
        Err(mut sig_problems) => problems.append(&mut sig_problems),
    }
    if let Some(expires_at) = doc.expires_at {
        if expires_at <= registry::now_secs() {
            out.status = "expired".into();
            problems.push(format!("allowlist expired at unix {expires_at}"));
        }
    }
    if problems.is_empty() {
        out.status = "valid".into();
        out.problems.clear();
        (out, Some(doc))
    } else {
        out.problems = problems;
        (out, None)
    }
}

/// Loads and verifies the resolved allowlist once, for reuse across all apps.
pub fn guard(home: &Path, keys: &[VerifyingKey]) -> AllowlistGuard {
    match resolve_path(home) {
        None => AllowlistGuard {
            info: AllowlistInfo::absent(),
            doc: None,
        },
        Some((path, source)) => {
            let (info, doc) = inspect(&path, source.as_str(), keys);
            AllowlistGuard { info, doc }
        }
    }
}

/// Loads and verifies the resolved allowlist. Absent document is not an error.
pub fn info(home: &Path, keys: &[VerifyingKey]) -> AllowlistInfo {
    guard(home, keys).info
}

/// Inspects a specific document, e.g. a signed allowlist before rollout.
pub fn inspect_file(path: &Path, source: &str, keys: &[VerifyingKey]) -> AllowlistInfo {
    inspect(path, source, keys).0
}

/// Gate for install, launch, and rollback. Absent allowlist keeps the previous
/// signature-only policy; a broken or expired allowlist blocks everything.
pub fn enforce(home: &Path, keys: &[VerifyingKey], app_id: &str, version: &str) -> Result<()> {
    guard(home, keys).decision(app_id, version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eco-allowlist-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_signed(home: &Path, body: &str, key: &SigningKey) {
        let dir = home.join("allowlist.json");
        fs::write(&dir, body).unwrap();
        let sig = key.sign(body.as_bytes());
        fs::write(signature_path(&dir), hex::encode(sig.to_bytes())).unwrap();
    }

    #[test]
    fn valid_document_allows_listed_apps_and_pins_versions() {
        let home = home("valid");
        let key = SigningKey::from_bytes(&[21u8; 32]);
        write_signed(
            &home,
            r#"{"schema_version":1,"apps":[{"app_id":"demo","versions":["1.0.0"]},{"app_id":"any"}],"revoked":["bad"]}"#,
            &key,
        );
        let keys = [key.verifying_key()];
        assert!(enforce(&home, &keys, "demo", "1.0.0").is_ok());
        assert!(enforce(&home, &keys, "any", "9.9.9").is_ok());
        assert!(enforce(&home, &keys, "demo", "1.1.0").is_err());
        assert!(enforce(&home, &keys, "bad", "1.0.0").is_err());
        assert!(enforce(&home, &keys, "missing", "1.0.0").is_err());
        let info = info(&home, &keys);
        assert_eq!(info.status, "valid", "{:?}", info.problems);
        assert_eq!(info.apps, 2);

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn absent_document_does_not_restrict() {
        let home = home("absent");
        let key = SigningKey::from_bytes(&[22u8; 32]);
        let keys = [key.verifying_key()];
        assert!(enforce(&home, &keys, "anything", "1.0.0").is_ok());
        assert_eq!(info(&home, &keys).status, "absent");
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn tampered_and_expired_documents_are_fail_closed() {
        let home = home("tampered");
        let key = SigningKey::from_bytes(&[23u8; 32]);
        write_signed(
            &home,
            r#"{"schema_version":1,"apps":[{"app_id":"demo"}]}"#,
            &key,
        );
        let keys = [key.verifying_key()];
        assert!(enforce(&home, &keys, "demo", "1.0.0").is_ok());

        let doc = home.join("allowlist.json");
        fs::write(&doc, r#"{"schema_version":1,"apps":[]}"#).unwrap();
        assert!(enforce(&home, &keys, "demo", "1.0.0").is_err());

        write_signed(
            &home,
            r#"{"schema_version":1,"expires_at":1,"apps":[{"app_id":"demo"}]}"#,
            &key,
        );
        let info = info(&home, &keys);
        assert_eq!(info.status, "expired", "{:?}", info.problems);
        assert!(enforce(&home, &keys, "demo", "1.0.0").is_err());

        let wrong = SigningKey::from_bytes(&[24u8; 32]);
        assert!(enforce(&home, &[wrong.verifying_key()], "demo", "1.0.0").is_err());

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn missing_signature_is_reported_incomplete_and_denied() {
        let home = home("incomplete");
        fs::write(
            home.join("allowlist.json"),
            r#"{"schema_version":1,"apps":[{"app_id":"demo"}]}"#,
        )
        .unwrap();
        let key = SigningKey::from_bytes(&[28u8; 32]);
        let keys = [key.verifying_key()];
        let info = info(&home, &keys);
        assert_eq!(info.status, "incomplete", "{:?}", info.problems);
        assert!(info.problems[0].contains("allowlist.sig"));
        assert!(enforce(&home, &keys, "demo", "1.0.0").is_err());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn rotated_document_can_carry_two_signatures() {
        let home = home("rotate");
        let old = SigningKey::from_bytes(&[25u8; 32]);
        let new = SigningKey::from_bytes(&[26u8; 32]);
        let body = r#"{"schema_version":1,"apps":[{"app_id":"demo"}]}"#;
        let doc = home.join("allowlist.json");
        fs::write(&doc, body).unwrap();
        let raw = format!(
            "{}\n{}\n",
            hex::encode(old.sign(body.as_bytes()).to_bytes()),
            hex::encode(new.sign(body.as_bytes()).to_bytes())
        );
        fs::write(signature_path(&doc), raw).unwrap();

        assert!(enforce(&home, &[old.verifying_key()], "demo", "1.0.0").is_ok());
        assert!(enforce(&home, &[new.verifying_key()], "demo", "1.0.0").is_ok());
        assert!(enforce(
            &home,
            &[SigningKey::from_bytes(&[27u8; 32]).verifying_key()],
            "demo",
            "1.0.0"
        )
        .is_err());

        let _ = fs::remove_dir_all(home);
    }
}
