use crate::error::{msg, Error, Result};
use crate::manifest::{self, Capabilities, Connection, Manifest, ToolDef};
use ed25519_dalek::VerifyingKey;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize)]
pub struct InstalledApp {
    pub app_id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub path: String,
    pub trusted: bool,
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

#[cfg(windows)]
fn machine_anchor_bytes() -> Option<[u8; 32]> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ,
    };

    let sub = wide("SOFTWARE\\EcoAIOps");
    let name = wide("TrustedPub");
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
            || kind != REG_SZ
            || len == 0
            || len > 256
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
        let raw = String::from_utf8_lossy(&buf);
        let bytes = hex::decode(raw.trim().trim_end_matches('\0')).ok()?;
        bytes.try_into().ok()
    }
}

#[cfg(not(windows))]
fn machine_anchor_bytes() -> Option<[u8; 32]> {
    None
}

#[cfg(windows)]
fn write_machine_anchor(hex_key: &str) -> Result<()> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_WRITE,
        REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    let sub = wide("SOFTWARE\\EcoAIOps");
    let name = wide("TrustedPub");
    let mut data: Vec<u8> = hex_key.as_bytes().to_vec();
    data.push(0);
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
fn write_machine_anchor(_hex_key: &str) -> Result<()> {
    msg("machine trust anchor is only supported on Windows")
}

/// Resolves the anchor and its bytes. Machine scope wins; the per-user anchor
/// is a dev fallback only.
fn resolve_anchor(home: &Path) -> Option<(String, AnchorSource, [u8; 32])> {
    if !cfg!(test) {
        if let Some(bytes) = machine_anchor_bytes() {
            return Some((MACHINE_ANCHOR_LABEL.to_string(), AnchorSource::Machine, bytes));
        }
    }
    if dev_mode() {
        let path = trusted_pub_path(home);
        if path.is_file() {
            let raw = fs::read_to_string(&path).ok()?;
            let bytes: [u8; 32] = hex::decode(raw.trim()).ok()?.try_into().ok()?;
            return Some((path.display().to_string(), AnchorSource::Dev, bytes));
        }
    }
    None
}

pub fn trusted_anchor(home: &Path) -> Option<(String, AnchorSource)> {
    resolve_anchor(home).map(|(label, source, _)| (label, source))
}

pub fn load_trusted_key(home: &Path) -> Option<VerifyingKey> {
    let (_, _, bytes) = resolve_anchor(home)?;
    VerifyingKey::from_bytes(&bytes).ok()
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

fn installed(
    home: &Path,
    path: &Path,
    manifest: &Manifest,
    report: &manifest::VerifyReport,
) -> InstalledApp {
    InstalledApp {
        app_id: manifest.app_id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        publisher: manifest.publisher.clone(),
        path: path.display().to_string(),
        trusted: report.ok,
        problems: report.problems.clone(),
        tools: manifest.tools.clone(),
        connections: manifest.connections.clone(),
        capabilities: manifest.capabilities.clone(),
        history: history(home, &manifest.app_id),
    }
}

pub fn list(home: &Path) -> Vec<InstalledApp> {
    let trusted = load_trusted_key(home);
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
                let report = manifest::verify_package(&path, trusted.as_ref());
                out.push(installed(home, &path, &m, &report));
            }
            Err(e) => out.push(InstalledApp {
                app_id: entry.file_name().to_string_lossy().to_string(),
                name: entry.file_name().to_string_lossy().to_string(),
                version: "?".into(),
                publisher: String::new(),
                path: path.display().to_string(),
                trusted: false,
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

fn archive_current(home: &Path, app_id: &str, src: &Path) -> Result<Option<PathBuf>> {
    let Ok(manifest) = manifest::load(src) else {
        return Ok(None);
    };
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
    Ok(Some(dst))
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

    let trusted = load_trusted_key(home);
    let report = manifest::verify_package(&archived, trusted.as_ref());
    if !report.ok {
        return msg(format!(
            "archived package rejected (fail-closed): {}",
            report.problems.join("; ")
        ));
    }
    let manifest = manifest::load(&archived)?;

    let staging = home.join("staging").join(format!("rollback-{}", now_secs()));
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    fs::create_dir_all(&staging)?;
    copy_package_dir(&archived, &staging)?;
    let staged_report = manifest::verify_package(&staging, trusted.as_ref());
    if !staged_report.ok {
        let _ = fs::remove_dir_all(&staging);
        return msg(format!(
            "rollback staging failed verification: {}",
            staged_report.problems.join("; ")
        ));
    }

    archive_current(home, app_id, &dest)?;
    fs::remove_dir_all(&dest)?;
    fs::rename(&staging, &dest)?;

    audit_rollback(home, app_id, &current.version, &manifest.version);

    let report = manifest::verify_package(&dest, trusted.as_ref());
    Ok(installed(home, &dest, &manifest, &report))
}

/// Loads the manifest only if the package is installed and passes signature
/// and hash verification. Every entry point that runs app content goes
/// through this gate.
pub fn verified_manifest(home: &Path, app_id: &str) -> Result<Manifest> {
    manifest::parse_id(app_id, "app")?;
    let path =
        find(home, app_id).ok_or_else(|| Error::Msg(format!("app {app_id} is not installed")))?;
    let trusted = load_trusted_key(home);
    let report = manifest::verify_package(&path, trusted.as_ref());
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
    manifest::load(&path)
}

pub fn install(home: &Path, src: &Path) -> Result<InstalledApp> {
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

    let trusted = load_trusted_key(home);
    let report = manifest::verify_package(&staging, trusted.as_ref());
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

    let dest = apps_dir(home).join(&manifest.app_id);
    if dest.exists() {
        if let Ok(existing) = manifest::load(&dest) {
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
        }
        if let Err(e) = archive_current(home, &manifest.app_id, &dest) {
            let _ = fs::remove_dir_all(&staging);
            return msg(format!(
                "cannot archive the installed version before update: {e}"
            ));
        }
        fs::remove_dir_all(&dest)?;
    }
    fs::create_dir_all(apps_dir(home))?;
    fs::rename(&staging, &dest)?;

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
    Ok(installed(home, &dest, &manifest, &report))
}

pub fn remove(home: &Path, app_id: &str) -> Result<()> {
    manifest::parse_id(app_id, "app")?;
    let dest = apps_dir(home).join(app_id);
    if !dest.exists() {
        return msg(format!("app {app_id} is not installed"));
    }
    fs::remove_dir_all(&dest)?;
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

fn extract_zip(src: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(src)?;
    let mut archive = zip::ZipArchive::new(file)?;
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
            std::io::copy(&mut entry, &mut f)?;
        }
    }
    Ok(())
}

/// Extracts a zip package into `dest`. Public for the headless `--verify` path.
pub fn extract_zip_public(src: &Path, dest: &Path) -> Result<()> {
    extract_zip(src, dest)
}



/// Installs the trust anchor (public signing key) into the platform home.
/// In production this is done by the installer under an admin account.
#[derive(Debug, Clone, Serialize)]
pub struct TrustAnchorInstalled {
    pub fingerprint: String,
    pub path: String,
    pub source: String,
    pub public_key: String,
}

/// Installs the trust anchor. Machine-wide when elevated; in a release build
/// without elevation this fails, so a user cannot pin their own signing key.
pub fn install_trust_anchor(home: &std::path::Path, src: &std::path::Path) -> Result<TrustAnchorInstalled> {
    let raw = fs::read_to_string(src)?;
    let bytes = hex::decode(raw.trim())?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Msg("public key must be 32 bytes".to_string()))?;
    let key = VerifyingKey::from_bytes(&arr)
        .map_err(|e| Error::Msg(format!("invalid public key: {e}")))?;

    let (dest, source) = if is_elevated() && !cfg!(test) {
        (MACHINE_ANCHOR_LABEL.to_string(), AnchorSource::Machine)
    } else if dev_mode() {
        (
            trusted_pub_path(home).display().to_string(),
            AnchorSource::Dev,
        )
    } else {
        return msg("administrator rights are required: the trust anchor must be installed machine-wide");
    };

    let hex_key = hex::encode(arr);
    if source == AnchorSource::Machine {
        write_machine_anchor(&hex_key)?;
    } else {
        fs::create_dir_all(home)?;
        fs::write(trusted_pub_path(home), &hex_key)?;
    }

    let digest = manifest::sha256_hex(&key.to_bytes());
    Ok(TrustAnchorInstalled {
        fingerprint: digest[..16].to_string(),
        path: dest,
        source: source.as_str().to_string(),
        public_key: hex_key,
    })
}

/// Short fingerprint of the installed trust anchor, so a human can compare it
/// with the value published by the security team.
pub fn anchor_fingerprint(home: &Path) -> Option<String> {
    let key = load_trusted_key(home)?;
    let digest = manifest::sha256_hex(&key.to_bytes());
    Some(digest[..16].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::fs;

    fn write_package(dir: &Path, version: &str, key: &SigningKey) {
        fs::create_dir_all(dir.join("prompt")).unwrap();
        fs::write(dir.join("prompt/system.md"), b"prompt").unwrap();
        let hash = manifest::sha256_hex(b"prompt");
        let body = format!(
            r#"{{"schema_version":2,"app_id":"demo","name":"Demo","version":"{version}","profile":{{"system_prompt":"prompt/system.md"}},"hashes":{{"prompt/system.md":"sha256:{hash}"}}}}"#
        );
        fs::write(dir.join("manifest.json"), &body).unwrap();
        let sig = key.sign(body.as_bytes());
        fs::write(dir.join("manifest.sig"), hex::encode(sig.to_bytes())).unwrap();
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
        assert!(load_trusted_key(&home).is_some());

        let _ = fs::remove_dir_all(home);
    }
}
