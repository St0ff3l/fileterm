pub mod commands;
pub mod services;
pub mod sessions;
pub mod storage;

pub fn run_mcp_stdio(arguments: &[String]) -> Result<(), String> {
    crate::services::mcp::run_stdio(arguments)
}

pub fn run_cli(arguments: &[String]) -> Result<(), String> {
    crate::services::mcp::run_cli(arguments)
}

use crate::commands::OpenWindowInput;
#[cfg(target_os = "linux")]
use gtk::prelude::GtkWindowExt;
#[cfg(target_os = "macos")]
use std::sync::atomic::AtomicU64;
use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::AtomicBool, atomic::Ordering, Mutex},
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use tauri::image::Image;
#[cfg(not(target_os = "linux"))]
use tauri::menu::{CheckMenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::Color,
    AppHandle, Emitter, LogicalPosition, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent, Wry,
};
use thiserror::Error;
use tokio::sync::oneshot;
use url::form_urlencoded::Serializer;
#[cfg(target_os = "windows")]
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{ICoreWebView2Settings3, ICoreWebView2Settings5},
    ZoomFactorChangedEventHandler,
};
#[cfg(target_os = "windows")]
use windows::core::Interface;

include!("lib/error.rs");
include!("lib/state.rs");
include!("lib/menu.rs");
include!("lib/platform.rs");
include!("lib/windows.rs");
include!("lib/runtime.rs");
include!("lib/tests.rs");
