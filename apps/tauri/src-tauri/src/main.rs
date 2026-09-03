#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
fn set_macos_process_name() {
    use objc2_foundation::{NSProcessInfo, NSString};

    NSProcessInfo::processInfo().setProcessName(&NSString::from_str("FileTerm"));
}

fn main() {
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments.get(1).is_some_and(|argument| argument == "mcp") {
        if let Err(error) = fileterm_lib::run_mcp_stdio(&arguments[2..]) {
            eprintln!("FileTerm MCP failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    if arguments.get(1).is_some() {
        if let Err(error) = fileterm_lib::run_cli(&arguments[1..]) {
            eprintln!("FileTerm CLI failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    #[cfg(target_os = "macos")]
    set_macos_process_name();
    fileterm_lib::run()
}
