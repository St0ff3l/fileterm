use super::*;
use sha2::{Digest, Sha256};

fn lock(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // A separate inode keeps rotation protected across processes and file renames.
    let file = options.open(path.with_file_name("mcp-calls.lock"))?;
    file.lock()?;
    Ok(file)
}

pub(super) fn append(path: &Path, entry: &Value) -> io::Result<()> {
    let _lock = lock(path)?;
    let mut line = serde_json::to_vec(entry)?;
    line.push(b'\n');
    match fs::metadata(path) {
        Ok(metadata)
            if metadata.len().saturating_add(line.len() as u64) > MCP_CALL_LOG_MAX_BYTES =>
        {
            fs::rename(path, path.with_file_name(MCP_CALL_LOG_ROTATED_FILE))?;
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true).read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    // A crash can leave a partial final line. Separate it from the next valid record.
    if file.metadata()?.len() > 0 {
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::End(-1))?;
        let mut last = [0];
        file.read_exact(&mut last)?;
        if last[0] != b'\n' {
            file.write_all(b"\n")?;
        }
    }
    file.write_all(&line)
}

fn read_shard(path: &Path) -> io::Result<String> {
    use std::io::{Seek, SeekFrom};

    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error),
    };
    // Older writers could exceed the rotation limit. Read only the retained tail.
    let start = file
        .metadata()?
        .len()
        .saturating_sub(MCP_CALL_LOG_MAX_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut content = Vec::new();
    file.take(MCP_CALL_LOG_MAX_BYTES)
        .read_to_end(&mut content)?;
    if start > 0 {
        let Some(end) = content.iter().position(|byte| *byte == b'\n') else {
            return Ok(String::new());
        };
        content.drain(..=end);
    }
    Ok(String::from_utf8_lossy(&content).into_owned())
}

pub(super) fn read_page(
    path: &Path,
    limit: usize,
    offset: usize,
    before: Option<&str>,
) -> io::Result<McpCallLogPage> {
    let contents = {
        let _lock = lock(path)?;
        [
            read_shard(&path.with_file_name(MCP_CALL_LOG_ROTATED_FILE))?,
            read_shard(path)?,
        ]
    };
    // Parse outside the file lock. Only the bounded file reads block appenders.
    let mut entries = Vec::new();
    let mut positions = HashMap::new();
    for line in contents.iter().flat_map(|content| content.lines()) {
        let Ok(raw) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(entry) = sanitize_entry(raw, line) else {
            continue;
        };
        let id = entry["id"].as_str().unwrap().to_owned();
        if let Some(index) = positions.get(&id) {
            entries[*index] = entry;
        } else {
            positions.insert(id, entries.len());
            entries.push(entry);
        }
    }
    // Completion events may move between shards. Order by immutable start time,
    // never by the position of whichever event remains after rotation.
    entries.sort_by(|left, right| {
        right["ts"]
            .as_str()
            .cmp(&left["ts"].as_str())
            .then_with(|| right["id"].as_str().cmp(&left["id"].as_str()))
    });
    let total = entries.len();
    let start = match before {
        Some(cursor) => entries
            .iter()
            .position(|entry| entry["id"].as_str() == Some(cursor))
            .map(|index| index + 1)
            .ok_or_else(|| io::Error::other("audit cursor expired; refresh the call log"))?,
        None => offset.min(total),
    };
    let items: Vec<_> = entries.into_iter().skip(start).take(limit).collect();
    let has_more = start.saturating_add(items.len()) < total;
    let next_cursor = items
        .last()
        .and_then(|entry| entry["id"].as_str())
        .map(ToOwned::to_owned);
    Ok(McpCallLogPage {
        items,
        total,
        offset: start,
        has_more,
        next_cursor,
    })
}

fn sanitize_entry(raw: Value, line: &str) -> Option<Value> {
    let ts = raw.get("ts")?.as_str()?;
    chrono::DateTime::parse_from_rfc3339(ts).ok()?;
    let action = safe_action(raw.get("action")?.as_str()?);
    let source = match raw.get("source")?.as_str()? {
        "mcp" => "mcp",
        "cli" => "cli",
        _ => return None,
    };
    let modern = raw.get("schemaVersion") == Some(&json!(2));
    let id = raw
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("legacy-{:x}", Sha256::digest(line.as_bytes())));
    let bridge_session = raw
        .get("bridgeSessionId")
        .and_then(Value::as_str)
        .filter(|id| {
            id.strip_prefix("bridge-")
                .is_some_and(|uuid| uuid::Uuid::parse_str(uuid).is_ok())
        });
    let approval = match raw.get("approval").and_then(Value::as_str) {
        Some(
            value @ ("pending"
            | "not-required"
            | "blocked-by-policy"
            | "rejected"
            | "dismissed"
            | "timed-out"
            | "delegated-to-terminal"
            | "approval-error"),
        ) => value,
        Some("user") if modern => "user",
        _ => "legacy-unknown",
    };
    let status = match raw.get("status").and_then(Value::as_str) {
        Some(
            value @ ("started" | "outcome-uncertain" | "call-failed" | "command-failed"
            | "command-succeeded" | "accepted" | "call-succeeded"),
        ) if modern => value,
        _ => "legacy-unknown",
    };
    let mut target = serde_json::Map::new();
    for key in ["tabId", "profileId", "host", "protocol"] {
        if let Some(value) = raw
            .get("target")
            .and_then(|target| target.get(key))
            .and_then(Value::as_str)
        {
            target.insert(key.to_string(), json!(truncate_text(value, 512)));
        }
    }
    if let Some(port) = raw.pointer("/target/port").and_then(Value::as_u64) {
        target.insert("port".to_string(), json!(port));
    }
    let mut execution = serde_json::Map::new();
    if let Some(code) = raw.pointer("/execution/exitCode").and_then(Value::as_u64) {
        execution.insert("exitCode".to_string(), json!(code));
    }
    if raw
        .pointer("/execution/exitSignal")
        .and_then(Value::as_str)
        .is_some()
    {
        execution.insert("exitSignal".to_string(), json!("signalled"));
    }
    for key in ["timedOut", "cancelled", "running", "inputRequired"] {
        if let Some(value) = raw
            .get("execution")
            .and_then(|value| value.get(key))
            .and_then(Value::as_bool)
        {
            execution.insert(key.to_string(), json!(value));
        }
    }
    let error = raw
        .get("error")
        .and_then(Value::as_str)
        .map(|error| match error {
            "FILETERM_CALL_INTERRUPTED"
            | "FILETERM_CALL_CANCELLED"
            | "FILETERM_EXECUTION_INCOMPLETE"
            | "FILETERM_COMMAND_FAILED" => error,
            _ => mcp_error_code(error),
        });
    Some(json!({
        "schemaVersion": if modern { 2 } else { 1 },
        "id": id, "ts": ts, "source": source,
        "clientId": bridge_session, "bridgeSessionId": bridge_session,
        "action": action, "approval": approval, "status": status,
        "ok": raw.get("ok").and_then(Value::as_bool).unwrap_or(false),
        "callOk": raw.get("callOk").and_then(Value::as_bool),
        "error": error,
        "params": redact_params(action, raw.get("params").unwrap_or(&Value::Null)),
        "target": target, "execution": execution,
        "durationMs": raw.get("durationMs").and_then(Value::as_u64).unwrap_or(0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_rotations_keep_previous_shard_and_every_new_record() {
        let dir = super::super::tests::AuditDirectory::new();
        let path = dir.path();
        let history = vec![b'x'; MCP_CALL_LOG_MAX_BYTES as usize - 1];
        fs::write(&path, &history).unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let threads: Vec<_> = (0..2)
            .map(|id| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    append(&path, &json!({"id":id})).unwrap();
                })
            })
            .collect();
        barrier.wait();
        for task in threads {
            task.join().unwrap();
        }
        assert_eq!(
            fs::read(path.with_file_name(MCP_CALL_LOG_ROTATED_FILE)).unwrap(),
            history
        );
        let current = fs::read_to_string(path).unwrap();
        assert!(current.contains("\"id\":0"));
        assert!(current.contains("\"id\":1"));
    }
}
