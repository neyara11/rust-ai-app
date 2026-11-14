use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde_json::Value;
use std::fs;
use std::path::Path;

use crate::core::{self, Result};

fn read_seed(path: &Path) -> Result<[u8; 32]> {
    let raw = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = hex::decode(raw.trim()).map_err(|e| format!("{}: {e}", path.display()))?;
    bytes
        .try_into()
        .map_err(|_| format!("{}: private key must be 32 bytes", path.display()))
}

fn read_public(path: &Path) -> Result<[u8; 32]> {
    let raw = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = hex::decode(raw.trim()).map_err(|e| format!("{}: {e}", path.display()))?;
    bytes
        .try_into()
        .map_err(|_| format!("{}: public key must be 32 bytes", path.display()))
}

fn write_hex(path: &Path, value: &str) -> Result<()> {
    fs::write(path, value).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn keygen(out_dir: &Path) -> Result<(String, String)> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| e.to_string())?;
    let signing = SigningKey::from_bytes(&seed);
    let public = hex::encode(signing.verifying_key().to_bytes());
    let private = hex::encode(seed);
    write_hex(&out_dir.join("dev.key"), &private)?;
    write_hex(&out_dir.join("trusted.pub"), &public)?;
    Ok((out_dir.join("dev.key").display().to_string(), public))
}

pub fn sign(dir: &Path, key: &Path) -> Result<()> {
    let seed = read_seed(key)?;
    let signing = SigningKey::from_bytes(&seed);
    let bytes = fs::read(core::manifest_path(dir))
        .map_err(|e| format!("manifest.json unreadable: {e}"))?;
    let signature = signing.sign(&bytes);
    write_hex(
        &dir.join("manifest.sig"),
        &hex::encode(signature.to_bytes()),
    )
}

pub fn verify(dir: &Path, public: &Path) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let manifest_bytes = match fs::read(core::manifest_path(dir)) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(vec![format!("manifest.json unreadable: {e}")]),
    };
    let manifest: Value = match serde_json::from_slice(&manifest_bytes) {
        Ok(value) => value,
        Err(e) => return Ok(vec![format!("manifest.json invalid: {e}")]),
    };

    let key_bytes = match read_public(public) {
        Ok(bytes) => bytes,
        Err(e) => return Ok(vec![e]),
    };
    let key = match VerifyingKey::from_bytes(&key_bytes) {
        Ok(key) => key,
        Err(e) => return Ok(vec![format!("invalid public key: {e}")]),
    };

    match fs::read_to_string(dir.join("manifest.sig")) {
        Ok(raw) => match hex::decode(raw.trim()) {
            Ok(bytes) => match Signature::from_slice(&bytes) {
                Ok(signature) => {
                    if let Err(e) = key.verify_strict(&manifest_bytes, &signature) {
                        problems.push(format!("signature mismatch: {e}"));
                    }
                }
                Err(e) => problems.push(format!("manifest.sig is invalid: {e}")),
            },
            Err(e) => problems.push(format!("manifest.sig is not hex: {e}")),
        },
        Err(e) => problems.push(format!("manifest.sig unreadable: {e}")),
    }

    if let Some(hashes) = manifest.get("hashes").and_then(Value::as_object) {
        for (rel, expected) in hashes {
            if !core::rel_path_ok(rel) {
                problems.push(format!("hashed path is not safe: {rel}"));
                continue;
            }
            let path = dir.join(rel);
            match (expected.as_str(), fs::metadata(&path)) {
                (Some(expected), Ok(meta)) if meta.is_file() => {
                    let Some(expected_hex) = expected.strip_prefix("sha256:") else {
                        problems.push(format!("hash for {rel} is not sha256:..."));
                        continue;
                    };
                    match core::hash_file(&path) {
                        Ok(actual) if actual == expected_hex.to_lowercase() => {}
                        Ok(_) => problems.push(format!("hash mismatch for {rel}")),
                        Err(e) => problems.push(e),
                    }
                }
                (None, _) => problems.push(format!("hash for {rel} is not a string")),
                _ => problems.push(format!("hashed file is missing: {rel}")),
            }
        }
    } else {
        problems.push("hashes object is missing".to_string());
    }
    Ok(problems)
}

pub fn trust(profile: &Path, public: &Path) -> Result<String> {
    let bytes = read_public(public)?;
    fs::create_dir_all(profile).map_err(|e| e.to_string())?;
    let value = hex::encode(bytes);
    fs::write(profile.join("trusted.pub"), &value)
        .map_err(|e| format!("{}: {e}", profile.display()))?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eco-pack-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sign_and_verify_roundtrip_detects_tampering() {
        let root = temp("keys");
        let app = root.join("app");
        core::init(&app, "demo-app", "Demo", core::TEMPLATE_MINIMAL).unwrap();
        core::recompute_hashes(&app).unwrap();

        let keys_dir = root.join("keys");
        let (key, _) = keygen(&keys_dir).unwrap();
        sign(&app, Path::new(&key)).unwrap();
        let public = keys_dir.join("trusted.pub");
        assert!(verify(&app, &public).unwrap().is_empty());

        let text = core::read_manifest_text(&app).unwrap().replace("0.1.0", "0.1.1");
        core::write_manifest_text(&app, &text).unwrap();
        let problems = verify(&app, &public).unwrap();
        assert!(problems.iter().any(|p| p.contains("signature mismatch")));
    }

    #[test]
    fn trust_writes_profile_anchor() {
        let root = temp("trust");
        let keys_dir = root.join("keys");
        let (_, public_hex) = keygen(&keys_dir).unwrap();
        let profile = root.join("profile");
        let value = trust(&profile, &keys_dir.join("trusted.pub")).unwrap();
        assert_eq!(value, public_hex);
        assert_eq!(
            fs::read_to_string(profile.join("trusted.pub")).unwrap(),
            public_hex
        );
    }
}

