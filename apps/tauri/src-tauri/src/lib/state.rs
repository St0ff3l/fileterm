/// Tracks file-editor close requests that are waiting for a renderer answer.
///
/// A Tauri `CloseRequested` event has no Promise to resolve like Electron's
/// `close` handler does. Keeping this state in main makes cancellation a real
/// lifecycle transition instead of a renderer-only no-op, and prevents two
/// close dialogs from being emitted for the same editor window.
#[derive(Default)]
struct FileEditorCloseState {
    pending_labels: HashSet<String>,
    waiters: HashMap<String, Vec<oneshot::Sender<bool>>>,
}

#[derive(Default)]
pub(crate) struct FileEditorCloseRegistry {
    state: Mutex<FileEditorCloseState>,
}

#[derive(Default)]
pub(crate) struct QuitPreparationRegistry {
    in_progress: AtomicBool,
}

/// Windows hidden together with the main window must be restored together as
/// well. This mirrors Electron's `childWindowsHiddenWithMain` lifecycle and
/// avoids losing standalone managers/editors after a tray hide/show cycle.
#[derive(Default)]
struct HiddenWithMainRegistry {
    labels: Mutex<HashSet<String>>,
}

#[cfg(target_os = "macos")]
static MACOS_TRAFFIC_LIGHTS_CALIBRATED: AtomicBool = AtomicBool::new(false);

/// A fullscreen transition emits several resize notifications while AppKit is
/// still rebuilding the title-bar hierarchy. Only the final notification may
/// position the traffic lights, otherwise they retain an obsolete y-coordinate
/// when the window returns from fullscreen.
#[cfg(target_os = "macos")]
static MACOS_TRAFFIC_LIGHT_RECALIBRATION_GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
const MACOS_RENDERER_TITLEBAR_HEIGHT: f64 = 48.0;
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_FRAME_SIZE: f64 = 14.0;
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_DRAWN_SIZE: f64 = 12.0;
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_LEFT_INSET: f64 = 20.0;
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_CENTER_SPACING: f64 = 23.0;
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_RECALIBRATION_DELAY_MS: u64 = 140;

#[cfg(target_os = "macos")]
fn macos_traffic_light_target_center(window_height: f64, index: usize) -> (f64, f64) {
    (
        MACOS_TRAFFIC_LIGHT_LEFT_INSET
            + MACOS_TRAFFIC_LIGHT_FRAME_SIZE / 2.0
            + index as f64 * MACOS_TRAFFIC_LIGHT_CENTER_SPACING,
        window_height - MACOS_RENDERER_TITLEBAR_HEIGHT / 2.0,
    )
}

impl FileEditorCloseRegistry {
    fn request(&self, label: &str) -> bool {
        self.state
            .lock()
            .expect("file editor close registry lock poisoned")
            .pending_labels
            .insert(label.to_string())
    }

    fn request_and_wait(&self, label: &str) -> (bool, oneshot::Receiver<bool>) {
        let (sender, receiver) = oneshot::channel();
        let mut state = self
            .state
            .lock()
            .expect("file editor close registry lock poisoned");
        let should_emit = state.pending_labels.insert(label.to_string());
        state
            .waiters
            .entry(label.to_string())
            .or_default()
            .push(sender);
        (should_emit, receiver)
    }

    fn resolve(&self, label: &str, approved: bool) {
        let waiters = {
            let mut state = self
                .state
                .lock()
                .expect("file editor close registry lock poisoned");
            state.pending_labels.remove(label);
            state.waiters.remove(label).unwrap_or_default()
        };
        for waiter in waiters {
            let _ = waiter.send(approved);
        }
    }
}

impl QuitPreparationRegistry {
    pub(crate) fn try_begin(&self) -> bool {
        self.in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn cancel(&self) {
        self.in_progress.store(false, Ordering::Release);
    }
}

pub(crate) fn request_file_editor_close(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>) -> bool {
    app.state::<FileEditorCloseRegistry>()
        .request(window.label())
}

pub(crate) fn resolve_file_editor_close(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>) {
    app.state::<FileEditorCloseRegistry>()
        .resolve(window.label(), true);
}

pub(crate) fn cancel_file_editor_close(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>) {
    app.state::<FileEditorCloseRegistry>()
        .resolve(window.label(), false);
}

/// Ask every standalone editor to resolve its dirty state before the app tears
/// down transfers or sessions. A cancel from any editor aborts the whole quit.
pub(crate) async fn request_file_editors_for_quit(app: &AppHandle<Wry>) -> Result<bool, AppError> {
    let mut labels = app
        .webview_windows()
        .into_keys()
        .filter(|label| label.starts_with("file-editor-"))
        .collect::<Vec<_>>();
    labels.sort();

    for label in labels {
        let Some(window) = app.get_webview_window(&label) else {
            continue;
        };
        let (should_emit, resolution) = app
            .state::<FileEditorCloseRegistry>()
            .request_and_wait(&label);
        if should_emit {
            if let Err(error) = window.emit("app:file-editor-close-request", ()) {
                // Do not leave a stale pending label/waiter behind. A later
                // quit request must be able to ask this editor again.
                app.state::<FileEditorCloseRegistry>()
                    .resolve(&label, false);
                return Err(AppError::Window(error.to_string()));
            }
        }
        match resolution.await {
            Ok(true) => {}
            Ok(false) => return Ok(false),
            Err(_) if app.get_webview_window(&label).is_none() => {}
            Err(_) => {
                return Err(AppError::Window(format!(
                    "File editor close request ended without a decision: {label}"
                )))
            }
        }
    }
    Ok(true)
}
