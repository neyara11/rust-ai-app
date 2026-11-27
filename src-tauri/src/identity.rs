use crate::error::{msg, Error, Result};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Per-app capability token lifetime. The token is bound to one engine start
/// and one app/version; revocation or expiry makes the tool gateway fail closed.
pub const DEFAULT_TTL_SECS: u64 = 12 * 60 * 60;

/// Expired or revoked token records are kept for this long for audit correlation.
const KEEP_SECS: u64 = 7 * 24 * 60 * 60;
const MAX_RECORDS: usize = 200;
/// A crashed process may leave the lock file behind; it must only be stolen
/// after a period longer than the longest legitimate hold below.
const LOCK_TIMEOUT: Duration = Duration::from_secs(25);
const LOCK_STALE: Duration = Duration::from_secs(30);

fn now_secs() -> u64 {
    crate::artifacts::now_secs()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecord {
    pub token_id: String,
    pub app_id: String,
    pub app_version: String,
    pub issued_at: u64,
    pub expires_at: u64,
    #[serde(default)]
    pub revoked_at: Option<u64>,
    /// sha256 of the raw token. The raw token is never written to disk.
    pub token_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicToken {
    pub token_id: String,
    pub app_id: String,
    pub app_version: String,
    pub issued_at: u64,
    pub expires_at: u64,
    pub revoked_at: Option<u64>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssuedToken {
    pub token: String,
    pub token_id: String,
    pub app_id: String,
    pub app_version: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    tokens: Vec<TokenRecord>,
    /// App ids revoked by an administrator. Durable until `unrevoke_app`:
    /// restarting the app must not issue a fresh token.
    #[serde(default)]
    revoked_apps: BTreeMap<String, u64>,
}

fn dir(home: &Path) -> PathBuf {
    home.join("identity")
}

fn path(home: &Path) -> PathBuf {
    dir(home).join("tokens.json")
}

fn salt_path(home: &Path) -> PathBuf {
    dir(home).join("user.salt")
}

/// Exclusive lock around token-store mutations. Works across processes: engine
/// starts and CLI issue/revoke can run concurrently without losing updates.
struct StoreLock {
    path: PathBuf,
}

impl StoreLock {
    fn acquire(home: &Path) -> Result<Self> {
        let dir = dir(home);
        retry_permission(|| std::fs::create_dir_all(&dir))?;
        let path = dir.join("tokens.lock");
        let started = Instant::now();
        loop {
            match std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
            {
                Ok(mut file) => {
                    let _ = writeln!(file, "{}", std::process::id());
                    return Ok(Self { path });
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::AlreadyExists
                        || e.kind() == std::io::ErrorKind::PermissionDenied =>
                {
                    if stale(&path) {
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    if started.elapsed() > LOCK_TIMEOUT {
                        return msg("token store is busy: another process holds the lock");
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => return Err(Error::Io(e)),
            }
        }
    }
}

fn stale(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .map(|age| age > LOCK_STALE)
        .unwrap_or(false)
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Antivirus scanners briefly hold freshly written files on Windows. Retry the
/// permission-denied window instead of failing a token issue under load.
fn retry_permission<T>(
    mut operation: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let mut attempts = 0;
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error)
                if attempts < 100 && error.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error),
        }
    }
}

/// A corrupt or unreadable store is an error, not an empty store: silently
/// dropping records would resurrect revoked tokens.
fn load(home: &Path) -> Result<Store> {
    let path = path(home);
    match retry_permission(|| std::fs::read_to_string(&path)) {
        Ok(raw) => serde_json::from_str(&raw)
            .map_err(|e| Error::Msg(format!("token store is corrupt: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(e) => Err(Error::Io(e)),
    }
}

fn save(home: &Path, store: &Store) -> Result<()> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = dir(home);
    retry_permission(|| std::fs::create_dir_all(&dir))?;
    let raw = serde_json::to_string_pretty(store)?;
    let tmp = dir.join(format!(
        "tokens.{}.{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    retry_permission(|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(raw.as_bytes())?;
        file.sync_all()
    })?;
    match retry_permission(|| std::fs::rename(&tmp, path(home))) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(Error::Io(e))
        }
    }
}

fn hash_token(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

fn is_active(record: &TokenRecord, now: u64) -> bool {
    record.revoked_at.is_none() && record.expires_at > now
}

/// Drops old records but never evicts an active token, so a busy machine
/// cannot invalidate a running engine.
fn prune(store: &mut Store, now: u64) {
    store.tokens.retain(|record| {
        let ended = record.revoked_at.unwrap_or(record.expires_at);
        ended.saturating_add(KEEP_SECS) > now
    });
    let overflow = store.tokens.len().saturating_sub(MAX_RECORDS);
    if overflow > 0 {
        let mut removed = 0;
        store.tokens.retain(|record| {
            if removed < overflow && !is_active(record, now) {
                removed += 1;
                false
            } else {
                true
            }
        });
    }
}

fn audit(home: &Path, app_id: &str, entry: serde_json::Value) {
    crate::commands::audit(home, app_id, entry);
}

/// Issues a short-lived token for one app/version. Only the hash is persisted;
/// the raw token is returned once to the caller (engine env / CLI output).
pub fn issue(
    home: &Path,
    app_id: &str,
    app_version: &str,
    ttl_secs: u64,
    source: &str,
) -> Result<IssuedToken> {
    crate::manifest::parse_id(app_id, "app")?;
    let now = now_secs();
    let ttl = ttl_secs.max(60);
    let expires_at = now.saturating_add(ttl);

    let _lock = StoreLock::acquire(home)?;
    let mut store = load(home)?;
    if store.revoked_apps.contains_key(app_id) {
        return msg(format!(
            "app {app_id} is revoked: run --unrevoke {app_id} after review"
        ));
    }

    let secret = random_bytes(32)?;
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&secret);
    let token_sha256 = hash_token(&token);
    let token_id = format!("tok_{}", &token_sha256[..16]);

    let record = TokenRecord {
        token_id: token_id.clone(),
        app_id: app_id.to_string(),
        app_version: app_version.to_string(),
        issued_at: now,
        expires_at,
        revoked_at: None,
        token_sha256,
    };
    store.tokens.push(record);
    prune(&mut store, now);
    save(home, &store)?;

    audit(
        home,
        app_id,
        serde_json::json!({
            "ts": now,
            "app_id": app_id,
            "app_version": app_version,
            "source": source,
            "action": "token_issue",
            "token_id": token_id,
            "expires_at": expires_at
        }),
    );

    Ok(IssuedToken {
        token,
        token_id,
        app_id: app_id.to_string(),
        app_version: app_version.to_string(),
        issued_at: now,
        expires_at,
    })
}

/// Validates a raw token against the local store: must be known, its app must
/// not be revoked, and the token itself must not be revoked or expired.
pub fn validate(home: &Path, raw: &str) -> Result<TokenRecord> {
    let raw = raw.trim();
    if raw.is_empty() {
        return msg("agent token is empty");
    }
    let hash = hash_token(raw);
    let now = now_secs();
    let store = load(home)?;
    let record = store
        .tokens
        .into_iter()
        .find(|record| record.token_sha256 == hash)
        .ok_or_else(|| Error::Msg("agent token is not recognized".to_string()))?;
    if store.revoked_apps.contains_key(&record.app_id) {
        return msg(format!("app {} is revoked", record.app_id));
    }
    if record.revoked_at.is_some() {
        return msg(format!("agent token {} is revoked", record.token_id));
    }
    if record.expires_at <= now {
        return msg(format!("agent token {} has expired", record.token_id));
    }
    Ok(record)
}

/// Renews an expired but otherwise valid token. Expiry alone must not brick a
/// long-running engine (the token is issued once at engine start); a revoked
/// token or app still fails closed. Returns `None` while the token is valid.
pub fn renew_if_expired(home: &Path, raw: &str) -> Result<Option<IssuedToken>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return msg("agent token is empty");
    }
    let hash = hash_token(raw);
    let now = now_secs();
    let store = load(home)?;
    let record = store
        .tokens
        .iter()
        .find(|record| record.token_sha256 == hash)
        .ok_or_else(|| Error::Msg("agent token is not recognized".to_string()))?;
    if store.revoked_apps.contains_key(&record.app_id) {
        return msg(format!("app {} is revoked", record.app_id));
    }
    if record.revoked_at.is_some() {
        return msg(format!("agent token {} is revoked", record.token_id));
    }
    if record.expires_at > now {
        return Ok(None);
    }
    issue(
        home,
        &record.app_id,
        &record.app_version,
        DEFAULT_TTL_SECS,
        "renewal",
    )
    .map(Some)
}

/// Revokes one specific token. Returns the records that changed; an unknown or
/// already revoked token yields an empty list.
pub fn revoke_token(home: &Path, token_id: &str) -> Result<Vec<TokenRecord>> {
    let token_id = token_id.trim();
    if token_id.is_empty() {
        return msg("token id is required");
    }
    let now = now_secs();
    let _lock = StoreLock::acquire(home)?;
    let mut store = load(home)?;
    let mut changed = Vec::new();
    for record in store.tokens.iter_mut() {
        if record.revoked_at.is_some() || record.token_id != token_id {
            continue;
        }
        record.revoked_at = Some(now);
        changed.push(record.clone());
    }
    if !changed.is_empty() {
        save(home, &store)?;
        for record in &changed {
            audit_token_revoke(home, record, now);
        }
    }
    Ok(changed)
}

/// Revokes an app durably and all of its active tokens. A marker is kept so a
/// restart cannot mint a new token until `unrevoke_app`.
pub fn revoke_app(home: &Path, app_id: &str) -> Result<Vec<TokenRecord>> {
    crate::manifest::parse_id(app_id, "app")?;
    let now = now_secs();
    let _lock = StoreLock::acquire(home)?;
    let mut store = load(home)?;
    store.revoked_apps.insert(app_id.to_string(), now);
    let mut changed = Vec::new();
    for record in store.tokens.iter_mut() {
        if record.app_id != app_id || record.revoked_at.is_some() {
            continue;
        }
        record.revoked_at = Some(now);
        changed.push(record.clone());
    }
    save(home, &store)?;
    audit(
        home,
        app_id,
        serde_json::json!({
            "ts": now,
            "app_id": app_id,
            "source": "admin",
            "action": "app_revoke",
            "tokens_revoked": changed.len()
        }),
    );
    for record in &changed {
        audit_token_revoke(home, record, now);
    }
    Ok(changed)
}

fn audit_token_revoke(home: &Path, record: &TokenRecord, now: u64) {
    audit(
        home,
        &record.app_id,
        serde_json::json!({
            "ts": now,
            "app_id": record.app_id,
            "app_version": record.app_version,
            "source": "admin",
            "action": "token_revoke",
            "token_id": record.token_id,
            "expires_at": record.expires_at
        }),
    );
}

/// Removes the durable app-level revocation marker. Returns false when the app
/// was not revoked.
pub fn unrevoke_app(home: &Path, app_id: &str) -> Result<bool> {
    crate::manifest::parse_id(app_id, "app")?;
    let _lock = StoreLock::acquire(home)?;
    let mut store = load(home)?;
    let removed = store.revoked_apps.remove(app_id).is_some();
    if removed {
        save(home, &store)?;
        audit(
            home,
            app_id,
            serde_json::json!({
                "ts": now_secs(),
                "app_id": app_id,
                "source": "admin",
                "action": "app_unrevoke"
            }),
        );
    }
    Ok(removed)
}

/// Lists tokens without hashes or raw values.
pub fn list(home: &Path, app: Option<&str>) -> Result<Vec<PublicToken>> {
    let now = now_secs();
    Ok(load(home)?
        .tokens
        .into_iter()
        .filter(|record| app.map(|id| record.app_id == id).unwrap_or(true))
        .map(|record| PublicToken {
            active: is_active(&record, now),
            token_id: record.token_id,
            app_id: record.app_id,
            app_version: record.app_version,
            issued_at: record.issued_at,
            expires_at: record.expires_at,
            revoked_at: record.revoked_at,
        })
        .collect())
}

/// App ids that are durably revoked.
pub fn revoked_apps(home: &Path) -> Result<Vec<String>> {
    Ok(load(home)?.revoked_apps.into_keys().collect())
}

/// Housekeeping on startup and on issuance: drop records that are long dead.
pub fn sweep(home: &Path) {
    let Ok(_lock) = StoreLock::acquire(home) else {
        return;
    };
    let Ok(mut store) = load(home) else {
        return;
    };
    let before = store.tokens.len();
    prune(&mut store, now_secs());
    if store.tokens.len() != before {
        let _ = save(home, &store);
    }
}

/// Stable pseudonym for `X-User`: never the raw Windows account name.
pub fn user_pseudonym(home: &Path) -> String {
    let salt = user_salt(home);
    let user = std::env::var("USERNAME").unwrap_or_default();
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    let raw = format!("{domain}\\{user}");
    let digest = Sha256::digest(format!("{salt}:{raw}").as_bytes());
    format!("u_{}", &hex::encode(digest)[..16])
}

fn user_salt(home: &Path) -> String {
    let path = salt_path(home);
    if let Ok(raw) = std::fs::read_to_string(&path) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let salt = random_bytes(16)
        .map(|bytes| hex::encode(bytes))
        .unwrap_or_else(|_| format!("{:x}", now_secs()));
    if std::fs::create_dir_all(dir(home)).is_ok() {
        let _ = std::fs::write(path, &salt);
    }
    salt
}

/// Fresh correlation id for one agent turn or tool call.
pub fn new_request_id() -> String {
    match random_bytes(8) {
        Ok(bytes) => format!("req_{}", hex::encode(bytes)),
        Err(_) => format!("req_{:x}", now_secs()),
    }
}

#[cfg(windows)]
fn random_bytes(len: usize) -> Result<Vec<u8>> {
    use windows_sys::Win32::Security::Cryptography::{
        BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
    };
    let mut buf = vec![0u8; len];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            len as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return msg(format!("BCryptGenRandom failed: {status}"));
    }
    Ok(buf)
}

#[cfg(not(windows))]
fn random_bytes(len: usize) -> Result<Vec<u8>> {
    let mut seed = format!(
        "{}-{}-{}",
        now_secs(),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        seed = hex::encode(Sha256::digest(seed.as_bytes()));
        if let Ok(bytes) = hex::decode(&seed) {
            out.extend_from_slice(&bytes);
        }
    }
    out.truncate(len);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp(name: &str) -> PathBuf {
        let nonce = random_bytes(4).map(|b| hex::encode(b)).unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "eco-identity-{name}-{}-{nonce}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn issues_validates_and_revokes() {
        let home = temp("revoke");
        let issued = issue(&home, "demo", "1.0.0", 3600, "test").unwrap();
        let record = validate(&home, &issued.token).unwrap();
        assert_eq!(record.app_id, "demo");
        assert_eq!(record.app_version, "1.0.0");
        assert_eq!(record.token_id, issued.token_id);

        let changed = revoke_token(&home, &issued.token_id).unwrap();
        assert_eq!(changed.len(), 1);
        assert!(validate(&home, &issued.token).is_err());

        let listed = list(&home, Some("demo")).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].active);
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn rejects_unknown_and_expired_tokens() {
        let home = temp("expired");
        assert!(validate(&home, "not-a-token").is_err());

        let issued = issue(&home, "demo", "1.0.0", 3600, "test").unwrap();
        let mut store = load(&home).unwrap();
        store.tokens[0].expires_at = now_secs().saturating_sub(1);
        save(&home, &store).unwrap();
        let err = validate(&home, &issued.token).unwrap_err().to_string();
        assert!(err.contains("expired"), "{err}");
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn app_revocation_is_durable_and_scoped() {
        let home = temp("scoped");
        let first = issue(&home, "one", "1.0.0", 3600, "test").unwrap();
        let second = issue(&home, "two", "1.0.0", 3600, "test").unwrap();
        revoke_app(&home, "one").unwrap();
        assert!(validate(&home, &first.token).is_err());
        assert!(validate(&home, &second.token).is_ok());
        assert!(issue(&home, "one", "1.0.1", 3600, "test").is_err());
        assert_eq!(revoked_apps(&home).unwrap(), vec!["one".to_string()]);
        assert!(unrevoke_app(&home, "one").unwrap());
        assert!(!unrevoke_app(&home, "one").unwrap());
        assert!(issue(&home, "one", "1.0.1", 3600, "test").is_ok());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn corrupt_store_fails_closed() {
        let home = temp("corrupt");
        fs::create_dir_all(dir(&home)).unwrap();
        fs::write(path(&home), b"{not json").unwrap();
        assert!(validate(&home, "anything").is_err());
        assert!(issue(&home, "demo", "1.0.0", 3600, "test").is_err());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn concurrent_issues_do_not_lose_tokens() {
        let home = temp("concurrent");
        let mut handles = Vec::new();
        for n in 0..8 {
            let home = home.clone();
            handles.push(std::thread::spawn(move || {
                let mut tokens = Vec::new();
                for step in 0..5 {
                    let issued = issue(
                        &home,
                        "demo",
                        "1.0.0",
                        3600,
                        &format!("thread-{n}-{step}"),
                    )
                    .unwrap();
                    tokens.push(issued.token);
                }
                tokens
            }));
        }
        let mut tokens = Vec::new();
        for handle in handles {
            tokens.extend(handle.join().unwrap());
        }
        assert_eq!(tokens.len(), 40);
        for token in tokens {
            validate(&home, &token).unwrap();
        }
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn expired_but_unrevoked_token_is_renewed() {
        let home = temp("renew");
        let issued = issue(&home, "demo", "0.1.0", 120, "test").unwrap();
        let mut store = load(&home).unwrap();
        for record in &mut store.tokens {
            if record.token_id == issued.token_id {
                record.expires_at = now_secs().saturating_sub(1);
            }
        }
        save(&home, &store).unwrap();
        assert!(validate(&home, &issued.token).is_err());

        let renewed = renew_if_expired(&home, &issued.token)
            .unwrap()
            .expect("expired token must be renewed");
        assert_ne!(renewed.token, issued.token);
        validate(&home, &renewed.token).unwrap();

        // Revocation is not bypassed by renewal.
        revoke_token(&home, &renewed.token_id).unwrap();
        assert!(renew_if_expired(&home, &renewed.token).is_err());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn pseudonym_is_stable_and_not_the_account_name() {
        let home = temp("pseudonym");
        let first = user_pseudonym(&home);
        let second = user_pseudonym(&home);
        assert_eq!(first, second);
        assert!(first.starts_with("u_"));
        let user = std::env::var("USERNAME").unwrap_or_default();
        if !user.is_empty() {
            assert_ne!(first, user);
            assert_ne!(first.to_lowercase(), user.to_lowercase());
        }
        let _ = fs::remove_dir_all(home);
    }
}
