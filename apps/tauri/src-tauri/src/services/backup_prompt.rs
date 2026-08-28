//! Compatibility boundary for remote backup password access.
//!
//! Remote backup passwords used to be requested through a one-shot renderer
//! prompt. They now live in the security page and are read only inside Rust;
//! keeping this small boundary avoids duplicating the WebDAV/S3 flow while
//! making a missing password a stable, renderer-detectable error.

use tauri::AppHandle;
use zeroize::Zeroizing;

use crate::AppError;

pub(crate) async fn request(
    app: &AppHandle,
    operation: &'static str,
    provider: &'static str,
) -> Result<Zeroizing<String>, AppError> {
    let result = crate::services::security::backup_password(app);
    match &result {
        Ok(_) => crate::services::logging::debug(
            app,
            "security",
            format!("remote backup password loaded operation={operation} provider={provider}"),
        ),
        Err(error) => crate::services::logging::warn(
            app,
            "security",
            format!("remote backup password unavailable operation={operation} provider={provider} error={error}"),
        ),
    }
    result
}
