use crate::error::{msg, Result};
use serde::Serialize;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DUMP_FILE: &str = "dumps.ndjson";
/// Full stdout/stderr of oversized tool calls lives next to the NDJSON dump.
pub const TOOL_OUTPUT_DIR: &str = "tool-output";
pub const MAX_READ_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    pub sessions: usize,
    pub bytes: u64,
    pub files: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DumpFile {
    pub session: String,
    pub path: String,
    pub bytes: u64,
    pub modified: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GrepHit {
    pub session: String,
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SweepReport {
    pub removed: usize,
    pub bytes_freed: u64,
    pub kept: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    pub dir: String,
    pub files: usize,
    pub bytes: u64,
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

/// Validates a session directory name (`s<timestamp>` style) and returns its
/// path under `root`.
pub fn session_dir(root: &Path, session: &str) -> Result<PathBuf> {
    let name = session.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return msg("invalid artifact session");
    }
    Ok(root.join(name))
}

/// Reads a byte range from a dump or a saved tool-output file. Line boundaries
/// are not required: the caller gets `next_offset` and can continue from there.
pub fn read_range(path: &Path, offset: u64, limit: usize) -> Result<serde_json::Value> {
    use std::io::{Read, Seek, SeekFrom};
    let limit = limit.clamp(1, MAX_READ_BYTES);
    let mut file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    if offset > size {
        return msg(format!("offset {offset} is beyond the file size {size}"));
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.take(limit as u64).read_to_end(&mut bytes)?;
    let next = offset + bytes.len() as u64;
    Ok(serde_json::json!({
        "offset": offset,
        "next_offset": next,
        "size": size,
        "eof": next >= size,
        "text": String::from_utf8_lossy(&bytes),
    }))
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

/// Appends one NDJSON record to the session dump. Returns `false` when the
/// session quota would be exceeded; callers must never fail the tool call just
/// because its optional dump could not be stored.
pub fn append_dump(
    session_dir: &Path,
    record: &serde_json::Value,
    policy: &Policy,
) -> Result<bool> {
    fs::create_dir_all(session_dir)?;
    let path = session_dir.join(DUMP_FILE);
    let current = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let mut line = serde_json::to_string(record)?;
    line.push('\n');
    if current.saturating_add(line.len() as u64) > policy.session_quota_bytes {
        return Ok(false);
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(line.as_bytes())?;
    file.flush()?;
    Ok(true)
}

/// Dump files across all sessions, newest session first.
pub fn list_dumps(root: &Path) -> Vec<DumpFile> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dump = path.join(DUMP_FILE);
        let Ok(meta) = fs::metadata(&dump) else {
            continue;
        };
        out.push(DumpFile {
            session: entry.file_name().to_string_lossy().to_string(),
            path: dump.display().to_string(),
            bytes: meta.len(),
            modified: meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// Case-insensitive substring search over `dumps.ndjson`. `session` narrows
/// the search to one session directory.
pub fn grep(
    root: &Path,
    query: &str,
    session: Option<&str>,
    limit: usize,
) -> Result<Vec<GrepHit>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return msg("grep query is empty");
    }
    let mut out = Vec::new();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(wanted) = session {
            if name != wanted {
                continue;
            }
        }
        let path = dir.join(DUMP_FILE);
        let Ok(file) = fs::File::open(&path) else {
            continue;
        };
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let Ok(line) = line else {
                break;
            };
            if !line.to_lowercase().contains(&query) {
                continue;
            }
            out.push(GrepHit {
                session: name.clone(),
                path: path.display().to_string(),
                line: index + 1,
                text: line.chars().take(400).collect(),
            });
            if out.len() >= limit {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

/// Copies dump files into `dest` as `<session>-dumps.ndjson`. With `session`
/// set, only that session's dump is exported. Read-only with respect to the
/// profile: the source sessions stay in place.
pub fn export(root: &Path, session: Option<&str>, dest: &Path) -> Result<ExportReport> {
    if let Some(name) = session {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            return msg("invalid artifact session");
        }
    }
    fs::create_dir_all(dest)?;
    let mut report = ExportReport {
        dir: dest.display().to_string(),
        files: 0,
        bytes: 0,
    };
    for dump in list_dumps(root) {
        if let Some(wanted) = session {
            if dump.session != wanted {
                continue;
            }
        }
        let target = dest.join(format!("{}-{}", dump.session, DUMP_FILE));
        fs::copy(&dump.path, &target)?;
        report.files += 1;
        report.bytes += dump.bytes;
    }
    Ok(report)
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

    #[test]
    fn dumps_are_quota_bound_and_searchable() {
        let dir = std::env::temp_dir().join(format!("eco-dump-{}", now_secs()));
        let _ = fs::remove_dir_all(&dir);
        let policy = Policy {
            session_quota_bytes: 220,
            ..Policy::default()
        };
        let session = open_session(&dir, &policy).unwrap();
        let record = serde_json::json!({
            "tool": "query-docs",
            "result": { "text": "PostgreSQL migration guidance" }
        });
        assert!(append_dump(&session, &record, &policy).unwrap());

        let hits = grep(&dir, "POSTGRESQL", None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 1);
        assert!(hits[0].text.contains("query-docs"));
        assert!(grep(&dir, "missing", None, 10).unwrap().is_empty());

        let large = serde_json::json!({ "text": "x".repeat(300) });
        assert!(!append_dump(&session, &large, &policy).unwrap());
        assert_eq!(list_dumps(&dir).len(), 1);

        close_session(&session);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_copies_selected_dump_and_rejects_bad_session() {
        let dir = std::env::temp_dir().join(format!("eco-export-{}", now_secs()));
        let dest = std::env::temp_dir().join(format!("eco-export-out-{}", now_secs()));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dest);
        let policy = Policy::default();
        let session = open_session(&dir, &policy).unwrap();
        append_dump(&session, &serde_json::json!({ "text": "needle" }), &policy).unwrap();
        close_session(&session);

        let name = session.file_name().unwrap().to_string_lossy().to_string();
        let report = export(&dir, Some(&name), &dest).unwrap();
        assert_eq!(report.files, 1);
        assert!(dest.join(format!("{name}-{DUMP_FILE}")).exists());

        assert!(export(&dir, None, &dest).unwrap().files >= 1);
        assert!(export(&dir, Some("../escape"), &dest).is_err());
        assert!(!dest.join("..-dumps.ndjson").exists());

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn ranges_read_in_bounded_chunks() {
        let dir = std::env::temp_dir().join(format!("eco-range-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("tool-output.txt");
        fs::write(&file, "0123456789").unwrap();

        let first = read_range(&file, 0, 4).unwrap();
        assert_eq!(first["text"], "0123");
        assert_eq!(first["next_offset"], 4);
        assert_eq!(first["eof"], false);

        let rest = read_range(&file, 4, 100).unwrap();
        assert_eq!(rest["text"], "456789");
        assert_eq!(rest["eof"], true);

        assert!(read_range(&file, 20, 4).is_err());
        assert!(session_dir(&dir, "../x").is_err());
        assert!(session_dir(&dir, "").is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
