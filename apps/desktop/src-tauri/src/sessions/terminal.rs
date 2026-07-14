use tauri::{AppHandle, Emitter, Manager};

pub fn decode_terminal(bytes: &[u8], encoding: &str) -> String {
    match encoding.trim().to_lowercase().as_str() {
        "gbk" | "gb18030" => encoding_rs::GB18030.decode(bytes).0.into_owned(),
        "big5" | "cp950" => encoding_rs::BIG5.decode(bytes).0.into_owned(),
        "shift-jis" | "shift_jis" | "sjis" => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
        "euc-jp" => encoding_rs::EUC_JP.decode(bytes).0.into_owned(),
        "euc-kr" | "cp949" => encoding_rs::EUC_KR.decode(bytes).0.into_owned(),
        "windows-1252" | "cp1252" | "latin1" | "iso-8859-1" => {
            encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned()
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

pub fn encode_terminal(value: &str, encoding: &str) -> Vec<u8> {
    match encoding.trim().to_lowercase().as_str() {
        "gbk" | "gb18030" => encoding_rs::GB18030.encode(value).0.into_owned(),
        "big5" | "cp950" => encoding_rs::BIG5.encode(value).0.into_owned(),
        "shift-jis" | "shift_jis" | "sjis" => encoding_rs::SHIFT_JIS.encode(value).0.into_owned(),
        "euc-jp" => encoding_rs::EUC_JP.encode(value).0.into_owned(),
        "euc-kr" | "cp949" => encoding_rs::EUC_KR.encode(value).0.into_owned(),
        "windows-1252" | "cp1252" | "latin1" | "iso-8859-1" => {
            encoding_rs::WINDOWS_1252.encode(value).0.into_owned()
        }
        _ => value.as_bytes().to_vec(),
    }
}

fn truncate_transcript(value: &mut String) {
    const LIMIT: usize = 200_000;
    const RETAIN: usize = 180_000;
    if value.len() <= LIMIT {
        return;
    }
    let mut start = value.len() - RETAIN;
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    *value = value[start..].to_string();
}

pub async fn emit_terminal_data(app: &AppHandle, tab_id: &str, chunk: &str) {
    let _ = app.emit(
        "terminal:data",
        serde_json::json!({ "tabId": tab_id, "chunk": chunk }),
    );
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut sessions = state.sessions.write().await;
    if let Some(session) = sessions.get_mut(tab_id) {
        session.terminal_transcript.push_str(chunk);
        truncate_transcript(&mut session.terminal_transcript);
    }
}

pub async fn set_terminal_state(app: &AppHandle, tab_id: &str, summary: String, connected: bool) {
    let status = if connected {
        "connected"
    } else {
        "disconnected"
    };
    let transcript = {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        if let Some(tab) = state
            .tabs
            .write()
            .await
            .iter_mut()
            .find(|tab| tab.id == tab_id)
        {
            tab.status = status.to_string();
        }
        let mut sessions = state.sessions.write().await;
        let Some(session) = sessions.get_mut(tab_id) else {
            return;
        };
        session.summary = summary.clone();
        session.connected = connected;
        session.terminal_transcript.clone()
    };
    let _ = app.emit(
        "terminal:state",
        serde_json::json!({
            "tabId": tab_id,
            "summary": summary,
            "transcript": transcript,
            "connected": connected,
        }),
    );
    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
}
