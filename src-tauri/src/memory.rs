use crate::error::{msg, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const MAX_FACTS: usize = 1000;
const MAX_TEXT_CHARS: usize = 2000;
const MAX_KEYWORDS: usize = 12;
const MAX_KEYWORD_CHARS: usize = 40;
const MAX_FIELD_CHARS: usize = 200;
const LOCK_STALE_SECS: u64 = 60;
const LOCK_TRIES: usize = 100;
const LOCK_SLEEP_MS: u64 = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub ts: u64,
    pub text: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub request: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub count: usize,
    pub bytes: u64,
    pub latest_ts: Option<u64>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Memory is scoped to app + workspace: different folders are different
/// projects, so their facts never mix.
pub fn scope_dir(home: &Path, app_id: &str) -> PathBuf {
    home.join("memory").join(app_id)
}

pub fn scope_key(workspace: &Path) -> String {
    let canonical = fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    let mut normalized = canonical.to_string_lossy().into_owned();
    if cfg!(windows) {
        normalized = normalized.to_lowercase();
    }
    let digest = Sha256::digest(normalized.as_bytes());
    hex::encode(digest)[..16].to_string()
}

pub fn facts_path(home: &Path, app_id: &str, workspace: &Path) -> PathBuf {
    scope_dir(home, app_id).join(format!("{}.jsonl", scope_key(workspace)))
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn clean_text(raw: &str) -> Result<String> {
    let text: String = raw.trim().chars().take(MAX_TEXT_CHARS).collect();
    if text.is_empty() {
        return msg("memory text is empty");
    }
    Ok(text)
}

fn clean_keywords(raw: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(Value::Array(items)) = raw {
        for item in items {
            let Some(keyword) = item.as_str() else {
                continue;
            };
            let keyword = keyword
                .trim()
                .chars()
                .take(MAX_KEYWORD_CHARS)
                .collect::<String>();
            if !keyword.is_empty() && !out.iter().any(|seen| seen.eq_ignore_ascii_case(&keyword)) {
                out.push(keyword);
            }
            if out.len() >= MAX_KEYWORDS {
                break;
            }
        }
    }
    out
}

fn clean_field(raw: Option<&str>) -> Option<String> {
    let value = raw?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.chars().take(MAX_FIELD_CHARS).collect())
    }
}

fn load(path: &Path) -> Result<Vec<Fact>> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::Io(e)),
    };
    let mut facts = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(fact) = serde_json::from_str::<Fact>(line) {
            facts.push(fact);
        }
    }
    Ok(facts)
}

fn rewrite(path: &Path, facts: &[Fact]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| Error::Msg("memory path has no parent".to_string()))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|name| name.to_str()).unwrap_or("memory"),
        std::process::id()
    ));
    let mut file = fs::File::create(&tmp)?;
    for fact in facts {
        file.write_all(serde_json::to_string(fact)?.as_bytes())?;
        file.write_all(b"\n")?;
    }
    file.flush()?;
    drop(file);
    // Windows `rename` refuses to replace; the lock keeps writers out.
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Cross-process lock around memory mutations (gateway and UI can run in
/// different processes). A stale lock from a crash is stolen after a minute.
struct ScopeLock {
    path: PathBuf,
}

impl ScopeLock {
    fn acquire(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join("memory.lock");
        for _ in 0..LOCK_TRIES {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    let _ = writeln!(file, "{} {}", std::process::id(), now_secs());
                    return Ok(Self { path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if let Ok(meta) = fs::metadata(&path) {
                        if let Ok(modified) = meta.modified() {
                            if let Ok(age) = SystemTime::now().duration_since(modified) {
                                if age.as_secs() > LOCK_STALE_SECS {
                                    let _ = fs::remove_file(&path);
                                    continue;
                                }
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(LOCK_SLEEP_MS));
                }
                Err(e) => return Err(Error::Io(e)),
            }
        }
        msg("memory is busy; try again")
    }
}

impl Drop for ScopeLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn new_id(ts: u64, normalized: &str) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let digest = Sha256::digest(format!("{ts}:{seq}:{normalized}").as_bytes());
    format!("mem_{}", &hex::encode(digest)[..12])
}

pub fn save(
    home: &Path,
    app_id: &str,
    workspace: &Path,
    text: &str,
    keywords: Option<&Value>,
    session: Option<&str>,
    request: Option<&str>,
) -> Result<Fact> {
    let text = clean_text(text)?;
    let normalized = normalize(&text);
    let dir = scope_dir(home, app_id);
    let path = facts_path(home, app_id, workspace);
    let _lock = ScopeLock::acquire(&dir)?;
    let facts = load(&path)?;
    if let Some(existing) = facts
        .iter()
        .find(|fact| normalize(&fact.text) == normalized)
        .cloned()
    {
        return Ok(existing);
    }
    if facts.len() >= MAX_FACTS {
        return msg(format!(
            "memory is full ({MAX_FACTS} facts); forget something first"
        ));
    }
    let fact = Fact {
        id: new_id(now_secs(), &normalized),
        ts: now_secs(),
        text,
        keywords: clean_keywords(keywords),
        session: clean_field(session),
        request: clean_field(request),
    };
    fs::create_dir_all(&dir)?;
    let mut file = fs::OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(serde_json::to_string(&fact)?.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()?;
    audit(home, app_id, "memory_save", Some(&fact.id), &fact.text);
    Ok(fact)
}

fn tokens(query: &str) -> Vec<String> {
    let lower = query.to_lowercase();
    let mut out = lower
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| token.chars().count() >= 2)
        .map(|token| token.to_string())
        .collect::<Vec<_>>();
    // Keyword matching cannot understand pronouns; "кто я" must still find a
    // saved name/role fact.
    let about_user = [
        "кто я",
        "меня зовут",
        "обо мне",
        "про меня",
        "мой ",
        "моя ",
        "мое ",
        "мои ",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern) || lower.ends_with(pattern.trim()));
    if about_user {
        for extra in ["пользователь", "имя"] {
            if !out.iter().any(|token| token == extra) {
                out.push(extra.to_string());
            }
        }
    }
    out
}

fn score(fact: &Fact, query_tokens: &[String], now: u64) -> u32 {
    if query_tokens.is_empty() {
        return 0;
    }
    let text = fact.text.to_lowercase();
    let keywords = fact
        .keywords
        .iter()
        .map(|keyword| keyword.to_lowercase())
        .collect::<Vec<_>>();
    let mut score = 0u32;
    for token in query_tokens {
        if keywords.iter().any(|keyword| keyword.contains(token)) {
            score += 4;
        }
        if text.contains(token) {
            score += 1;
        }
    }
    if score > 0 {
        let age_days = now.saturating_sub(fact.ts) / 86_400;
        score = score.saturating_add((30u64.saturating_sub(age_days.min(30))) as u32);
    }
    score
}

pub fn search(
    home: &Path,
    app_id: &str,
    workspace: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<Fact>> {
    let facts = load(&facts_path(home, app_id, workspace))?;
    let query_tokens = tokens(query);
    let now = now_secs();
    let mut ranked = facts
        .into_iter()
        .map(|fact| (score(&fact, &query_tokens, now), fact.ts, fact))
        .filter(|(score, _, _)| query_tokens.is_empty() || *score > 0)
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    Ok(ranked
        .into_iter()
        .take(limit.max(1))
        .map(|(_, _, fact)| fact)
        .collect())
}

pub fn list(
    home: &Path,
    app_id: &str,
    workspace: &Path,
    query: &str,
    offset: usize,
    limit: usize,
) -> Result<(Vec<Fact>, usize)> {
    let mut facts = load(&facts_path(home, app_id, workspace))?;
    let needle = query.trim().to_lowercase();
    if !needle.is_empty() {
        facts.retain(|fact| {
            fact.text.to_lowercase().contains(&needle)
                || fact
                    .keywords
                    .iter()
                    .any(|keyword| keyword.to_lowercase().contains(&needle))
        });
    }
    facts.sort_by(|a, b| b.ts.cmp(&a.ts));
    let total = facts.len();
    let page = facts
        .into_iter()
        .skip(offset)
        .take(limit.clamp(1, 200))
        .collect();
    Ok((page, total))
}

pub fn forget(home: &Path, app_id: &str, workspace: &Path, id: &str) -> Result<bool> {
    let dir = scope_dir(home, app_id);
    let path = facts_path(home, app_id, workspace);
    let _lock = ScopeLock::acquire(&dir)?;
    let mut facts = load(&path)?;
    let before = facts.len();
    facts.retain(|fact| fact.id != id);
    if facts.len() == before {
        return Ok(false);
    }
    rewrite(&path, &facts)?;
    audit(home, app_id, "memory_forget", Some(id), "");
    Ok(true)
}

pub fn clear(home: &Path, app_id: &str, workspace: &Path) -> Result<usize> {
    let dir = scope_dir(home, app_id);
    let path = facts_path(home, app_id, workspace);
    let _lock = ScopeLock::acquire(&dir)?;
    let facts = load(&path)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    audit(home, app_id, "memory_clear", None, "");
    Ok(facts.len())
}

/// Questions that should still trigger recall even when keyword search finds
/// nothing (pronouns, identity, "what do you remember").
pub fn wants_recall(query: &str) -> bool {
    let lower = query.to_lowercase();
    [
        "кто", "имя", "зовут", "меня", "себя", "тво", "твой", "моя", "мой", "мои",
        "помнишь", "помни", "память",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
}

/// Compact block injected into the user turn so recall does not depend on the
/// model deciding to call a tool. The UI strips it from history.
pub fn context_block(facts: &[Fact]) -> Option<String> {
    if facts.is_empty() {
        return None;
    }
    let mut out = String::from(
        "[platform-memory: факты для этого приложения и рабочей папки. Учитывай их, если они относятся к вопросу; не пересказывай список без необходимости.",
    );
    for fact in facts.iter().take(5) {
        let text: String = fact.text.chars().take(300).collect();
        if out.chars().count() + text.chars().count() > 1500 {
            break;
        }
        out.push_str("\n- ");
        out.push_str(&text);
    }
    out.push(']');
    Some(out)
}

pub fn stats(home: &Path, app_id: &str, workspace: &Path) -> Result<Stats> {    let path = facts_path(home, app_id, workspace);
    let facts = load(&path)?;
    Ok(Stats {
        count: facts.len(),
        bytes: fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0),
        latest_ts: facts.iter().map(|fact| fact.ts).max(),
    })
}

fn audit(home: &Path, app_id: &str, action: &str, id: Option<&str>, text: &str) {
    crate::commands::audit(
        home,
        app_id,
        json!({
            "ts": now_secs(),
            "app_id": app_id,
            "source": "platform",
            "action": action,
            "id": id,
            "chars": text.chars().count()
        }),
    );
}

/// Dispatches one of the platform memory tools. Scope params never come from
/// the model: app id and workspace are bound by the gateway.
pub fn call(
    home: &Path,
    app_id: &str,
    workspace: &Path,
    name: &str,
    args: &Value,
) -> Result<Value> {
    match name {
        "memory_save" => {
            let text = args
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Msg("missing string argument text".to_string()))?;
            let fact = save(
                home,
                app_id,
                workspace,
                text,
                args.get("keywords"),
                None,
                None,
            )?;
            Ok(json!({ "fact": fact }))
        }
        "memory_search" => {
            let query = args.get("query").and_then(Value::as_str).unwrap_or("");
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(5) as usize;
            let limit = limit.clamp(1, 20);
            let mut facts = search(home, app_id, workspace, query, limit)?;
            let mut fallback = false;
            if facts.is_empty() && !query.trim().is_empty() {
                // Vague questions ("кто я", typos) must not look like an empty
                // memory: show recent facts so the model can answer.
                facts = list(home, app_id, workspace, "", 0, limit)?.0;
                fallback = !facts.is_empty();
            }
            Ok(json!({
                "facts": facts,
                "count": facts.len(),
                "fallback": fallback
            }))
        }
        "memory_recent" => {
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize;
            let facts = list(home, app_id, workspace, "", 0, limit.clamp(1, 50))?.0;
            Ok(json!({ "facts": facts, "count": facts.len() }))
        }
        "memory_forget" => {
            let id = args
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Msg("missing string argument id".to_string()))?;
            let removed = forget(home, app_id, workspace, id)?;
            Ok(json!({ "removed": removed }))
        }
        _ => msg(format!("unknown memory tool {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "eco-memory-{tag}-{}-{}",
            std::process::id(),
            now_secs()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn memory_is_scoped_deduplicated_and_paginated() {
        let home = temp("scope");
        let first_ws = home.join("ws-a");
        let second_ws = home.join("ws-b");
        fs::create_dir_all(&first_ws).unwrap();
        fs::create_dir_all(&second_ws).unwrap();

        let fact = save(
            &home,
            "app",
            &first_ws,
            "Любимый формат отчёта — PDF",
            Some(&json!(["формат", "pdf"])),
            None,
            None,
        )
        .unwrap();
        let duplicate = save(
            &home,
            "app",
            &first_ws,
            "  Любимый формат отчёта — PDF  ",
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(fact.id, duplicate.id);
        assert_eq!(stats(&home, "app", &first_ws).unwrap().count, 1);

        save(&home, "app", &first_ws, "Второй факт", None, None, None).unwrap();
        save(&home, "app", &first_ws, "Третий факт", None, None, None).unwrap();
        let (page, total) = list(&home, "app", &first_ws, "", 0, 2).unwrap();
        assert_eq!(total, 3);
        assert_eq!(page.len(), 2);
        let (tail, total) = list(&home, "app", &first_ws, "", 2, 2).unwrap();
        assert_eq!(total, 3);
        assert_eq!(tail.len(), 1);

        let found = search(&home, "app", &first_ws, "pdf", 5).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, fact.id);

        save(&home, "app", &second_ws, "Другой проект", None, None, None).unwrap();
        assert_eq!(stats(&home, "app", &first_ws).unwrap().count, 3);
        assert_eq!(stats(&home, "app", &second_ws).unwrap().count, 1);
        assert!(search(&home, "app", &first_ws, "проект", 5)
            .unwrap()
            .is_empty());

        assert!(forget(&home, "app", &first_ws, &fact.id).unwrap());
        assert_eq!(stats(&home, "app", &first_ws).unwrap().count, 2);
        assert_eq!(clear(&home, "app", &first_ws).unwrap(), 2);
        assert_eq!(stats(&home, "app", &first_ws).unwrap().count, 0);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn vague_user_questions_fall_back_to_recent_facts() {
        let home = temp("recall");
        let workspace = home.join("ws");
        fs::create_dir_all(&workspace).unwrap();
        let name = save(
            &home,
            "app",
            &workspace,
            "Пользователя зовут Денис",
            Some(&json!(["имя", "пользователь"])),
            None,
            None,
        )
        .unwrap();

        let found = search(&home, "app", &workspace, "кто я", 5).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, name.id);

        let result = call(
            &home,
            "app",
            &workspace,
            "memory_search",
            &json!({ "query": "к я кто", "limit": 5 }),
        )
        .unwrap();
        assert_eq!(result["fallback"], true);
        assert_eq!(result["facts"][0]["id"], name.id);
        let _ = fs::remove_dir_all(home);
    }
}
