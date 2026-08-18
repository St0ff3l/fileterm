use std::cell::RefCell;
use std::path::PathBuf;
use std::ptr::null_mut;

use block2::RcBlock;
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject},
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{
    NSApp, NSBezierPath, NSColor, NSDragOperation, NSDraggingContext, NSDraggingItem,
    NSDraggingSession, NSDraggingSource, NSEvent, NSEventModifierFlags, NSEventType,
    NSFilePromiseProvider, NSFilePromiseProviderDelegate, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSImage, NSPasteboardWriting, NSStringDrawing, NSView,
};
use objc2_foundation::{
    NSAttributedStringKey, NSDictionary, NSError, NSMutableArray, NSOperationQueue, NSPoint,
    NSRect, NSSize, NSString, NSURL,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::{AppHandle, Manager, WebviewWindow};
use tokio::sync::oneshot;

use super::RemoteFileDragItem;
use crate::AppError;

const FILE_PROMISE_UTI: &str = "public.data";
const FOLDER_PROMISE_UTI: &str = "public.folder";

thread_local! {
    static ACTIVE_REMOTE_DRAG: RefCell<Option<ActiveRemoteDrag>> = const { RefCell::new(None) };
}

struct ActiveRemoteDrag {
    _session: Retained<NSDraggingSession>,
    _source: Retained<RemoteFileDragSource>,
    remaining_promises: usize,
    drag_ended: bool,
}

type CompletionBlock = block2::DynBlock<dyn Fn(*mut NSError)>;

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "FileTermRemoteFilePromiseDelegate"]
    #[ivars = RemoteFilePromiseDelegateIvars]
    struct RemoteFilePromiseDelegate;

    unsafe impl NSObjectProtocol for RemoteFilePromiseDelegate {}

    unsafe impl NSFilePromiseProviderDelegate for RemoteFilePromiseDelegate {
        #[unsafe(method_id(filePromiseProvider:fileNameForType:))]
        fn file_name_for_type(
            &self,
            _provider: &NSFilePromiseProvider,
            _file_type: &NSString,
        ) -> Retained<NSString> {
            NSString::from_str(&self.ivars().item.name)
        }

        #[unsafe(method(filePromiseProvider:writePromiseToURL:completionHandler:))]
        fn write_promise(
            &self,
            _provider: &NSFilePromiseProvider,
            url: &NSURL,
            completion_handler: &CompletionBlock,
        ) {
            let Some(destination) = url.path().map(|path| PathBuf::from(path.to_string())) else {
                complete_promise(
                    self.ivars().app.clone(),
                    completion_handler,
                    Err("Finder 没有提供有效的目标路径".to_string()),
                );
                return;
            };

            let item = self.ivars().item.clone();
            let app = self.ivars().app.clone();
            let tab_id = self.ivars().tab_id.clone();
            let completion = completion_handler.copy();
            let completion_raw = RcBlock::into_raw(completion) as usize;

            tauri::async_runtime::spawn(async move {
                let result = write_remote_item(&app, &tab_id, &item, destination).await;
                complete_promise_raw(app, completion_raw, result);
            });
        }

        #[unsafe(method_id(operationQueueForFilePromiseProvider:))]
        fn operation_queue(&self, _provider: &NSFilePromiseProvider) -> Retained<NSOperationQueue> {
            NSOperationQueue::mainQueue()
        }
    }
);

struct RemoteFilePromiseDelegateIvars {
    app: AppHandle,
    tab_id: String,
    item: RemoteFileDragItem,
}

impl RemoteFilePromiseDelegate {
    fn new(
        app: AppHandle,
        tab_id: String,
        item: RemoteFileDragItem,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RemoteFilePromiseDelegateIvars { app, tab_id, item });
        unsafe { msg_send![super(this), init] }
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "FileTermRemoteFileDragSource"]
    #[ivars = RemoteFileDragSourceIvars]
    struct RemoteFileDragSource;

    unsafe impl NSObjectProtocol for RemoteFileDragSource {}

    unsafe impl NSDraggingSource for RemoteFileDragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        unsafe fn dragging_session(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        unsafe fn dragging_session_ended(
            &self,
            _session: &NSDraggingSession,
            _screen_point: NSPoint,
            operation: NSDragOperation,
        ) {
            ACTIVE_REMOTE_DRAG.with(|active| {
                let mut active = active.borrow_mut();
                let Some(state) = active.as_mut() else {
                    return;
                };
                if operation == NSDragOperation::None {
                    active.take();
                    return;
                }
                state.drag_ended = true;
                if state.remaining_promises == 0 {
                    active.take();
                }
            });
        }
    }
);

struct RemoteFileDragSourceIvars {
    #[allow(dead_code)]
    delegates: Vec<Retained<RemoteFilePromiseDelegate>>,
}

impl RemoteFileDragSource {
    fn new(
        delegates: Vec<Retained<RemoteFilePromiseDelegate>>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RemoteFileDragSourceIvars { delegates });
        unsafe { msg_send![super(this), init] }
    }
}

pub(super) async fn start_remote_file_drag(
    app: &AppHandle,
    window_label: &str,
    tab_id: &str,
    items: Vec<RemoteFileDragItem>,
) -> Result<(), AppError> {
    validate_items(&items)?;

    let window = app
        .get_webview_window(window_label)
        .ok_or_else(|| AppError::Command("拖出窗口不存在".to_string()))?;
    let (result_sender, result_receiver) = oneshot::channel::<Result<(), String>>();
    let window_for_main = window.clone();
    let app_for_main = app.clone();
    let tab_id = tab_id.to_string();
    window
        .run_on_main_thread(move || {
            let result = start_on_main_thread(window_for_main, app_for_main, tab_id, items);
            let _ = result_sender.send(result);
        })
        .map_err(|error| AppError::Command(format!("切换到原生拖出线程失败：{error}")))?;

    result_receiver
        .await
        .map_err(|_| AppError::Command("原生拖出线程未响应".to_string()))?
        .map_err(AppError::Command)
}

fn validate_items(items: &[RemoteFileDragItem]) -> Result<(), AppError> {
    let mut names = std::collections::HashSet::new();
    for item in items {
        super::safe_drag_name(&item.name)?;
        if !matches!(item.item_type.as_str(), "file" | "folder") {
            return Err(AppError::Command("远程拖出项目类型无效".to_string()));
        }
        if !names.insert(item.name.clone()) {
            return Err(AppError::Command(format!(
                "拖出项目名称重复：{}",
                item.name
            )));
        }
    }
    Ok(())
}

fn start_on_main_thread(
    window: WebviewWindow,
    app: AppHandle,
    tab_id: String,
    items: Vec<RemoteFileDragItem>,
) -> Result<(), String> {
    let handle = window
        .window_handle()
        .map_err(|error| format!("读取原生窗口句柄失败：{error}"))?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return Err("当前窗口不是 macOS 原生窗口".to_string());
    };

    unsafe {
        let mtm = MainThreadMarker::new_unchecked();
        let ns_view = &*(appkit.ns_view.as_ptr() as *const NSView);
        let ns_window = ns_view
            .window()
            .ok_or_else(|| "获取 macOS 窗口失败".to_string())?;
        let content_view = ns_window
            .contentView()
            .ok_or_else(|| "获取 macOS 内容视图失败".to_string())?;
        let current_position = ns_window.mouseLocationOutsideOfEventStream();

        let dragging_items = NSMutableArray::new();
        let item_count = items.len();
        let mut delegates = Vec::with_capacity(items.len());
        for item in items {
            let image = make_drag_preview_image(&item);
            let image_size = image.size();
            let image_rect = NSRect::new(
                NSPoint::new(
                    current_position.x - image_size.width / 2.0,
                    current_position.y - image_size.height / 2.0,
                ),
                image_size,
            );
            let delegate =
                RemoteFilePromiseDelegate::new(app.clone(), tab_id.clone(), item.clone(), mtm);
            let delegate_for_provider = delegate.clone();
            delegates.push(delegate);

            let file_type = NSString::from_str(if item.item_type == "folder" {
                FOLDER_PROMISE_UTI
            } else {
                FILE_PROMISE_UTI
            });
            let delegate_protocol =
                ProtocolObject::<dyn NSFilePromiseProviderDelegate>::from_retained(
                    delegate_for_provider,
                );
            let provider = NSFilePromiseProvider::initWithFileType_delegate(
                NSFilePromiseProvider::alloc(),
                &file_type,
                &delegate_protocol,
            );
            let pasteboard_writer =
                ProtocolObject::<dyn NSPasteboardWriting>::from_retained(provider);
            let drag_item = NSDraggingItem::initWithPasteboardWriter(
                NSDraggingItem::alloc(),
                &pasteboard_writer,
            );
            drag_item.setDraggingFrame_contents(image_rect, Some(&*image));
            dragging_items.addObject(&*drag_item);
        }

        let current_event = NSApp(mtm).currentEvent();
        let timestamp = current_event.map(|event| event.timestamp()).unwrap_or(0.0);
        let drag_event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            NSEventType::LeftMouseDragged,
            current_position,
            NSEventModifierFlags::empty(),
            timestamp,
            ns_window.windowNumber(),
            None,
            0,
            1,
            1.0,
        )
        .ok_or_else(|| "创建原生拖动事件失败".to_string())?;
        let source = RemoteFileDragSource::new(delegates, mtm);
        let source_protocol = ProtocolObject::<dyn NSDraggingSource>::from_retained(source.clone());

        let session = content_view.beginDraggingSessionWithItems_event_source(
            &dragging_items,
            &drag_event,
            &source_protocol,
        );
        ACTIVE_REMOTE_DRAG.with(|active| {
            *active.borrow_mut() = Some(ActiveRemoteDrag {
                _session: session,
                _source: source,
                remaining_promises: item_count,
                drag_ended: false,
            });
        });
        Ok(())
    }
}

const DRAG_PREVIEW_HEIGHT: f64 = 28.0;
const DRAG_PREVIEW_ICON_SIZE: f64 = 16.0;
const DRAG_PREVIEW_MAX_WIDTH: f64 = 360.0;

#[allow(deprecated)]
fn make_drag_preview_image(item: &RemoteFileDragItem) -> Retained<NSImage> {
    let font = NSFont::systemFontOfSize(12.0);
    let text_color = NSColor::labelColor();
    let font_object: Retained<AnyObject> = font.into_super().into();
    let text_color_object: Retained<AnyObject> = text_color.clone().into_super().into();
    let attributes = unsafe {
        NSDictionary::<NSAttributedStringKey, AnyObject>::from_slices(
            &[NSFontAttributeName, NSForegroundColorAttributeName],
            &[&*font_object, &*text_color_object],
        )
    };

    let display_name = truncate_drag_name(&item.name, &attributes);
    let display_name = NSString::from_str(&display_name);
    let text_size = unsafe { display_name.sizeWithAttributes(Some(&attributes)) };
    let width =
        (10.0 + DRAG_PREVIEW_ICON_SIZE + 7.0 + text_size.width + 10.0).min(DRAG_PREVIEW_MAX_WIDTH);
    let size = NSSize::new(width, DRAG_PREVIEW_HEIGHT);
    let image = NSImage::initWithSize(NSImage::alloc(), size);

    image.lockFocus();

    let bounds = NSRect::new(NSPoint::new(0.0, 0.0), size);
    let background = NSColor::controlBackgroundColor().colorWithAlphaComponent(0.96);
    let background_path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, 5.0, 5.0);
    background.setFill();
    background_path.fill();

    let border = NSColor::separatorColor().colorWithAlphaComponent(0.9);
    let border_path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
        NSRect::new(
            NSPoint::new(0.5, 0.5),
            NSSize::new(width - 1.0, DRAG_PREVIEW_HEIGHT - 1.0),
        ),
        4.5,
        4.5,
    );
    border.setStroke();
    border_path.setLineWidth(1.0);
    border_path.stroke();

    let icon_name = NSString::from_str(if item.item_type == "folder" {
        "folder"
    } else {
        "doc"
    });
    if let Some(icon) =
        NSImage::imageWithSystemSymbolName_accessibilityDescription(&icon_name, None)
    {
        icon.setTemplate(true);
        NSColor::controlAccentColor().set();
        icon.drawInRect(NSRect::new(
            NSPoint::new(10.0, 6.0),
            NSSize::new(DRAG_PREVIEW_ICON_SIZE, DRAG_PREVIEW_ICON_SIZE),
        ));
    }

    unsafe {
        display_name.drawAtPoint_withAttributes(
            NSPoint::new(10.0 + DRAG_PREVIEW_ICON_SIZE + 7.0, 7.0),
            Some(&attributes),
        );
    }
    image.unlockFocus();
    image
}

fn truncate_drag_name(
    name: &str,
    attributes: &NSDictionary<NSAttributedStringKey, AnyObject>,
) -> String {
    let max_text_width = DRAG_PREVIEW_MAX_WIDTH - 10.0 - DRAG_PREVIEW_ICON_SIZE - 7.0 - 10.0;
    let mut candidate = name.to_string();
    loop {
        let candidate_string = NSString::from_str(&candidate);
        let text_size = unsafe { candidate_string.sizeWithAttributes(Some(attributes)) };
        if text_size.width <= max_text_width || candidate.chars().count() <= 1 {
            return candidate;
        }

        let mut chars: Vec<char> = candidate.chars().collect();
        chars.pop();
        if chars.last() != Some(&'…') {
            chars.pop();
            chars.push('…');
        }
        candidate = chars.into_iter().collect();
    }
}

async fn write_remote_item(
    app: &AppHandle,
    tab_id: &str,
    item: &RemoteFileDragItem,
    destination: PathBuf,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| "Finder 目标路径没有父目录".to_string())?;
    let target_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| item.name.clone());
    let local_directory = parent.to_string_lossy().into_owned();
    let transfer_id = match item.item_type.as_str() {
        "file" => {
            crate::services::transfers::create_download(
                app,
                tab_id.to_string(),
                item.path.clone(),
                local_directory,
                Some(target_name),
            )
            .await
        }
        "folder" => {
            crate::services::transfers::create_download_directory(
                app,
                tab_id.to_string(),
                item.path.clone(),
                local_directory,
                Some(target_name),
            )
            .await
        }
        _ => Err(AppError::Command("远程拖出项目类型无效".to_string())),
    }
    .map_err(|error| error.to_string())?;

    crate::services::transfers::wait_for_transfer(app, &transfer_id)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn complete_promise(
    app: AppHandle,
    completion_handler: &CompletionBlock,
    result: Result<(), String>,
) {
    let completion = completion_handler.copy();
    let completion_raw = RcBlock::into_raw(completion) as usize;
    complete_promise_raw(app, completion_raw, result);
}

fn complete_promise_raw(app: AppHandle, completion_raw: usize, result: Result<(), String>) {
    let _ = app.run_on_main_thread(move || {
        let completion = unsafe {
            RcBlock::<dyn Fn(*mut NSError)>::from_raw(completion_raw as *mut CompletionBlock)
        };
        let Some(completion) = completion else {
            return;
        };

        let error = result.err().map(|message| {
            let _ = message;
            let domain = NSString::from_str("com.fileterm.remote-drag");
            unsafe { NSError::errorWithDomain_code_userInfo(&domain, 1, None) }
        });
        let error_ptr = error
            .as_deref()
            .map(|error| (error as *const NSError).cast_mut())
            .unwrap_or(null_mut());
        completion.call((error_ptr,));
        mark_remote_promise_completed();
    });
}

fn mark_remote_promise_completed() {
    ACTIVE_REMOTE_DRAG.with(|active| {
        let mut active = active.borrow_mut();
        let Some(state) = active.as_mut() else {
            return;
        };
        state.remaining_promises = state.remaining_promises.saturating_sub(1);
        if state.drag_ended && state.remaining_promises == 0 {
            active.take();
        }
    });
}
