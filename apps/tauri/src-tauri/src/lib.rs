#[cfg(all(
    feature = "desktop",
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
mod app_icon;
pub mod commands;
#[cfg(all(feature = "desktop", target_os = "macos"))]
mod macos_icon;
pub mod runtime;
pub mod services;
pub mod sessions;
pub mod storage;
#[cfg(feature = "web")]
pub mod web;

#[cfg(feature = "desktop")]
pub(crate) fn runtime_app(app: &tauri::AppHandle) -> crate::runtime::AppHandle {
    crate::runtime::AppHandle::from_tauri(app.clone())
}

pub fn run_mcp_stdio(arguments: &[String]) -> Result<(), String> {
    crate::services::mcp::run_stdio(arguments)
}

pub fn run_cli(arguments: &[String]) -> Result<(), String> {
    crate::services::mcp::run_cli(arguments)
}

#[cfg(feature = "desktop")]
use crate::commands::OpenWindowInput;
#[cfg(all(feature = "desktop", target_os = "linux"))]
use gtk::prelude::GtkWindowExt;
#[cfg(feature = "desktop")]
use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::AtomicBool, atomic::Ordering, Mutex},
};
#[cfg(all(
    feature = "desktop",
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
use tauri::image::Image;
#[cfg(all(feature = "desktop", not(target_os = "linux")))]
use tauri::menu::{CheckMenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
#[cfg(feature = "desktop")]
use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::Color,
    AppHandle, Emitter, LogicalPosition, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent, Wry,
};
use thiserror::Error;
#[cfg(feature = "desktop")]
use tokio::sync::oneshot;
#[cfg(feature = "desktop")]
use url::form_urlencoded::Serializer;
#[cfg(all(feature = "desktop", target_os = "windows"))]
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2Settings3, ICoreWebView2Settings5,
};
#[cfg(all(feature = "desktop", target_os = "windows"))]
use windows::core::Interface;

include!("lib/error.rs");
#[cfg(feature = "desktop")]
include!("lib/state.rs");
#[cfg(feature = "desktop")]
include!("lib/menu.rs");
#[cfg(feature = "desktop")]
include!("lib/platform.rs");
#[cfg(all(feature = "desktop", target_os = "macos"))]
include!("lib/macos_titlebar/mod.rs");
#[cfg(feature = "desktop")]
include!("lib/window_geometry.rs");
#[cfg(feature = "desktop")]
include!("lib/windows.rs");
#[cfg(feature = "desktop")]
include!("lib/runtime.rs");
#[cfg(all(feature = "desktop", test))]
include!("lib/tests/mod.rs");
