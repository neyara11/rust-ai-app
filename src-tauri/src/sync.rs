//! Cross-process synchronization shared by the platform and its `--mcp-proxy`
//! children. Journals and limit counters are written by both, so a per-process
//! mutex is not enough.

use std::path::Path;

/// Windows named-mutex guard. Dropping it releases the mutex.
#[cfg(windows)]
pub struct NamedLock(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for NamedLock {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::ReleaseMutex;
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

/// Acquires a mutex named after `scope` and the canonical `key` path. The
/// platform and its engine children run in the same session, so a `Local\`
/// name is enough. Returns `None` when the mutex cannot be created or
/// acquired; callers keep working unsequenced instead of failing the operation.
#[cfg(windows)]
pub fn named_lock(scope: &str, key: &Path) -> Option<NamedLock> {
    use std::hash::{Hash, Hasher};
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{CreateMutexW, WaitForSingleObject, INFINITE};

    // The proxy may see a differently normalized path, so key by the
    // canonical, case-folded path.
    let canonical = std::fs::canonicalize(key).unwrap_or_else(|_| key.to_path_buf());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical
        .to_string_lossy()
        .to_ascii_lowercase()
        .hash(&mut hasher);
    let name = format!("Local\\EcoAIOps.{scope}.{:016x}", hasher.finish());
    let wide: Vec<u16> = std::ffi::OsStr::new(&name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, wide.as_ptr());
        if handle.is_null() {
            return None;
        }
        let waited = WaitForSingleObject(handle, INFINITE);
        // WAIT_ABANDONED means the previous owner died while holding the lock;
        // the data is still usable, so take ownership and continue.
        if waited != WAIT_OBJECT_0 && waited != WAIT_ABANDONED {
            CloseHandle(handle);
            return None;
        }
        Some(NamedLock(handle))
    }
}

#[cfg(not(windows))]
pub struct NamedLock;

#[cfg(not(windows))]
pub fn named_lock(_scope: &str, _key: &Path) -> Option<NamedLock> {
    None
}
