use crate::error::{msg, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Secrets live encrypted with DPAPI (per-user scope) and never in the config
/// file. The blob is only readable by the same Windows user on the same machine.
pub struct SecretStore {
    dir: PathBuf,
}

fn validate(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return msg(format!("invalid secret name: {name}"));
    }
    Ok(())
}

impl SecretStore {
    pub fn new(home: &Path) -> Self {
        Self {
            dir: home.join("secrets"),
        }
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        validate(name)?;
        Ok(self.dir.join(format!("{name}.bin")))
    }

    pub fn store(&self, name: &str, value: &str) -> Result<()> {
        let blob = protect(value.as_bytes())?;
        fs::create_dir_all(&self.dir)?;
        fs::write(self.path(name)?, blob)?;
        Ok(())
    }

    pub fn load(&self, name: &str) -> Result<Option<String>> {
        let path = self.path(name)?;
        if !path.exists() {
            return Ok(None);
        }
        let blob = fs::read(path)?;
        let plain = unprotect(&blob)?;
        String::from_utf8(plain)
            .map(Some)
            .map_err(|_| crate::error::Error::Msg("secret is not valid utf-8".into()))
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        let path = self.path(name)?;
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Namespaced secret for one application. The app id is validated by the
    /// caller; dots in the resulting platform name are a deliberate namespace
    /// separator and never collide with a package-declared secret name.
    pub fn scoped_name(app_id: &str, name: &str) -> String {
        format!("app.{app_id}.{name}")
    }

    pub fn load_scoped(&self, app_id: &str, name: &str) -> Result<Option<String>> {
        self.load(&Self::scoped_name(app_id, name))
    }

    pub fn delete_scoped(&self, app_id: &str, name: &str) -> Result<()> {
        self.delete(&Self::scoped_name(app_id, name))
    }

    /// App-scoped value first, then the global secret. Used by package tools so
    /// one script can have different endpoints/credentials per application.
    pub fn load_for_app(&self, app_id: &str, name: &str) -> Result<Option<String>> {
        if let Some(value) = self.load_scoped(app_id, name)? {
            return Ok(Some(value));
        }
        self.load(name)
    }

    pub fn list(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Ok(out);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("bin") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                out.push(stem.to_string());
            }
        }
        out.sort();
        Ok(out)
    }
}

#[cfg(windows)]
fn protect(data: &[u8]) -> Result<Vec<u8>> {
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};

    unsafe {
        let mut input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        let ok = CryptProtectData(
            &mut input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            0,
            &mut output,
        );
        if ok == 0 {
            return msg("DPAPI CryptProtectData failed");
        }
        let blob = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as *mut core::ffi::c_void);
        Ok(blob)
    }
}

#[cfg(windows)]
fn unprotect(blob: &[u8]) -> Result<Vec<u8>> {
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    unsafe {
        let mut input = CRYPT_INTEGER_BLOB {
            cbData: blob.len() as u32,
            pbData: blob.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        let ok = CryptUnprotectData(
            &mut input,
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            0,
            &mut output,
        );
        if ok == 0 {
            return msg("DPAPI CryptUnprotectData failed");
        }
        let plain = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as *mut core::ffi::c_void);
        Ok(plain)
    }
}

#[cfg(not(windows))]
fn protect(data: &[u8]) -> Result<Vec<u8>> {
    Ok(data.to_vec())
}

#[cfg(not(windows))]
fn unprotect(blob: &[u8]) -> Result<Vec<u8>> {
    Ok(blob.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_scoped_secret_overrides_global_and_falls_back() {
        let home = std::env::temp_dir().join(format!(
            "eco-secrets-scope-{}-{}",
            std::process::id(),
            crate::artifacts::now_secs()
        ));
        let store = SecretStore::new(&home);
        store.store("svc_url", "https://global.example").unwrap();
        store
            .store(&SecretStore::scoped_name("doc-writer", "svc_url"), "https://app.example")
            .unwrap();

        assert_eq!(
            store.load_for_app("doc-writer", "svc_url").unwrap().as_deref(),
            Some("https://app.example")
        );
        assert_eq!(
            store.load_for_app("other-app", "svc_url").unwrap().as_deref(),
            Some("https://global.example")
        );

        store.delete_scoped("doc-writer", "svc_url").unwrap();
        assert_eq!(
            store.load_for_app("doc-writer", "svc_url").unwrap().as_deref(),
            Some("https://global.example")
        );
        let _ = fs::remove_dir_all(home);
    }
}
