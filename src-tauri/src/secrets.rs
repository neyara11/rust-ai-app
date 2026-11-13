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
