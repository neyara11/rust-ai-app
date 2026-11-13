use crate::error::{msg, Error, Result};
use crate::manifest;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct DocMeta {
    pub id: String,
    pub bytes: u64,
    pub revision: String,
    pub finalized: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocView {
    pub id: String,
    pub markdown: String,
    pub revision: String,
    pub finalized: bool,
}

/// Markdown documents of a session. The agent never sees a filesystem path:
/// only a document id and a revision.
pub struct DocStore {
    root: PathBuf,
}

fn revision_of(markdown: &str) -> String {
    let mut h = Sha256::new();
    h.update(markdown.as_bytes());
    hex::encode(h.finalize())[..12].to_string()
}

fn validate_id(id: &str) -> Result<()> {
    manifest::parse_id(id, "document")
}

impl DocStore {
    pub fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn path(&self, id: &str) -> Result<PathBuf> {
        validate_id(id)?;
        Ok(self.root.join(format!("{id}.md")))
    }

    fn final_marker(&self, id: &str) -> Result<PathBuf> {
        validate_id(id)?;
        Ok(self.root.join(format!("{id}.final")))
    }

    pub fn list(&self) -> Result<Vec<DocMeta>> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Ok(out);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let markdown = fs::read_to_string(&path)?;
            out.push(DocMeta {
                id: stem.to_string(),
                bytes: markdown.len() as u64,
                revision: revision_of(&markdown),
                finalized: self.final_marker(stem)?.exists(),
            });
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    pub fn read(&self, id: &str) -> Result<DocView> {
        let path = self.path(id)?;
        if !path.exists() {
            return msg(format!("document {id} does not exist"));
        }
        let markdown = fs::read_to_string(&path)?;
        Ok(DocView {
            id: id.to_string(),
            revision: revision_of(&markdown),
            finalized: self.final_marker(id)?.exists(),
            markdown,
        })
    }

    pub fn create(&self, id: &str, initial: &str) -> Result<DocView> {
        let path = self.path(id)?;
        if path.exists() {
            return msg(format!("document {id} already exists"));
        }
        self.write_raw(id, initial)?;
        self.read(id)
    }

    /// Whole-document replace. Used by the human UI; revision check protects
    /// against clobbering an edit made in another view.
    pub fn write(&self, id: &str, markdown: &str, expected: Option<&str>) -> Result<DocView> {
        let current = self.read(id)?;
        if let Some(expected) = expected {
            if current.revision != expected {
                return msg(format!(
                    "revision conflict on {id}: expected {expected}, current {}",
                    current.revision
                ));
            }
        }
        self.write_raw(id, markdown)?;
        self.read(id)
    }

    pub fn append(&self, id: &str, markdown: &str, expected: Option<&str>) -> Result<DocView> {
        let current = self.read(id)?;
        self.check_rev(id, &current.revision, expected)?;
        let mut content = current.markdown;
        if !content.ends_with('\n') {
            content.push('\n');
        }
        if !content.ends_with("\n\n") {
            content.push('\n');
        }
        content.push_str(markdown);
        if !content.ends_with('\n') {
            content.push('\n');
        }
        self.write_raw(id, &content)?;
        self.read(id)
    }

    pub fn insert_after(
        &self,
        id: &str,
        anchor: &str,
        markdown: &str,
        expected: Option<&str>,
    ) -> Result<DocView> {
        let current = self.read(id)?;
        self.check_rev(id, &current.revision, expected)?;
        let mut lines: Vec<String> = current.markdown.lines().map(str::to_string).collect();
        let at = find_anchor(&lines, anchor)
            .ok_or_else(|| Error::Msg(format!("anchor {anchor} not found in {id}")))?;
        let end = block_end(&lines, at);
        let mut inserted: Vec<String> = markdown.lines().map(str::to_string).collect();
        let tail = lines.split_off(end);
        lines.append(&mut inserted);
        lines.extend(tail);
        self.write_raw(id, &join_lines(&lines))?;
        self.read(id)
    }

    pub fn replace_block(
        &self,
        id: &str,
        anchor: &str,
        markdown: &str,
        expected: Option<&str>,
    ) -> Result<DocView> {
        let current = self.read(id)?;
        self.check_rev(id, &current.revision, expected)?;
        let mut lines: Vec<String> = current.markdown.lines().map(str::to_string).collect();
        let at = find_anchor(&lines, anchor)
            .ok_or_else(|| Error::Msg(format!("anchor {anchor} not found in {id}")))?;
        let end = block_end(&lines, at);
        let mut replacement: Vec<String> = markdown.lines().map(str::to_string).collect();
        lines.splice(at + 1..end, replacement.drain(..));
        self.write_raw(id, &join_lines(&lines))?;
        self.read(id)
    }

    pub fn finalize(&self, id: &str) -> Result<DocView> {
        let view = self.read(id)?;
        fs::write(self.final_marker(id)?, b"1")?;
        Ok(DocView {
            finalized: true,
            ..view
        })
    }

    fn check_rev(&self, id: &str, current: &str, expected: Option<&str>) -> Result<()> {
        if let Some(expected) = expected {
            if current != expected {
                return msg(format!(
                    "revision conflict on {id}: expected {expected}, current {current}"
                ));
            }
        }
        Ok(())
    }

    fn write_raw(&self, id: &str, content: &str) -> Result<()> {
        let path = self.path(id)?;
        let tmp = path.with_extension("md.tmp");
        fs::write(&tmp, content)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }
}

fn anchor_line(anchor: &str) -> String {
    format!("<!-- id: {anchor} -->")
}

fn find_anchor(lines: &[String], anchor: &str) -> Option<usize> {
    let needle = anchor_line(anchor);
    lines.iter().position(|l| l.trim() == needle)
}

fn is_anchor(line: &str) -> bool {
    line.trim_start().starts_with("<!-- id:")
}

fn block_end(lines: &[String], anchor_index: usize) -> usize {
    let mut i = anchor_index + 1;
    while i < lines.len() && !is_anchor(&lines[i]) {
        i += 1;
    }
    i
}

fn join_lines(lines: &[String]) -> String {
    let mut s = lines.join("\n");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (DocStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("eco-docs-test-{}", registry_now()));
        let _ = fs::remove_dir_all(&dir);
        (DocStore::new(dir.clone()).unwrap(), dir)
    }

    fn registry_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }

    const DOC: &str = "## Симптом\n\n<!-- id: symptom -->\nсервис отдаёт 500\n\n<!-- id: timeline -->\n10:00 старт\n";

    #[test]
    fn create_read_append() {
        let (s, dir) = store();
        let v = s.create("report", DOC).unwrap();
        assert_eq!(v.revision.len(), 12);
        let v2 = s.append("report", "## Вывод\n\nдеградация БД", Some(&v.revision)).unwrap();
        assert!(v2.markdown.contains("деградация БД"));
        assert_ne!(v.revision, v2.revision);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_revision_is_rejected() {
        let (s, dir) = store();
        let v = s.create("report", DOC).unwrap();
        s.append("report", "x", Some(&v.revision)).unwrap();
        assert!(s.append("report", "y", Some(&v.revision)).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn insert_after_and_replace_block() {
        let (s, dir) = store();
        s.create("report", DOC).unwrap();
        let v = s
            .insert_after("report", "symptom", "дополнительная деталь\n", None)
            .unwrap();
        assert!(v.markdown.contains("дополнительная деталь"));
        let v = s
            .replace_block("report", "timeline", "10:05 ретрай\n", None)
            .unwrap();
        assert!(v.markdown.contains("10:05 ретрай"));
        assert!(!v.markdown.contains("10:00 старт"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn path_traversal_ids_are_rejected() {
        let (s, dir) = store();
        assert!(s.read("../../secrets").is_err());
        assert!(s.create("a/b", "x").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn finalize_marks_document() {
        let (s, dir) = store();
        s.create("defect", "# Дефект\n").unwrap();
        let v = s.finalize("defect").unwrap();
        assert!(v.finalized);
        let _ = fs::remove_dir_all(dir);
    }
}
