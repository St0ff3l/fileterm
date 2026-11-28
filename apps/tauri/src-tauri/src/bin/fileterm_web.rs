use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use fileterm_lib::runtime::{default_web_directories, AppHandle, WebRuntime};
use fileterm_lib::web::{build_router, initialize_runtime, shutdown_runtime, WebServerState};
use tokio::net::TcpListener;

const DEFAULT_WEB_BIND: &str = "127.0.0.1:18420";
const DEFAULT_WEB_DIST_DIR: &str = "./dist";

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        } else {
            std::future::pending::<()>().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

#[tokio::main]
async fn main() {
    let dist_dir = std::env::var_os("FILETERM_WEB_DIST_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_WEB_DIST_DIR));

    let (data_dir, config_dir, home_dir) = default_web_directories();
    let runtime = WebRuntime::new(data_dir, config_dir, home_dir);
    let app = AppHandle::from_web(Arc::clone(&runtime));

    if let Err(error) = initialize_runtime(&app).await {
        eprintln!("Failed to initialize FileTerm Web runtime: {error}");
        std::process::exit(1);
    }

    let session_token = std::env::var("FILETERM_WEB_SESSION_SECRET")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let router = build_router(WebServerState::new(
        app.clone(),
        runtime,
        dist_dir,
        session_token,
    ));

    let result = serve(router).await;
    shutdown_runtime(&app).await;
    if let Err(error) = result {
        eprintln!("FileTerm Web server exited with error: {error}");
        std::process::exit(1);
    }
}

async fn serve(router: axum::Router) -> std::io::Result<()> {
    #[cfg(unix)]
    if let Some(path) = std::env::var_os("FILETERM_WEB_SOCKET").filter(|value| !value.is_empty()) {
        let listener = tokio::net::UnixListener::bind(PathBuf::from(path))?;
        return axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await;
    }

    let raw = std::env::var("FILETERM_WEB_BIND").unwrap_or_else(|_| DEFAULT_WEB_BIND.into());
    let address: SocketAddr = raw.parse().map_err(std::io::Error::other)?;
    if !address.ip().is_loopback() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "The development HTTP listener must use a loopback address",
        ));
    }
    let listener = TcpListener::bind(address).await?;
    println!(
        "FileTerm Web development listener on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
}
