#[tauri::command]
pub async fn app_select_local_files(
    _app: AppHandle,
    default_path: Option<String>,
) -> Result<Vec<String>, AppError> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(p) = default_path {
        dialog = dialog.set_directory(p);
    }
    // 不加 "All files" filter（&["*"] 在某些平台不匹配任何文件，导致
    // 对话框里所有文件灰显不可选——用户报告"点上传选不到任何文件"）。
    // 不加 filter 默认显示所有文件。
    let result = dialog.pick_files().await.unwrap_or_default();
    Ok(result
        .into_iter()
        .map(|h| h.path().to_string_lossy().into_owned())
        .collect())
}

#[tauri::command]
pub async fn app_select_local_directory(
    _app: AppHandle,
    default_path: Option<String>,
) -> Result<Option<String>, AppError> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(p) = default_path {
        dialog = dialog.set_directory(p);
    }
    let result = dialog.pick_folder().await;
    Ok(result.map(|h| h.path().to_string_lossy().into_owned()))
}

// ── Encoding helpers ────────────────────────────────────────────────────────

fn encoding_for(label: &str) -> &'static encoding_rs::Encoding {
    encoding_rs::Encoding::for_label(label.as_bytes()).unwrap_or(encoding_rs::UTF_8)
}

fn decode_bytes(bytes: &[u8], encoding: &str) -> String {
    let enc = encoding_for(encoding);
    let (cow, _, _) = enc.decode(bytes);
    cow.into_owned()
}

fn encode_text(text: &str, encoding: &str) -> Vec<u8> {
    let enc = encoding_for(encoding);
    let (cow, _, _) = enc.encode(text);
    cow.into_owned()
}
