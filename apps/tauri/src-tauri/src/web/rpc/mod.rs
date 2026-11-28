mod core;
mod profiles;
mod workspace;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::web::{api_error, WebServerState};
use crate::AppError;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcRequest {
    pub command: String,
    #[serde(default)]
    pub args: Value,
    pub client_id: Option<String>,
    pub window_label: Option<String>,
    pub channel_id: Option<u32>,
}

pub async fn handle_rpc(
    State(state): State<WebServerState>,
    Json(request): Json<RpcRequest>,
) -> Response {
    let command = request.command.as_str();
    let window_label = request
        .window_label
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("main")
        .to_string();

    let result = if let Some(outcome) =
        core::dispatch(&state, command, &request.args, &window_label, &request).await
    {
        outcome
    } else if let Some(outcome) = workspace::dispatch(&state, command, &request.args).await {
        outcome
    } else if let Some(outcome) =
        profiles::dispatch(&state, command, &request.args, &window_label).await
    {
        outcome
    } else {
        return api_error(
            StatusCode::NOT_FOUND,
            format!("未知或仅桌面端可用的 RPC 命令: {command}"),
        );
    };

    match result {
        Ok(data) => (StatusCode::OK, Json(json!({ "data": data }))).into_response(),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error.to_string()),
    }
}

pub(super) fn required_arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, AppError> {
    let raw = args
        .get(name)
        .cloned()
        .ok_or_else(|| AppError::Command(format!("缺少参数 {name}")))?;
    serde_json::from_value(raw)
        .map_err(|error| AppError::Serialization(format!("参数 {name} 格式无效: {error}")))
}

pub(super) fn optional_arg<T: DeserializeOwned>(
    args: &Value,
    name: &str,
) -> Result<Option<T>, AppError> {
    let Some(raw) = args.get(name) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    serde_json::from_value(raw.clone())
        .map(Some)
        .map_err(|error| AppError::Serialization(format!("参数 {name} 格式无效: {error}")))
}

pub(super) fn serialize<T: serde::Serialize>(
    result: Result<T, AppError>,
) -> Result<Value, AppError> {
    result.and_then(|value| {
        serde_json::to_value(value).map_err(|error| AppError::Serialization(error.to_string()))
    })
}
