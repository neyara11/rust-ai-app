use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use std::fs;
use std::path::PathBuf;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("genkey") => genkey(&args[1..]),
        Some("sign") => sign(&args[1..]),
        Some("verify") => verify(&args[1..]),
        _ => {
            eprintln!("usage:");
            eprintln!("  signer genkey --out <dir>");
            eprintln!("  signer sign <manifest.json> --key <dev.key> [--out <manifest.sig>]");
            eprintln!("  signer verify <manifest.json> --pub <trusted.pub>");
            exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        exit(1);
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn positional(args: &[String]) -> Option<&str> {
    args.iter()
        .find(|a| !a.starts_with("--"))
        .map(String::as_str)
        .filter(|a| !a.ends_with(".key") && !a.ends_with(".pub"))
}

fn genkey(args: &[String]) -> Result<(), String> {
    let out = PathBuf::from(flag(args, "--out").ok_or("--out is required")?);
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;

    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| e.to_string())?;
    let signing = SigningKey::from_bytes(&seed);
    let verifying = signing.verifying_key();

    fs::write(out.join("dev.key"), hex::encode(seed)).map_err(|e| e.to_string())?;
    fs::write(out.join("trusted.pub"), hex::encode(verifying.to_bytes()))
        .map_err(|e| e.to_string())?;

    println!("private key: {}", out.join("dev.key").display());
    println!("trust anchor: {}", out.join("trusted.pub").display());
    println!("public key:  {}", hex::encode(verifying.to_bytes()));
    Ok(())
}

fn sign(args: &[String]) -> Result<(), String> {
    let manifest = PathBuf::from(positional(args).ok_or("manifest path is required")?);
    let key_path = PathBuf::from(flag(args, "--key").ok_or("--key is required")?);
    let out = flag(args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.with_file_name("manifest.sig"));

    let seed_hex = fs::read_to_string(&key_path).map_err(|e| e.to_string())?;
    let seed: [u8; 32] = hex::decode(seed_hex.trim())
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "private key must be 32 bytes".to_string())?;
    let signing = SigningKey::from_bytes(&seed);

    let bytes = fs::read(&manifest).map_err(|e| e.to_string())?;
    let signature = signing.sign(&bytes);
    fs::write(&out, hex::encode(signature.to_bytes())).map_err(|e| e.to_string())?;

    println!("signed {} -> {}", manifest.display(), out.display());
    Ok(())
}

fn verify(args: &[String]) -> Result<(), String> {
    let manifest = PathBuf::from(positional(args).ok_or("manifest path is required")?);
    let pub_path = PathBuf::from(flag(args, "--pub").ok_or("--pub is required")?);

    let pub_hex = fs::read_to_string(&pub_path).map_err(|e| e.to_string())?;
    let pub_bytes: [u8; 32] = hex::decode(pub_hex.trim())
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "public key must be 32 bytes".to_string())?;
    let key = VerifyingKey::from_bytes(&pub_bytes).map_err(|e| e.to_string())?;

    let sig_path = manifest.with_file_name("manifest.sig");
    let sig_hex = fs::read_to_string(&sig_path).map_err(|e| e.to_string())?;
    let sig_bytes = hex::decode(sig_hex.trim()).map_err(|e| e.to_string())?;
    let signature = Signature::from_slice(&sig_bytes).map_err(|e| e.to_string())?;

    let bytes = fs::read(&manifest).map_err(|e| e.to_string())?;
    key.verify(&bytes, &signature).map_err(|e| e.to_string())?;

    println!("signature ok: {}", manifest.display());
    Ok(())
}
