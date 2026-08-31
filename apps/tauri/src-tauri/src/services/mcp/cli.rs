// One-shot and JSONL CLI argument parsing, secrets, and help.
fn write_cli_jsonl_value(
    stdout: &Arc<Mutex<io::BufWriter<io::Stdout>>>,
    value: &Value,
) -> io::Result<()> {
    let payload = serde_json::to_vec(value).map_err(|error| io::Error::other(error.to_string()))?;
    if payload.len() > MCP_MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "FileTerm CLI JSONL response exceeds the size limit",
        ));
    }
    let mut stdout = stdout
        .lock()
        .map_err(|_| io::Error::other("FileTerm CLI JSONL output is unavailable"))?;
    stdout.write_all(&payload)?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}

/// Entry point for the small FileTerm CLI. The CLI intentionally
/// shares the MCP bridge and returns JSON so user-run shell scripts can use
/// the same capability boundary without duplicating authorization logic.
/// External Agents should use `fileterm cli --jsonl` instead of spawning this
/// one-shot entry point for every request.
pub fn run_cli(arguments: &[String]) -> Result<(), String> {
    if arguments.first().is_some_and(|argument| argument == "cli")
        && arguments
            .get(1)
            .is_some_and(|argument| argument == "--jsonl")
    {
        return run_cli_jsonl(&arguments[2..]);
    }

    let command_index = usize::from(arguments.first().is_some_and(|argument| argument == "cli"));
    let Some(command) = arguments.get(command_index).map(String::as_str) else {
        print_cli_help();
        return Ok(());
    };
    let options = &arguments[command_index + 1..];

    match command {
        "help" | "-h" | "--help" => {
            print_cli_help();
            Ok(())
        }
        "-V" | "--version" => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "connections" => {
            if has_cli_help(options) {
                print_cli_command_help("connections");
                return Ok(());
            }
            let values = parse_cli_options(options, &["limit", "offset"])?;
            let mut params = serde_json::Map::new();
            if let Some(limit) = values.get("limit") {
                params.insert("limit".to_string(), json!(parse_cli_usize("limit", limit)?));
            }
            if let Some(offset) = values.get("offset") {
                params.insert(
                    "offset".to_string(),
                    json!(parse_cli_usize("offset", offset)?),
                );
            }
            print_cli_result(call_desktop_bridge(cli_bridge_request(
                "list_connections",
                Value::Object(params),
            ))?)
        }
        "sessions" => {
            if has_cli_help(options) {
                print_cli_command_help("sessions");
                return Ok(());
            }
            let values = parse_cli_options(options, &["profile-id"])?;
            let mut params = serde_json::Map::new();
            if let Some(profile_id) = values.get("profile-id") {
                params.insert("profile_id".to_string(), json!(profile_id));
            }
            print_cli_result(call_desktop_bridge(cli_bridge_request(
                "get_session_context",
                Value::Object(params),
            ))?)
        }
        "directory" | "ls" => {
            if has_cli_help(options) {
                print_cli_command_help("directory");
                return Ok(());
            }
            let values = parse_cli_options(options, &["tab-id", "path", "limit", "offset"])?;
            let tab_id = values
                .get("tab-id")
                .ok_or_else(|| "directory requires --tab-id <TAB_ID>".to_string())?;
            let mut params = serde_json::Map::new();
            params.insert("tab_id".to_string(), json!(tab_id));
            if let Some(path) = values.get("path") {
                params.insert("path".to_string(), json!(path));
            }
            if let Some(limit) = values.get("limit") {
                params.insert("limit".to_string(), json!(parse_cli_usize("limit", limit)?));
            }
            if let Some(offset) = values.get("offset") {
                params.insert(
                    "offset".to_string(),
                    json!(parse_cli_usize("offset", offset)?),
                );
            }
            print_cli_result(call_desktop_bridge(cli_bridge_request(
                "list_remote_directory",
                Value::Object(params),
            ))?)
        }
        "commands" | "command-templates" => {
            cli_action("get_command_templates", options, &["limit", "offset"], &[])
        }
        "read" | "cat" => cli_action(
            "read_remote_file",
            options,
            &["tab-id", "path", "encoding"],
            &["tab-id", "path"],
        ),
        "transfers" => cli_action("list_transfers", options, &["limit", "offset"], &[]),
        "wait-transfer" => cli_action(
            "wait_for_transfer",
            options,
            &["transfer-id", "timeout-ms"],
            &["transfer-id"],
        ),
        "wait-connection" => cli_action(
            "wait_for_connection",
            options,
            &["operation-id", "timeout-ms"],
            &["operation-id"],
        ),
        "tunnels" => cli_action("list_ssh_tunnels", options, &["tab-id"], &["tab-id"]),
        "open" => cli_action(
            "open_connection",
            options,
            &["profile-id", "wait-for-ready", "timeout-ms"],
            &["profile-id"],
        ),
        "activate" => cli_action("activate_session", options, &["tab-id"], &["tab-id"]),
        "reconnect" => cli_action("reconnect_session", options, &["tab-id"], &["tab-id"]),
        "disconnect" => cli_action("disconnect_session", options, &["tab-id"], &["tab-id"]),
        "close" => cli_action("close_session", options, &["tab-id"], &["tab-id"]),
        "exec" | "execute" => cli_exec_action(options),
        "command-template" => cli_action(
            "execute_command_template",
            options,
            &["tab-id", "command-id", "args-json", "options-json"],
            &["tab-id", "command-id"],
        ),
        "write" => cli_action(
            "write_remote_file",
            options,
            &["tab-id", "path", "content", "encoding"],
            &["tab-id", "path", "content"],
        ),
        "mkdir" => cli_action(
            "create_remote_directory",
            options,
            &["tab-id", "parent-path", "name"],
            &["tab-id", "parent-path", "name"],
        ),
        "touch" => cli_action(
            "create_remote_file",
            options,
            &["tab-id", "parent-path", "name"],
            &["tab-id", "parent-path", "name"],
        ),
        "copy" => cli_action(
            "copy_remote_path",
            options,
            &["tab-id", "target-path", "destination-path", "target-type"],
            &["tab-id", "target-path", "destination-path", "target-type"],
        ),
        "move" => cli_action(
            "move_remote_path",
            options,
            &["tab-id", "target-path", "destination-path"],
            &["tab-id", "target-path", "destination-path"],
        ),
        "rename" => cli_action(
            "rename_remote_path",
            options,
            &["tab-id", "target-path", "new-name"],
            &["tab-id", "target-path", "new-name"],
        ),
        "delete" => cli_action(
            "delete_remote_path",
            options,
            &["tab-id", "target-path", "target-type"],
            &["tab-id", "target-path", "target-type"],
        ),
        "chmod" => cli_action(
            "change_remote_permissions",
            options,
            &["tab-id", "path", "mode", "recursive", "apply-to"],
            &["tab-id", "path", "mode"],
        ),
        "access" => cli_action(
            "set_remote_file_access_mode",
            options,
            &["tab-id", "mode"],
            &["tab-id", "mode"],
        ),
        "upload" => cli_action(
            "upload_file",
            options,
            &["tab-id", "local-path", "remote-directory", "target-name"],
            &["tab-id", "local-path", "remote-directory"],
        ),
        "download" => cli_action(
            "download_file",
            options,
            &["tab-id", "remote-path", "local-directory", "target-name"],
            &["tab-id", "remote-path", "local-directory"],
        ),
        "download-directory" => cli_action(
            "download_remote_directory",
            options,
            &["tab-id", "remote-path", "local-directory", "target-name"],
            &["tab-id", "remote-path", "local-directory"],
        ),
        "pause-transfer" => cli_action(
            "pause_transfer",
            options,
            &["transfer-id"],
            &["transfer-id"],
        ),
        "resume-transfer" => cli_action(
            "resume_transfer",
            options,
            &["transfer-id"],
            &["transfer-id"],
        ),
        "discard-transfer" | "cancel-transfer" => cli_action(
            "discard_transfer",
            options,
            &["transfer-id"],
            &["transfer-id"],
        ),
        "clear-transfers" => cli_action(
            "clear_transfers",
            options,
            &["transfer-ids"],
            &["transfer-ids"],
        ),
        "create-tunnel" => cli_action(
            "create_ssh_tunnel",
            options,
            &["tab-id", "rule-json"],
            &["tab-id", "rule-json"],
        ),
        "start-tunnel" => cli_action(
            "start_ssh_tunnel",
            options,
            &["tab-id", "rule-id"],
            &["tab-id", "rule-id"],
        ),
        "stop-tunnel" => cli_action(
            "stop_ssh_tunnel",
            options,
            &["tab-id", "rule-id"],
            &["tab-id", "rule-id"],
        ),
        "delete-tunnel" => cli_action(
            "delete_ssh_tunnel",
            options,
            &["tab-id", "rule-id"],
            &["tab-id", "rule-id"],
        ),
        "call" => cli_call_action(options),
        _ => Err(format!(
            "Unknown FileTerm CLI command: {command}. Run `fileterm --help` for usage."
        )),
    }
}

fn has_cli_help(arguments: &[String]) -> bool {
    arguments
        .iter()
        .any(|argument| argument == "-h" || argument == "--help")
}

fn cli_bridge_request(action: &str, params: Value) -> BridgeRequest {
    BridgeRequest {
        action: action.to_string(),
        params,
        source: WorkspaceSessionSource::Cli,
        // Direct CLI is still an external bridge caller. Read-only actions
        // pass automatically in the basic-safe policy; side effects use the
        // same FileTerm approval dialog as MCP and CLI JSONL.
        requires_approval: true,
        progress_token: None,
    }
}

fn cli_action(
    action: &str,
    arguments: &[String],
    allowed: &[&str],
    required: &[&str],
) -> Result<(), String> {
    if has_cli_help(arguments) {
        print_cli_command_help(action);
        return Ok(());
    }
    let values = parse_cli_options(arguments, allowed)?;
    for key in required {
        if !values.contains_key(*key) {
            return Err(format!("{action} requires --{key} <value>"));
        }
    }
    let params = cli_values_to_params(&values)?;
    print_cli_result(call_desktop_bridge(cli_bridge_request(action, params))?)
}

fn cli_exec_action(arguments: &[String]) -> Result<(), String> {
    if has_cli_help(arguments) {
        print_cli_command_help("execute_remote_command");
        return Ok(());
    }
    let (values, stdin_flags) = parse_cli_options_with_flags(
        arguments,
        &[
            "tab-id",
            "command",
            "cwd",
            "timeout-ms",
            "sudo-password",
            "su-password",
            "save-sudo-password",
            "save-su-password",
            "sudo-password-stdin",
            "su-password-stdin",
        ],
        &["sudo-password-stdin", "su-password-stdin"],
    )?;
    for key in ["tab-id", "command"] {
        if !values.contains_key(key) {
            return Err(format!("exec requires --{key} <value>"));
        }
    }

    let use_sudo_stdin = stdin_flags.contains("sudo-password-stdin");
    let use_su_stdin = stdin_flags.contains("su-password-stdin");
    if use_sudo_stdin && use_su_stdin {
        return Err(
            "--sudo-password-stdin and --su-password-stdin cannot be used together".to_string(),
        );
    }
    if use_sudo_stdin && values.contains_key("sudo-password") {
        return Err("Use either --sudo-password or --sudo-password-stdin, not both".to_string());
    }
    if use_su_stdin && values.contains_key("su-password") {
        return Err("Use either --su-password or --su-password-stdin, not both".to_string());
    }

    let mut params = cli_values_to_params(&values)?;
    let params = params
        .as_object_mut()
        .ok_or_else(|| "exec parameters must be a JSON object".to_string())?;
    if use_sudo_stdin {
        params.insert(
            "sudo_password".to_string(),
            Value::String(read_cli_secret_from_stdin("--sudo-password-stdin")?),
        );
    }
    if use_su_stdin {
        params.insert(
            "su_password".to_string(),
            Value::String(read_cli_secret_from_stdin("--su-password-stdin")?),
        );
    }
    if values.contains_key("sudo-password") || values.contains_key("su-password") {
        eprintln!(
            "Warning: --sudo-password/--su-password is visible to local process inspection and may be saved in shell history; prefer the matching --*-password-stdin option."
        );
    }
    print_cli_result(call_desktop_bridge(cli_bridge_request(
        "execute_remote_command",
        Value::Object(params.clone()),
    ))?)
}

fn cli_call_action(arguments: &[String]) -> Result<(), String> {
    let action = arguments
        .first()
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(|| "call requires an action name".to_string())?;
    let values = parse_cli_options(&arguments[1..], &["params-json"])?;
    let params_json = values
        .get("params-json")
        .ok_or_else(|| "call requires --params-json JSON".to_string())?;
    let params = serde_json::from_str::<Value>(params_json)
        .map_err(|error| format!("--params-json must be valid JSON: {error}"))?;
    if !params.is_object() {
        return Err("--params-json must contain a JSON object".to_string());
    }
    print_cli_result(call_desktop_bridge(cli_bridge_request(action, params))?)
}

fn cli_values_to_params(values: &HashMap<String, String>) -> Result<Value, String> {
    let mut params = serde_json::Map::new();
    for (key, value) in values {
        let parameter = match key.as_str() {
            "rule-json" => "rule".to_string(),
            "args-json" => "args".to_string(),
            "options-json" => "options".to_string(),
            "transfer-ids" => "transfer_ids".to_string(),
            _ => key.replace('-', "_"),
        };
        let converted = match key.as_str() {
            "rule-json" => serde_json::from_str::<Value>(value)
                .map_err(|error| format!("--rule-json must be valid JSON: {error}"))?,
            "args-json" => serde_json::from_str::<Value>(value)
                .map_err(|error| format!("--args-json must be valid JSON: {error}"))?,
            "options-json" => serde_json::from_str::<Value>(value)
                .map_err(|error| format!("--options-json must be valid JSON: {error}"))?,
            "transfer-ids" => Value::Array(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(|item| Value::String(item.to_string()))
                    .collect(),
            ),
            "recursive" => Value::Bool(parse_cli_bool("recursive", value)?),
            "wait-for-ready" => Value::Bool(parse_cli_bool("wait-for-ready", value)?),
            "save-sudo-password" | "save-su-password" => Value::Bool(parse_cli_bool(key, value)?),
            "limit" | "offset" | "timeout-ms" => json!(parse_cli_usize(key, value)?),
            _ => Value::String(value.clone()),
        };
        params.insert(parameter, converted);
    }
    Ok(Value::Object(params))
}

fn parse_cli_bool(key: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(format!("Option --{key} must be true or false")),
    }
}

fn parse_cli_options(
    arguments: &[String],
    allowed: &[&str],
) -> Result<HashMap<String, String>, String> {
    parse_cli_options_with_flags(arguments, allowed, &[]).map(|(values, _)| values)
}

fn parse_cli_options_with_flags(
    arguments: &[String],
    allowed: &[&str],
    flags: &[&str],
) -> Result<(HashMap<String, String>, HashSet<String>), String> {
    let mut values = HashMap::new();
    let mut present_flags = HashSet::new();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        let key = argument
            .strip_prefix("--")
            .filter(|key| !key.is_empty())
            .ok_or_else(|| format!("Expected a long option, got {argument}"))?;
        if !allowed.contains(&key) {
            return Err(format!("Unknown option --{key}"));
        }
        if values.contains_key(key) || present_flags.contains(key) {
            return Err(format!("Option --{key} may only be provided once"));
        }
        if flags.contains(&key) {
            present_flags.insert(key.to_string());
            index += 1;
            continue;
        }
        let value = arguments
            .get(index + 1)
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| format!("Option --{key} requires a value"))?;
        if value.is_empty() {
            return Err(format!("Option --{key} must not be empty"));
        }
        values.insert(key.to_string(), value.clone());
        index += 2;
    }
    Ok((values, present_flags))
}

const CLI_STDIN_SECRET_MAX_BYTES: usize = 4 * 1024;

/// Read exactly one newline-delimited secret for a one-shot CLI request.
/// Reading is bounded before decoding so a redirected stdin cannot make the
/// CLI allocate an unbounded buffer. The delimiter is removed, while all
/// other password characters—including spaces—are preserved for the backend
/// validator.
fn read_cli_secret_from_stdin(option: &str) -> Result<String, String> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut bytes = Vec::with_capacity(CLI_STDIN_SECRET_MAX_BYTES);
    let mut terminated = false;
    loop {
        let mut byte = [0_u8; 1];
        let read = reader
            .read(&mut byte)
            .map_err(|_| format!("{option} could not read a password from stdin"))?;
        if read == 0 {
            break;
        }
        if byte[0] == b'\n' {
            terminated = true;
            break;
        }
        bytes.push(byte[0]);
        if bytes.len() > CLI_STDIN_SECRET_MAX_BYTES {
            return Err(format!("{option} password exceeds the 4 KiB limit"));
        }
    }
    decode_cli_secret_bytes(option, bytes, terminated)
}

fn decode_cli_secret_bytes(
    option: &str,
    mut bytes: Vec<u8>,
    terminated: bool,
) -> Result<String, String> {
    if bytes.len() > CLI_STDIN_SECRET_MAX_BYTES {
        return Err(format!("{option} password exceeds the 4 KiB limit"));
    }
    if terminated && bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    if bytes.is_empty() {
        return Err(format!(
            "{option} requires a non-empty password line on stdin"
        ));
    }
    let value = String::from_utf8(bytes)
        .map_err(|_| format!("{option} password must be valid UTF-8 text"))?;
    if value.chars().any(char::is_control) {
        return Err(format!(
            "{option} password contains unsupported control characters"
        ));
    }
    Ok(value)
}

fn parse_cli_usize(key: &str, value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("Option --{key} must be a non-negative integer"))
}

fn print_cli_result(result: Value) -> Result<(), String> {
    let output = serde_json::to_string_pretty(&result)
        .map_err(|error| format!("Unable to encode FileTerm CLI response: {error}"))?;
    println!("{output}");
    Ok(())
}

fn print_cli_help() {
    println!(
        "Persistent external-Agent mode: `fileterm cli --jsonl` keeps one JSONL stdin/stdout process alive. FileTerm must already be running; send one JSON request per line and reuse this process."
    );
    println!(
        "FileTerm CLI {}\n\nUsage:\n  fileterm connections [--limit N] [--offset N]\n  fileterm sessions [--profile-id PROFILE_ID]\n  fileterm directory --tab-id TAB_ID [--path REMOTE_PATH] [--limit N] [--offset N]\n  fileterm read --tab-id TAB_ID --path REMOTE_PATH [--encoding utf-8]\n  fileterm exec --tab-id TAB_ID --command COMMAND [--cwd PATH] [--timeout-ms N]\n  fileterm write --tab-id TAB_ID --path REMOTE_PATH --content TEXT\n  fileterm upload --tab-id TAB_ID --local-path PATH --remote-directory PATH\n  fileterm download --tab-id TAB_ID --remote-path REMOTE_PATH --local-directory PATH\n  fileterm transfers [--limit N] [--offset N]\n  fileterm wait-transfer --transfer-id ID [--timeout-ms N]\n  fileterm mkdir|touch|copy|move|rename|delete|chmod|access ...\n  fileterm tunnels|create-tunnel|start-tunnel|stop-tunnel|delete-tunnel ...\n  fileterm call ACTION --params-json JSON\n  fileterm mcp\n\n`exec` uses a dedicated non-interactive SSH channel for ordinary servers. A network-device session instead sends one single-line native CLI command through the visible raw terminal and returns `rawTerminal=true` with `exitCode=null`; its output can include the command echo and prompt. If a command needs generic input such as MFA, a confirmation, or a REPL answer, it returns REMOTE_INTERACTIVE_INPUT_REQUIRED; finish that operation in the visible SSH terminal and retry. Sudo/su credentials use explicit trusted parameters, encrypted profiles, or the FileTerm main-window secure prompt, and apply only to ordinary server sessions. CLI operations are explicit user-invoked JSON commands and require a running FileTerm desktop app. The shared policy runs queries and ordinary safe commands automatically; dangerous, privileged, mutating or unrecognized commands, session changes, file or transfer changes, tunnels, sudo/su and unknown actions use the FileTerm main-window approval unless Full access is selected.\nUse `fileterm cli <command>` as an equivalent spelling.",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "When FileTerm opens its secure sudo/su prompt, `exec` waits and reports input-required on stderr; enter the password in the FileTerm window and do not retry the command."
    );
    println!(
        "Connection lifecycle: `fileterm open --profile-id ID [--wait-for-ready true|false] [--timeout-ms N]`; resume with `fileterm wait-connection --operation-id ID [--timeout-ms N]`."
    );
}

fn print_cli_command_help(command: &str) {
    match command {
        "connections" => println!("Usage: fileterm connections [--limit N] [--offset N]"),
        "sessions" => println!("Usage: fileterm sessions [--profile-id PROFILE_ID]"),
        "directory" => println!(
            "Usage: fileterm directory --tab-id TAB_ID [--path REMOTE_PATH] [--limit N] [--offset N]\n       fileterm ls --tab-id TAB_ID [--path REMOTE_PATH] [--limit N] [--offset N]"
        ),
        "read_remote_file" => println!("Usage: fileterm read --tab-id TAB_ID --path REMOTE_PATH [--encoding utf-8]"),
        "execute_remote_command" => println!("Usage: fileterm exec --tab-id TAB_ID --command COMMAND [--cwd PATH] [--timeout-ms N] [--sudo-password PASSWORD | --sudo-password-stdin] [--save-sudo-password true] [--su-password PASSWORD | --su-password-stdin] [--save-su-password true]\n       --*-password-stdin reads one password line from stdin; prefer it for scripts and Agent-generated commands."),
        "wait_for_transfer" => println!("Usage: fileterm wait-transfer --transfer-id ID [--timeout-ms N]"),
        "wait_for_connection" => println!("Usage: fileterm wait-connection --operation-id ID [--timeout-ms N]"),
        "open_connection" => println!("Usage: fileterm open --profile-id PROFILE_ID [--wait-for-ready true|false] [--timeout-ms N]"),
        "write_remote_file" => println!("Usage: fileterm write --tab-id TAB_ID --path REMOTE_PATH --content TEXT [--encoding utf-8]"),
        "upload_file" => println!("Usage: fileterm upload --tab-id TAB_ID --local-path PATH --remote-directory PATH [--target-name NAME]"),
        "download_file" => println!("Usage: fileterm download --tab-id TAB_ID --remote-path PATH --local-directory PATH [--target-name NAME]"),
        "download_remote_directory" => println!("Usage: fileterm download-directory --tab-id TAB_ID --remote-path PATH --local-directory PATH [--target-name NAME]"),
        "clear_transfers" => println!("Usage: fileterm clear-transfers --transfer-ids ID1,ID2"),
        "create_ssh_tunnel" => println!("Usage: fileterm create-tunnel --tab-id TAB_ID --rule-json JSON"),
        "call" => println!("Usage: fileterm call ACTION --params-json JSON"),
        _ => print_cli_help(),
    }
}

