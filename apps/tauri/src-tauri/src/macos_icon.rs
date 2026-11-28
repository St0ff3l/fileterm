use objc2::{runtime::AnyObject, AllocAnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSAboutPanelOptionApplicationIcon, NSApplication, NSImage, NSWorkspace,
    NSWorkspaceIconCreationOptions,
};
use objc2_foundation::{NSData, NSDictionary, NSString, NSUserDefaults};
use std::path::Path;
use tauri::AppHandle;

const APP_ICON_PREFERENCE_KEY: &str = "app_icon";

fn dock_icon(variant: &str) -> Option<objc2::rc::Retained<NSImage>> {
    let data = NSData::with_bytes(crate::app_icon::macos_icon_bytes(variant));
    NSImage::initWithData(NSImage::alloc(), &data)
}

pub fn set_app_icon(app: &AppHandle, variant: &str) -> Result<(), String> {
    let variant = variant.to_owned();
    let logging_app = app.clone();

    app.run_on_main_thread(move || {
        // SAFETY: Tauri runs this closure on the AppKit main thread.
        let main_thread = unsafe { MainThreadMarker::new_unchecked() };
        let Some(icon) = dock_icon(&variant) else {
            crate::services::logging::warn(
                &crate::runtime_app(&logging_app),
                "app-icon",
                "unable to decode embedded Dock icon",
            );
            return;
        };
        let defaults = NSUserDefaults::standardUserDefaults();
        let preference_key = NSString::from_str(APP_ICON_PREFERENCE_KEY);
        let selected_variant = NSString::from_str(&variant);
        // The Dock tile plug-in runs in Dock and reads this app preference while FileTerm is closed.
        unsafe { defaults.setObject_forKey(Some(&selected_variant), &preference_key) };

        if let Some(bundle_path) = std::env::current_exe().ok().and_then(|executable| {
            executable
                .ancestors()
                .find(|path| path.extension().is_some_and(|extension| extension == "app"))
                .map(Path::to_path_buf)
        }) {
            let bundle_path = NSString::from_str(&bundle_path.to_string_lossy());
            let workspace = NSWorkspace::sharedWorkspace();
            let options = NSWorkspaceIconCreationOptions(0);

            // Clear Finder metadata left by earlier versions; keep this signed app bundle's icon unchanged.
            if !workspace.setIcon_forFile_options(None, &bundle_path, options) {
                crate::services::logging::warn(
                    &crate::runtime_app(&logging_app),
                    "app-icon",
                    "failed to clear a previous Finder icon override",
                );
            }
        }

        // SAFETY: The image is decoded from a bundled PNG and AppKit's setter accepts it.
        unsafe {
            NSApplication::sharedApplication(main_thread).setApplicationIconImage(Some(&icon))
        };
    })
    .map_err(|error| error.to_string())
}

pub(crate) fn show_about_panel(app: &AppHandle) -> Result<(), String> {
    app.run_on_main_thread(|| {
        // SAFETY: Tauri runs this closure on the AppKit main thread.
        let main_thread = unsafe { MainThreadMarker::new_unchecked() };
        let application = NSApplication::sharedApplication(main_thread);

        let variant = NSUserDefaults::standardUserDefaults()
            .stringForKey(&NSString::from_str(APP_ICON_PREFERENCE_KEY))
            .map(|variant| variant.to_string())
            .unwrap_or_default();

        // With no icon option, AppKit looks up the static bundle artwork.
        if let Some(icon) = dock_icon(&variant) {
            // SAFETY: AppKit's static key expects an NSImage value.
            unsafe {
                let options = NSDictionary::<NSString, AnyObject>::from_slices(
                    &[NSAboutPanelOptionApplicationIcon],
                    &[&icon],
                );
                application.orderFrontStandardAboutPanelWithOptions(&options);
            }
        } else {
            application.orderFrontStandardAboutPanel(None);
        }
    })
    .map_err(|error| format!("unable to open the About panel: {error}"))
}

#[cfg(test)]
mod tests {
    use super::dock_icon;

    #[test]
    fn dock_artwork_used_by_the_about_panel_decodes_at_native_resolution() {
        for variant in ["classic", "material", "clean"] {
            let icon = dock_icon(variant).expect("embedded Dock icon must decode");
            let size = icon.size();
            assert_eq!((size.width, size.height), (1024.0, 1024.0), "{variant}");
        }
    }
}
