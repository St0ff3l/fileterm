use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{multipart::Field, Multipart, Path as AxumPath, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::runtime::remove_staged_path;
use crate::services::transfers::TransferConflictPolicy;
use crate::web::{api_error, content_disposition_attachment, sanitize_file_name, WebServerState};

const PREPARE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

/// Owns a staging path until a successful handoff. Dropping an HTTP request or
/// response also drops this guard and schedules filesystem cleanup.
struct StagedCleanup(Option<PathBuf>);

impl Drop for StagedCleanup {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            drop(crate::runtime::spawn_blocking(move || {
                remove_staged_path(path)
            }));
        }
    }
}

async fn write_multipart_file(
    mut field: Field<'_>,
    path: &Path,
) -> Result<(), (StatusCode, String)> {
    let storage_error =
        |error: std::io::Error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    let mut file = tokio::fs::File::create_new(path)
        .await
        .map_err(storage_error)?;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|error| (error.status(), error.to_string()))?
    {
        file.write_all(&chunk).await.map_err(storage_error)?;
    }
    file.flush().await.map_err(storage_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareDownloadInput {
    pub tab_id: String,
    pub remote_path: String,
    pub target_type: String,
    pub options: Option<Value>,
}

#[derive(Deserialize)]
pub struct ExportConnectionsQuery {
    pub format: Option<String>,
}

pub async fn stage_upload(
    State(state): State<WebServerState>,
    mut multipart: Multipart,
) -> Response {
    let staging_dir = state
        .runtime
        .data_dir
        .join("web-staging")
        .join("uploads")
        .join(uuid::Uuid::new_v4().to_string());
    if let Err(error) = tokio::fs::create_dir_all(&staging_dir).await {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("无法创建上传暂存目录: {error}"),
        );
    }

    let mut cleanup = StagedCleanup(Some(staging_dir.clone()));
    let mut staged_paths = Vec::new();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) => return api_error(error.status(), error.to_string()),
        };
        let fallback = format!("upload-{}", staged_paths.len() + 1);
        let file_name = sanitize_file_name(field.file_name().unwrap_or(&fallback), &fallback);
        let target_path = unique_file_path(&staging_dir, &file_name);
        if let Err((status, error)) = write_multipart_file(field, &target_path).await {
            return api_error(status, error);
        }
        staged_paths.push(target_path.to_string_lossy().into_owned());
    }

    if staged_paths.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "未收到任何上传文件");
    }

    cleanup.0.take();
    Json(json!({ "paths": staged_paths })).into_response()
}

pub async fn prepare_download(
    State(state): State<WebServerState>,
    Json(input): Json<PrepareDownloadInput>,
) -> Response {
    let staging_dir = state
        .runtime
        .data_dir
        .join("web-staging")
        .join("downloads")
        .join(uuid::Uuid::new_v4().to_string());
    if let Err(error) = tokio::fs::create_dir_all(&staging_dir).await {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("无法创建下载暂存目录: {error}"),
        );
    }

    let target_name = input
        .options
        .as_ref()
        .and_then(|value| value.get("targetName"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    let conflict_policy = parse_conflict_policy(input.options.as_ref());
    let staging_str = staging_dir.to_string_lossy().into_owned();

    let mut events_rx = state.runtime.subscribe_events();
    let transfer_result = match input.target_type.as_str() {
        "file" => {
            crate::services::transfers::create_download(
                &state.app,
                input.tab_id.clone(),
                input.remote_path.clone(),
                staging_str,
                target_name,
                conflict_policy,
            )
            .await
        }
        "folder" => {
            crate::services::transfers::create_download_directory(
                &state.app,
                input.tab_id.clone(),
                input.remote_path.clone(),
                staging_str,
                target_name,
                conflict_policy,
            )
            .await
        }
        _ => {
            remove_staged_path(staging_dir);
            return api_error(StatusCode::BAD_REQUEST, "远端传输目标类型无效");
        }
    };

    let transfer_id = match transfer_result {
        Ok(id) => id,
        Err(error) => {
            remove_staged_path(staging_dir);
            return api_error(StatusCode::BAD_REQUEST, error.to_string());
        }
    };

    let completed =
        wait_for_transfer_completion(&state.runtime.workspace, &mut events_rx, &transfer_id).await;

    let (local_path, transfer_name) = match completed {
        Ok(pair) => pair,
        Err(message) => {
            remove_staged_path(staging_dir);
            return api_error(StatusCode::BAD_REQUEST, message);
        }
    };

    let (download_path, file_name, content_type) = if input.target_type == "folder" {
        let base_name = sanitize_file_name(&transfer_name, "download");
        let archive_name = format!("{base_name}.tar.gz");
        let archive_path = staging_dir.join(&archive_name);
        let source_dir = local_path.clone();
        let archive_target = archive_path.clone();
        let folder_entry_name = base_name.clone();
        let archived = crate::runtime::spawn_blocking(move || {
            create_tar_gz(&source_dir, &folder_entry_name, &archive_target)
        })
        .await;
        match archived {
            Ok(Ok(())) => (archive_path, archive_name, "application/gzip".to_string()),
            Ok(Err(error)) | Err(error) => {
                remove_staged_path(staging_dir);
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("压缩下载目录失败: {error}"),
                );
            }
        }
    } else {
        let fallback_name = local_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&transfer_name)
            .to_string();
        let file_name = sanitize_file_name(&fallback_name, "download");
        let mime = mime_guess::from_path(&file_name)
            .first_or_octet_stream()
            .essence_str()
            .to_string();
        (local_path, file_name, mime)
    };

    let token = state
        .runtime
        .register_download(
            download_path,
            Some(staging_dir),
            file_name.clone(),
            content_type,
        )
        .await;
    let snapshot = crate::commands::app_get_snapshot(state.app.clone())
        .await
        .unwrap_or(Value::Null);

    Json(json!({
        "token": token,
        "fileName": file_name,
        "snapshot": snapshot,
    }))
    .into_response()
}

pub async fn serve_download(
    State(state): State<WebServerState>,
    AxumPath(token): AxumPath<String>,
) -> Response {
    let Some(entry) = state.runtime.take_download(&token).await else {
        return api_error(StatusCode::NOT_FOUND, "下载链接已失效或已被使用");
    };

    let cleanup = StagedCleanup(Some(
        entry.cleanup_dir.unwrap_or_else(|| entry.file_path.clone()),
    ));
    let Ok(file) = tokio::fs::File::open(&entry.file_path).await else {
        return api_error(StatusCode::NOT_FOUND, "下载文件不存在");
    };
    let size = file.metadata().await.ok().map(|metadata| metadata.len());
    let stream =
        futures_util::stream::try_unfold((file, cleanup), |(mut file, cleanup)| async move {
            let mut chunk = vec![0; 64 * 1024];
            let count = file.read(&mut chunk).await?;
            if count == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            chunk.truncate(count);
            Ok(Some((Bytes::from(chunk), (file, cleanup))))
        });
    let mut response = Body::from_stream(stream).into_response();
    if let Some(size) = size {
        response
            .headers_mut()
            .insert(axum::http::header::CONTENT_LENGTH, size.into());
    }
    if let Ok(ct) = axum::http::HeaderValue::from_str(&entry.content_type) {
        response.headers_mut().insert(CONTENT_TYPE, ct);
    }
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        content_disposition_attachment(&entry.file_name),
    );
    response.headers_mut().insert(
        CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

pub async fn export_connections(
    State(state): State<WebServerState>,
    Query(query): Query<ExportConnectionsQuery>,
) -> Response {
    let format = query.format.unwrap_or_else(|| "fileterm".to_string());
    let extension = if format == "compatible" {
        "json"
    } else {
        "fileterm.json"
    };
    match crate::services::connections::export_bundle(&state.app, &format) {
        Ok(bytes) => {
            let mut response = (StatusCode::OK, bytes).into_response();
            response.headers_mut().insert(
                CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json; charset=utf-8"),
            );
            response.headers_mut().insert(
                CONTENT_DISPOSITION,
                content_disposition_attachment(&format!("fileterm-connections.{extension}")),
            );
            response
        }
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

pub async fn preview_connection_import(
    State(state): State<WebServerState>,
    mut multipart: Multipart,
) -> Response {
    let staging_dir = state
        .runtime
        .data_dir
        .join("web-staging")
        .join("imports")
        .join(uuid::Uuid::new_v4().to_string());
    if let Err(error) = tokio::fs::create_dir_all(&staging_dir).await {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("无法创建导入暂存目录: {error}"),
        );
    }

    let _cleanup = StagedCleanup(Some(staging_dir.clone()));
    let mut paths = Vec::new();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) => return api_error(error.status(), error.to_string()),
        };
        let fallback = format!("import-{}.json", paths.len() + 1);
        let file_name = sanitize_file_name(field.file_name().unwrap_or(&fallback), &fallback);
        let target_path = unique_file_path(&staging_dir, &file_name);
        if let Err((status, error)) = write_multipart_file(field, &target_path).await {
            return api_error(status, error);
        }
        paths.push(target_path);
    }

    if paths.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "未选择任何导入文件");
    }
    let result =
        crate::services::connections::create_import_plan_from_paths(&state.app, paths).await;
    match result {
        Ok(plan) => Json(plan).into_response(),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error.to_string()),
    }
}

pub async fn stage_ssh_key(
    State(state): State<WebServerState>,
    mut multipart: Multipart,
) -> Response {
    let staging_dir = state
        .runtime
        .data_dir
        .join("web-staging")
        .join("ssh-keys")
        .join(uuid::Uuid::new_v4().to_string());
    if let Err(error) = tokio::fs::create_dir_all(&staging_dir).await {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("无法创建密钥暂存目录: {error}"),
        );
    }

    let Ok(Some(field)) = multipart.next_field().await else {
        remove_staged_path(staging_dir);
        return api_error(StatusCode::BAD_REQUEST, "未上传私钥文件");
    };
    let raw_name = field.file_name().unwrap_or("id_key").to_string();
    let file_name = sanitize_file_name(&raw_name, "id_key");
    let target_path = staging_dir.join(file_name);
    let bytes = match field.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            remove_staged_path(staging_dir);
            return api_error(StatusCode::BAD_REQUEST, format!("读取私钥失败: {error}"));
        }
    };
    if let Err(error) = tokio::fs::write(&target_path, bytes).await {
        remove_staged_path(staging_dir);
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("保存暂存私钥失败: {error}"),
        );
    }

    match crate::services::ssh_keys::inspect_file_for_selection(&state.app, target_path) {
        Ok(selection) => Json(selection).into_response(),
        Err(error) => {
            remove_staged_path(staging_dir);
            api_error(StatusCode::BAD_REQUEST, error.to_string())
        }
    }
}

pub async fn import_font(
    State(state): State<WebServerState>,
    mut multipart: Multipart,
) -> Response {
    let staging_dir = state
        .runtime
        .data_dir
        .join("web-staging")
        .join("fonts")
        .join(uuid::Uuid::new_v4().to_string());
    if let Err(error) = tokio::fs::create_dir_all(&staging_dir).await {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("无法创建字体暂存目录: {error}"),
        );
    }

    let Ok(Some(field)) = multipart.next_field().await else {
        remove_staged_path(staging_dir);
        return api_error(StatusCode::BAD_REQUEST, "未上传字体文件");
    };
    let raw_name = field.file_name().unwrap_or("font.ttf").to_string();
    let file_name = sanitize_file_name(&raw_name, "font.ttf");
    let target_path = staging_dir.join(file_name);
    let bytes = match field.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            remove_staged_path(staging_dir);
            return api_error(
                StatusCode::BAD_REQUEST,
                format!("读取字体文件失败: {error}"),
            );
        }
    };
    if let Err(error) = tokio::fs::write(&target_path, bytes).await {
        remove_staged_path(staging_dir);
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("写入字体暂存文件失败: {error}"),
        );
    }

    let result = crate::services::fonts::import_from_path(&state.app, target_path);
    remove_staged_path(staging_dir);
    match result {
        Ok(font) => Json(font).into_response(),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error.to_string()),
    }
}

pub async fn download_session_log(
    State(state): State<WebServerState>,
    AxumPath(tab_id): AxumPath<String>,
) -> Response {
    match crate::services::session_logs::export_session_log_bytes(&state.app, &tab_id).await {
        Ok((file_name, bytes)) => {
            let mut response = (StatusCode::OK, bytes).into_response();
            response.headers_mut().insert(
                CONTENT_TYPE,
                axum::http::HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            response.headers_mut().insert(
                CONTENT_DISPOSITION,
                content_disposition_attachment(&file_name),
            );
            response
        }
        Err(error) => api_error(StatusCode::BAD_REQUEST, error.to_string()),
    }
}

pub async fn download_app_log(State(state): State<WebServerState>) -> Response {
    let Ok(state_path) = crate::storage::state_path(&state.app) else {
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法解析日志目录");
    };
    crate::services::logging::flush();
    let log_path = state_path.with_file_name("logs").join("app.log");
    let bytes = match tokio::fs::read(&log_path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("读取应用日志失败: {error}"),
            );
        }
    };
    let mut response = (StatusCode::OK, bytes).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        content_disposition_attachment("fileterm-app.log"),
    );
    response
}

fn parse_conflict_policy(options: Option<&Value>) -> Option<TransferConflictPolicy> {
    options
        .and_then(|value| value.get("conflictPolicy"))
        .and_then(Value::as_str)
        .and_then(|value| match value {
            "skip" => Some(TransferConflictPolicy::Skip),
            "replace" => Some(TransferConflictPolicy::Replace),
            _ => None,
        })
}

fn unique_file_path(directory: &Path, file_name: &str) -> PathBuf {
    let candidate = directory.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    directory.join(format!("{}-{file_name}", uuid::Uuid::new_v4()))
}

async fn wait_for_transfer_completion(
    workspace: &crate::services::workspace::WorkspaceState,
    events_rx: &mut tokio::sync::broadcast::Receiver<crate::runtime::RuntimeEvent>,
    transfer_id: &str,
) -> Result<(PathBuf, String), String> {
    let deadline = tokio::time::Instant::now() + PREPARE_DOWNLOAD_TIMEOUT;
    let mut fallback = tokio::time::interval(Duration::from_millis(150));
    loop {
        if let Some(outcome) = inspect_transfer_status(workspace, transfer_id).await {
            return outcome;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err("准备下载超时".to_string());
        }
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                return Err("准备下载超时".to_string());
            }
            _ = fallback.tick() => {}
            _ = events_rx.recv() => {}
        }
    }
}

async fn inspect_transfer_status(
    workspace: &crate::services::workspace::WorkspaceState,
    transfer_id: &str,
) -> Option<Result<(PathBuf, String), String>> {
    let transfers = workspace.transfers.read().await;
    let task = transfers.iter().find(|item| item.id == transfer_id)?;
    match task.status.as_str() {
        "completed" => {
            let dest = task.destination_path.clone()?;
            Some(Ok((PathBuf::from(dest), task.name.clone())))
        }
        "failed" | "canceled" => Some(Err(task
            .message
            .clone()
            .unwrap_or_else(|| format!("传输结束状态: {}", task.status)))),
        _ => None,
    }
}

fn create_tar_gz(source_dir: &Path, entry_name: &str, archive_path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(archive_path).map_err(|error| error.to_string())?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder
        .append_dir_all(entry_name, source_dir)
        .map_err(|error| error.to_string())?;
    let encoder = builder.into_inner().map_err(|error| error.to_string())?;
    encoder.finish().map_err(|error| error.to_string())?;
    Ok(())
}
