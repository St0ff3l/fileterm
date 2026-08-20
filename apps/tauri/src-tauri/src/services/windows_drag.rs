//! Windows drag-out for remote files — browser-style streaming by default:
//!
//! `FileGroupDescriptorW` + `FileContents` IStream, the same mechanism
//! browsers use. The OLE drag starts immediately with no download; when the
//! user drops on Explorer, the shell pulls the bytes through the stream and
//! writes them directly into the drop target (with its own progress UI via
//! `FD_PROGRESSUI`). SSH user mode reads through the dedicated SFTP transfer
//! channel, SSH root view through the exec/su channel, and FTP through a
//! dedicated connection that reuses one RETR stream for sequential reads.
//!
//! The staged `CF_HDROP` path (download to a private temp dir first) remains
//! only as a defensive fallback for session types without range reads.
//!
//! A lazy CF_HDROP provider is not viable in either mode: the OLE drop target
//! of our own window (tao) synchronously requests CF_HDROP during DragEnter,
//! which fires the moment the drag leaves the file row. A blocking download
//! there would freeze the whole app (see PR #202's original attempt). For the
//! streaming mode we therefore only answer CF_HDROP while the cursor is over
//! our own window — that is the in-app local-pane drop path, and the renderer
//! downloads the items directly. Explorer never sees CF_HDROP and uses the
//! virtual-file formats instead.

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;
use windows::{
    core::{implement, Error, Result, BOOL, HRESULT},
    Win32::{
        Foundation::{
            GlobalFree, COLORREF, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS,
            DV_E_FORMATETC, DV_E_LINDEX, DV_E_TYMED, E_FAIL, E_INVALIDARG, E_NOTIMPL,
            E_OUTOFMEMORY, E_POINTER, E_UNEXPECTED, HANDLE, HWND, OLE_E_ADVISENOTSUPPORTED, POINT,
            SIZE, S_OK,
        },
        Graphics::Gdi::{
            CreateDIBSection, DeleteObject, GetDC, ReleaseDC, ScreenToClient, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
        },
        System::{
            Com::{
                CoCreateInstance, CoTaskMemAlloc, IDataObject, IDataObject_Impl, IEnumFORMATETC,
                IEnumFORMATETC_Impl, ISequentialStream_Impl, IStream, IStream_Impl,
                CLSCTX_INPROC_SERVER, DATADIR_GET, DVASPECT_CONTENT, FORMATETC, STATFLAG,
                STATFLAG_NONAME, STATSTG, STGMEDIUM, STGMEDIUM_0, STGTY_STREAM, STREAM_SEEK,
                STREAM_SEEK_CUR, STREAM_SEEK_END, STREAM_SEEK_SET, TYMED_HGLOBAL, TYMED_ISTREAM,
            },
            DataExchange::RegisterClipboardFormatW,
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
            Ole::{
                DoDragDrop, IDropSource, IDropSource_Impl, OleDuplicateData, OleInitialize,
                ReleaseStgMedium, CF_HDROP, CLIPBOARD_FORMAT, DROPEFFECT, DROPEFFECT_COPY,
            },
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
        },
        UI::{
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
            Shell::{
                BHID_DataObject, CLSID_DragDropHelper, Common, IDragSourceHelper,
                ILCreateFromPathW, ILFree, IShellItemArray, SHCreateShellItemArrayFromIDLists,
                CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, FD_ATTRIBUTES, FD_FILESIZE,
                FD_PROGRESSUI, FILEDESCRIPTORW, FILEGROUPDESCRIPTORW, SHDRAGIMAGE,
            },
            WindowsAndMessaging::{GetAncestor, GetCursorPos, WindowFromPoint, GA_ROOT},
        },
    },
};

use super::{
    prepare_staged_remote_paths, remove_staging_dir_sync, schedule_staging_cleanup,
    RemoteFileDragItem,
};

/// 单次 IStream 网络预取块大小：解耦 Explorer 的小块 Read 与 SFTP 往返。
const STREAM_CHUNK_SIZE: u64 = 1024 * 1024;
/// 虚拟文件描述符条目上限，防御异常巨大的目录树。
const MAX_DRAG_ENTRIES: usize = 2000;
/// `cFileName` 为 [u16; 260]，含结尾 NUL。
const MAX_VIRTUAL_NAME_LEN: usize = 259;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;

#[implement(IDropSource)]
struct DropSource {
    app: AppHandle,
    window_label: String,
    source_hwnd: isize,
    /// 上次上报的光标状态（屏幕物理像素 x/y + 是否在源窗口内），用于去重。
    last_report: Mutex<(i32, i32, bool)>,
}

#[allow(non_snake_case)]
impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, escape_pressed: BOOL, key_state: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape_pressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if (key_state & MK_LBUTTON) == MODIFIERKEYS_FLAGS(0) {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        report_drag_cursor(
            &self.app,
            &self.window_label,
            self.source_hwnd,
            &self.last_report,
        );
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// Shell 拖拽图像只会在配合 IDropTargetHelper 的拖拽目标（桌面 / 资源管理
/// 器）上渲染；悬停在本应用自己的窗口上时（WebView2 的 drop target 不调
/// 用 helper）只剩系统光标。因此在 GiveFeedback（拖拽循环每次鼠标移动都
/// 会回调）里把光标位置与是否在源窗口内上报给 renderer：窗口内由 DOM
/// ghost 补位跟随，离开窗口后交还给 Shell 拖拽图像。
fn report_drag_cursor(
    app: &AppHandle,
    window_label: &str,
    source_hwnd: isize,
    last_report: &Mutex<(i32, i32, bool)>,
) {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return;
    }
    let in_window = point_over_window(point, source_hwnd);

    // 屏幕坐标 → 源窗口客户区物理像素 → 按 DPI 换算成 CSS 像素（webview
    // 铺满整个客户区，与 clientX/clientY 同一坐标系）。
    let hwnd = HWND(source_hwnd as *mut core::ffi::c_void);
    let mut local = point;
    if !unsafe { ScreenToClient(hwnd, &mut local) }.as_bool() {
        return;
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let scale = if dpi > 0 { dpi as f64 / 96.0 } else { 1.0 };

    let mut last = last_report.lock().unwrap();
    if *last == (point.x, point.y, in_window) {
        return;
    }
    *last = (point.x, point.y, in_window);
    let _ = app.emit_to(
        window_label,
        "fileterm://remote-native-drag-cursor",
        DragCursorEvent {
            x: local.x as f64 / scale,
            y: local.y as f64 / scale,
            in_window,
        },
    );
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct DragCursorEvent {
    x: f64,
    y: f64,
    in_window: bool,
}

pub async fn start_remote_file_drag(
    app: &AppHandle,
    window_label: &str,
    tab_id: &str,
    items: Vec<RemoteFileDragItem>,
    drag_image: Option<super::RemoteDragImage>,
) -> std::result::Result<(), crate::AppError> {
    let window = app
        .get_webview_window(window_label)
        .ok_or_else(|| crate::AppError::Command("拖出窗口不存在".to_string()))?;

    if session_supports_streaming(app, tab_id).await {
        start_streaming_drag(app.clone(), window, window_label, tab_id, items, drag_image).await
    } else {
        start_staged_drag(app, window, window_label, tab_id, items, drag_image).await
    }
}

/// SSH（含 root 视图，走 exec 通道）与 FTP 都具备范围读取能力，全部走
/// 流式拖出；暂存回退仅保留给未知会话类型的防御路径。
async fn session_supports_streaming(app: &AppHandle, tab_id: &str) -> bool {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let tabs = state.tabs.read().await;
    tabs.iter()
        .any(|tab| tab.id == tab_id && (tab.session_type == "ssh" || tab.session_type == "ftp"))
}

// ─────────────────────────────────────────────────────────────────────────────
// 流式虚拟文件拖出（浏览器同款 FileGroupDescriptorW + FileContents）
// ─────────────────────────────────────────────────────────────────────────────

async fn start_streaming_drag(
    app: AppHandle,
    window: tauri::WebviewWindow,
    window_label: &str,
    tab_id: &str,
    items: Vec<RemoteFileDragItem>,
    drag_image: Option<super::RemoteDragImage>,
) -> std::result::Result<(), crate::AppError> {
    // HWND 不是 Send，跨线程以 isize 传递。
    let source_hwnd = window
        .hwnd()
        .map_err(|error| crate::AppError::Command(format!("无法获取拖出窗口句柄：{error}")))?
        .0 as isize;

    // 描述符（大小 stat 与目录递归展平）在进入拖拽循环前构建：这些网络
    // 往返走异步 worker，若留到 Explorer 首次请求 FileGroupDescriptorW 时才做，
    // 主线程模态循环会在 DragEnter 里长时间阻塞。
    let entries = Arc::new(
        build_virtual_entries(&app, tab_id, &items)
            .await
            .map_err(crate::AppError::Command)?,
    );

    // Shell 拖拽图像的 PNG 解码同样前置到异步阶段，主线程只做位图拷贝。
    let drag_image = drag_image.and_then(|spec| match decode_drag_image(&spec) {
        Ok(decoded) => Some(decoded),
        Err(error) => {
            log::warn!("跳过 Shell 拖拽图像：{error}");
            None
        }
    });

    let (result_sender, result_receiver) = oneshot::channel::<std::result::Result<(), String>>();
    let window_label = window_label.to_string();
    let tab_id = tab_id.to_string();
    window
        .run_on_main_thread(move || {
            let result = run_streaming_drag(
                app,
                window_label,
                tab_id,
                source_hwnd,
                items,
                entries,
                drag_image,
            );
            let _ = result_sender.send(result);
        })
        .map_err(|error| crate::AppError::Command(format!("切换到主线程拖出失败：{error}")))?;

    result_receiver
        .await
        .map_err(|_| crate::AppError::Command("原生拖出未响应".to_string()))?
        .map_err(crate::AppError::Command)
}

/// OLE 拖拽循环必须在主线程运行：DoDragDrop 内部会把鼠标捕获设置到调用
/// 线程的隐藏窗口，而 Windows 仅允许前台窗口所属线程夺取鼠标捕获——后台
/// 线程的隐藏窗口收不到任何鼠标消息，拖拽循环会永远挂起。Explorer 落点
/// 后通过 FileContents 拉流，`Read` 回调在主线程模态循环内同步预取网络
/// 数据（浏览器拖出同款行为，进度由 Explorer 的 FD_PROGRESSUI 呈现）。
fn run_streaming_drag(
    app: AppHandle,
    window_label: String,
    tab_id: String,
    source_hwnd: isize,
    items: Vec<RemoteFileDragItem>,
    entries: Arc<Vec<VirtualEntry>>,
    drag_image: Option<DecodedDragImage>,
) -> std::result::Result<(), String> {
    // OleInitialize 隐含 STA；重复初始化返回 S_FALSE 同样视为成功。
    unsafe { OleInitialize(None) }.map_err(|error| error.to_string())?;

    // 描述符构建耗时较长：用户已松开时直接取消，避免在光标处意外落盘。
    if !left_mouse_button_pressed() {
        let _ = app.emit_to(&window_label, "fileterm://remote-native-drag-finished", ());
        return Ok(());
    }

    let data_object: IDataObject = VirtualFileDataObject {
        app: app.clone(),
        tab_id: tab_id.clone(),
        source_hwnd,
        items,
        entries,
        set_data: Mutex::new(Vec::new()),
    }
    .into();

    // Shell 拖拽图像（DragImageBits）：交给 DragDropHelper 后由系统在光标
    // 处渲染，拖出窗口也持续可见（浏览器拖出同款视觉）。失败时降级为
    // 默认拖拽光标，不阻断拖拽本身。
    if let Some(image) = drag_image.as_ref() {
        if let Err(error) = attach_drag_image(&data_object, image) {
            log::warn!("{error}");
        }
    }

    let drop_source: IDropSource = DropSource {
        app: app.clone(),
        window_label: window_label.clone(),
        source_hwnd,
        last_report: Mutex::new((i32::MIN, i32::MIN, false)),
    }
    .into();

    let _ = app.emit_to(&window_label, "fileterm://remote-native-drag-started", ());

    let mut performed_effect = DROPEFFECT::default();
    let result = unsafe {
        DoDragDrop(
            &data_object,
            &drop_source,
            DROPEFFECT_COPY,
            &mut performed_effect,
        )
    };

    let _ = app.emit_to(&window_label, "fileterm://remote-native-drag-finished", ());
    match result {
        result if result == DRAGDROP_S_DROP || result == DRAGDROP_S_CANCEL => Ok(()),
        result => Err(format!(
            "Windows 原生拖出失败：HRESULT 0x{:08X}",
            result.0 as u32
        )),
    }
}

/// 虚拟文件条目：Explorer 按此列表在落点创建目录结构并逐个拉流。
struct VirtualEntry {
    /// 相对落点的名字，子目录用反斜杠分隔。
    name: String,
    remote_path: String,
    size: Option<u64>,
    is_dir: bool,
}

/// renderer 生成的 Shell 拖拽图像，异步阶段解码完成，主线程只做位图拷贝。
struct DecodedDragImage {
    /// 自上而下 32bpp BGRA，非预乘（InitializeFromBitmap 自行做 RGB 乘法）。
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    /// 光标热点在图像内的物理像素位置（负值 = 图像位于光标右下方）。
    offset_x: i32,
    offset_y: i32,
}

fn decode_drag_image(
    spec: &super::RemoteDragImage,
) -> std::result::Result<DecodedDragImage, String> {
    let payload = match spec.data_url.strip_prefix("data:") {
        Some(rest) => rest.rsplit_once(',').map(|(_, data)| data).unwrap_or(rest),
        None => spec.data_url.as_str(),
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|error| format!("拖拽图像 base64 解码失败：{error}"))?;
    let image = image::load_from_memory(&bytes)
        .map_err(|error| format!("拖拽图像解码失败：{error}"))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Err("拖拽图像为空".to_string());
    }
    let mut pixels = Vec::with_capacity(image.as_raw().len());
    for px in image.chunks_exact(4) {
        // RGBA → BGRA（DIB 字节序）
        pixels.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    Ok(DecodedDragImage {
        pixels,
        width,
        height,
        offset_x: spec.offset_x,
        offset_y: spec.offset_y,
    })
}

/// 把解码后的像素包装成 32bpp 顶向下 DIB 位图。
fn create_drag_bitmap(image: &DecodedDragImage) -> std::result::Result<HBITMAP, String> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: image.width as i32,
            biHeight: -(image.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let hdc = unsafe { GetDC(None) };
    let bitmap = unsafe { CreateDIBSection(Some(hdc), &info, DIB_RGB_COLORS, &mut bits, None, 0) };
    unsafe { ReleaseDC(None, hdc) };
    let bitmap = bitmap.map_err(|error| format!("创建拖拽位图失败：{error}"))?;
    if bits.is_null() {
        let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
        return Err("拖拽位图缺少像素缓冲".to_string());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(image.pixels.as_ptr(), bits as *mut u8, image.pixels.len())
    };
    Ok(bitmap)
}

/// 经 DragDropHelper 把拖拽图像写入数据对象（DragImageBits 等 Shell 格式，
/// 以 SetData 存回本对象），此后拖拽全程由系统在光标处渲染该图像。
fn attach_drag_image(
    data_object: &IDataObject,
    image: &DecodedDragImage,
) -> std::result::Result<(), String> {
    let bitmap = create_drag_bitmap(image)?;
    let shdi = SHDRAGIMAGE {
        sizeDragImage: SIZE {
            cx: image.width as i32,
            cy: image.height as i32,
        },
        ptOffset: POINT {
            x: image.offset_x,
            y: image.offset_y,
        },
        hbmpDragImage: bitmap,
        // CLR_NONE：使用位图自身的 alpha 通道。
        crColorKey: COLORREF(0xFFFF_FFFF),
    };
    let helper: IDragSourceHelper =
        unsafe { CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| format!("创建 DragDropHelper 失败：{error}"))?;
    match unsafe { helper.InitializeFromBitmap(&shdi, data_object) } {
        // 成功后 helper 接管位图所有权，由其负责释放。
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = unsafe { DeleteObject(HGDIOBJ(shdi.hbmpDragImage.0)) };
            Err(format!("设置 Shell 拖拽图像失败：{error}"))
        }
    }
}

/// 通过 SetData 存入的数据对象格式（DragImageBits、DragContext 等）。
struct StoredMedium {
    format: FORMATETC,
    medium: STGMEDIUM,
}

#[implement(IDataObject)]
struct VirtualFileDataObject {
    app: AppHandle,
    tab_id: String,
    source_hwnd: isize,
    items: Vec<RemoteFileDragItem>,
    /// 描述符在进入拖拽循环前于异步上下文预构建（含目录递归展平与大小
    /// stat），DragEnter 应答与 FileContents 定位都直接复用。
    entries: Arc<Vec<VirtualEntry>>,
    /// Shell DragDropHelper 等外部调用方 SetData 进来的格式存储。
    set_data: Mutex<Vec<StoredMedium>>,
}

impl Drop for VirtualFileDataObject {
    fn drop(&mut self) {
        let stored = std::mem::take(&mut self.set_data)
            .into_inner()
            .unwrap_or_default();
        for entry in stored {
            let mut medium = entry.medium;
            unsafe { ReleaseStgMedium(&mut medium as *mut STGMEDIUM) };
        }
    }
}

fn file_descriptor_format() -> u16 {
    static FORMAT: OnceLock<u16> = OnceLock::new();
    *FORMAT.get_or_init(|| unsafe { RegisterClipboardFormatW(CFSTR_FILEDESCRIPTORW) } as u16)
}

fn file_contents_format() -> u16 {
    static FORMAT: OnceLock<u16> = OnceLock::new();
    *FORMAT.get_or_init(|| unsafe { RegisterClipboardFormatW(CFSTR_FILECONTENTS) } as u16)
}

fn format_etc(cf: u16, tymed: u32, lindex: i32) -> FORMATETC {
    FORMATETC {
        cfFormat: cf,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex,
        tymed,
    }
}

fn hglobal_medium(handle: windows::Win32::Foundation::HGLOBAL) -> STGMEDIUM {
    STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 { hGlobal: handle },
        pUnkForRelease: std::mem::ManuallyDrop::new(None),
    }
}

fn istream_medium(stream: IStream) -> STGMEDIUM {
    STGMEDIUM {
        tymed: TYMED_ISTREAM.0 as u32,
        u: STGMEDIUM_0 {
            pstm: std::mem::ManuallyDrop::new(Some(stream)),
        },
        pUnkForRelease: std::mem::ManuallyDrop::new(None),
    }
}

/// 仅当光标位于源窗口内时才应答 CF_HDROP：这是应用内本地面板拖放的专属通道，
/// Explorer 永远拿不到该格式，只能走虚拟文件流。
fn cursor_over_window(source_hwnd: isize) -> bool {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return false;
    }
    point_over_window(point, source_hwnd)
}

fn point_over_window(point: POINT, source_hwnd: isize) -> bool {
    let hovered = unsafe { WindowFromPoint(point) };
    if hovered.is_invalid() {
        return false;
    }
    let root = unsafe { GetAncestor(hovered, GA_ROOT) };
    !root.is_invalid() && root.0 as isize == source_hwnd
}

/// 复制 STGMEDIUM（调用方持有副本的所有权；HGLOBAL 走 OleDuplicateData，
/// IStream 以 AddRef 共享）。
fn duplicate_medium(cf_format: u16, medium: &STGMEDIUM) -> Result<STGMEDIUM> {
    if medium.tymed == TYMED_HGLOBAL.0 as u32 {
        // SAFETY: 按 tymed 判别后访问 union 的 hGlobal 字段。
        let source = unsafe { medium.u.hGlobal };
        if source.is_invalid() {
            return Err(Error::from_hresult(E_POINTER));
        }
        let duplicated = unsafe {
            OleDuplicateData(HANDLE(source.0), CLIPBOARD_FORMAT(cf_format), GMEM_MOVEABLE)
        };
        if duplicated.is_invalid() {
            return Err(Error::from_hresult(E_OUTOFMEMORY));
        }
        return Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 {
                hGlobal: windows::Win32::Foundation::HGLOBAL(duplicated.0),
            },
            pUnkForRelease: std::mem::ManuallyDrop::new(None),
        });
    }
    if medium.tymed == TYMED_ISTREAM.0 as u32 {
        // SAFETY: 按 tymed 判别后访问 union 的 pstm 字段；clone 即 AddRef。
        let stream = unsafe { medium.u.pstm.clone() };
        match std::mem::ManuallyDrop::into_inner(stream) {
            Some(stream) => {
                return Ok(STGMEDIUM {
                    tymed: TYMED_ISTREAM.0 as u32,
                    u: STGMEDIUM_0 {
                        pstm: std::mem::ManuallyDrop::new(Some(stream)),
                    },
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                });
            }
            None => return Err(Error::from_hresult(E_POINTER)),
        }
    }
    Err(Error::from_hresult(DV_E_TYMED))
}

#[allow(non_snake_case)]
impl IDataObject_Impl for VirtualFileDataObject_Impl {
    fn GetData(&self, pformatetcin: *const FORMATETC) -> Result<STGMEDIUM> {
        let format = unsafe { &*pformatetcin };
        let cf = format.cfFormat;

        // Shell 通过 SetData 存入的格式（DragImageBits 等）优先应答，
        // 返回副本，所有权归调用方。
        if let Some(stored) = self.set_data.lock().unwrap().iter().find(|stored| {
            stored.format.cfFormat == cf
                && (stored.format.tymed & format.tymed) != 0
                && (stored.format.lindex == format.lindex || stored.format.lindex == -1)
        }) {
            return duplicate_medium(stored.format.cfFormat, &stored.medium);
        }

        if cf == CF_HDROP.0 {
            if (format.tymed & TYMED_HGLOBAL.0 as u32) == 0 || !cursor_over_window(self.source_hwnd)
            {
                return Err(Error::from_hresult(DV_E_FORMATETC));
            }
            let root = std::env::temp_dir().join("fileterm-drag-virtual");
            let paths: Vec<PathBuf> = self
                .items
                .iter()
                .map(|item| root.join(sanitize_virtual_name(&item.name)))
                .collect();
            let handle = build_hdrop_hglobal(&paths)?;
            return Ok(hglobal_medium(handle));
        }

        if cf == file_descriptor_format() {
            if (format.tymed & TYMED_HGLOBAL.0 as u32) == 0 {
                return Err(Error::from_hresult(DV_E_FORMATETC));
            }
            let handle = build_descriptor_hglobal(&self.entries)?;
            return Ok(hglobal_medium(handle));
        }

        if cf == file_contents_format() {
            if (format.tymed & TYMED_ISTREAM.0 as u32) == 0 || format.lindex < 0 {
                return Err(Error::from_hresult(DV_E_FORMATETC));
            }
            let entry = self
                .entries
                .get(format.lindex as usize)
                .ok_or_else(|| Error::from_hresult(DV_E_LINDEX))?;
            if entry.is_dir {
                return Err(Error::from_hresult(E_INVALIDARG));
            }
            let stream: IStream = RemoteFileStream {
                app: self.app.clone(),
                tab_id: self.tab_id.clone(),
                remote_path: entry.remote_path.clone(),
                size: entry.size,
                state: Mutex::new(StreamState {
                    position: 0,
                    cache: Vec::new(),
                    eof: false,
                }),
            }
            .into();
            return Ok(istream_medium(stream));
        }

        Err(Error::from_hresult(DV_E_FORMATETC))
    }

    fn GetDataHere(&self, _pformatetc: *const FORMATETC, _pmedium: *mut STGMEDIUM) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        let format = unsafe { &*pformatetc };
        let cf = format.cfFormat;
        if self.set_data.lock().unwrap().iter().any(|stored| {
            stored.format.cfFormat == cf
                && (stored.format.tymed & format.tymed) != 0
                && (stored.format.lindex == format.lindex || stored.format.lindex == -1)
        }) {
            return S_OK;
        }
        if cf == CF_HDROP.0 && cursor_over_window(self.source_hwnd) {
            return S_OK;
        }
        if cf == file_descriptor_format() && (format.tymed & TYMED_HGLOBAL.0 as u32) != 0 {
            return S_OK;
        }
        if cf == file_contents_format() && (format.tymed & TYMED_ISTREAM.0 as u32) != 0 {
            return S_OK;
        }
        DV_E_FORMATETC
    }

    fn GetCanonicalFormatEtc(
        &self,
        _pformatectin: *const FORMATETC,
        _pformatetcout: *mut FORMATETC,
    ) -> HRESULT {
        E_NOTIMPL
    }

    fn SetData(
        &self,
        pformatetc: *const FORMATETC,
        pmedium: *const STGMEDIUM,
        frelease: BOOL,
    ) -> Result<()> {
        let format = unsafe { &*pformatetc };
        let medium = unsafe { &*pmedium };
        let supported_tymed = (TYMED_HGLOBAL.0 | TYMED_ISTREAM.0) as u32;
        if format.cfFormat == 0 || (format.tymed & supported_tymed) == 0 {
            return Err(Error::from_hresult(DV_E_FORMATETC));
        }

        // Shell DragDropHelper（InitializeFromBitmap）以 fRelease=TRUE 存入
        // DragImageBits 等格式并移交所有权；此时整体搬移 medium 结构即可。
        let stored_medium: STGMEDIUM = if frelease.as_bool() {
            // SAFETY: 所有权移交语义下按位搬移，原持有方不再释放。
            unsafe { std::ptr::read(medium) }
        } else {
            duplicate_medium(format.cfFormat, medium)?
        };
        let mut stored_format = *format;
        stored_format.ptd = std::ptr::null_mut();

        let mut stored = self.set_data.lock().unwrap();
        stored.retain(|entry| {
            !(entry.format.cfFormat == stored_format.cfFormat
                && entry.format.lindex == stored_format.lindex
                && entry.format.dwAspect == stored_format.dwAspect)
        });
        stored.push(StoredMedium {
            format: stored_format,
            medium: stored_medium,
        });
        Ok(())
    }

    fn EnumFormatEtc(&self, dwdirection: u32) -> Result<IEnumFORMATETC> {
        if dwdirection != DATADIR_GET.0 as u32 {
            return Err(Error::from_hresult(E_NOTIMPL));
        }
        let mut formats: Vec<(u16, u32)> = vec![
            (file_descriptor_format(), TYMED_HGLOBAL.0 as u32),
            (file_contents_format(), TYMED_ISTREAM.0 as u32),
        ];
        for stored in self.set_data.lock().unwrap().iter() {
            formats.push((stored.format.cfFormat, stored.format.tymed));
        }
        Ok(FormatEtcEnum {
            formats,
            index: Mutex::new(0),
        }
        .into())
    }

    fn DAdvise(
        &self,
        _pformatetc: *const FORMATETC,
        _advf: u32,
        _padvsink: windows::core::Ref<'_, windows::Win32::System::Com::IAdviseSink>,
    ) -> Result<u32> {
        Err(Error::from_hresult(OLE_E_ADVISENOTSUPPORTED))
    }

    fn DUnadvise(&self, _dwconnection: u32) -> Result<()> {
        Err(Error::from_hresult(OLE_E_ADVISENOTSUPPORTED))
    }

    fn EnumDAdvise(&self) -> Result<windows::Win32::System::Com::IEnumSTATDATA> {
        Err(Error::from_hresult(OLE_E_ADVISENOTSUPPORTED))
    }
}

#[implement(IEnumFORMATETC)]
struct FormatEtcEnum {
    formats: Vec<(u16, u32)>,
    index: Mutex<usize>,
}

#[allow(non_snake_case)]
impl IEnumFORMATETC_Impl for FormatEtcEnum_Impl {
    fn Next(&self, celt: u32, rgelt: *mut FORMATETC, pceltfetched: *mut u32) -> HRESULT {
        let mut fetched = 0_u32;
        let mut index = self.index.lock().unwrap();
        while fetched < celt && *index < self.formats.len() {
            let (cf, tymed) = self.formats[*index];
            if !rgelt.is_null() {
                unsafe {
                    *rgelt.add(fetched as usize) = format_etc(cf, tymed, -1);
                }
            }
            *index += 1;
            fetched += 1;
        }
        drop(index);
        if !pceltfetched.is_null() {
            unsafe { *pceltfetched = fetched };
        }
        if fetched == celt {
            S_OK
        } else {
            windows::Win32::Foundation::S_FALSE
        }
    }

    fn Skip(&self, celt: u32) -> Result<()> {
        let mut index = self.index.lock().unwrap();
        *index += celt as usize;
        if *index >= self.formats.len() {
            *index = self.formats.len();
            return Err(Error::from_hresult(windows::Win32::Foundation::S_FALSE));
        }
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        *self.index.lock().unwrap() = 0;
        Ok(())
    }

    fn Clone(&self) -> Result<IEnumFORMATETC> {
        Ok(FormatEtcEnum {
            formats: self.formats.clone(),
            index: Mutex::new(*self.index.lock().unwrap()),
        }
        .into())
    }
}

struct StreamState {
    /// 下一次网络预取的绝对偏移（= 已预取末尾）。
    position: u64,
    cache: Vec<u8>,
    eof: bool,
}

#[implement(IStream)]
struct RemoteFileStream {
    app: AppHandle,
    tab_id: String,
    remote_path: String,
    size: Option<u64>,
    state: Mutex<StreamState>,
}

#[allow(non_snake_case)]
impl ISequentialStream_Impl for RemoteFileStream_Impl {
    fn Read(&self, pv: *mut core::ffi::c_void, cb: u32, pcbread: *mut u32) -> HRESULT {
        if !pcbread.is_null() {
            unsafe { *pcbread = 0 };
        }
        if cb == 0 || pv.is_null() {
            return S_OK;
        }
        let mut state = self.state.lock().unwrap();
        loop {
            if !state.cache.is_empty() {
                let count = (cb as usize).min(state.cache.len());
                unsafe {
                    std::ptr::copy_nonoverlapping(state.cache.as_ptr(), pv as *mut u8, count);
                }
                state.cache.drain(0..count);
                if !pcbread.is_null() {
                    unsafe { *pcbread = count as u32 };
                }
                return S_OK;
            }
            if state.eof {
                return S_OK;
            }
            let remaining = self
                .size
                .map(|size| size.saturating_sub(state.position))
                .unwrap_or(STREAM_CHUNK_SIZE);
            if remaining == 0 {
                state.eof = true;
                continue;
            }
            let fetch_len = remaining.min(STREAM_CHUNK_SIZE);
            let chunk = match tauri::async_runtime::block_on(read_remote_range(
                &self.app,
                &self.tab_id,
                &self.remote_path,
                state.position,
                fetch_len,
            )) {
                Ok(chunk) => chunk,
                Err(error) => {
                    log::warn!("流式拖出读取失败 {}: {error}", self.remote_path);
                    return E_FAIL;
                }
            };
            if chunk.is_empty() {
                state.eof = true;
                continue;
            }
            state.position += chunk.len() as u64;
            state.cache = chunk;
        }
    }

    fn Write(&self, _pv: *const core::ffi::c_void, _cb: u32, pcbwritten: *mut u32) -> HRESULT {
        if !pcbwritten.is_null() {
            unsafe { *pcbwritten = 0 };
        }
        E_NOTIMPL
    }
}

#[allow(non_snake_case)]
impl IStream_Impl for RemoteFileStream_Impl {
    fn Seek(&self, dlibmove: i64, dworigin: STREAM_SEEK, plibnewposition: *mut u64) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let logical_position = state.position - state.cache.len() as u64;
        let base: i128 = match dworigin {
            STREAM_SEEK_SET => 0,
            STREAM_SEEK_CUR => logical_position as i128,
            STREAM_SEEK_END => match self.size {
                Some(size) => size as i128,
                None => return Err(Error::from_hresult(E_NOTIMPL)),
            },
            _ => return Err(Error::from_hresult(E_INVALIDARG)),
        };
        let target = base + dlibmove as i128;
        if target < 0 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let target = target as u64;
        state.position = target;
        state.cache.clear();
        state.eof = self.size.is_some_and(|size| target >= size);
        if !plibnewposition.is_null() {
            unsafe { *plibnewposition = target };
        }
        Ok(())
    }

    fn SetSize(&self, _libnewsize: u64) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn CopyTo(
        &self,
        _pstm: windows::core::Ref<'_, IStream>,
        _cb: u64,
        _pcbread: *mut u64,
        _pcbwritten: *mut u64,
    ) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn Commit(&self, _grfcommitflags: &windows::Win32::System::Com::STGC) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn Revert(&self) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn LockRegion(
        &self,
        _liboffset: u64,
        _cb: u64,
        _dwlocktype: &windows::Win32::System::Com::LOCKTYPE,
    ) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn UnlockRegion(&self, _liboffset: u64, _cb: u64, _dwlocktype: u32) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn Stat(&self, pstatstg: *mut STATSTG, grfstatflag: &STATFLAG) -> Result<()> {
        if pstatstg.is_null() {
            return Err(Error::from_hresult(E_POINTER));
        }
        unsafe {
            std::ptr::write(
                pstatstg,
                STATSTG {
                    pwcsName: windows::core::PWSTR::null(),
                    r#type: STGTY_STREAM.0 as u32,
                    cbSize: self.size.unwrap_or(0),
                    mtime: windows::Win32::Foundation::FILETIME::default(),
                    ctime: windows::Win32::Foundation::FILETIME::default(),
                    atime: windows::Win32::Foundation::FILETIME::default(),
                    grfMode: windows::Win32::System::Com::STGM::default(),
                    grfLocksSupported: 0,
                    clsid: windows::core::GUID::zeroed(),
                    grfStateBits: 0,
                    reserved: 0,
                },
            );
            if *grfstatflag != STATFLAG_NONAME {
                let name = self
                    .remote_path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(&self.remote_path);
                let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                let alloc = CoTaskMemAlloc(wide.len() * std::mem::size_of::<u16>()) as *mut u16;
                if alloc.is_null() {
                    return Err(Error::from_hresult(E_OUTOFMEMORY));
                }
                std::ptr::copy_nonoverlapping(wide.as_ptr(), alloc, wide.len());
                (*pstatstg).pwcsName = windows::core::PWSTR(alloc);
            }
        }
        Ok(())
    }

    fn Clone(&self) -> Result<IStream> {
        Err(Error::from_hresult(E_NOTIMPL))
    }
}

async fn read_remote_range(
    app: &AppHandle,
    tab_id: &str,
    path: &str,
    offset: u64,
    length: u64,
) -> std::result::Result<Vec<u8>, String> {
    crate::services::transfers::worker_call(app, tab_id, |respond_to| {
        crate::sessions::WorkerCmd::ReadRemoteFileRange {
            path: path.to_string(),
            offset,
            length,
            respond_to,
        }
    })
    .await
    .map_err(|error| error.to_string())
}

/// 构建虚拟文件描述符：顶层文件取真实大小（Explorer 进度条显示总量），
/// 目录递归展平为 `dir\child` 相对路径，Explorer 会自动创建中间目录。
async fn build_virtual_entries(
    app: &AppHandle,
    tab_id: &str,
    items: &[RemoteFileDragItem],
) -> std::result::Result<Vec<VirtualEntry>, String> {
    let mut entries = Vec::new();
    for item in items {
        if item.item_type == "folder" {
            entries.push(VirtualEntry {
                name: item.name.clone(),
                remote_path: item.path.clone(),
                size: None,
                is_dir: true,
            });
            flatten_remote_dir(app, tab_id, &item.path, &item.name, &mut entries).await?;
        } else {
            let size = stat_remote_size(app, tab_id, &item.path)
                .await
                .ok()
                .flatten();
            entries.push(VirtualEntry {
                name: item.name.clone(),
                remote_path: item.path.clone(),
                size,
                is_dir: false,
            });
        }
    }
    Ok(entries)
}

fn flatten_remote_dir<'a>(
    app: &'a AppHandle,
    tab_id: &'a str,
    dir_path: &'a str,
    prefix: &'a str,
    entries: &'a mut Vec<VirtualEntry>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), String>> + Send + 'a>>
{
    Box::pin(async move {
        let listing = crate::services::transfers::worker_call(app, tab_id, |respond_to| {
            crate::sessions::WorkerCmd::ListRemoteFiles {
                path: dir_path.to_string(),
                respond_to,
            }
        })
        .await
        .map_err(|error| error.to_string())?;

        for entry in listing {
            let name = entry["name"].as_str().unwrap_or_default().to_string();
            let path = entry["path"].as_str().unwrap_or_default().to_string();
            if name.is_empty() || name == "." || name == ".." || path.is_empty() {
                continue;
            }
            if entries.len() >= MAX_DRAG_ENTRIES {
                return Err(format!("拖出目录条目超过上限 {MAX_DRAG_ENTRIES}"));
            }
            let relative_name = format!("{prefix}\\{name}");
            if entry["type"].as_str() == Some("folder") {
                entries.push(VirtualEntry {
                    name: relative_name.clone(),
                    remote_path: path.clone(),
                    size: None,
                    is_dir: true,
                });
                flatten_remote_dir(app, tab_id, &path, &relative_name, entries).await?;
            } else {
                entries.push(VirtualEntry {
                    name: relative_name,
                    remote_path: path,
                    size: None,
                    is_dir: false,
                });
            }
        }
        Ok(())
    })
}

async fn stat_remote_size(
    app: &AppHandle,
    tab_id: &str,
    path: &str,
) -> std::result::Result<Option<u64>, String> {
    let stat = crate::services::transfers::worker_call(app, tab_id, |respond_to| {
        crate::sessions::WorkerCmd::StatRemoteFile {
            path: path.to_string(),
            respond_to,
        }
    })
    .await
    .map_err(|error| error.to_string())?;
    Ok(stat.map(|stat| stat.size))
}

fn sanitize_virtual_name(name: &str) -> String {
    // 顶层名字已经过 file_drag::safe_drag_name 校验，这里仅防御性截断。
    name.chars().take(MAX_VIRTUAL_NAME_LEN).collect()
}

/// CF_HDROP 虚拟路径：仅用于让本窗口的 tao drop target 认可拖放并上报
/// 文件名；文件本身不存在，renderer 收到事件后走直接下载。
fn build_hdrop_hglobal(paths: &[PathBuf]) -> Result<windows::Win32::Foundation::HGLOBAL> {
    let header_len = std::mem::size_of::<windows::Win32::UI::Shell::DROPFILES>();
    let mut wide: Vec<u16> = Vec::new();
    for path in paths {
        wide.extend(path.as_os_str().encode_wide());
        wide.push(0);
    }
    wide.push(0);
    let total = header_len + wide.len() * std::mem::size_of::<u16>();

    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, total) }
        .map_err(|_| Error::new(E_OUTOFMEMORY, "分配拖出内存失败"))?;
    let base = unsafe { GlobalLock(handle) };
    if base.is_null() {
        let _ = unsafe { GlobalFree(Some(handle)) };
        return Err(Error::new(E_UNEXPECTED, "锁定拖出内存失败"));
    }
    unsafe {
        std::ptr::write_bytes(base as *mut u8, 0, total);
        let files = base as *mut windows::Win32::UI::Shell::DROPFILES;
        (*files).pFiles = header_len as u32;
        (*files).fWide = BOOL(1);
        let destination = (base as *mut u8).add(header_len) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), destination, wide.len());
        // GlobalUnlock 在锁计数归零时会返回 FALSE + NO_ERROR，结果不可信，忽略即可。
        let _ = GlobalUnlock(handle);
    }
    Ok(handle)
}

/// 组装 FileGroupDescriptorW（packed 结构必须用裸指针写入）。
fn build_descriptor_hglobal(
    entries: &[VirtualEntry],
) -> Result<windows::Win32::Foundation::HGLOBAL> {
    if entries.is_empty() {
        return Err(Error::new(E_INVALIDARG, "没有可拖出的文件"));
    }
    let item_size = std::mem::size_of::<FILEDESCRIPTORW>();
    let total = std::mem::size_of::<FILEGROUPDESCRIPTORW>() + item_size * (entries.len() - 1);

    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, total) }
        .map_err(|_| Error::new(E_OUTOFMEMORY, "分配拖出描述符失败"))?;
    let base = unsafe { GlobalLock(handle) };
    if base.is_null() {
        let _ = unsafe { GlobalFree(Some(handle)) };
        return Err(Error::new(E_UNEXPECTED, "锁定拖出描述符失败"));
    }
    unsafe {
        let group = base as *mut FILEGROUPDESCRIPTORW;
        (*group).cItems = entries.len() as u32;
        let first = std::ptr::addr_of_mut!((*group).fgd) as *mut FILEDESCRIPTORW;
        for (index, entry) in entries.iter().enumerate() {
            let descriptor = first.add(index);
            std::ptr::write(descriptor, FILEDESCRIPTORW::default());
            let mut flags = FD_ATTRIBUTES.0 as u32 | FD_PROGRESSUI.0 as u32;
            if entry.size.is_some() {
                flags |= FD_FILESIZE.0 as u32;
            }
            (*descriptor).dwFlags = flags;
            (*descriptor).dwFileAttributes = if entry.is_dir {
                FILE_ATTRIBUTE_DIRECTORY
            } else {
                FILE_ATTRIBUTE_NORMAL
            };
            if let Some(size) = entry.size {
                (*descriptor).nFileSizeLow = (size & 0xFFFF_FFFF) as u32;
                (*descriptor).nFileSizeHigh = (size >> 32) as u32;
            }
            let wide: Vec<u16> = entry.name.encode_utf16().collect();
            let copy_len = wide.len().min(MAX_VIRTUAL_NAME_LEN);
            if copy_len < wide.len() {
                log::warn!("拖出条目名过长，已截断：{}", entry.name);
            }
            for (offset, character) in wide[..copy_len].iter().enumerate() {
                (*descriptor).cFileName[offset] = *character;
            }
        }
        let _ = GlobalUnlock(handle);
    }
    Ok(handle)
}

// ─────────────────────────────────────────────────────────────────────────────
// 暂存回退（FTP / SSH root 视图）：先下载到私有临时目录，再以真实 CF_HDROP 拖出
// ─────────────────────────────────────────────────────────────────────────────

async fn start_staged_drag(
    app: &AppHandle,
    window: tauri::WebviewWindow,
    window_label: &str,
    tab_id: &str,
    items: Vec<RemoteFileDragItem>,
    drag_image: Option<super::RemoteDragImage>,
) -> std::result::Result<(), crate::AppError> {
    let source_hwnd = window
        .hwnd()
        .map_err(|error| crate::AppError::Command(format!("无法获取拖出窗口句柄：{error}")))?
        .0 as isize;
    let stage_root =
        std::env::temp_dir().join(format!("fileterm-remote-drag-{}", uuid::Uuid::new_v4()));

    // prepare_staged_remote_paths 在所有错误路径上自行清理传输与临时目录。
    let paths = prepare_staged_remote_paths(app, tab_id, &items, &stage_root).await?;

    // PNG 解码前置到异步阶段，主线程只做位图拷贝。
    let drag_image = drag_image.and_then(|spec| match decode_drag_image(&spec) {
        Ok(decoded) => Some(decoded),
        Err(error) => {
            log::warn!("跳过 Shell 拖拽图像：{error}");
            None
        }
    });

    let (result_sender, result_receiver) = oneshot::channel::<std::result::Result<(), String>>();
    let app_for_main = app.clone();
    let window_label = window_label.to_string();
    window
        .run_on_main_thread(move || {
            let result = run_staged_drag_on_main_thread(
                app_for_main,
                window_label,
                source_hwnd,
                stage_root,
                paths,
                drag_image,
            );
            let _ = result_sender.send(result);
        })
        .map_err(|error| crate::AppError::Command(format!("切换到原生拖出线程失败：{error}")))?;

    result_receiver
        .await
        .map_err(|_| crate::AppError::Command("原生拖出线程未响应".to_string()))?
        .map_err(crate::AppError::Command)
}

fn run_staged_drag_on_main_thread(
    app: AppHandle,
    window_label: String,
    source_hwnd: isize,
    stage_root: PathBuf,
    paths: Vec<PathBuf>,
    drag_image: Option<DecodedDragImage>,
) -> std::result::Result<(), String> {
    unsafe { OleInitialize(None) }.map_err(|error| error.to_string())?;

    // 暂存下载耗时较长：用户已松开时直接取消，避免在光标处意外落盘。
    if !left_mouse_button_pressed() {
        cleanup_staged_drag(&app, &stage_root);
        let _ = app.emit_to(&window_label, "fileterm://remote-native-drag-finished", ());
        return Ok(());
    }

    let data_object = get_file_data_object(&paths).map_err(|error| error.to_string())?;
    if let Some(image) = drag_image.as_ref() {
        if let Err(error) = attach_drag_image(&data_object, image) {
            log::warn!("{error}");
        }
    }
    let drop_source: IDropSource = DropSource {
        app: app.clone(),
        window_label: window_label.clone(),
        source_hwnd,
        last_report: Mutex::new((i32::MIN, i32::MIN, false)),
    }
    .into();

    let _ = app.emit_to(&window_label, "fileterm://remote-native-drag-started", ());

    let mut performed_effect = DROPEFFECT::default();
    let result = unsafe {
        DoDragDrop(
            &data_object,
            &drop_source,
            DROPEFFECT_COPY,
            &mut performed_effect,
        )
    };

    if result == DRAGDROP_S_DROP {
        // Explorer 在 drop 返回后仍会异步从暂存目录拷贝，清理必须延迟。
        schedule_staging_cleanup(stage_root);
    } else {
        cleanup_staged_drag(&app, &stage_root);
    }

    let _ = app.emit_to(&window_label, "fileterm://remote-native-drag-finished", ());
    match result {
        result if result == DRAGDROP_S_DROP || result == DRAGDROP_S_CANCEL => Ok(()),
        result => Err(format!(
            "Windows 原生拖出失败：HRESULT 0x{:08X}",
            result.0 as u32
        )),
    }
}

fn left_mouse_button_pressed() -> bool {
    unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 }
}

/// 拖拽未被任何目标消费时，清理暂存传输与临时目录。
fn cleanup_staged_drag(app: &AppHandle, stage_root: &Path) {
    let app_for_cleanup = app.clone();
    let stage_root_for_cleanup = stage_root.to_path_buf();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = crate::services::transfers::cleanup_drag_transfers_in_stage(
            &app_for_cleanup,
            &stage_root_for_cleanup,
        )
        .await
        {
            log::warn!("清理 Windows 拖出阶段任务失败：{error}");
        }
    });
    remove_staging_dir_sync(stage_root);
}

fn get_file_data_object(paths: &[PathBuf]) -> Result<windows::Win32::System::Com::IDataObject> {
    if paths.is_empty() {
        return Err(Error::new(E_INVALIDARG, "没有可拖出的文件"));
    }
    let shell_item_array = get_shell_item_array(paths)?;
    unsafe { shell_item_array.BindToHandler(None, &BHID_DataObject) }
}

fn get_shell_item_array(paths: &[PathBuf]) -> Result<IShellItemArray> {
    let item_ids: Vec<OwnedPidl> = paths
        .iter()
        .map(|path| get_file_item_id(path))
        .collect::<Result<Vec<_>>>()?;
    let raw_item_ids: Vec<*const Common::ITEMIDLIST> = item_ids
        .iter()
        .map(|item_id| item_id.0.cast_const())
        .collect();
    unsafe { SHCreateShellItemArrayFromIDLists(&raw_item_ids) }
}

struct OwnedPidl(*mut Common::ITEMIDLIST);

impl Drop for OwnedPidl {
    fn drop(&mut self) {
        unsafe { ILFree(Some(self.0.cast_const())) };
    }
}

fn get_file_item_id(path: &Path) -> Result<OwnedPidl> {
    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let item_id = unsafe { ILCreateFromPathW(windows::core::PCWSTR::from_raw(wide_path.as_ptr())) };
    if item_id.is_null() {
        return Err(Error::new(
            E_UNEXPECTED,
            format!("无法解析拖出路径：{}", path.display()),
        ));
    }
    Ok(OwnedPidl(item_id))
}
