use crate::error::{msg, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    pub sessions: usize,
    pub bytes: u64,
    pub files: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SweepReport {
    pub removed: usize,
    pub bytes_freed: u64,
    pub kept: usize,
}

#[derive(Debug, Clone)]
pub struct Policy {
    pub session_quota_bytes: u64,
    pub session_quota_files: usize,
    pub global_quota_bytes: u64,
    pub global_quota_files: usize,
    pub ttl_hours: u64,
    pub orphan_hours: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            session_quota_bytes: 500 * 1024 * 1024,
            session_quota_files: 100,
            global_quota_bytes: 5 * 1024 * 1024 * 1024,
            global_quota_files: 500,
            ttl_hours: 168,
            orphan_hours: 24,
        }
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn sessions_root(home: &Path, app_id: &str) -> PathBuf {
    home.join("apps").join(app_id).join("sessions")
}

fn dir_stats(dir: &Path) -> Result<(u64, usize)> {
    let mut bytes = 0u64;
    let mut files = 0usize;
    if !dir.exists() {
        return Ok((0, 0));
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            let (b, f) = dir_stats(&entry.path())?;
            bytes += b;
            files += f;
        } else {
            bytes += meta.len();
            files += 1;
        }
    }
    Ok((bytes, files))
}

pub fn usage(root: &Path) -> Usage {
    let mut out = Usage {
        sessions: 0,
        bytes: 0,
        files: 0,
    };
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        out.sessions += 1;
        if let Ok((b, f)) = dir_stats(&path) {
            out.bytes += b;
            out.files += f;
        }
    }
    out
}

pub fn open_session(root: &Path, policy: &Policy) -> Result<PathBuf> {
    fs::create_dir_all(root)?;
    let current = usage(root);
    if current.bytes >= policy.global_quota_bytes {
        return msg(format!(
            "global artifact quota exceeded: {} bytes >= {}",
            current.bytes, policy.global_quota_bytes
        ));
    }
    if current.files >= policy.global_quota_files {
        return msg(format!(
            "global artifact file quota exceeded: {} >= {}",
            current.files, policy.global_quota_files
        ));
    }

    let id = format!("s{}", now_secs());
    let dir = root.join(id);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(".active"), b"1")?;
    Ok(dir)
}

pub fn close_session(dir: &Path) {
    let marker = dir.join(".active");
    if marker.exists() {
        let _ = fs::remove_file(marker);
    }
}

pub fn sweep(root: &Path, policy: &Policy) -> Result<SweepReport> {
    let mut report = SweepReport {
        removed: 0,
        bytes_freed: 0,
        kept: 0,
    };
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(report);
    };
    let now = now_secs();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let active = path.join(".active").exists();
        let age_secs = fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| now.saturating_sub(d.as_secs()))
            .unwrap_or(0);

        let expired = age_secs > policy.ttl_hours * 3600;
        let orphan = !active && age_secs >= policy.orphan_hours * 3600;

        if (expired || orphan) && !active {
            let (bytes, _) = dir_stats(&path)?;
            fs::remove_dir_all(&path)?;
            report.removed += 1;
            report.bytes_freed += bytes;
        } else {
            report.kept += 1;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_sessions_are_not_swept() {
        let dir = std::env::temp_dir().join(format!("eco-art-{}", now_secs()));
        let _ = fs::remove_dir_all(&dir);
        let policy = Policy {
            orphan_hours: 0,
            ttl_hours: 24,
            ..Policy::default()
        };
        let session = open_session(&dir, &policy).unwrap();
        fs::write(session.join("dump.ndjson"), b"line\n").unwrap();

        let report = sweep(&dir, &policy).unwrap();
        assert_eq!(report.removed, 0);
        assert!(session.exists());

        close_session(&session);
        let report = sweep(&dir, &policy).unwrap();
        assert_eq!(report.removed, 1);
        assert!(!session.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
