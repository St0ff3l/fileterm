mod files;
mod rpc;
mod ws;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::header::{
    AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, COOKIE, HOST, ORIGIN, SET_COOKIE,
};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tower_http::services::{ServeDir, ServeFile};

use crate::runtime::{AppHandle, WebRuntime};
use crate::AppError;

pub const WEB_SESSION_COOKIE: &str = "fileterm_session";
const MAX_UPLOAD_BODY_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone)]
pub struct WebServerState {
    pub app: AppHandle,
    pub runtime: Arc<WebRuntime>,
    pub dist_dir: PathBuf,
    pub session_token: String,
}

impl WebServerState {
    pub fn new(
        app: AppHandle,
        runtime: Arc<WebRuntime>,
        dist_dir: PathBuf,
        session_token: String,
    ) -> Self {
        Self {
            app,
            runtime,
            dist_dir,
            session_token,
        }
    }
}

pub async fn initialize_runtime(app: &AppHandle) -> Result<(), AppError> {
    crate::services::logging::init(app);
    let migration_result = crate::storage::migrate_legacy_data_once(app);
    crate::services::logging::install_panic_hook();
    if let Err(error) = migration_result.as_ref() {
        crate::services::logging::error(
            app,
            "storage",
            format!("startup migration failed: {error}"),
        );
    }
    migration_result?;

    match crate::commands::app_get_ui_preferences(app.clone()) {
        Ok(preferences) => crate::services::logging::set_preferences(
            preferences.basic_logging_enabled,
            preferences.diagnostic_log_categories,
        ),
        Err(error) => crate::services::logging::warn(
            app,
            "logging",
            format!("unable to load log preferences: {error}"),
        ),
    }

    match crate::storage::storage_root(app) {
        Ok(root) => crate::services::logging::info(
            app,
            "storage",
            format!("web runtime storage root={}", root.display()),
        ),
        Err(error) => crate::services::logging::error(
            app,
            "storage",
            format!("unable to resolve web storage root: {error}"),
        ),
    }

    crate::services::logging::info(
        app,
        "app",
        format!(
            "startup runtime=web version={} platform={} arch={}",
            app.package_info().version,
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
    );
    crate::services::serial_ports::start_watcher(app);
    if let Err(error) = crate::services::mcp::start_runtime(app) {
        crate::services::logging::warn(
            app,
            "mcp",
            format!("unable to start MCP local bridge in web mode: {error}"),
        );
    }
    Ok(())
}

pub async fn shutdown_runtime(app: &AppHandle) {
    if let Err(error) = crate::services::transfers::shutdown(app).await {
        crate::services::logging::warn(
            app,
            "transfers",
            format!("failed to flush transfers during web shutdown: {error}"),
        );
    }
    crate::commands::shutdown_session_workers(app).await;
    crate::services::mcp::remove_runtime_descriptor(app);
    crate::services::logging::info(app, "app", "web server exiting");
    crate::services::logging::flush();
}

pub fn build_router(state: WebServerState) -> Router {
    let dist_dir = state.dist_dir.clone();
    let index_file = dist_dir.join("index.html");
    let static_service = ServeDir::new(&dist_dir).not_found_service(ServeFile::new(index_file));

    Router::new()
        .route("/healthz", get(handle_healthz))
        .route("/api/bootstrap", get(handle_bootstrap))
        .route("/api/rpc", post(rpc::handle_rpc))
        .route("/api/ws", get(ws::handle_ws))
        .route(
            "/api/web/stage-upload",
            post(files::stage_upload).layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)),
        )
        .route("/api/web/prepare-download", post(files::prepare_download))
        .route("/api/web/downloads/{token}", get(files::serve_download))
        .route(
            "/api/web/export-connections",
            get(files::export_connections),
        )
        .route(
            "/api/web/preview-connection-import",
            post(files::preview_connection_import)
                .layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)),
        )
        .route(
            "/api/web/stage-ssh-key",
            post(files::stage_ssh_key).layer(DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route(
            "/api/web/import-font",
            post(files::import_font).layer(DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route(
            "/api/web/session-log/{tab_id}",
            get(files::download_session_log),
        )
        .route("/api/web/logs/app.log", get(files::download_app_log))
        .fallback_service(static_service)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security_and_cache_middleware,
        ))
        .with_state(state)
}

async fn handle_healthz() -> Json<Value> {
    Json(json!({
        "ok": true,
        "runtime": "web",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn handle_bootstrap(
    State(state): State<WebServerState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let mut response = Json(json!({
        "platform": "web",
        "serverOs": std::env::consts::OS,
        "arch": crate::commands::app_get_arch(),
        "runtimeName": "Web",
        "runtimeVersion": format!("web-{}", env!("CARGO_PKG_VERSION")),
        "appVersion": env!("CARGO_PKG_VERSION"),
        "appName": "FileTerm",
        "isDesktop": false,
        "capabilities": {
            "localTerminal": true,
            "localFiles": true,
            "serial": true,
            "updates": false,
            "windowControls": false
        }
    }))
    .into_response();

    let is_https = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|proto| proto.eq_ignore_ascii_case("https"));
    let cookie = if is_https {
        format!(
            "{WEB_SESSION_COOKIE}={}; HttpOnly; SameSite=Strict; Path=/; Secure",
            state.session_token
        )
    } else {
        format!(
            "{WEB_SESSION_COOKIE}={}; HttpOnly; SameSite=Strict; Path=/",
            state.session_token
        )
    };
    if let Ok(header_value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(SET_COOKIE, header_value);
    }
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn security_and_cache_middleware(
    State(state): State<WebServerState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().clone();
    let headers = request.headers();

    if path == "/api/ws" {
        if !origin_matches_host(headers) {
            return (StatusCode::FORBIDDEN, "Origin mismatch").into_response();
        }
        if !has_valid_session_cookie(headers, &state.session_token) {
            return (
                StatusCode::UNAUTHORIZED,
                "Missing or invalid session cookie",
            )
                .into_response();
        }
    } else if matches!(
        method,
        Method::POST | Method::PUT | Method::DELETE | Method::PATCH
    ) && !origin_matches_host(headers)
    {
        return (StatusCode::FORBIDDEN, "Origin mismatch").into_response();
    }

    // Never allow authorization/session headers to accidentally bypass reverse proxy expectations.
    let _ = headers.get(AUTHORIZATION);

    let mut response = next.run(request).await;
    if !response.headers().contains_key(CACHE_CONTROL) {
        if path.starts_with("/assets/") {
            response.headers_mut().insert(
                CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
        } else if path == "/" || path.ends_with(".html") {
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        }
    }
    response
}

fn origin_matches_host(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN).and_then(|value| value.to_str().ok()) else {
        return true;
    };
    let Ok(parsed_origin) = url::Url::parse(origin) else {
        return false;
    };
    let Some(origin_host) = parsed_origin.host_str() else {
        return false;
    };
    let origin_authority = match parsed_origin.port() {
        Some(port) => format!("{origin_host}:{port}"),
        None => origin_host.to_string(),
    };
    let request_host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(HOST))
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').next().unwrap_or(value).trim())
        .filter(|value| !value.is_empty());

    let Some(request_host) = request_host else {
        return false;
    };

    if origin_authority.eq_ignore_ascii_case(request_host) {
        return true;
    }

    // Also match bare hostname when standard ports (80/443) or reverse-proxy ports differ only by implicit default port.
    let request_hostname = request_host
        .strip_prefix('[')
        .and_then(|s| s.split(']').next())
        .unwrap_or_else(|| request_host.split(':').next().unwrap_or(request_host));
    parsed_origin.port().is_none() && origin_host.eq_ignore_ascii_case(request_hostname)
}

fn has_valid_session_cookie(headers: &HeaderMap, expected_token: &str) -> bool {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|raw| raw.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .any(|(name, value)| name.trim() == WEB_SESSION_COOKIE && value.trim() == expected_token)
}

pub(super) fn sanitize_file_name(raw: &str, fallback: &str) -> String {
    let base = Path::new(raw)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(fallback)
        .trim();
    let cleaned: String = base
        .chars()
        .map(|ch| {
            if matches!(ch, '/' | '\\' | '\0' | '\r' | '\n') {
                '_'
            } else {
                ch
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        fallback.to_string()
    } else {
        cleaned
    }
}

pub(super) fn content_disposition_attachment(file_name: &str) -> HeaderValue {
    let ascii_fallback: String = file_name
        .chars()
        .map(|ch| {
            if ch.is_ascii() && !matches!(ch, '"' | '\\' | '\r' | '\n') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let encoded: String = url::form_urlencoded::byte_serialize(file_name.as_bytes()).collect();
    let header_value =
        format!("attachment; filename=\"{ascii_fallback}\"; filename*=UTF-8''{encoded}");
    HeaderValue::from_str(&header_value).unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

pub(super) fn api_error(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        [(CONTENT_TYPE, "application/json")],
        Json(json!({ "error": message.into() })),
    )
        .into_response()
}
