//! Opening workspace artifacts from the chat.
//!
//! The files the user clicks are authored by the model, so a click must never
//! turn into code execution: only documents, images and plain text reach the
//! shell handler, and every path goes through the same containment rules as the
//! agent's file tools.

use crate::error::{msg, Result};
use crate::proxy;
use std::path::{Path, PathBuf};

/// Extensions the shell may hand to their default handler. Legacy binary Office
/// formats and RTF stay out: they carry macro/embedded-object payloads and the
/// platform never produces them.
const OPENABLE: &[&str] = &[
    "md", "markdown", "txt", "log", "csv", "tsv", "json", "jsonl", "yaml", "yml", "xml", "css",
    "pdf", "docx", "xlsx", "pptx", "odt", "ods", "png", "jpg", "jpeg", "gif", "webp", "bmp",
    "drawio",
];

/// Extensions that never reach the shell, even when a handler is registered.
/// HTML/SVG are included because a model-authored document must not be able to
/// execute script in the browser the platform opens for it; `doc`/`xls`/`rtf`
/// are legacy container formats with macro/object payloads.
const BLOCKED: &[&str] = &[
    "exe", "bat", "cmd", "com", "scr", "pif", "msi", "msp", "dll", "cpl", "jar", "js", "jse",
    "mjs", "cjs", "vbs", "vbe", "wsf", "wsh", "hta", "lnk", "reg", "ps1", "psm1", "py", "sh",
    "url", "scf", "inf", "html", "htm", "svg", "doc", "xls", "rtf",
];

/// Whether a click on this file may be handed to the Windows shell.
pub fn openable(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    let ext = ext.to_ascii_lowercase();
    OPENABLE.contains(&ext.as_str()) && !BLOCKED.contains(&ext.as_str())
}

/// Resolves a chat-supplied path to an existing file inside the workspace.
pub fn resolve(workspace: &Path, raw: &str) -> Result<PathBuf> {
    let path = proxy::resolve_workspace_path(workspace, raw).map_err(|error| match error {
        // The OS message for a missing file is localised and cryptic in a toast.
        crate::error::Error::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
            crate::error::Error::Msg(format!("файл не найден в рабочей папке: {raw}"))
        }
        other => other,
    })?;
    if !path.is_file() {
        return msg("path is not a file");
    }
    Ok(path)
}

/// Only these schemes reach the shell. The text is model-authored, so `file:`,
/// `javascript:`, `data:` and custom protocol handlers must stay out of reach.
fn url_allowed(raw: &str) -> Result<()> {
    let url = raw.trim();
    if url.is_empty() || url.len() > 2048 {
        return msg("link is empty or too long");
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return msg("link contains whitespace or control characters");
    }
    let lower = url.to_ascii_lowercase();
    let allowed = lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:");
    if !allowed {
        return msg("only http(s) and mailto links are opened");
    }
    Ok(())
}

/// Opens a chat link in the user's default browser or mail client.
pub fn open_url(raw: &str) -> Result<()> {
    url_allowed(raw)?;
    shell_open(std::ffi::OsStr::new(raw.trim()))
}

#[cfg(windows)]
fn shell_open(target: &std::ffi::OsStr) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide: Vec<u16> = target.encode_wide().chain(std::iter::once(0)).collect();
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value above 32 on success.
    if result as isize <= 32 {
        return msg(format!(
            "the shell could not open {}",
            target.to_string_lossy()
        ));
    }
    Ok(())
}

#[cfg(windows)]
pub fn open_path(path: &Path) -> Result<()> {
    shell_open(path.as_os_str())
}

#[cfg(not(windows))]
pub fn open_path(path: &Path) -> Result<()> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| msg(format!("could not open this file: {e}")))?;
    Ok(())
}

#[cfg(not(windows))]
fn shell_open(target: &std::ffi::OsStr) -> Result<()> {
    std::process::Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map_err(|e| msg(format!("could not open this target: {e}")))?;
    Ok(())
}

/// Reveals the file in Explorer with it selected.
#[cfg(windows)]
pub fn reveal_path(path: &Path) -> Result<()> {
    let mut arg = std::ffi::OsString::from("/select,");
    arg.push(path.as_os_str());
    std::process::Command::new("explorer.exe")
        .arg(arg)
        .spawn()?;
    Ok(())
}

#[cfg(not(windows))]
pub fn reveal_path(path: &Path) -> Result<()> {
    let parent = path.parent().unwrap_or(path);
    open_path(parent)
}

/// Copies an artifact out of the workspace to a destination the user picks.
pub fn save_copy(source: &Path) -> Result<Option<PathBuf>> {
    let name = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("artifact");
    let Some(target) = rfd::FileDialog::new().set_file_name(name).save_file() else {
        return Ok(None);
    };
    if target == source {
        return msg("source and destination are the same file");
    }
    std::fs::copy(source, &target)?;
    Ok(Some(target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "eco-workspace-{name}-{}-{}",
            std::process::id(),
            crate::artifacts::now_secs()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn only_web_and_mail_links_are_allowed() {
        assert!(url_allowed("https://example.com/docs?id=1").is_ok());
        assert!(url_allowed("http://localhost:8080/health").is_ok());
        assert!(url_allowed("mailto:ops@example.com").is_ok());
        assert!(url_allowed(" file:///C:/Windows/System32/calc.exe ").is_err());
        assert!(url_allowed("javascript:alert(1)").is_err());
        assert!(url_allowed("data:text/html,<script>x</script>").is_err());
        assert!(url_allowed("ms-msdt:/id PCWDiagnostic").is_err());
        assert!(url_allowed("https://example.com/a b").is_err());
        assert!(url_allowed("").is_err());
    }

    #[test]
    fn cyrillic_workspace_paths_resolve() {
        let root = temp_dir("кириллица");
        fs::write(root.join("отчёт.md"), "x").unwrap();

        let resolved = resolve(&root, "отчёт.md").unwrap();
        assert!(resolved.ends_with("отчёт.md"));

        let missing = resolve(&root, ".docx").unwrap_err().to_string();
        assert!(missing.contains("не найден"), "unexpected error: {missing}");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn accepts_documents_and_rejects_executables() {
        assert!(openable(Path::new("report.md")));
        assert!(openable(Path::new("out/report.DOCX")));
        assert!(openable(Path::new("chart.png")));
        assert!(!openable(Path::new("run.ps1")));
        assert!(!openable(Path::new("setup.exe")));
        assert!(!openable(Path::new("payload.js")));
        assert!(!openable(Path::new("page.html")));
        assert!(!openable(Path::new("icon.svg")));
        assert!(!openable(Path::new("legacy.doc")));
        assert!(!openable(Path::new("legacy.xls")));
        assert!(!openable(Path::new("note.rtf")));
        assert!(!openable(Path::new("shortcut.lnk")));
        assert!(!openable(Path::new("no-extension")));
    }

    #[test]
    fn resolves_files_inside_the_workspace() {
        let root = temp_dir("inside");
        let nested = root.join("out");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("report.md"), "x").unwrap();

        let resolved = resolve(&root, "out/report.md").unwrap();
        assert!(resolved.ends_with("report.md"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_escapes_and_directories() {
        let root = temp_dir("escape");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("file.md"), "x").unwrap();
        let outside = root.join("..").join("outside.md");
        fs::write(&outside, "x").unwrap();

        assert!(resolve(&root, "../outside.md").is_err());
        assert!(resolve(&root, "sub").is_err());
        assert!(resolve(&root, "missing.md").is_err());
        let absolute = root.join("file.md").display().to_string();
        assert!(resolve(&root, &absolute).is_err());

        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&root);
    }
}
