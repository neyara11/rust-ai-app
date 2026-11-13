#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--verify") => std::process::exit(eco_platform_lib::cli_verify(args.get(1))),
        Some("--install") => std::process::exit(eco_platform_lib::cli_install(args.get(1))),
        Some("--list") => std::process::exit(eco_platform_lib::cli_list()),
        Some("--trust") => std::process::exit(eco_platform_lib::cli_trust(args.get(1))),
        Some("--secret-add") => std::process::exit(eco_platform_lib::cli_secret_add(args.get(1), args.get(2))),
        Some("--import-auth") => std::process::exit(eco_platform_lib::cli_import_auth(args.get(1), args.get(2), args.get(3))),
        Some("--chat-sessions") => std::process::exit(eco_platform_lib::cli_chat_sessions(args.get(1))),
        Some("--chat-async") => std::process::exit(eco_platform_lib::cli_chat_async(args.get(1), args.get(2))),
        Some("--chat") => std::process::exit(eco_platform_lib::cli_chat(args.get(1), args.get(2), args.get(3))),
        Some("--mcp-probe") => std::process::exit(eco_platform_lib::cli_mcp_probe(args.get(1))),
        Some("--mcp-call") => std::process::exit(eco_platform_lib::cli_mcp_call(
            args.get(1),
            args.get(2),
            args.get(3),
        )),
        Some("--mcp-add") => std::process::exit(eco_platform_lib::cli_mcp_add(
            args.get(1),
            args.get(2),
            args.get(3),
        )),
        Some("--mcp-call-app") => std::process::exit(eco_platform_lib::cli_mcp_call_app(
            args.get(1),
            args.get(2),
            args.get(3),
            args.get(4),
        )),
        _ => eco_platform_lib::run(),
    }
}
