use crate::config::Policy;
use crate::error::{msg, Error, Result};
use crate::manifest::{self, Capabilities, Connection, Manifest, ToolDef};
use ed25519_dalek::VerifyingKey;
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize)]
pub struct InstalledApp {
    pub app_id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    /// Optional one-liner from the manifest.
    pub description: Option<String>,
    /// Sanitized SVG icon as a data URL; `None` falls back to initials.
    pub icon: Option<String>,
    pub path: String,
    pub trusted: bool,
    pub signer_fingerprint: Option<String>,
    pub allowed: bool,
    pub allowlist_problem: Option<String>,
    pub problems: Vec<String>,
    pub tools: Vec<ToolDef>,
    pub connections: Vec<Connection>,
    pub capabilities: Capabilities,
    pub history: Vec<ArchivedVersion>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchivedVersion {
    pub version: String,
    pub archived_at: u64,
    pub path: String,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn home() -> PathBuf {
    if let Ok(h) = std::env::var("ECO_PLATFORM_HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        if !l.is_empty() {
            return PathBuf::from(l).join("EcoAIOps");
        }
    }
    PathBuf::from(".data")
}

pub fn apps_dir(home: &Path) -> PathBuf {
    home.join("apps")
}

pub fn trusted_pub_path(home: &Path) -> PathBuf {
    home.join("trusted.pub")
}

/// Machine-wide anchor lives in HKLM: only administrators can write it, unlike
/// a ProgramData file that a regular user could create in some configurations.
pub const MACHINE_ANCHOR_LABEL: &str = "HKLM\\SOFTWARE\\EcoAIOps\\TrustedPub";

/// Staged rotation keeps up to four active keys: sign with the new one, wait
/// until the park is updated, then remove the old fingerprint.
pub const MAX_ANCHORS: usize = 4;

/// Development mode allows a per-user anchor fallback. A release build is
/// production-only unless `ECO_PLATFORM_DEV=1` is set explicitly.
pub fn dev_mode() -> bool {
    cfg!(debug_assertions)
        || std::env::var("ECO_PLATFORM_DEV")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

/// Where the trusted signing key comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSource {
    Machine,
    Dev,
}

impl AnchorSource {
    pub fn as_str(self) -> &'static str {
        match self {
            AnchorSource::Machine => "machine",
            AnchorSource::Dev => "dev",
        }
    }
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Parses one or more hex keys separated by commas, semicolons, or whitespace.
/// Never truncates: callers that write a set must reject an over-limit input
/// instead of silently dropping still-valid anchors.
fn decode_anchor_keys(raw: &str) -> Vec<[u8; 32]> {
    raw.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|token| !token.is_empty())
        .filter_map(|token| hex::decode(token).ok())
        .filter_map(|bytes| bytes.try_into().ok())
        .collect()
}

#[cfg(windows)]
fn decode_utf16(buf: &[u8]) -> String {
    let units: Vec<u16> = buf
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// Registry strings can be UTF-16 (`REG_SZ`/`REG_MULTI_SZ` as written by GPO
/// or an admin tool) or ANSI bytes written by older builds. Accept both.
#[cfg(windows)]
fn registry_key_text(buf: &[u8], kind: u32) -> String {
    use windows_sys::Win32::System::Registry::REG_MULTI_SZ;
    if kind == REG_MULTI_SZ {
        let text = decode_utf16(buf);
        return text
            .split('\0')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(",");
    }
    let utf16 = decode_utf16(buf);
    let trimmed = utf16.trim_matches('\0').trim();
    if !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == ',' || c == ';' || c.is_whitespace())
    {
        return trimmed.to_string();
    }
    String::from_utf8_lossy(buf)
        .trim_matches('\0')
        .trim()
        .to_string()
}

#[cfg(windows)]
fn machine_anchor_keys() -> Vec<[u8; 32]> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_MULTI_SZ,
        REG_SZ,
    };

    let sub = wide("SOFTWARE\\EcoAIOps");
    let name = wide("TrustedPub");
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_LOCAL_MACHINE, sub.as_ptr(), 0, KEY_READ, &mut key) != 0 {
            return Vec::new();
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
            || (kind != REG_SZ && kind != REG_MULTI_SZ)
            || len == 0
            || len > 4096
        {
            RegCloseKey(key);
            return Vec::new();
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
            return Vec::new();
        }
        RegCloseKey(key);
        decode_anchor_keys(&registry_key_text(&buf, kind))
    }
}

#[cfg(not(windows))]
fn machine_anchor_keys() -> Vec<[u8; 32]> {
    Vec::new()
}

#[cfg(windows)]
fn set_machine_anchor_keys(keys: &[[u8; 32]]) -> Result<()> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE,
        KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    let sub = wide("SOFTWARE\\EcoAIOps");
    let name = wide("TrustedPub");
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        let mut disposition: u32 = 0;
        let rc = RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            sub.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut key,
            &mut disposition,
        );
        if rc != 0 {
            return msg(format!("cannot open HKLM\\SOFTWARE\\EcoAIOps (error {rc})"));
        }
        if keys.is_empty() {
            RegDeleteValueW(key, name.as_ptr());
            RegCloseKey(key);
            return Ok(());
        }
        let text = keys
            .iter()
            .map(hex::encode)
            .collect::<Vec<_>>()
            .join(",");
        // Older platform builds read this REG_SZ value as ANSI hex bytes, so
        // keep writing that representation for mixed-version fleets and
        // downgrades. The reader also accepts UTF-16 and REG_MULTI_SZ values
        // that GPO or an admin tool may write.
        let mut data: Vec<u8> = text.as_bytes().to_vec();
        data.push(0);
        let rc = RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_SZ,
            data.as_ptr(),
            data.len() as u32,
        );
        RegCloseKey(key);
        if rc != 0 {
            return msg(format!("cannot write machine trust anchor (error {rc})"));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_machine_anchor_keys(_keys: &[[u8; 32]]) -> Result<()> {
    msg("machine trust anchor is only supported on Windows")
}

fn read_dev_anchor_keys(home: &Path) -> Vec<[u8; 32]> {
    fs::read_to_string(trusted_pub_path(home))
        .map(|raw| decode_anchor_keys(&raw))
        .unwrap_or_default()
}

fn write_dev_anchor_keys(home: &Path, keys: &[[u8; 32]]) -> Result<()> {
    let path = trusted_pub_path(home);
    if keys.is_empty() {
        let _ = fs::remove_file(path);
        return Ok(());
    }
    fs::create_dir_all(home)?;
    let text = keys
        .iter()
        .map(hex::encode)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, format!("{text}\n"))?;
    Ok(())
}

/// Resolves the active anchors. Machine scope wins; the per-user anchor is a
/// dev fallback only. All keys in the chosen scope are active, which supports
/// staged signing-key rotation.
fn resolve_anchor(home: &Path) -> Option<(String, AnchorSource, Vec<VerifyingKey>)> {
    if !cfg!(test) {
        let keys = machine_anchor_keys();
        if !keys.is_empty() {
            let parsed = keys
                .iter()
                .filter_map(|bytes| VerifyingKey::from_bytes(bytes).ok())
                .collect::<Vec<_>>();
            if !parsed.is_empty() {
                return Some((MACHINE_ANCHOR_LABEL.to_string(), AnchorSource::Machine, parsed));
            }
        }
    }
    if dev_mode() {
        let bytes = read_dev_anchor_keys(home);
        let parsed = bytes
            .iter()
            .filter_map(|key| VerifyingKey::from_bytes(key).ok())
            .collect::<Vec<_>>();
        if !parsed.is_empty() {
            return Some((
                trusted_pub_path(home).display().to_string(),
                AnchorSource::Dev,
                parsed,
            ));
        }
    }
    None
}

pub fn trusted_anchor(home: &Path) -> Option<(String, AnchorSource)> {
    resolve_anchor(home).map(|(label, source, _)| (label, source))
}

pub fn load_trusted_keys(home: &Path) -> Vec<VerifyingKey> {
    resolve_anchor(home)
        .map(|(_, _, keys)| keys)
        .unwrap_or_default()
}

pub fn anchor_fingerprints(home: &Path) -> Vec<String> {
    load_trusted_keys(home)
        .iter()
        .map(|key| {
            let digest = manifest::sha256_hex(&key.to_bytes());
            digest[..16].to_string()
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct TrustKeyInfo {
    pub fingerprint: String,
    pub public_key: String,
    pub source: String,
}

pub fn trust_key_info(home: &Path) -> Vec<TrustKeyInfo> {
    let source = resolve_anchor(home)
        .map(|(_, source, _)| source.as_str().to_string())
        .unwrap_or_else(|| "none".to_string());
    load_trusted_keys(home)
        .iter()
        .map(|key| {
            let public_key = hex::encode(key.to_bytes());
            let digest = manifest::sha256_hex(&key.to_bytes());
            TrustKeyInfo {
                fingerprint: digest[..16].to_string(),
                public_key,
                source: source.clone(),
            }
        })
        .collect()
}

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
        let mut len: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Verifies a package according to the active environment. Local mode trusts
/// the local seal (or a valid signature for packages installed before the seal
/// existed); corporate mode keeps the signature + allowlist gate.
pub(crate) fn verify_for(
    home: &Path,
    app_id: &str,
    dir: &Path,
    trusted: &[VerifyingKey],
    policy: Policy,
) -> manifest::VerifyReport {
    if !policy.local() {
        return manifest::verify_package(dir, trusted);
    }
    let mut report = manifest::verify_integrity(dir);
    if !report.ok {
        return report;
    }
    match crate::seal::verify(home, app_id, dir) {
        Ok(()) => report,
        Err(seal_error) => {
            // A package installed before local seals existed may still carry
            // a valid signature. Accept it once and seal it lazily, but only
            // when no seal exists: a present seal that does not match means
            // the installed files changed after installation.
            let seal_missing = !crate::seal::seal_path(home, app_id).exists();
            if seal_missing && !trusted.is_empty() {
                let signed = manifest::verify_package(dir, trusted);
                if signed.ok {
                    let _ = crate::seal::write(home, app_id, dir);
                    return signed;
                }
            }
            report.ok = false;
            report.problems.push(seal_error.to_string());
            report
        }
    }
}

fn installed(
    home: &Path,
    path: &Path,
    manifest: &Manifest,
    report: &manifest::VerifyReport,
    allow: &crate::allowlist::AllowlistGuard,
    policy: Policy,
) -> InstalledApp {
    let (allowed, allowlist_problem) = if policy.enforce_allowlist() {
        match allow.decision(&manifest.app_id, &manifest.version) {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        }
    } else {
        (true, None)
    };
    InstalledApp {
        app_id: manifest.app_id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        publisher: manifest.publisher.clone(),
        description: manifest.clean_description(),
        icon: manifest.icon_data_url(path),
        path: path.display().to_string(),
        trusted: report.ok,
        signer_fingerprint: report.signer_fingerprint.clone(),
        allowed,
        allowlist_problem,
        problems: report.problems.clone(),
        tools: manifest.tools.clone(),
        connections: manifest.connections.clone(),
        capabilities: manifest.capabilities.clone(),
        history: history(home, &manifest.app_id),
    }
}

pub fn list(home: &Path) -> Vec<InstalledApp> {
    let trusted = load_trusted_keys(home);
    let allow = crate::allowlist::guard(home, &trusted);
    let policy = crate::config::policy_for(home);
    let dir = apps_dir(home);
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        match manifest::load(&path) {
            Ok(m) => {
                let report = verify_for(home, &m.app_id, &path, &trusted, policy);
                out.push(installed(home, &path, &m, &report, &allow, policy));
            }
            Err(e) => out.push(InstalledApp {
                app_id: entry.file_name().to_string_lossy().to_string(),
                name: entry.file_name().to_string_lossy().to_string(),
                version: "?".into(),
                publisher: String::new(),
                description: None,
                icon: None,
                path: path.display().to_string(),
                trusted: false,
                signer_fingerprint: None,
                allowed: false,
                allowlist_problem: Some("manifest unreadable".into()),
                problems: vec![format!("manifest unreadable: {e}")],
                tools: Vec::new(),
                connections: Vec::new(),
                capabilities: Capabilities::default(),
                history: Vec::new(),
            }),
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn find(home: &Path, app_id: &str) -> Option<PathBuf> {
    if manifest::parse_id(app_id, "app").is_err() {
        return None;
    }
    let p = apps_dir(home).join(app_id);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

pub fn versions_root(home: &Path) -> PathBuf {
    home.join("versions")
}

fn versions_dir(home: &Path, app_id: &str) -> PathBuf {
    versions_root(home).join(app_id)
}

const KEEP_VERSIONS: usize = 5;

fn is_runtime_entry(name: &std::ffi::OsStr) -> bool {
    name == "engine" || name == "sessions" || name == "tmp"
}

/// Copies a package without the per-app runtime state: `engine` and `sessions`
/// are recreated on start and must never be restored by a rollback.
fn copy_package_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        if is_runtime_entry(&entry.file_name()) {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_package_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Archived package versions for `app_id`, newest first.
pub fn history(home: &Path, app_id: &str) -> Vec<ArchivedVersion> {
    let dir = versions_dir(home, app_id);
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Some((version, stamp)) = name.rsplit_once("__") else {
            continue;
        };
        let Ok(archived_at) = stamp.parse::<u64>() else {
            continue;
        };
        out.push(ArchivedVersion {
            version: version.to_string(),
            archived_at,
            path: entry.path().display().to_string(),
        });
    }
    out.sort_by(|a, b| b.archived_at.cmp(&a.archived_at));
    out
}

fn prune_versions(dir: &Path) -> Result<()> {
    let mut dirs: Vec<(u64, PathBuf)> = Vec::new();
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some((_, stamp)) = name.rsplit_once("__") {
            if let Ok(ts) = stamp.parse::<u64>() {
                dirs.push((ts, path));
            }
        }
    }
    dirs.sort_by(|a, b| b.0.cmp(&a.0));
    while dirs.len() > KEEP_VERSIONS {
        let (_, path) = dirs.remove(0);
        let _ = fs::remove_dir_all(path);
    }
    Ok(())
}

struct RuntimeKeep {
    dir: PathBuf,
    moved: Vec<(&'static str, PathBuf)>,
}

/// Moves per-app runtime state out of the package directory before the package
/// is replaced, so chat history and artifact dumps survive installs/rollbacks.
fn park_runtime(home: &Path, app_id: &str, dest: &Path) -> Result<RuntimeKeep> {
    let dir = home
        .join("staging")
        .join(format!("keep-{}-{}", now_secs(), app_id));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    let mut moved: Vec<(&'static str, PathBuf)> = Vec::new();
    for name in ["engine", "sessions"] {
        let src = dest.join(name);
        if !src.exists() {
            continue;
        }
        let target = dir.join(name);
        if let Err(e) = fs::rename(&src, &target) {
            // Put already-moved entries back: the caller cannot restore what
            // it never received.
            for (moved_name, moved_path) in moved.iter().rev() {
                let _ = fs::rename(moved_path, dest.join(moved_name));
            }
            let _ = fs::remove_dir_all(&dir);
            return Err(e.into());
        }
        moved.push((name, target));
    }
    Ok(RuntimeKeep { dir, moved })
}

/// Restores runtime state and keeps the parked copy if any move fails, so a
/// failed swap never destroys the only copy of chat history/artifacts.
fn restore_runtime(keep: Option<RuntimeKeep>, dest: &Path) -> Result<()> {
    let Some(keep) = keep else {
        return Ok(());
    };
    if !dest.exists() {
        fs::create_dir_all(dest)?;
    }
    let mut failed: Option<Error> = None;
    for (name, path) in &keep.moved {
        if !path.exists() {
            continue;
        }
        let target = dest.join(name);
        if target.exists() {
            let _ = fs::remove_dir_all(&target);
        }
        if let Err(e) = fs::rename(path, &target) {
            eprintln!(
                "eco-platform: cannot restore {} from {}: {e}",
                name,
                path.display()
            );
            failed = Some(e.into());
        }
    }
    if failed.is_none() {
        let _ = fs::remove_dir_all(&keep.dir);
    }
    match failed {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Restores any `staging/keep-<ts>-<app_id>` directory left by an interrupted
/// install or rollback, before the staging sweep can delete it.
pub fn recover_runtime(home: &Path) {
    let root = home.join("staging");
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(rest) = name.strip_prefix("keep-") else {
            continue;
        };
        let Some((_ts, app_id)) = rest.split_once('-') else {
            continue;
        };
        if manifest::parse_id(app_id, "app").is_err() {
            continue;
        }
        let keep = entry.path();
        let dest = apps_dir(home).join(app_id);
        if let Err(e) = fs::create_dir_all(&dest) {
            eprintln!("eco-platform: cannot recover runtime for {app_id}: {e}");
            continue;
        }
        let mut failed = false;
        for sub in ["engine", "sessions"] {
            let src = keep.join(sub);
            if !src.exists() {
                continue;
            }
            let target = dest.join(sub);
            if target.exists() {
                // A newer runtime already exists; keep the backup untouched.
                failed = true;
                continue;
            }
            if let Err(e) = fs::rename(&src, &target) {
                eprintln!("eco-platform: cannot recover {sub} for {app_id}: {e}");
                failed = true;
            }
        }
        if !failed {
            let _ = fs::remove_dir_all(&keep);
        }
    }
}

pub fn installed_ids(home: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(apps_dir(home)) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        if manifest::parse_id(&id, "app").is_ok() {
            out.push(id);
        }
    }
    out
}

/// Read-only variant of `verify_for`: it never writes a seal, so CLI checks
/// do not mutate an installed profile.
pub(crate) fn verify_for_readonly(
    home: &Path,
    app_id: &str,
    dir: &Path,
    trusted: &[VerifyingKey],
    policy: Policy,
) -> manifest::VerifyReport {
    if !policy.local() {
        return manifest::verify_package(dir, trusted);
    }
    let mut report = manifest::verify_integrity(dir);
    if !report.ok {
        return report;
    }
    match crate::seal::verify(home, app_id, dir) {
        Ok(()) => report,
        Err(seal_error) => {
            let seal_missing = !crate::seal::seal_path(home, app_id).exists();
            if seal_missing && !trusted.is_empty() {
                let signed = manifest::verify_package(dir, trusted);
                if signed.ok {
                    return signed;
                }
            }
            report.ok = false;
            report.problems.push(seal_error.to_string());
            report
        }
    }
}

fn archive_current(home: &Path, app_id: &str, src: &Path) -> Result<PathBuf> {
    let manifest = manifest::load(src)?;
    let dir = versions_dir(home, app_id);
    fs::create_dir_all(&dir)?;
    let mut stamp = now_secs();
    let mut dst = dir.join(format!("{}__{}", manifest.version, stamp));
    while dst.exists() {
        stamp += 1;
        dst = dir.join(format!("{}__{}", manifest.version, stamp));
    }
    copy_package_dir(src, &dst)?;
    prune_versions(&dir)?;
    Ok(dst)
}

fn audit_rollback(home: &Path, app_id: &str, from: &str, to: &str) {
    crate::commands::audit(
        home,
        app_id,
        serde_json::json!({
            "ts": now_secs(),
            "source": "admin",
            "action": "rollback",
            "app_id": app_id,
            "from": from,
            "to": to,
            "ok": true
        }),
    );
}

/// Explicit administrator rollback to an archived version. Normal `install`
/// keeps rejecting downgrades; this function is the only path that goes back.
pub fn rollback(home: &Path, app_id: &str, version: Option<&str>) -> Result<InstalledApp> {
    manifest::parse_id(app_id, "app")?;
    let dest = apps_dir(home).join(app_id);
    if !dest.is_dir() {
        return msg(format!("app {app_id} is not installed"));
    }
    let current = manifest::load(&dest)?;

    let entries = history(home, app_id);
    let target = match version {
        Some(wanted) => entries
            .iter()
            .find(|e| e.version == wanted)
            .cloned()
            .ok_or_else(|| Error::Msg(format!("version {wanted} is not archived for {app_id}")))?,
        None => entries
            .iter()
            .find(|e| e.version != current.version)
            .or_else(|| entries.first())
            .cloned()
            .ok_or_else(|| Error::Msg(format!("no archived versions for {app_id}")))?,
    };
    let archived = PathBuf::from(&target.path);

    let trusted = load_trusted_keys(home);
    let policy = crate::config::policy_for(home);
    let report = if policy.local() {
        manifest::verify_integrity(&archived)
    } else {
        manifest::verify_package(&archived, &trusted)
    };
    if !report.ok {
        return msg(format!(
            "archived package rejected (fail-closed): {}",
            report.problems.join("; ")
        ));
    }
    let manifest = manifest::load(&archived)?;
    let allow = crate::allowlist::guard(home, &trusted);
    if policy.enforce_allowlist() {
        if let Err(e) = allow.decision(&manifest.app_id, &manifest.version) {
            crate::commands::audit(
                home,
                app_id,
                serde_json::json!({
                    "ts": now_secs(),
                    "source": "admin",
                    "action": "rollback",
                    "app_id": app_id,
                    "from": current.version,
                    "to": manifest.version,
                    "ok": false,
                    "reason": "allowlist",
                    "problem": e.to_string()
                }),
            );
            return Err(e);
        }
    }

    let staging = home.join("staging").join(format!("rollback-{}", now_secs()));
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    fs::create_dir_all(&staging)?;
    copy_package_dir(&archived, &staging)?;
    let staged_report = if policy.local() {
        manifest::verify_integrity(&staging)
    } else {
        manifest::verify_package(&staging, &trusted)
    };
    if !staged_report.ok {
        let _ = fs::remove_dir_all(&staging);
        return msg(format!(
            "rollback staging failed verification: {}",
            staged_report.problems.join("; ")
        ));
    }

    archive_current(home, app_id, &dest)?;
    let mut parked = Some(park_runtime(home, app_id, &dest)?);
    if let Err(e) = fs::remove_dir_all(&dest) {
        if let Err(restore) = restore_runtime(parked.take(), &dest) {
            eprintln!("eco-platform: runtime restore failed for {app_id}: {restore}");
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(e.into());
    }
    if let Err(e) = fs::rename(&staging, &dest) {
        if let Err(restore) = restore_runtime(parked.take(), &dest) {
            eprintln!("eco-platform: runtime restore failed for {app_id}: {restore}");
        }
        return Err(e.into());
    }
    if let Err(e) = restore_runtime(parked.take(), &dest) {
        return msg(format!(
            "package rolled back, but chat history could not be restored (data kept in staging): {e}"
        ));
    }

    if policy.local() {
        if let Err(e) = crate::seal::write(home, app_id, &dest) {
            return msg(format!("cannot seal the rolled-back package: {e}"));
        }
    }

    audit_rollback(home, app_id, &current.version, &manifest.version);

    let report = verify_for(home, app_id, &dest, &trusted, policy);
    Ok(installed(home, &dest, &manifest, &report, &allow, policy))
}

/// Loads the manifest only if the package is installed and passes signature,
/// hash, and allowlist checks. Every entry point that runs app content goes
/// through this gate.
pub fn verified_manifest(home: &Path, app_id: &str) -> Result<Manifest> {
    manifest::parse_id(app_id, "app")?;
    let path =
        find(home, app_id).ok_or_else(|| Error::Msg(format!("app {app_id} is not installed")))?;
    let trusted = load_trusted_keys(home);
    let policy = crate::config::policy_for(home);
    let report = verify_for(home, app_id, &path, &trusted, policy);
    if !report.ok {
        crate::commands::audit(
            home,
            app_id,
            serde_json::json!({
                "ts": now_secs(),
                "source": "user",
                "action": "open",
                "app_id": app_id,
                "ok": false,
                "reason": "verification",
                "problems": report.problems
            }),
        );
        return msg(format!(
            "app {app_id} failed verification: {}",
            report.problems.join("; ")
        ));
    }
    let app = manifest::load(&path)?;
    if !policy.enforce_allowlist() {
        return Ok(app);
    }
    let allow = crate::allowlist::guard(home, &trusted);
    if let Err(e) = allow.decision(&app.app_id, &app.version) {
        crate::commands::audit(
            home,
            app_id,
            serde_json::json!({
                "ts": now_secs(),
                "source": "user",
                "action": "open",
                "app_id": app_id,
                "version": app.version,
                "ok": false,
                "reason": "allowlist",
                "problem": e.to_string()
            }),
        );
        return Err(e);
    }
    Ok(app)
}

pub fn install(home: &Path, src: &Path) -> Result<InstalledApp> {
    install_with(home, src, |_| {})
}

/// `before_replace` runs after the package passed verification, allowlist and
/// downgrade checks, immediately before the installed directory is replaced.
/// The UI uses it to stop the app's engine at the last safe moment.
pub fn install_with<F: FnOnce(&str)>(
    home: &Path,
    src: &Path,
    before_replace: F,
) -> Result<InstalledApp> {
    let mut before_replace = Some(before_replace);
    let staging = home.join("staging").join(format!("{}", now_secs()));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    let staged = (|| -> Result<()> {
        if src.is_dir() {
            copy_dir(src, &staging)?;
        } else {
            extract_zip(src, &staging)?;
        }
        Ok(())
    })();
    if let Err(e) = staged {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    let manifest = match manifest::load(&staging) {
        Ok(m) => m,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    if let Err(e) = manifest::parse_id(&manifest.app_id, "app") {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    let trusted = load_trusted_keys(home);
    let policy = crate::config::policy_for(home);
    let report = if policy.local() {
        manifest::verify_integrity_install(&staging)
    } else {
        manifest::verify_package_install(&staging, &trusted)
    };
    if !report.ok {
        crate::commands::audit(
            home,
            &manifest.app_id,
            serde_json::json!({
                "ts": now_secs(),
                "source": "user",
                "action": "install",
                "app_id": manifest.app_id,
                "version": manifest.version,
                "ok": false,
                "reason": "verification",
                "problems": report.problems
            }),
        );
        let _ = fs::remove_dir_all(&staging);
        return msg(format!(
            "package rejected (fail-closed): {}",
            report.problems.join("; ")
        ));
    }
    let allow = crate::allowlist::guard(home, &trusted);
    if policy.enforce_allowlist() {
        if let Err(e) = allow.decision(&manifest.app_id, &manifest.version) {
            crate::commands::audit(
                home,
                &manifest.app_id,
                serde_json::json!({
                    "ts": now_secs(),
                    "source": "user",
                    "action": "install",
                    "app_id": manifest.app_id,
                    "version": manifest.version,
                    "ok": false,
                    "reason": "allowlist",
                    "problem": e.to_string()
                }),
            );
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    }

    let dest = apps_dir(home).join(&manifest.app_id);
    let mut parked: Option<RuntimeKeep> = None;
    if dest.exists() {
        let existing = match manifest::load(&dest) {
            Ok(existing) => existing,
            Err(e) => {
                let _ = fs::remove_dir_all(&staging);
                return msg(format!(
                    "installed package {} is unreadable ({e}); restore or remove it before updating",
                    manifest.app_id
                ));
            }
        };
        if let Err(e) = manifest::ensure_not_downgrade(&existing.version, &manifest.version) {
            crate::commands::audit(
                home,
                &manifest.app_id,
                serde_json::json!({
                    "ts": now_secs(),
                    "source": "user",
                    "action": "install",
                    "app_id": manifest.app_id,
                    "version": manifest.version,
                    "installed": existing.version,
                    "ok": false,
                    "reason": "downgrade"
                }),
            );
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
        if let Err(e) = archive_current(home, &manifest.app_id, &dest) {
            let _ = fs::remove_dir_all(&staging);
            return msg(format!(
                "cannot archive the installed version before update: {e}"
            ));
        }
        if let Some(hook) = before_replace.take() {
            hook(&manifest.app_id);
        }
        parked = match park_runtime(home, &manifest.app_id, &dest) {
            Ok(keep) => Some(keep),
            Err(e) => {
                let _ = fs::remove_dir_all(&staging);
                return msg(format!(
                    "cannot preserve chat history and artifacts before update: {e}"
                ));
            }
        };
        if let Err(e) = fs::remove_dir_all(&dest) {
            if let Err(restore) = restore_runtime(parked.take(), &dest) {
                eprintln!(
                    "eco-platform: runtime restore failed for {}: {restore}",
                    manifest.app_id
                );
            }
            let _ = fs::remove_dir_all(&staging);
            return Err(e.into());
        }
    } else {
        fs::create_dir_all(apps_dir(home))?;
    }
    if let Err(e) = fs::rename(&staging, &dest) {
        if let Err(restore) = restore_runtime(parked.take(), &dest) {
            eprintln!(
                "eco-platform: runtime restore failed for {}: {restore}",
                manifest.app_id
            );
        }
        return Err(e.into());
    }
    if let Err(e) = restore_runtime(parked.take(), &dest) {
        return msg(format!(
            "package updated, but chat history could not be restored (data kept in staging): {e}"
        ));
    }

    if policy.local() {
        if let Err(e) = crate::seal::write(home, &manifest.app_id, &dest) {
            return msg(format!("cannot seal the installed package: {e}"));
        }
    }

    crate::commands::audit(
        home,
        &manifest.app_id,
        serde_json::json!({
            "ts": now_secs(),
            "source": "user",
            "action": "install",
            "app_id": manifest.app_id,
            "version": manifest.version,
            "ok": true
        }),
    );
    Ok(installed(home, &dest, &manifest, &report, &allow, policy))
}

pub fn remove(home: &Path, app_id: &str) -> Result<()> {
    manifest::parse_id(app_id, "app")?;
    let dest = apps_dir(home).join(app_id);
    if !dest.exists() {
        return msg(format!("app {app_id} is not installed"));
    }
    fs::remove_dir_all(&dest)?;
    crate::seal::remove(home, app_id);
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

const MAX_ZIP_ENTRIES: usize = 10_000;
const MAX_ZIP_BYTES: u64 = 512 * 1024 * 1024;

fn extract_zip(src: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(src)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(Error::Msg(format!(
            "zip has {} entries; at most {} are supported",
            archive.len(),
            MAX_ZIP_ENTRIES
        )));
    }
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            return Err(Error::Msg(format!("zip entry escapes package root: {}", entry.name())));
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut f = fs::File::create(&out)?;
            let remaining = MAX_ZIP_BYTES.saturating_sub(total);
            let written = {
                // Bound the declared entry size, not just the final total.
                let mut limited = (&mut entry).take(remaining.saturating_add(1));
                std::io::copy(&mut limited, &mut f)?
            };
            if written > remaining {
                drop(f);
                let _ = fs::remove_file(&out);
                return Err(Error::Msg(format!(
                    "zip expands to more than {} bytes; refusing to extract",
                    MAX_ZIP_BYTES
                )));
            }
            total = total.saturating_add(written);
        }
    }
    Ok(())
}

/// Extracts a zip package into `dest`. Public for the headless `--verify` path.
pub fn extract_zip_public(src: &Path, dest: &Path) -> Result<()> {
    extract_zip(src, dest)
}



/// Adds one or more public signing keys to the active trust anchor set.
#[derive(Debug, Clone, Serialize)]
pub struct TrustAnchorInstalled {
    pub fingerprint: String,
    pub path: String,
    pub source: String,
    pub public_key: String,
    pub added: usize,
    pub keys: Vec<TrustKeyInfo>,
}

/// Adds keys machine-wide when elevated; in a release build without elevation
/// this fails, so a user cannot pin their own signing key. Existing keys stay
/// active, which makes rotation a two-step operation: add the new key, update
/// the park, then remove the old fingerprint.
pub fn install_trust_anchor(home: &std::path::Path, src: &std::path::Path) -> Result<TrustAnchorInstalled> {
    let raw = fs::read_to_string(src)?;
    let incoming = decode_anchor_keys(&raw);
    if incoming.is_empty() {
        return msg("public key file contains no valid 32-byte hex key");
    }
    if incoming.len() > MAX_ANCHORS {
        return msg(format!(
            "public key file contains {} keys; at most {} active anchors are supported",
            incoming.len(),
            MAX_ANCHORS
        ));
    }

    let use_machine = is_elevated() && !cfg!(test);
    if !use_machine && !dev_mode() {
        return msg("administrator rights are required: the trust anchor must be installed machine-wide");
    }
    let (dest, source, mut current) = if use_machine {
        (
            MACHINE_ANCHOR_LABEL.to_string(),
            AnchorSource::Machine,
            machine_anchor_keys(),
        )
    } else {
        (
            trusted_pub_path(home).display().to_string(),
            AnchorSource::Dev,
            read_dev_anchor_keys(home),
        )
    };

    let mut added = 0;
    for key in incoming {
        if !current.contains(&key) {
            current.push(key);
            added += 1;
        }
    }
    if current.len() > MAX_ANCHORS {
        return msg(format!(
            "too many active anchors ({}); remove an old key with --untrust first",
            current.len()
        ));
    }
    if source == AnchorSource::Machine {
        set_machine_anchor_keys(&current)?;
    } else {
        write_dev_anchor_keys(home, &current)?;
    }

    let primary = VerifyingKey::from_bytes(&current[0])
        .map_err(|e| Error::Msg(format!("invalid public key: {e}")))?;
    let digest = manifest::sha256_hex(&primary.to_bytes());
    Ok(TrustAnchorInstalled {
        fingerprint: digest[..16].to_string(),
        path: dest,
        source: source.as_str().to_string(),
        public_key: hex::encode(current[0]),
        added,
        keys: trust_key_info(home),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct TrustAnchorRemoved {
    pub removed: usize,
    pub source: String,
    pub keys: Vec<TrustKeyInfo>,
}

/// Removes an active anchor by fingerprint prefix, full public key, or `all`.
/// Removing the last key is refused unless `force` is set, because zero anchors
/// makes every installed package unlaunchable until an admin re-runs `--trust`.
pub fn untrust_anchor(home: &Path, target: &str, force: bool) -> Result<TrustAnchorRemoved> {
    let use_machine = is_elevated() && !cfg!(test);
    if !use_machine && !dev_mode() {
        return msg("administrator rights are required to change the machine trust anchor");
    }
    let (source, current) = if use_machine {
        (AnchorSource::Machine, machine_anchor_keys())
    } else {
        (AnchorSource::Dev, read_dev_anchor_keys(home))
    };
    if current.is_empty() {
        return msg("no active trust anchor to remove");
    }

    let wanted = target.trim().to_ascii_lowercase();
    let remove_all = wanted == "all";
    let (mut kept, mut removed) = (Vec::new(), 0usize);
    for key in current {
        let public_key = hex::encode(key);
        let digest = manifest::sha256_hex(&key);
        let fingerprint = &digest[..16];
        let matches = remove_all
            || public_key == wanted
            || (wanted.len() >= 8 && fingerprint.starts_with(&wanted));
        if matches {
            removed += 1;
        } else {
            kept.push(key);
        }
    }
    if removed == 0 {
        return msg(format!("no active anchor matches {target}"));
    }
    if kept.is_empty() && !force {
        return msg(
            "refusing to remove the last active anchor: every installed package would stop opening; pass --force to do it anyway",
        );
    }
    if source == AnchorSource::Machine {
        set_machine_anchor_keys(&kept)?;
    } else {
        write_dev_anchor_keys(home, &kept)?;
    }
    Ok(TrustAnchorRemoved {
        removed,
        source: source.as_str().to_string(),
        keys: trust_key_info(home),
    })
}

/// Short fingerprint of the primary installed trust anchor, so a human can
/// compare it with the value published by the security team.
pub fn anchor_fingerprint(home: &Path) -> Option<String> {
    anchor_fingerprints(home).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::fs;

    fn write_manifest(dir: &Path, version: &str) -> String {
        fs::create_dir_all(dir.join("prompt")).unwrap();
        fs::write(dir.join("prompt/system.md"), b"prompt").unwrap();
        let hash = manifest::sha256_hex(b"prompt");
        let body = format!(
            r#"{{"schema_version":2,"app_id":"demo","name":"Demo","version":"{version}","profile":{{"system_prompt":"prompt/system.md"}},"hashes":{{"prompt/system.md":"sha256:{hash}"}}}}"#
        );
        fs::write(dir.join("manifest.json"), &body).unwrap();
        body
    }

    fn write_package(dir: &Path, version: &str, key: &SigningKey) {
        let body = write_manifest(dir, version);
        let sig = key.sign(body.as_bytes());
        fs::write(dir.join("manifest.sig"), hex::encode(sig.to_bytes())).unwrap();
    }

    fn write_unsigned_package(dir: &Path, version: &str) {
        write_manifest(dir, version);
    }

    fn use_local_mode(home: &Path, role: crate::config::Role) {
        let mut cfg = crate::config::load(home);
        cfg.environment = crate::config::Environment::Local;
        cfg.role = role;
        cfg.mode_chosen = true;
        crate::config::save(home, &cfg).unwrap();
    }

    #[test]
    fn local_mode_installs_unsigned_packages_and_detects_tampering() {
        let home = std::env::temp_dir().join(format!("eco-registry-local-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        use_local_mode(&home, crate::config::Role::User);

        let pkg = home.join("pkg");
        write_unsigned_package(&pkg, "1.0.0");
        install(&home, &pkg).unwrap();
        verified_manifest(&home, "demo").unwrap();

        // A declared file is caught by the manifest hashes.
        let prompt = home.join("apps/demo/prompt/system.md");
        fs::write(&prompt, b"tampered").unwrap();
        let err = verified_manifest(&home, "demo").unwrap_err().to_string();
        assert!(
            err.contains("hash mismatch") || err.contains("package content changed"),
            "{err}"
        );

        // An explicit reinstall restores a trusted copy and reseals it.
        install(&home, &pkg).unwrap();
        verified_manifest(&home, "demo").unwrap();

        // An undeclared file is caught by the seal.
        fs::write(home.join("apps/demo/extra.txt"), b"x").unwrap();
        let err = verified_manifest(&home, "demo").unwrap_err().to_string();
        assert!(err.contains("package content changed"), "{err}");

        install(&home, &pkg).unwrap();
        verified_manifest(&home, "demo").unwrap();

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn corporate_mode_rejects_unsigned_packages() {
        let home =
            std::env::temp_dir().join(format!("eco-registry-corp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();

        let pkg = home.join("pkg");
        write_unsigned_package(&pkg, "1.0.0");
        let err = install(&home, &pkg).unwrap_err().to_string();
        assert!(err.contains("trust anchor") || err.contains("manifest.sig"), "{err}");

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn downgrade_is_rejected_but_equal_version_is_allowed() {
        let home = std::env::temp_dir().join(format!("eco-registry-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();

        let key = SigningKey::from_bytes(&[11u8; 32]);
        fs::write(
            trusted_pub_path(&home),
            hex::encode(key.verifying_key().to_bytes()),
        )
        .unwrap();

        let pkg = home.join("pkg");
        write_package(&pkg, "1.1.0", &key);
        install(&home, &pkg).unwrap();

        write_package(&pkg, "1.0.0", &key);
        let err = install(&home, &pkg).unwrap_err().to_string();
        assert!(err.contains("downgrade"), "{err}");

        write_package(&pkg, "1.1.0", &key);
        install(&home, &pkg).unwrap();

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn update_archives_previous_version_and_rollback_restores_it() {
        let home = std::env::temp_dir().join(format!("eco-registry-rb-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();

        let key = SigningKey::from_bytes(&[12u8; 32]);
        fs::write(
            trusted_pub_path(&home),
            hex::encode(key.verifying_key().to_bytes()),
        )
        .unwrap();

        let pkg = home.join("pkg");
        write_package(&pkg, "1.0.0", &key);
        install(&home, &pkg).unwrap();
        assert!(history(&home, "demo").is_empty());

        write_package(&pkg, "1.1.0", &key);
        let app = install(&home, &pkg).unwrap();
        assert_eq!(app.version, "1.1.0");
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0].version, "1.0.0");

        write_package(&pkg, "1.0.0", &key);
        let err = install(&home, &pkg).unwrap_err().to_string();
        assert!(err.contains("downgrade"), "{err}");

        let rolled = rollback(&home, "demo", None).unwrap();
        assert_eq!(rolled.version, "1.0.0");
        assert!(history(&home, "demo").iter().any(|v| v.version == "1.1.0"));

        let audit = fs::read_to_string(home.join("audit").join("demo.jsonl")).unwrap();
        assert!(audit.contains("\"action\":\"rollback\""), "{audit}");
        assert!(audit.contains("\"source\":\"admin\""), "{audit}");
        assert!(audit.contains("\"action\":\"install\""), "{audit}");
        assert!(audit.contains("\"reason\":\"downgrade\""), "{audit}");
        assert!(audit.contains("\"ok\":true"), "{audit}");

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn trust_anchor_installs_into_the_dev_profile_in_tests() {
        let home = std::env::temp_dir().join(format!("eco-registry-anchor-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();

        let key = SigningKey::from_bytes(&[13u8; 32]);
        let pub_path = home.join("pub.txt");
        fs::write(&pub_path, hex::encode(key.verifying_key().to_bytes())).unwrap();

        let info = install_trust_anchor(&home, &pub_path).unwrap();
        assert_eq!(info.source, "dev");
        assert!(Path::new(&info.path).is_file());
        assert_eq!(load_trusted_keys(&home).len(), 1);

        let second = SigningKey::from_bytes(&[14u8; 32]);
        fs::write(&pub_path, hex::encode(second.verifying_key().to_bytes())).unwrap();
        let info = install_trust_anchor(&home, &pub_path).unwrap();
        assert_eq!(info.added, 1);
        assert_eq!(load_trusted_keys(&home).len(), 2);
        let first_fingerprint = anchor_fingerprints(&home)[0].clone();

        let again = install_trust_anchor(&home, &pub_path).unwrap();
        assert_eq!(again.added, 0);
        assert_eq!(load_trusted_keys(&home).len(), 2);

        let removed = untrust_anchor(&home, &first_fingerprint, false).unwrap();
        assert_eq!(removed.removed, 1);
        let remaining = load_trusted_keys(&home);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].to_bytes(), second.verifying_key().to_bytes());

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn packages_signed_by_any_active_anchor_are_accepted() {
        let home = std::env::temp_dir().join(format!("eco-registry-rot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();

        let old = SigningKey::from_bytes(&[15u8; 32]);
        let new = SigningKey::from_bytes(&[16u8; 32]);
        fs::write(
            trusted_pub_path(&home),
            format!(
                "{}\n{}\n",
                hex::encode(old.verifying_key().to_bytes()),
                hex::encode(new.verifying_key().to_bytes())
            ),
        )
        .unwrap();

        let pkg = home.join("pkg");
        write_package(&pkg, "1.0.0", &new);
        let app = install(&home, &pkg).unwrap();
        assert!(app.trusted);
        assert!(app.allowed);
        assert!(app.signer_fingerprint.is_some());

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn untrust_refuses_to_leave_zero_anchors_without_force() {
        let home = std::env::temp_dir().join(format!("eco-registry-zero-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();

        let key = SigningKey::from_bytes(&[30u8; 32]);
        let pub_path = home.join("pub.txt");
        fs::write(&pub_path, hex::encode(key.verifying_key().to_bytes())).unwrap();
        install_trust_anchor(&home, &pub_path).unwrap();

        let err = untrust_anchor(&home, "all", false).unwrap_err().to_string();
        assert!(err.contains("last active anchor"), "{err}");
        assert_eq!(load_trusted_keys(&home).len(), 1);

        let removed = untrust_anchor(&home, "all", true).unwrap();
        assert_eq!(removed.removed, 1);
        assert!(load_trusted_keys(&home).is_empty());

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn trust_anchor_rejects_more_keys_than_the_limit() {
        let home = std::env::temp_dir().join(format!("eco-registry-max-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();

        let mut raw = String::new();
        for seed in 31u8..=35 {
            let key = SigningKey::from_bytes(&[seed; 32]);
            raw.push_str(&hex::encode(key.verifying_key().to_bytes()));
            raw.push('\n');
        }
        let pub_path = home.join("pub.txt");
        fs::write(&pub_path, raw).unwrap();

        let err = install_trust_anchor(&home, &pub_path).unwrap_err().to_string();
        assert!(err.contains("at most"), "{err}");
        assert!(load_trusted_keys(&home).is_empty());

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn allowlist_denied_rollback_is_audited() {
        let home = std::env::temp_dir().join(format!("eco-registry-rbal-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();

        let key = SigningKey::from_bytes(&[36u8; 32]);
        fs::write(
            trusted_pub_path(&home),
            hex::encode(key.verifying_key().to_bytes()),
        )
        .unwrap();

        let pkg = home.join("pkg");
        write_package(&pkg, "1.0.0", &key);
        install(&home, &pkg).unwrap();
        write_package(&pkg, "1.1.0", &key);
        install(&home, &pkg).unwrap();

        let doc = home.join("allowlist.json");
        let body = r#"{"schema_version":1,"apps":[{"app_id":"demo","versions":["1.1.0"]}]}"#;
        fs::write(&doc, body).unwrap();
        fs::write(
            doc.with_extension("sig"),
            hex::encode(key.sign(body.as_bytes()).to_bytes()),
        )
        .unwrap();

        let err = rollback(&home, "demo", Some("1.0.0"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("not approved"), "{err}");
        let audit = fs::read_to_string(home.join("audit").join("demo.jsonl")).unwrap();
        assert!(
            audit.contains("\"action\":\"rollback\"") && audit.contains("\"reason\":\"allowlist\""),
            "{audit}"
        );

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn allowlist_denies_install_and_open_but_absent_allows() {
        let home = std::env::temp_dir().join(format!("eco-registry-al-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();

        let key = SigningKey::from_bytes(&[17u8; 32]);
        fs::write(
            trusted_pub_path(&home),
            hex::encode(key.verifying_key().to_bytes()),
        )
        .unwrap();

        let pkg = home.join("pkg");
        write_package(&pkg, "1.0.0", &key);
        install(&home, &pkg).unwrap();

        let doc = home.join("allowlist.json");
        let body = r#"{"schema_version":1,"apps":[{"app_id":"other"}]}"#;
        fs::write(&doc, body).unwrap();
        fs::write(
            doc.with_extension("sig"),
            hex::encode(key.sign(body.as_bytes()).to_bytes()),
        )
        .unwrap();

        let err = verified_manifest(&home, "demo").unwrap_err().to_string();
        assert!(err.contains("not approved"), "{err}");
        let err = install(&home, &pkg).unwrap_err().to_string();
        assert!(err.contains("not approved"), "{err}");
        let audit = fs::read_to_string(home.join("audit").join("demo.jsonl")).unwrap();
        assert!(audit.contains("\"reason\":\"allowlist\""), "{audit}");

        let body = r#"{"schema_version":1,"apps":[{"app_id":"demo","versions":["1.0.0"]}]}"#;
        fs::write(&doc, body).unwrap();
        fs::write(
            doc.with_extension("sig"),
            hex::encode(key.sign(body.as_bytes()).to_bytes()),
        )
        .unwrap();
        assert!(verified_manifest(&home, "demo").is_ok());

        let body = r#"{"schema_version":1,"revoked":["demo"]}"#;
        fs::write(&doc, body).unwrap();
        fs::write(
            doc.with_extension("sig"),
            hex::encode(key.sign(body.as_bytes()).to_bytes()),
        )
        .unwrap();
        let err = verified_manifest(&home, "demo").unwrap_err().to_string();
        assert!(err.contains("revoked"), "{err}");

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn update_and_rollback_preserve_runtime_state() {
        let home = std::env::temp_dir().join(format!("eco-registry-runtime-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();

        let key = SigningKey::from_bytes(&[13u8; 32]);
        fs::write(
            trusted_pub_path(&home),
            hex::encode(key.verifying_key().to_bytes()),
        )
        .unwrap();

        let pkg = home.join("pkg");
        write_package(&pkg, "1.0.0", &key);
        install(&home, &pkg).unwrap();

        let db = home.join("apps/demo/engine/data/opencode.db");
        fs::create_dir_all(db.parent().unwrap()).unwrap();
        fs::write(&db, b"chat history").unwrap();
        let dump = home.join("apps/demo/sessions/s1/dumps.ndjson");
        fs::create_dir_all(dump.parent().unwrap()).unwrap();
        fs::write(&dump, b"dump").unwrap();

        write_package(&pkg, "1.1.0", &key);
        install(&home, &pkg).unwrap();
        assert!(db.exists(), "update removed chat history");
        assert!(dump.exists(), "update removed artifacts");

        rollback(&home, "demo", None).unwrap();
        assert!(db.exists(), "rollback removed chat history");
        assert!(dump.exists(), "rollback removed artifacts");

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn uncovered_skill_is_rejected_before_sealing() {
        let home = std::env::temp_dir().join(format!("eco-registry-cover-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        use_local_mode(&home, crate::config::Role::User);

        let pkg = home.join("pkg");
        write_unsigned_package(&pkg, "1.0.0");
        fs::create_dir_all(pkg.join("skills/evil")).unwrap();
        fs::write(pkg.join("skills/evil/SKILL.md"), b"evil").unwrap();

        let err = install(&home, &pkg).unwrap_err().to_string();
        assert!(err.contains("skill is not covered by hashes"), "{err}");

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn recover_runtime_restores_parked_state() {
        let home = std::env::temp_dir().join(format!("eco-registry-recover-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();

        let keep = home.join("staging").join("keep-123-demo");
        fs::create_dir_all(keep.join("engine")).unwrap();
        fs::write(keep.join("engine/opencode.db"), b"db").unwrap();

        recover_runtime(&home);
        assert!(home.join("apps/demo/engine/opencode.db").is_file());
        assert!(!keep.exists());

        let _ = fs::remove_dir_all(home);
    }
}
