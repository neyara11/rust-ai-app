//! Local package seal.
//!
//! Local mode allows installing unsigned packages, but the installed app must
//! stay immutable. At install time the platform records sha256 of every
//! package file (excluding per-app runtime state) in a profile-level seal and
//! re-checks it on every launch. Editing the prompt, a skill or a script — or
//! even the manifest itself — invalidates the seal.
//!
//! The seal protects against accidental or casual modification, not against a
//! determined local administrator: the same OS user owns the seal file. That
//! is why corporate mode still requires a signature from a machine anchor.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const SEAL_SCHEMA: u32 = 1;
const CACHE_SCHEMA: u32 = 1;
const MAX_FILES: usize = 10_000;

#[cfg(test)]
thread_local! {
    /// Counts full (hashing) verifications so a test can assert the cache hit.
    static FULL_VERIFICATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Verification cache. The seal is immutable, so a successful full check can
/// be reused while the seal bytes and every package file's size/mtime stay the
/// same. Editing a file changes its mtime, which forces a full re-hash.
///
/// This is a performance cache, not additional protection: the seal already
/// only guards against casual modification, and the same OS user owns both.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerifyCache {
    schema: u32,
    seal_sha256: String,
    app_id: String,
    version: String,
    file_count: usize,
    tree_sha256: String,
}

/// Per-app runtime state lives inside the package directory but is not part of
/// the package and changes while the app runs. `attachments` is the chat image
/// cache written by the platform on every uploaded picture; sealing it would
/// invalidate the package on the first attachment.
fn is_runtime_entry(name: &std::ffi::OsStr) -> bool {
    name == "engine" || name == "sessions" || name == "tmp" || name == "attachments"
}

/// True for a package-relative path inside a runtime directory. Seals written
/// by builds that still sealed `attachments/` keep those entries, so they must
/// be ignored on both sides instead of being reported as removed files.
fn is_runtime_path(rel: &str) -> bool {
    rel.split('/')
        .next()
        .map(|top| is_runtime_entry(std::ffi::OsStr::new(top)))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seal {
    pub schema: u32,
    pub app_id: String,
    pub version: String,
    /// Relative path (forward slashes) -> `sha256:<hex>`.
    pub files: BTreeMap<String, String>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn seal_path(home: &Path, app_id: &str) -> PathBuf {
    home.join("seals").join(format!("{app_id}.json"))
}

fn collect(
    dir: &Path,
    root: &Path,
    top: bool,
    files: &mut BTreeMap<String, String>,
    count: &mut usize,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if top && is_runtime_entry(&name) {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(Error::Msg(format!(
                "package contains a symlink, refusing to seal: {}",
                path.display()
            )));
        }
        if kind.is_dir() {
            collect(&path, root, false, files, count)?;
        } else if kind.is_file() {
            *count += 1;
            if *count > MAX_FILES {
                return Err(Error::Msg(format!(
                    "package has more than {MAX_FILES} files, refusing to seal"
                )));
            }
            let rel = path
                .strip_prefix(root)
                .map_err(|_| Error::Msg("cannot compute package-relative path".to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(rel, format!("sha256:{}", sha256_hex(&fs::read(&path)?)));
        }
    }
    Ok(())
}

/// Builds a seal from the package on disk.
pub fn build(dir: &Path) -> Result<Seal> {
    let manifest = crate::manifest::load(dir)?;
    let mut files = BTreeMap::new();
    let mut count = 0;
    collect(dir, dir, true, &mut files, &mut count)?;
    Ok(Seal {
        schema: SEAL_SCHEMA,
        app_id: manifest.app_id,
        version: manifest.version,
        files,
    })
}

fn cache_path(home: &Path, app_id: &str) -> PathBuf {
    home.join("seals").join(format!("{app_id}.verify.json"))
}

fn collect_meta(
    dir: &Path,
    root: &Path,
    top: bool,
    out: &mut Vec<(String, u64, u128)>,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if top && is_runtime_entry(&name) {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(Error::Msg(format!(
                "package contains a symlink, refusing to verify: {}",
                path.display()
            )));
        }
        if kind.is_dir() {
            collect_meta(&path, root, false, out)?;
        } else if kind.is_file() {
            if out.len() >= MAX_FILES {
                return Err(Error::Msg(format!(
                    "package has more than {MAX_FILES} files, refusing to verify"
                )));
            }
            let meta = entry.metadata()?;
            let rel = path
                .strip_prefix(root)
                .map_err(|_| Error::Msg("cannot compute package-relative path".to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|age| age.as_nanos())
                .unwrap_or(0);
            out.push((rel, meta.len(), modified));
        }
    }
    Ok(())
}

/// Hashes paths, sizes and mtimes without reading file contents. Traversal
/// order differs between filesystems, so entries are sorted first.
fn tree_fingerprint(dir: &Path) -> Result<(String, usize)> {
    let mut entries = Vec::new();
    collect_meta(dir, dir, true, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (rel, size, mtime) in &entries {
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update(size.to_le_bytes());
        hasher.update([0]);
        hasher.update(mtime.to_le_bytes());
        hasher.update(b"\n");
    }
    Ok((hex::encode(hasher.finalize()), entries.len()))
}

/// Writes (or replaces) the seal for an installed package.
pub fn write(home: &Path, app_id: &str, dir: &Path) -> Result<Seal> {
    let manifest = crate::manifest::load(dir)?;
    if manifest.app_id != app_id {
        return Err(Error::Msg(format!(
            "seal target app_id mismatch: {app_id} vs {}",
            manifest.app_id
        )));
    }
    let seal = build(dir)?;
    let dest = seal_path(home, app_id);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(&seal)?)?;
    fs::rename(&tmp, &dest)?;
    // The seal changed; any previous verification snapshot is stale.
    let _ = fs::remove_file(cache_path(home, app_id));
    Ok(seal)
}

fn verify_cache_hit(
    home: &Path,
    app_id: &str,
    seal: &Seal,
    seal_digest: &str,
    dir: &Path,
) -> bool {
    let Ok(raw) = fs::read(cache_path(home, app_id)) else {
        return false;
    };
    let Ok(cache) = serde_json::from_slice::<VerifyCache>(&raw) else {
        return false;
    };
    if cache.schema != CACHE_SCHEMA
        || cache.seal_sha256 != seal_digest
        || cache.app_id != app_id
        || cache.version != seal.version
    {
        return false;
    }
    match tree_fingerprint(dir) {
        Ok((tree, count)) => tree == cache.tree_sha256 && count == cache.file_count,
        Err(_) => false,
    }
}

fn write_verify_cache(home: &Path, app_id: &str, seal: &Seal, seal_digest: &str, dir: &Path) {
    let Ok((tree, count)) = tree_fingerprint(dir) else {
        return;
    };
    let cache = VerifyCache {
        schema: CACHE_SCHEMA,
        seal_sha256: seal_digest.to_string(),
        app_id: app_id.to_string(),
        version: seal.version.clone(),
        file_count: count,
        tree_sha256: tree,
    };
    let Ok(bytes) = serde_json::to_vec(&cache) else {
        return;
    };
    let dest = cache_path(home, app_id);
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = dest.with_extension("tmp");
    if fs::write(&tmp, bytes).is_ok() {
        let _ = fs::rename(&tmp, &dest);
    }
}

/// Verifies that every package file still matches the seal and that no files
/// were added or removed.
pub fn verify(home: &Path, app_id: &str, dir: &Path) -> Result<()> {
    verify_inner(home, app_id, dir, true)
}

/// Read-only variant: uses an existing verification cache but never writes a
/// new one. Used by CLI checks that must not mutate the installed profile.
pub fn verify_readonly(home: &Path, app_id: &str, dir: &Path) -> Result<()> {
    verify_inner(home, app_id, dir, false)
}

fn verify_inner(home: &Path, app_id: &str, dir: &Path, persist_cache: bool) -> Result<()> {
    let path = seal_path(home, app_id);
    let raw = fs::read(&path).map_err(|e| {
        Error::Msg(format!(
            "package seal is missing ({}); reinstall the app: {e}",
            path.display()
        ))
    })?;
    let seal: Seal = serde_json::from_slice(&raw)
        .map_err(|e| Error::Msg(format!("package seal is unreadable: {e}")))?;
    if seal.schema != SEAL_SCHEMA {
        return Err(Error::Msg(format!(
            "package seal schema {} is not supported",
            seal.schema
        )));
    }
    if seal.app_id != app_id {
        return Err(Error::Msg(format!(
            "package seal belongs to {}, not {app_id}",
            seal.app_id
        )));
    }

    let seal_digest = sha256_hex(&raw);
    if verify_cache_hit(home, app_id, &seal, &seal_digest, dir) {
        return Ok(());
    }

    #[cfg(test)]
    FULL_VERIFICATIONS.with(|count| count.set(count.get() + 1));
    let actual = build(dir)?;
    if actual.app_id != seal.app_id {
        return Err(Error::Msg(format!(
            "package app_id changed since install: {} → {}",
            seal.app_id, actual.app_id
        )));
    }
    if actual.version != seal.version {
        return Err(Error::Msg(format!(
            "package version changed since install: {} → {}",
            seal.version, actual.version
        )));
    }
    let sealed: BTreeMap<String, String> = seal
        .files
        .iter()
        .filter(|(rel, _)| !is_runtime_path(rel))
        .map(|(rel, hash)| (rel.clone(), hash.clone()))
        .collect();
    if actual.files == sealed {
        if persist_cache {
            write_verify_cache(home, app_id, &seal, &seal_digest, dir);
        }
        return Ok(());
    }

    let mut changes: Vec<String> = Vec::new();
    for (rel, expected) in &sealed {
        match actual.files.get(rel) {
            None => changes.push(format!("removed {rel}")),
            Some(found) if found != expected => changes.push(format!("modified {rel}")),
            Some(_) => {}
        }
    }
    for rel in actual.files.keys() {
        if !sealed.contains_key(rel) {
            changes.push(format!("added {rel}"));
        }
    }
    changes.truncate(6);
    Err(Error::Msg(format!(
        "package content changed since install: {}",
        changes.join(", ")
    )))
}

pub fn remove(home: &Path, app_id: &str) {
    let _ = fs::remove_file(seal_path(home, app_id));
    let _ = fs::remove_file(cache_path(home, app_id));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eco-seal-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("home")).unwrap();
        fs::create_dir_all(dir.join("pkg/prompt")).unwrap();
        fs::write(dir.join("pkg/manifest.json"), r#"{"schema_version":2,"app_id":"demo","name":"Demo","version":"1.0.0","profile":{"system_prompt":"prompt/system.md"},"hashes":{}}"#).unwrap();
        fs::write(dir.join("pkg/prompt/system.md"), b"hello").unwrap();
        dir
    }

    #[test]
    fn seal_detects_modified_added_and_removed_files() {
        let root = tmp("files");
        let home = root.join("home");
        let pkg = root.join("pkg");
        write(&home, "demo", &pkg).unwrap();
        verify(&home, "demo", &pkg).unwrap();

        fs::write(pkg.join("prompt/system.md"), b"tampered").unwrap();
        assert!(verify(&home, "demo", &pkg).is_err());

        write(&home, "demo", &pkg).unwrap();
        fs::write(pkg.join("extra.txt"), b"x").unwrap();
        assert!(verify(&home, "demo", &pkg).is_err());

        let _ = fs::remove_file(pkg.join("extra.txt"));
        write(&home, "demo", &pkg).unwrap();
        fs::remove_file(pkg.join("prompt/system.md")).unwrap();
        assert!(verify(&home, "demo", &pkg).is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn seal_ignores_runtime_state() {
        let root = tmp("runtime");
        let home = root.join("home");
        let pkg = root.join("pkg");
        write(&home, "demo", &pkg).unwrap();
        fs::create_dir_all(pkg.join("engine/config")).unwrap();
        fs::write(pkg.join("engine/config/opencode.json"), b"{}").unwrap();
        fs::create_dir_all(pkg.join("sessions/s1")).unwrap();
        // The chat image cache is platform state, not package content: writing
        // it must not invalidate the seal (regression: first image bricked the
        // app with "added attachments/<hash>.json").
        fs::create_dir_all(pkg.join("attachments")).unwrap();
        fs::write(
            pkg.join("attachments/hash.json"),
            b"{\"mime\":\"image/png\"}",
        )
        .unwrap();
        verify(&home, "demo", &pkg).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn seal_written_with_cached_attachments_still_verifies() {
        let root = tmp("poisoned");
        let home = root.join("home");
        let pkg = root.join("pkg");
        write(&home, "demo", &pkg).unwrap();

        // A seal produced before the cache was treated as runtime state lists
        // the attachment files; those entries must not read as removed.
        let mut seal = build(&pkg).unwrap();
        seal.files.insert(
            "attachments/hash.json".to_string(),
            "sha256:deadbeef".to_string(),
        );
        fs::write(seal_path(&home, "demo"), serde_json::to_vec(&seal).unwrap()).unwrap();
        fs::create_dir_all(pkg.join("attachments")).unwrap();
        fs::write(pkg.join("attachments/hash.json"), b"{}").unwrap();

        verify(&home, "demo", &pkg).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verification_cache_skips_unchanged_packages() {
        let root = tmp("cache");
        let home = root.join("home");
        let pkg = root.join("pkg");
        write(&home, "demo", &pkg).unwrap();
        let checks = || FULL_VERIFICATIONS.with(|count| count.get());

        let before = checks();
        verify(&home, "demo", &pkg).unwrap();
        assert_eq!(checks(), before + 1);
        assert!(cache_path(&home, "demo").exists());

        // Second run over an unchanged tree must not hash the files again.
        verify(&home, "demo", &pkg).unwrap();
        assert_eq!(checks(), before + 1, "unchanged tree must hit the cache");

        // Runtime state is not sealed and must not invalidate the cache.
        fs::create_dir_all(pkg.join("sessions/s1")).unwrap();
        fs::write(pkg.join("sessions/s1/meta.json"), b"{}").unwrap();
        verify(&home, "demo", &pkg).unwrap();
        assert_eq!(checks(), before + 1);

        // Same size, different content: the mtime change forces a full check.
        fs::write(pkg.join("prompt/system.md"), b"HELLO").unwrap();
        assert!(verify(&home, "demo", &pkg).is_err());
        assert_eq!(checks(), before + 2);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resealing_invalidates_the_verification_cache() {
        let root = tmp("resel");
        let home = root.join("home");
        let pkg = root.join("pkg");
        write(&home, "demo", &pkg).unwrap();
        verify(&home, "demo", &pkg).unwrap();
        assert!(cache_path(&home, "demo").exists());

        fs::write(pkg.join("prompt/system.md"), b"new prompt").unwrap();
        write(&home, "demo", &pkg).unwrap();
        assert!(!cache_path(&home, "demo").exists());
        verify(&home, "demo", &pkg).unwrap();

        let _ = fs::remove_dir_all(root);
    }
}
