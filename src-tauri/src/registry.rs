use crate::error::{msg, Error, Result};
use crate::manifest::{self, Connection, DocDef, Manifest, ToolDef};
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
    pub docs: Vec<DocDef>,
    pub tools: Vec<ToolDef>,
    pub connections: Vec<Connection>,
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

pub fn load_trusted_key(home: &Path) -> Option<VerifyingKey> {
    let raw = fs::read_to_string(trusted_pub_path(home)).ok()?;
    let bytes = hex::decode(raw.trim()).ok()?;
    let arr: [u8; 32] = bytes.try_into().ok()?;
    VerifyingKey::from_bytes(&arr).ok()
}

fn installed(path: &Path, manifest: &Manifest, report: &manifest::VerifyReport) -> InstalledApp {
    InstalledApp {
        app_id: manifest.app_id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        publisher: manifest.publisher.clone(),
        path: path.display().to_string(),
        trusted: report.ok,
        problems: report.problems.clone(),
        docs: manifest.docs.clone(),
        tools: manifest.tools.clone(),
        connections: manifest.connections.clone(),
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
                out.push(installed(&path, &m, &report));
            }
            Err(e) => out.push(InstalledApp {
                app_id: entry.file_name().to_string_lossy().to_string(),
                name: entry.file_name().to_string_lossy().to_string(),
                version: "?".into(),
                publisher: String::new(),
                path: path.display().to_string(),
                trusted: false,
                problems: vec![format!("manifest unreadable: {e}")],
                docs: Vec::new(),
                tools: Vec::new(),
                connections: Vec::new(),
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
                let _ = fs::remove_dir_all(&staging);
                return Err(e);
            }
        }
        fs::remove_dir_all(&dest)?;
    }
    fs::create_dir_all(apps_dir(home))?;
    fs::rename(&staging, &dest)?;

    Ok(installed(&dest, &manifest, &report))
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
pub fn install_trust_anchor(home: &std::path::Path, src: &std::path::Path) -> Result<String> {
    let raw = fs::read_to_string(src)?;
    let bytes = hex::decode(raw.trim())?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Msg("public key must be 32 bytes".to_string()))?;
    VerifyingKey::from_bytes(&arr)
        .map_err(|e| Error::Msg(format!("invalid public key: {e}")))?;
    fs::create_dir_all(home)?;
    fs::write(trusted_pub_path(home), hex::encode(arr))?;
    Ok(hex::encode(arr))
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
}
