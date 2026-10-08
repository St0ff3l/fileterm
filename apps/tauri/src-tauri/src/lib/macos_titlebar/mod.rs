thread_local! {
    static MACOS_TITLEBAR_OBSERVERS: std::cell::RefCell<Vec<objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_foundation::NSObjectProtocol>>>> = const { std::cell::RefCell::new(Vec::new()) };
    static MACOS_TITLEBAR_LAYOUT_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

static MACOS_TITLEBAR_ZOOM: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(100);

struct MacosTitlebarLayoutGuard;

impl Drop for MacosTitlebarLayoutGuard {
    fn drop(&mut self) {
        MACOS_TITLEBAR_LAYOUT_ACTIVE.with(|active| active.set(false));
    }
}

fn refresh_macos_titlebar_zoom(app: &AppHandle<Wry>) {
    let percent = crate::commands::app_get_ui_preferences(app.clone())
        .map(|preferences| preferences.ui_zoom_percent)
        .unwrap_or(100);
    MACOS_TITLEBAR_ZOOM.store(percent, Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
fn calibrate_macos_traffic_lights(window: &WebviewWindow<Wry>) -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSView, NSWindowButton, NSWindowStyleMask};
    use objc2_quartz_core::CATransaction;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    if MACOS_TITLEBAR_LAYOUT_ACTIVE.with(|active| active.replace(true)) {
        return false;
    }
    let _guard = MacosTitlebarLayoutGuard;
    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return false;
    };
    let Some(_main_thread) = MainThreadMarker::new() else {
        return false;
    };

    // Tauri exposes the AppKit view through raw-window-handle. AppKit objects
    // are main-thread-only. Setup and native layout notifications both run here.
    let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
    let Some(ns_window) = view.window() else {
        return false;
    };
    // AppKit owns the auto-hiding title bar while fully fullscreen. Restore
    // the custom geometry synchronously as the window returns to normal.
    if ns_window
        .styleMask()
        .contains(NSWindowStyleMask::FullScreen)
    {
        return true;
    }
    let Some(close) = ns_window.standardWindowButton(NSWindowButton::CloseButton) else {
        return false;
    };
    let Some(miniaturize) = ns_window.standardWindowButton(NSWindowButton::MiniaturizeButton)
    else {
        return false;
    };
    let Some(zoom) = ns_window.standardWindowButton(NSWindowButton::ZoomButton) else {
        return false;
    };
    // Standard window buttons are retained above and remain attached to the
    // NSWindow title-bar hierarchy for the duration of this main-thread call.
    let Some(close_superview) = (unsafe { close.superview() }) else {
        return false;
    };
    let Some(miniaturize_superview) = (unsafe { miniaturize.superview() }) else {
        return false;
    };
    let Some(zoom_superview) = (unsafe { zoom.superview() }) else {
        return false;
    };

    let buttons = [
        (&close, &close_superview),
        (&miniaturize, &miniaturize_superview),
        (&zoom, &zoom_superview),
    ];
    let window_height = ns_window.frame().size.height;
    // WebView page zoom scales the renderer's 48px title bar, while AppKit
    // stays in logical points. Use the same persisted zoom for native layout.
    let ui_zoom_percent = MACOS_TITLEBAR_ZOOM.load(Ordering::Relaxed);
    let titlebar_height = macos_renderer_titlebar_height(ui_zoom_percent);
    let Some(titlebar_container) = (unsafe { close_superview.superview() }) else {
        return false;
    };

    CATransaction::begin();
    CATransaction::setDisableActions(true);
    // Match the native title-bar container to the renderer title bar before
    // positioning its buttons. Moving only the buttons leaves AppKit's group
    // hover region at the original title-bar height (above the visible lights).
    // Tao's inset_traffic_lights uses the same native container adjustment.
    let mut titlebar_frame = titlebar_container.frame();
    titlebar_frame.size.height = titlebar_height;
    titlebar_frame.origin.y = window_height - titlebar_height;
    let mut changed = titlebar_container.frame() != titlebar_frame;
    if changed {
        titlebar_container.setFrame(titlebar_frame);
        titlebar_container.layoutSubtreeIfNeeded();
    }
    for (index, (button, button_superview)) in buttons.into_iter().enumerate() {
        // The design target is expressed in window coordinates, independent
        // of AppKit's Debug/Release title-bar container geometry. Convert that
        // absolute center into each native button's own superview before
        // assigning its frame.

        let (target_center_x, target_center_y) =
            macos_traffic_light_target_center(window_height, titlebar_height, index);
        let mut target_center_in_window = button.frame().origin;
        target_center_in_window.x = target_center_x;
        target_center_in_window.y = target_center_y;
        let target_center = button_superview.convertPoint_fromView(target_center_in_window, None);

        // Keep AppKit's native frame/bounds and drawing geometry. A standard
        // title-bar button is not a generic NSButton: sizeToFit, forcing a
        // square frame, or scaling its layer can distort its bezel on macOS 15.
        let frame = button.frame();
        let mut origin = frame.origin;
        origin.x = target_center.x - frame.size.width / 2.0;
        origin.y = target_center.y - frame.size.height / 2.0;
        if (origin.x - frame.origin.x).abs() > 0.01 || (origin.y - frame.origin.y).abs() > 0.01 {
            button.setFrameOrigin(origin);
            button.updateTrackingAreas();
            NSView::setNeedsDisplay(button, true);
            changed = true;
        }
    }
    // Native hover glyphs are managed by the title-bar hierarchy as well as
    // the buttons. Refresh both levels after all frame changes are complete.
    if changed {
        close_superview.updateTrackingAreas();
        miniaturize_superview.updateTrackingAreas();
        zoom_superview.updateTrackingAreas();
        titlebar_container.updateTrackingAreas();
    }
    CATransaction::commit();

    true
}

pub(crate) fn schedule_macos_traffic_light_recalibration(app: &AppHandle<Wry>) {
    refresh_macos_titlebar_zoom(app);
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let calibration_window = window.clone();
    let _ = window.run_on_main_thread(move || {
        let _ = calibrate_macos_traffic_lights(&calibration_window);
    });
}

#[cfg(target_os = "macos")]
fn install_macos_titlebar_observers(window: &WebviewWindow<Wry>) {
    use objc2_app_kit::{
        NSView, NSViewFrameDidChangeNotification, NSWindowButton,
        NSWindowDidExitFullScreenNotification, NSWindowDidResizeNotification,
        NSWindowDidUpdateNotification,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    // Called from Tauri setup on AppKit's main thread.
    let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
    let Some(native_window) = view.window() else {
        return;
    };
    for name in unsafe {
        [
            NSWindowDidUpdateNotification,
            NSWindowDidResizeNotification,
            NSWindowDidExitFullScreenNotification,
        ]
    } {
        observe_macos_titlebar_layout(window, name, &native_window);
    }
    if let Some(close) = native_window.standardWindowButton(NSWindowButton::CloseButton) {
        if let Some(titlebar) = unsafe { close.superview() } {
            titlebar.setPostsFrameChangedNotifications(true);
            observe_macos_titlebar_layout(
                window,
                unsafe { NSViewFrameDidChangeNotification },
                &titlebar,
            );
            if let Some(container) = unsafe { titlebar.superview() } {
                container.setPostsFrameChangedNotifications(true);
                observe_macos_titlebar_layout(
                    window,
                    unsafe { NSViewFrameDidChangeNotification },
                    &container,
                );
            }
        }
    }
}

fn observe_macos_titlebar_layout(
    window: &WebviewWindow<Wry>,
    name: &objc2_foundation::NSNotificationName,
    object: &objc2::runtime::AnyObject,
) {
    use block2::RcBlock;
    use objc2_foundation::{NSNotificationCenter, NSOperationQueue};

    let window = window.clone();
    let callback = RcBlock::new(
        move |_notification: std::ptr::NonNull<objc2_foundation::NSNotification>| {
            let _ = calibrate_macos_traffic_lights(&window);
        },
    );
    // Each observed object belongs to the main-window hierarchy. Main-queue
    // delivery and the layout guard prevent recursive frame notifications.
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(name),
            Some(object),
            Some(&NSOperationQueue::mainQueue()),
            &callback,
        )
    };
    MACOS_TITLEBAR_OBSERVERS.with(|observers| observers.borrow_mut().push(observer));
}

fn remove_macos_titlebar_observers() {
    let center = objc2_foundation::NSNotificationCenter::defaultCenter();
    MACOS_TITLEBAR_OBSERVERS.with(|observers| {
        for observer in observers.borrow_mut().drain(..) {
            // Registered on the main thread; removed on the main-thread Exit event.
            unsafe {
                center.removeObserver((*observer).as_ref());
            }
        }
    });
}
