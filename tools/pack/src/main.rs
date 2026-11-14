mod core;
mod keys;

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

const USAGE: &str = "\
eco-pack — офлайн-упаковка мини-приложений ECO AIOps

  eco-pack init <dir> --app-id <id> [--name <name>] [--template minimal|files]
  eco-pack validate <dir>
  eco-pack build <dir> [-o <package.zip>]
  eco-pack keygen --out <dir>
  eco-pack sign <dir> --key <dev.key>
  eco-pack verify <dir> --pub <trusted.pub>
  eco-pack trust --profile <path> --pub <trusted.pub>
  eco-pack dev-install <dir> --key <dev.key> --profile <path> [--pub <trusted.pub>] [--platform <exe>]
";

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|value| value == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn positional(args: &[String]) -> Option<&str> {
    args.iter()
        .find(|value| !value.starts_with("--"))
        .map(String::as_str)
}

fn print_report(report: &core::Report) -> bool {
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
    for error in &report.errors {
        println!("error: {error}");
    }
    report.ok()
}

fn cmd_init(args: &[String]) -> i32 {
    let Some(dir) = positional(args) else {
        eprintln!("usage: eco-pack init <dir> --app-id <id>");
        return 2;
    };
    let Some(app_id) = flag(args, "--app-id") else {
        eprintln!("--app-id is required");
        return 2;
    };
    let name = flag(args, "--name").unwrap_or(app_id);
    let template = flag(args, "--template").unwrap_or(core::TEMPLATE_MINIMAL);
    match core::init(Path::new(dir), app_id, name, template) {
        Ok(()) => {
            println!("created {dir} (template: {template})");
            println!("next: edit prompt/system.md, then `eco-pack validate {dir}`");
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn cmd_validate(args: &[String]) -> i32 {
    let Some(dir) = positional(args) else {
        eprintln!("usage: eco-pack validate <dir>");
        return 2;
    };
    match core::validate(Path::new(dir)) {
        Ok(report) => {
            if print_report(&report) {
                println!("ok: package is valid");
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn cmd_build(args: &[String]) -> i32 {
    let Some(dir) = positional(args) else {
        eprintln!("usage: eco-pack build <dir> [-o <package.zip>]");
        return 2;
    };
    let dir = Path::new(dir);
    if let Err(e) = core::recompute_hashes(dir) {
        eprintln!("error: {e}");
        return 1;
    }
    match core::validate(dir) {
        Ok(report) => {
            if !print_report(&report) {
                return 1;
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    }

    let output = match flag(args, "-o").or_else(|| flag(args, "--output")) {
        Some(path) => PathBuf::from(path),
        None => {
            let manifest = core::read_manifest(dir).unwrap_or(Value::Null);
            let app_id = manifest
                .get("app_id")
                .and_then(Value::as_str)
                .unwrap_or("package");
            let version = manifest
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("0.0.0");
            let parent = dir.parent().unwrap_or(Path::new("."));
            parent.join(format!("{app_id}-{version}.zip"))
        }
    };
    if let Err(e) = core::build_zip(dir, &output) {
        eprintln!("error: {e}");
        return 1;
    }
    println!("built: {}", output.display());
    0
}

fn cmd_keygen(args: &[String]) -> i32 {
    let Some(out) = flag(args, "--out") else {
        eprintln!("usage: eco-pack keygen --out <dir>");
        return 2;
    };
    match keys::keygen(Path::new(out)) {
        Ok((key, public)) => {
            println!("private key: {key}");
            println!("public key:  {public}");
            println!("warning: dev key only; never use it for production packages");
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn cmd_sign(args: &[String]) -> i32 {
    let Some(dir) = positional(args) else {
        eprintln!("usage: eco-pack sign <dir> --key <dev.key>");
        return 2;
    };
    let Some(key) = flag(args, "--key") else {
        eprintln!("--key is required");
        return 2;
    };
    match keys::sign(Path::new(dir), Path::new(key)) {
        Ok(()) => {
            println!("signed: {}", Path::new(dir).join("manifest.sig").display());
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn cmd_verify(args: &[String]) -> i32 {
    let Some(dir) = positional(args) else {
        eprintln!("usage: eco-pack verify <dir> --pub <trusted.pub>");
        return 2;
    };
    let Some(public) = flag(args, "--pub") else {
        eprintln!("--pub is required");
        return 2;
    };
    match keys::verify(Path::new(dir), Path::new(public)) {
        Ok(problems) if problems.is_empty() => {
            println!("ok: signature and hashes match");
            0
        }
        Ok(problems) => {
            for problem in problems {
                println!("error: {problem}");
            }
            1
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn cmd_trust(args: &[String]) -> i32 {
    let Some(profile) = flag(args, "--profile") else {
        eprintln!("usage: eco-pack trust --profile <path> --pub <trusted.pub>");
        return 2;
    };
    let Some(public) = flag(args, "--pub") else {
        eprintln!("--pub is required");
        return 2;
    };
    match keys::trust(Path::new(profile), Path::new(public)) {
        Ok(value) => {
            println!("dev anchor installed in {profile}");
            println!("public key: {value}");
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn find_platform(explicit: Option<&str>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!("platform binary not found: {}", path.display()));
    }
    if let Ok(value) = std::env::var("ECO_PLATFORM") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Ok(path);
        }
    }
    let mut candidates = Vec::new();
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(base) = std::env::var(key) {
            candidates.push(PathBuf::from(base).join("ECO AIOps").join("eco-platform.exe"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("eco-platform.exe"));
        }
    }
    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err("eco-platform.exe not found; pass --platform <path>".to_string())
}

fn cmd_dev_install(args: &[String]) -> i32 {
    let Some(dir) = positional(args) else {
        eprintln!("usage: eco-pack dev-install <dir> --key <dev.key> --profile <path>");
        return 2;
    };
    let Some(key) = flag(args, "--key") else {
        eprintln!("--key is required");
        return 2;
    };
    let Some(profile) = flag(args, "--profile") else {
        eprintln!("--profile is required");
        return 2;
    };
    let profile_path = Path::new(profile);

    if let Err(e) = core::recompute_hashes(Path::new(dir)) {
        eprintln!("error: {e}");
        return 1;
    }
    if let Err(e) = keys::sign(Path::new(dir), Path::new(key)) {
        eprintln!("error: {e}");
        return 1;
    }
    if let Some(public) = flag(args, "--pub") {
        if let Err(e) = keys::trust(profile_path, Path::new(public)) {
            eprintln!("error: {e}");
            return 1;
        }
    }

    let platform = match find_platform(flag(args, "--platform")) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let status = Command::new(&platform)
        .arg("--install")
        .arg(dir)
        .env("ECO_PLATFORM_HOME", profile_path)
        .env("ECO_PLATFORM_DEV", "1")
        .status();
    match status {
        Ok(status) if status.success() => {
            println!("installed {dir} into {profile}");
            println!("next: open the app in the platform and test the scenario");
            0
        }
        Ok(status) => {
            eprintln!("error: platform --install exited with {status}");
            1
        }
        Err(e) => {
            eprintln!("error: cannot run {}: {e}", platform.display());
            1
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        print!("{USAGE}");
        std::process::exit(2);
    };
    let rest = &args[1..];
    let code = match command {
        "init" => cmd_init(rest),
        "validate" => cmd_validate(rest),
        "build" => cmd_build(rest),
        "keygen" => cmd_keygen(rest),
        "sign" => cmd_sign(rest),
        "verify" => cmd_verify(rest),
        "trust" => cmd_trust(rest),
        "dev-install" => cmd_dev_install(rest),
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            0
        }
        "-V" | "--version" => {
            println!("eco-pack {}", env!("CARGO_PKG_VERSION"));
            0
        }
        other => {
            eprintln!("unknown command: {other}");
            print!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}
