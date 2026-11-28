mod channel;
mod web_runtime;

use std::future::Future;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use semver::Version;
use serde::Serialize;

use crate::services::workspace::WorkspaceState;
use crate::AppError;

pub use channel::StreamChannel;
pub use web_runtime::{remove_staged_path, RuntimeEvent, StagedDownload, WebRuntime, WebServerMessage};

const WEB_APP_IDENTIFIER: &str = "com.fileterm.desktop";

#[derive(Clone)]
pub struct AppHandle {
    inner: AppHandleInner,
}

#[derive(Clone)]
enum AppHandleInner {
    #[cfg(feature = "desktop")]
    Desktop(tauri::AppHandle),
    Web(Arc<WebRuntime>),
}

#[derive(Clone, Debug)]
pub enum EventTarget {
    WebviewWindow(String),
}

impl EventTarget {
    pub fn webview_window(label: impl Into<String>) -> Self {
        Self::WebviewWindow(label.into())
    }
}

pub struct AppConfigRef {
    pub identifier: String,
}

pub struct PackageInfoRef {
    pub version: Version,
}

pub struct PathResolver<'a> {
    app: &'a AppHandle,
}

impl PathResolver<'_> {
    pub fn app_data_dir(&self) -> Result<PathBuf, AppError> {
        match &self.app.inner {
            #[cfg(feature = "desktop")]
            AppHandleInner::Desktop(app) => {
                use tauri::Manager;
                app.path()
                    .app_data_dir()
                    .map_err(|error| AppError::Storage(error.to_string()))
            }
            AppHandleInner::Web(runtime) => Ok(runtime.data_dir.clone()),
        }
    }

    pub fn app_config_dir(&self) -> Result<PathBuf, AppError> {
        match &self.app.inner {
            #[cfg(feature = "desktop")]
            AppHandleInner::Desktop(app) => {
                use tauri::Manager;
                app.path()
                    .app_config_dir()
                    .map_err(|error| AppError::Storage(error.to_string()))
            }
            AppHandleInner::Web(runtime) => Ok(runtime.config_dir.clone()),
        }
    }

    pub fn home_dir(&self) -> Result<PathBuf, AppError> {
        match &self.app.inner {
            #[cfg(feature = "desktop")]
            AppHandleInner::Desktop(app) => {
                use tauri::Manager;
                app.path()
                    .home_dir()
                    .map_err(|error| AppError::Storage(error.to_string()))
            }
            AppHandleInner::Web(runtime) => Ok(runtime.home_dir.clone()),
        }
    }
}

pub trait RuntimeState: Sized + Send + Sync + 'static {
    fn from_web_runtime(runtime: &WebRuntime) -> &Self;
}

impl RuntimeState for WorkspaceState {
    fn from_web_runtime(runtime: &WebRuntime) -> &Self {
        &runtime.workspace
    }
}

pub struct StateRef<'a, T: 'static> {
    inner: StateRefInner<'a, T>,
}

enum StateRefInner<'a, T: 'static> {
    #[cfg(feature = "desktop")]
    Desktop(tauri::State<'a, T>),
    Web(&'a T),
}

pub type State<'a, T> = StateRef<'a, T>;

impl<T: 'static> Deref for StateRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match &self.inner {
            #[cfg(feature = "desktop")]
            StateRefInner::Desktop(state) => state,
            StateRefInner::Web(state) => state,
        }
    }
}

impl AppHandle {
    #[cfg(feature = "desktop")]
    pub fn from_tauri(handle: tauri::AppHandle) -> Self {
        Self {
            inner: AppHandleInner::Desktop(handle),
        }
    }

    pub fn from_web(runtime: Arc<WebRuntime>) -> Self {
        Self {
            inner: AppHandleInner::Web(runtime),
        }
    }

    #[cfg(feature = "desktop")]
    pub fn tauri(&self) -> Option<&tauri::AppHandle> {
        match &self.inner {
            AppHandleInner::Desktop(handle) => Some(handle),
            AppHandleInner::Web(_) => None,
        }
    }

    pub fn is_web(&self) -> bool {
        matches!(self.inner, AppHandleInner::Web(_))
    }

    pub fn web_runtime(&self) -> Option<&Arc<WebRuntime>> {
        match &self.inner {
            #[cfg(feature = "desktop")]
            AppHandleInner::Desktop(_) => None,
            AppHandleInner::Web(runtime) => Some(runtime),
        }
    }

    pub fn state<T: RuntimeState>(&self) -> StateRef<'_, T> {
        match &self.inner {
            #[cfg(feature = "desktop")]
            AppHandleInner::Desktop(handle) => {
                use tauri::Manager;
                StateRef {
                    inner: StateRefInner::Desktop(handle.state::<T>()),
                }
            }
            AppHandleInner::Web(runtime) => StateRef {
                inner: StateRefInner::Web(T::from_web_runtime(runtime)),
            },
        }
    }

    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), AppError> {
        match &self.inner {
            #[cfg(feature = "desktop")]
            AppHandleInner::Desktop(handle) => {
                use tauri::Emitter;
                handle
                    .emit(event, payload)
                    .map_err(|error| AppError::Command(error.to_string()))
            }
            AppHandleInner::Web(runtime) => {
                let value = serde_json::to_value(payload)
                    .map_err(|error| AppError::Serialization(error.to_string()))?;
                runtime.emit_event(None, event, value);
                Ok(())
            }
        }
    }

    pub fn emit_to<S: Serialize + Clone>(
        &self,
        target: EventTarget,
        event: &str,
        payload: S,
    ) -> Result<(), AppError> {
        let EventTarget::WebviewWindow(label) = target;
        match &self.inner {
            #[cfg(feature = "desktop")]
            AppHandleInner::Desktop(handle) => {
                use tauri::Emitter;
                handle
                    .emit_to(tauri::EventTarget::webview_window(label), event, payload)
                    .map_err(|error| AppError::Command(error.to_string()))
            }
            AppHandleInner::Web(runtime) => {
                let value = serde_json::to_value(payload)
                    .map_err(|error| AppError::Serialization(error.to_string()))?;
                runtime.emit_event(Some(label), event, value);
                Ok(())
            }
        }
    }

    pub fn path(&self) -> PathResolver<'_> {
        PathResolver { app: self }
    }

    pub fn config(&self) -> AppConfigRef {
        match &self.inner {
            #[cfg(feature = "desktop")]
            AppHandleInner::Desktop(handle) => AppConfigRef {
                identifier: handle.config().identifier.clone(),
            },
            AppHandleInner::Web(_) => AppConfigRef {
                identifier: WEB_APP_IDENTIFIER.to_string(),
            },
        }
    }

    pub fn package_info(&self) -> PackageInfoRef {
        match &self.inner {
            #[cfg(feature = "desktop")]
            AppHandleInner::Desktop(handle) => PackageInfoRef {
                version: handle.package_info().version.clone(),
            },
            AppHandleInner::Web(_) => PackageInfoRef {
                version: Version::parse(env!("CARGO_PKG_VERSION"))
                    .unwrap_or_else(|_| Version::new(0, 0, 0)),
            },
        }
    }

    pub fn has_window(&self, label: &str) -> bool {
        match &self.inner {
            #[cfg(feature = "desktop")]
            AppHandleInner::Desktop(handle) => {
                use tauri::Manager;
                handle.get_webview_window(label).is_some()
            }
            AppHandleInner::Web(runtime) => runtime.has_connected_window(label),
        }
    }

    pub fn show_main_window(&self) {
        #[cfg(feature = "desktop")]
        if let AppHandleInner::Desktop(handle) = &self.inner {
            crate::show_main_window(handle);
        }
    }
}

pub fn default_web_directories() -> (PathBuf, PathBuf, PathBuf) {
    let home_dir = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let fallback = PathBuf::from("/data");
            if fallback.is_dir() {
                fallback
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
            }
        });
    let data_base = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir.join(".local/share"));
    let config_base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir.join(".config"));
    let data_dir = std::env::var_os("FILETERM_DATA_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| data_base.join(WEB_APP_IDENTIFIER));
    let config_dir = config_base.join(WEB_APP_IDENTIFIER);
    (data_dir, config_dir, home_dir)
}

pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(future);
        return;
    }
    #[cfg(feature = "desktop")]
    {
        tauri::async_runtime::spawn(future);
    }
    #[cfg(not(feature = "desktop"))]
    {
        panic!("crate::runtime::spawn called outside a Tokio runtime");
    }
}

pub fn spawn_with_handle<F, R>(future: F) -> JoinHandle<R>
where
    F: Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return JoinHandle::Tokio(handle.spawn(future));
    }
    #[cfg(feature = "desktop")]
    {
        JoinHandle::Tauri(tauri::async_runtime::spawn(future))
    }
    #[cfg(not(feature = "desktop"))]
    {
        panic!("crate::runtime::spawn_with_handle called outside a Tokio runtime");
    }
}

pub fn spawn_blocking<F, R>(task: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return JoinHandle::Tokio(handle.spawn_blocking(task));
    }
    #[cfg(feature = "desktop")]
    {
        JoinHandle::Tauri(tauri::async_runtime::spawn_blocking(task))
    }
    #[cfg(not(feature = "desktop"))]
    {
        panic!("crate::runtime::spawn_blocking called outside a Tokio runtime");
    }
}

pub fn block_on<F: Future>(future: F) -> F::Output {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return handle.block_on(future);
    }
    #[cfg(feature = "desktop")]
    {
        tauri::async_runtime::block_on(future)
    }
    #[cfg(not(feature = "desktop"))]
    {
        panic!("crate::runtime::block_on called outside a Tokio runtime");
    }
}

pub fn tokio_handle() -> tokio::runtime::Handle {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return handle;
    }
    #[cfg(feature = "desktop")]
    {
        tauri::async_runtime::handle().inner().clone()
    }
    #[cfg(not(feature = "desktop"))]
    {
        panic!("crate::runtime::tokio_handle called outside a Tokio runtime");
    }
}

pub enum JoinHandle<T> {
    Tokio(tokio::task::JoinHandle<T>),
    #[cfg(feature = "desktop")]
    Tauri(tauri::async_runtime::JoinHandle<T>),
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, String>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.get_mut() {
            JoinHandle::Tokio(handle) => std::pin::Pin::new(handle)
                .poll(cx)
                .map_err(|error| error.to_string()),
            #[cfg(feature = "desktop")]
            JoinHandle::Tauri(handle) => std::pin::Pin::new(handle)
                .poll(cx)
                .map_err(|error| error.to_string()),
        }
    }
}
