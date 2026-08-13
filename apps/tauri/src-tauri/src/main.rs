#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments.get(1).is_some_and(|argument| argument == "mcp") {
        if let Err(error) = fileterm_lib::run_mcp_stdio(&arguments[2..]) {
            eprintln!("FileTerm MCP failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    if fileterm_lib::is_cli_command(arguments.get(1).map(String::as_str)) {
        if let Err(error) = fileterm_lib::run_cli(&arguments[1..]) {
            eprintln!("FileTerm CLI failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    fileterm_lib::run()
}
