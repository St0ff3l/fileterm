use std::path::PathBuf;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use tokio::sync::{watch, Mutex};

use super::{api_error, has_valid_session_cookie, set_session_cookie, WebServerState};

const DEFAULT_CREDENTIAL: &str = "admin";
const MIN_PASSWORD_LEN: usize = 8;

#[derive(Deserialize, Serialize)]
struct Account {
    username: String,
    salt: [u8; 16],
    hash: [u8; 32],
}

pub(super) struct WebLogin {
    path: PathBuf,
    account: Mutex<Option<Account>>,
    token: watch::Sender<String>,
}

impl WebLogin {
    pub(super) fn open(path: PathBuf) -> std::io::Result<Self> {
        let account = match std::fs::read(&path) {
            Ok(data) => Some(serde_json::from_slice(&data)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        Ok(Self {
            path,
            account: Mutex::new(account),
            token: watch::Sender::new(new_token()),
        })
    }

    pub(super) async fn status(&self, headers: &HeaderMap) -> Value {
        let account = self.account.lock().await;
        if !has_valid_session_cookie(headers, &self.token.borrow()) {
            return json!({ "mode": "login" });
        }
        match account.as_ref() {
            Some(account) => json!({ "mode": "managed", "username": account.username }),
            None => json!({ "mode": "setup", "username": "" }),
        }
    }

    pub(super) async fn authorize(
        &self,
        headers: &HeaderMap,
    ) -> Result<watch::Receiver<String>, StatusCode> {
        let account = self.account.lock().await;
        let token = self.token.subscribe();
        if !has_valid_session_cookie(headers, &token.borrow()) {
            return Err(StatusCode::UNAUTHORIZED);
        }
        if account.is_none() {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(token)
    }
}

fn new_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

async fn password_hash(password: String, salt: [u8; 16]) -> Result<[u8; 32], StatusCode> {
    tokio::task::spawn_blocking(move || {
        let params = argon2::Params::new(19 * 1024, 2, 1, Some(32))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let argon =
            argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let mut output = [0_u8; 32];
        argon
            .hash_password_into(password.as_bytes(), &salt, &mut output)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(output)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Credentials {
    username: String,
    password: String,
    current_password: Option<String>,
}

pub(super) async fn sign_in(
    State(state): State<WebServerState>,
    headers: HeaderMap,
    Json(input): Json<Credentials>,
) -> Result<Response, StatusCode> {
    let login = state.login.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    if input.password.len() > 256 || input.username.len() > 64 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let account = login
        .account
        .try_lock()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    let valid = match account.as_ref() {
        Some(account) if account.username == input.username => {
            let hash = password_hash(input.password, account.salt).await?;
            bool::from(hash.ct_eq(&account.hash))
        }
        None => input.username == DEFAULT_CREDENTIAL && input.password == DEFAULT_CREDENTIAL,
        _ => false,
    };
    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    set_session_cookie(&mut response, &headers, &login.token.borrow());
    Ok(response)
}

pub(super) async fn change(
    State(state): State<WebServerState>,
    headers: HeaderMap,
    Json(input): Json<Credentials>,
) -> Result<Response, StatusCode> {
    let login = state.login.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let mut account = login.account.lock().await;
    if !has_valid_session_cookie(&headers, &login.token.borrow()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let password_valid = if account.is_some() && input.password.is_empty() {
        true
    } else {
        (MIN_PASSWORD_LEN..=256).contains(&input.password.len())
    };
    if input.username.is_empty()
        || input.username.len() > 64
        || !input
            .username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || !password_valid
    {
        return Ok(api_error(
            StatusCode::BAD_REQUEST,
            "Invalid username or password length",
        ));
    }
    let (salt, hash) = if let Some(account) = account.as_ref() {
        let current = input
            .current_password
            .filter(|value| value.len() <= 256)
            .ok_or(StatusCode::FORBIDDEN)?;
        let current_hash = password_hash(current, account.salt).await?;
        if !bool::from(current_hash.ct_eq(&account.hash)) {
            return Err(StatusCode::FORBIDDEN);
        }
        if input.password.is_empty() {
            (account.salt, account.hash)
        } else {
            let salt = *uuid::Uuid::new_v4().as_bytes();
            let hash = password_hash(input.password, salt).await?;
            (salt, hash)
        }
    } else {
        let salt = *uuid::Uuid::new_v4().as_bytes();
        let hash = password_hash(input.password, salt).await?;
        (salt, hash)
    };
    let updated = Account {
        username: input.username,
        salt,
        hash,
    };
    let staged = login.path.with_extension(format!("{}.tmp", new_token()));
    let content = serde_json::to_vec(&updated).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let save_result = crate::storage::write_restricted_file(&staged, &content)
        .map_err(std::io::Error::other)
        .and_then(|()| std::fs::rename(&staged, &login.path));
    if save_result.is_err() {
        let _ = std::fs::remove_file(&staged);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    *account = Some(updated);
    let token = new_token();
    login.token.send_replace(token.clone());
    let mut response = StatusCode::NO_CONTENT.into_response();
    set_session_cookie(&mut response, &headers, &token);
    Ok(response)
}
