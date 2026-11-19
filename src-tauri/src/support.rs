//! Windows startup guards that must run before Tauri creates the WebView2
//! window: a readable WebView2 preflight, a single-instance mutex for the
//! profile, and a file log usable from a `windows_subsystem` binary.

use std::path::{Path, PathBuf};

#[cfg(windows)]
use windows_sys::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
};

#[cfg(windows)]
const WEBVIEW2_CLIENT: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

/// Appends one line to `<home>\logs\platform.log` and mirrors it to stderr.
/// Best effort: never fails startup because the log is unavailable.
pub fn log_line(home: &Path, line: &str) {
    use std::io::Write;
    eprintln!("{line}");
    let dir = home.join("logs");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("platform.log"))
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0);
        let _ = writeln!(file, "{secs} {line}");
    }
}

pub fn show_error(caption: &str, text: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let caption = wide(caption);
        let text = wide(text);
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("{caption}: {text}");
    }
}

pub fn show_webview2_error() {
    show_error(
        "ECO AIOps",
        "Не найден Microsoft Edge WebView2 Runtime.\n\n\
         Установите Evergreen Standalone Installer (x64) от имени администратора \
         (он ставится per-machine и виден всем пользователям) и запустите ECO AIOps снова.\n\n\
         https://developer.microsoft.com/microsoft-edge/webview2/",
    );
}

/// Checks the runtime the WebView2 loader will use. The loader fails with
/// "Could not find the WebView2 Runtime" before the app can show its own UI, so
/// this preflight runs first and gives an actionable message.
pub fn webview2_available() -> bool {
    #[cfg(windows)]
    {
        // Fixed-version deployments point the loader at an unpacked runtime.
        // Do not report it missing when that override is valid.
        if let Ok(folder) = std::env::var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER") {
            if !folder.is_empty() && Path::new(&folder).join("msedgewebview2.exe").is_file() {
                return true;
            }
        }
        // The runtime is registered under both registry views, depending on
        // whether it was installed per-machine (elevated) or per-user.
        let machine64 = format!(r"SOFTWARE\{WEBVIEW2_CLIENT}");
        let machine32 = format!(r"SOFTWARE\WOW6432Node\{WEBVIEW2_CLIENT}");
        let user64 = format!(r"Software\{WEBVIEW2_CLIENT}");
        let user32 = format!(r"Software\WOW6432Node\{WEBVIEW2_CLIENT}");
        let versions = [
            registry_version(HKEY_LOCAL_MACHINE, &machine32),
            registry_version(HKEY_LOCAL_MACHINE, &machine64),
            registry_version(HKEY_CURRENT_USER, &user64),
            registry_version(HKEY_CURRENT_USER, &user32),
        ];
        let mut registered = false;
        for version in versions.into_iter().flatten() {
            let version = version.trim().to_string();
            if version.is_empty() || version == "0.0.0.0" {
                continue;
            }
            registered = true;
            if runtime_files_exist(&version) {
                return true;
            }
        }
        if registered {
            // Registered but the files are gone: an interrupted uninstall or
            // reinstall. The loader will fail just like a missing runtime.
            return false;
        }
        folder_has_runtime()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// One instance per profile home. A named mutex is not enough here: `Local\`
/// namespaces are per logon session, so console + RDP sessions of the same user
/// would both pass, and a mutex created by a higher-integrity instance can deny
/// access. An exclusive lock file is cross-session and matches the resource we
/// actually protect (`engine.pid` and the engine home).
pub fn single_instance(home: &Path) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{
            GetLastError, ERROR_SHARING_VIOLATION, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
        };
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_ALWAYS,
        };

        let path = wide(&home.join(".platform.lock").to_string_lossy());
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            // A sharing violation means another process holds the lock. Other
            // errors (for example an inaccessible home) should not brick
            // startup; the app will surface the real error when it uses the home.
            return unsafe { GetLastError() } != ERROR_SHARING_VIOLATION;
        }
        // Keep the handle open for the process lifetime; Windows releases the
        // lock when the process exits. (`let _` intentionally does not close.)
        let _ = handle;
        true
    }
    #[cfg(not(windows))]
    {
        let _ = home;
        true
    }
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn registry_version(root: HKEY, subkey: &str) -> Option<String> {
    let subkey = wide(subkey);
    let value = wide("pv");
    let mut buffer = [0u16; 256];
    let mut size = (buffer.len() * std::mem::size_of::<u16>()) as u32;
    let mut kind = 0u32;
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            &mut kind,
            buffer.as_mut_ptr() as *mut _,
            &mut size,
        )
    };
    if status != 0 || size < 2 {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1).min(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..len]))
}

#[cfg(windows)]
fn webview2_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(value) = std::env::var("ProgramFiles(x86)") {
        roots.push(PathBuf::from(value).join(r"Microsoft\EdgeWebView\Application"));
    }
    if let Ok(value) = std::env::var("ProgramFiles") {
        let path = PathBuf::from(value).join(r"Microsoft\EdgeWebView\Application");
        if !roots.contains(&path) {
            roots.push(path);
        }
    }
    if let Ok(value) = std::env::var("LOCALAPPDATA") {
        roots.push(PathBuf::from(value).join(r"Microsoft\EdgeWebView\Application"));
    }
    roots
}

#[cfg(windows)]
fn runtime_files_exist(version: &str) -> bool {
    webview2_roots()
        .iter()
        .any(|root| root.join(version).join("msedgewebview2.exe").is_file())
}

#[cfg(windows)]
fn folder_has_runtime() -> bool {
    webview2_roots().iter().any(|root| {
        std::fs::read_dir(root)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|entry| entry.path().join("msedgewebview2.exe").is_file())
            })
            .unwrap_or(false)
    })
}
