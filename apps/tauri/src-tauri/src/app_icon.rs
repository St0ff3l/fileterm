#[cfg(any(target_os = "linux", target_os = "windows"))]
use tauri::image::Image;
use tauri::AppHandle;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use tauri::Manager;

#[cfg(any(target_os = "linux", test))]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(any(target_os = "linux", target_os = "windows", test))]
pub(crate) fn icon_bytes(variant: &str) -> &'static [u8] {
    platform_icon_bytes(variant, 1024)
}

#[cfg(any(target_os = "windows", target_os = "linux", test))]
pub(crate) fn platform_icon_bytes(variant: &str, size: u16) -> &'static [u8] {
    match variant {
        "material" if size == 1024 => include_bytes!("../icons/app/shared/material-1024.png"),
        "clean" if size == 1024 => include_bytes!("../icons/app/shared/clean-1024.png"),
        _ if size == 1024 => include_bytes!("../icons/app/shared/classic-1024.png"),
        "material" if size == 256 => {
            include_bytes!("../icons/app/shared/material-256.png")
        }
        "clean" if size == 256 => {
            include_bytes!("../icons/app/shared/clean-256.png")
        }
        "material" => {
            include_bytes!("../icons/app/shared/material-512.png")
        }
        "clean" => include_bytes!("../icons/app/shared/clean-512.png"),
        _ if size == 256 => {
            include_bytes!("../icons/app/shared/classic-256.png")
        }
        _ => include_bytes!("../icons/app/shared/classic-512.png"),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_icon_bytes(variant: &str) -> &'static [u8] {
    match variant {
        "material" => include_bytes!("../../src/renderer/assets/app-icons/dock-material.png"),
        "clean" => include_bytes!("../../src/renderer/assets/app-icons/dock-clean.png"),
        _ => include_bytes!("../../src/renderer/assets/app-icons/dock-classic.png"),
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn window_icon_image(variant: &str) -> Result<Image<'static>, String> {
    Image::from_bytes(icon_bytes(variant)).map_err(|error| error.to_string())
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn set_window_icon(window: &tauri::WebviewWindow, variant: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::set_window_icon(window, variant)
    }

    #[cfg(target_os = "linux")]
    {
        window
            .set_icon(window_icon_image(variant)?)
            .map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "linux")]
fn tray_icon_bytes(variant: &str) -> &'static [u8] {
    match variant {
        "material" => include_bytes!("../icons/tray/linux/material-32.png"),
        "clean" => include_bytes!("../icons/tray/linux/clean-32.png"),
        _ => include_bytes!("../icons/tray/linux/classic-32.png"),
    }
}

#[cfg(target_os = "windows")]
fn tray_icon_bytes(variant: &str) -> &'static [u8] {
    platform_icon_bytes(variant, 256)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn tray_icon_image(variant: &str) -> Result<Image<'static>, String> {
    Image::from_bytes(tray_icon_bytes(variant)).map_err(|error| error.to_string())
}

pub fn set_app_icon(app: &AppHandle, variant: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    crate::macos_icon::set_app_icon(app, variant)?;

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        for window in app.webview_windows().values() {
            set_window_icon(window, variant)?;
        }

        if let Some(tray) = app.tray_by_id("main") {
            tray.set_icon(Some(tray_icon_image(variant)?))
                .map_err(|error| error.to_string())?;
        }

        #[cfg(target_os = "windows")]
        windows::refresh_shortcut_icons(app, variant);
    }

    #[cfg(target_os = "linux")]
    linux::set_launcher_icon(app, variant)?;

    Ok(())
}

#[cfg(test)]
mod tests;
