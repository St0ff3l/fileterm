//! Lazy Windows drag-out for remote files.
//!
//! `CF_HDROP` is still used for maximum Explorer compatibility, but the shell
//! data object is deliberately created only when the drop target asks for the
//! format. Before that request the drag contains no downloaded bytes. This is
//! the Windows equivalent of the user-visible part of macOS file promises:
//! cancelling a drag or moving it over another window does not start a
//! transfer.

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;
use windows::{
    core::{implement, Error, Ref, Result, BOOL, HRESULT, HSTRING},
    Win32::{
        Foundation::{
            DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, E_INVALIDARG,
            E_NOTIMPL, E_UNEXPECTED, OLE_E_ADVISENOTSUPPORTED, S_FALSE, S_OK,
        },
        System::{
            Com::{
                IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumFORMATETC_Impl,
                IEnumSTATDATA, DATADIR_GET, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL,
            },
            Ole::{
                DoDragDrop, IDropSource, IDropSource_Impl, OleInitialize, CF_HDROP, DROPEFFECT,
                DROPEFFECT_COPY,
            },
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
        },
        UI::Shell::{
            BHID_DataObject, Common, ILCreateFromPathW, ILFree, IShellItemArray,
            SHCreateShellItemArrayFromIDLists,
        },
    },
};

use super::{
    prepare_staged_remote_paths, remove_staging_dir_sync, schedule_staging_cleanup,
    RemoteFileDragItem,
};

static OLE_RESULT: OnceLock<Result<()>> = OnceLock::new();

fn init_ole() -> Result<()> {
    // This mirrors the ownership model used by the `drag` crate: the main
    // thread keeps OLE initialized for the lifetime of the process.
    OLE_RESULT
        .get_or_init(|| unsafe { OleInitialize(None) })
        .clone()
}

struct LazyRemoteData {
    app: AppHandle,
    tab_id: String,
    items: Vec<RemoteFileDragItem>,
    stage_root: PathBuf,
    staged_paths: OnceLock<Result<Vec<PathBuf>>>,
    shell_data_object: Mutex<Option<IDataObject>>,
}

impl LazyRemoteData {
    fn new(
        app: AppHandle,
        tab_id: String,
        items: Vec<RemoteFileDragItem>,
        stage_root: PathBuf,
    ) -> Self {
        Self {
            app,
            tab_id,
            items,
            stage_root,
            staged_paths: OnceLock::new(),
            shell_data_object: Mutex::new(None),
        }
    }

    fn materialize(&self) -> Result<Vec<PathBuf>> {
        self.staged_paths
            .get_or_init(|| {
                let app = self.app.clone();
                let tab_id = self.tab_id.clone();
                let items = self.items.clone();
                let stage_root = self.stage_root.clone();
                tauri::async_runtime::block_on(async move {
                    prepare_staged_remote_paths(&app, &tab_id, &items, &stage_root)
                        .await
                        .map_err(|error| Error::new(E_UNEXPECTED, HSTRING::from(error.to_string())))
                })
            })
            .clone()
    }

    fn shell_data_object(&self) -> Result<IDataObject> {
        let paths = self.materialize()?;
        let mut guard = self
            .shell_data_object
            .lock()
            .map_err(|_| Error::new(E_UNEXPECTED, HSTRING::from("拖出数据对象状态损坏")))?;
        if let Some(data_object) = guard.as_ref() {
            return Ok(data_object.clone());
        }

        let data_object = get_file_data_object(&paths)?;
        *guard = Some(data_object.clone());
        Ok(data_object)
    }
}

#[implement(IDataObject)]
struct LazyDataObject {
    data: LazyRemoteData,
}

#[implement(IEnumFORMATETC)]
struct HdropFormatEnumerator {
    next_index: Mutex<u32>,
}

fn hdrop_format_etc() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

#[allow(non_snake_case)]
impl IEnumFORMATETC_Impl for HdropFormatEnumerator {
    fn Next(&self, celt: u32, rgelt: *mut FORMATETC, pceltfetched: *mut u32) -> HRESULT {
        if celt == 0 {
            if !pceltfetched.is_null() {
                unsafe { pceltfetched.write(0) };
            }
            return S_OK;
        }
        if rgelt.is_null() || (celt != 1 && pceltfetched.is_null()) {
            return E_INVALIDARG;
        }

        let mut next_index = match self.next_index.lock() {
            Ok(next_index) => next_index,
            Err(_) => return E_UNEXPECTED,
        };
        let fetched = if *next_index == 0 {
            unsafe { rgelt.write(hdrop_format_etc()) };
            *next_index = 1;
            1
        } else {
            0
        };
        if !pceltfetched.is_null() {
            unsafe { pceltfetched.write(fetched) };
        }
        if fetched == celt {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn Skip(&self, celt: u32) -> Result<()> {
        let mut next_index = self
            .next_index
            .lock()
            .map_err(|_| Error::new(E_UNEXPECTED, HSTRING::from("拖放格式状态损坏")))?;
        let remaining = 1u32.saturating_sub((*next_index).min(1));
        let skipped = celt.min(remaining);
        *next_index += skipped;
        if skipped == celt {
            Ok(())
        } else {
            Err(Error::from_hresult(S_FALSE))
        }
    }

    fn Reset(&self) -> Result<()> {
        *self
            .next_index
            .lock()
            .map_err(|_| Error::new(E_UNEXPECTED, HSTRING::from("拖放格式状态损坏")))? = 0;
        Ok(())
    }

    fn Clone(&self) -> Result<IEnumFORMATETC> {
        let next_index = *self
            .next_index
            .lock()
            .map_err(|_| Error::new(E_UNEXPECTED, HSTRING::from("拖放格式状态损坏")))?;
        Ok(Self {
            next_index: Mutex::new(next_index),
        }
        .into())
    }
}

#[implement(IDropSource)]
struct DropSource(());

#[allow(non_snake_case)]
impl IDropSource_Impl for DropSource {
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
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

fn is_supported_hdrop(format_etc: *const FORMATETC) -> bool {
    let Some(format_etc) = (unsafe { format_etc.as_ref() }) else {
        return false;
    };

    format_etc.cfFormat == CF_HDROP.0
        && format_etc.tymed & TYMED_HGLOBAL.0 as u32 != 0
        && format_etc.dwAspect == DVASPECT_CONTENT.0
}

#[allow(non_snake_case)]
impl IDataObject_Impl for LazyDataObject {
    fn GetData(&self, format_etc: *const FORMATETC) -> Result<STGMEDIUM> {
        if format_etc.is_null() {
            return Err(Error::new(E_INVALIDARG, HSTRING::from("拖放格式为空")));
        }
        // The first actual data request is the lazy boundary. Explorer,
        // Nautilus-like Windows clients, and FileTerm's own drop target all
        // ask for CF_HDROP only after accepting the drag.
        if is_supported_hdrop(format_etc) {
            return unsafe { self.data.shell_data_object()?.GetData(format_etc) };
        }

        let guard = self
            .data
            .shell_data_object
            .lock()
            .map_err(|_| Error::new(E_UNEXPECTED, HSTRING::from("拖出数据对象状态损坏")))?;
        unsafe {
            guard
                .as_ref()
                .ok_or_else(|| Error::new(E_NOTIMPL, HSTRING::from("暂不支持该拖放格式")))?
                .GetData(format_etc)
        }
    }

    fn GetDataHere(&self, _format_etc: *const FORMATETC, _medium: *mut STGMEDIUM) -> Result<()> {
        Err(Error::new(E_NOTIMPL, HSTRING::from("不支持就地拖放数据")))
    }

    fn QueryGetData(&self, format_etc: *const FORMATETC) -> HRESULT {
        if format_etc.is_null() {
            return E_INVALIDARG;
        }
        if is_supported_hdrop(format_etc) {
            return S_OK;
        }

        match self.data.shell_data_object.lock() {
            Ok(guard) => guard
                .as_ref()
                .map(|data_object| unsafe { data_object.QueryGetData(format_etc) })
                .unwrap_or(E_NOTIMPL),
            Err(_) => E_UNEXPECTED,
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        _format_etc_in: *const FORMATETC,
        format_etc_out: *mut FORMATETC,
    ) -> HRESULT {
        if !format_etc_out.is_null() {
            unsafe {
                (*format_etc_out).ptd = std::ptr::null_mut();
            }
        }
        E_NOTIMPL
    }

    fn SetData(
        &self,
        format_etc: *const FORMATETC,
        medium: *const STGMEDIUM,
        release: BOOL,
    ) -> Result<()> {
        let guard = self
            .data
            .shell_data_object
            .lock()
            .map_err(|_| Error::new(E_UNEXPECTED, HSTRING::from("拖出数据对象状态损坏")))?;
        unsafe {
            guard
                .as_ref()
                .ok_or_else(|| Error::new(E_NOTIMPL, HSTRING::from("暂不支持设置拖放数据")))?
                .SetData(format_etc, medium, release.as_bool())
        }
    }

    fn EnumFormatEtc(&self, direction: u32) -> Result<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(Error::new(E_NOTIMPL, HSTRING::from("不支持写入拖放格式")));
        }
        Ok(HdropFormatEnumerator {
            next_index: Mutex::new(0),
        }
        .into())
    }

    fn DAdvise(
        &self,
        _format_etc: *const FORMATETC,
        _flags: u32,
        _sink: Ref<'_, IAdviseSink>,
    ) -> Result<u32> {
        Err(Error::new(
            OLE_E_ADVISENOTSUPPORTED,
            HSTRING::from("不支持拖放通知"),
        ))
    }

    fn DUnadvise(&self, _connection: u32) -> Result<()> {
        Err(Error::new(
            OLE_E_ADVISENOTSUPPORTED,
            HSTRING::from("不支持拖放通知"),
        ))
    }

    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(Error::new(
            OLE_E_ADVISENOTSUPPORTED,
            HSTRING::from("不支持拖放通知"),
        ))
    }
}

pub async fn start_remote_file_drag(
    app: &AppHandle,
    window_label: &str,
    tab_id: &str,
    items: Vec<RemoteFileDragItem>,
) -> std::result::Result<(), crate::AppError> {
    let window = app
        .get_webview_window(window_label)
        .ok_or_else(|| crate::AppError::Command("拖出窗口不存在".to_string()))?;
    let (result_sender, result_receiver) = oneshot::channel::<std::result::Result<(), String>>();
    let app_for_main = app.clone();
    let window_label = window_label.to_string();
    let tab_id = tab_id.to_string();
    window
        .run_on_main_thread(move || {
            let result =
                start_remote_file_drag_on_main_thread(app_for_main, window_label, tab_id, items);
            let _ = result_sender.send(result);
        })
        .map_err(|error| crate::AppError::Command(format!("切换到原生拖出线程失败：{error}")))?;

    result_receiver
        .await
        .map_err(|_| crate::AppError::Command("原生拖出线程未响应".to_string()))?
        .map_err(crate::AppError::Command)
}

fn start_remote_file_drag_on_main_thread(
    app: AppHandle,
    window_label: String,
    tab_id: String,
    items: Vec<RemoteFileDragItem>,
) -> std::result::Result<(), String> {
    init_ole().map_err(|error| error.to_string())?;

    let stage_root =
        std::env::temp_dir().join(format!("fileterm-remote-drag-{}", uuid::Uuid::new_v4()));
    let data = LazyRemoteData::new(app.clone(), tab_id, items, stage_root.clone());
    let data_object: IDataObject = LazyDataObject { data }.into();
    let drop_source: IDropSource = DropSource(()).into();

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
        schedule_staging_cleanup(stage_root);
    } else {
        let app_for_cleanup = app.clone();
        let stage_root_for_cleanup = stage_root.clone();
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
        remove_staging_dir_sync(&stage_root);
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

fn get_file_data_object(paths: &[PathBuf]) -> Result<IDataObject> {
    if paths.is_empty() {
        return Err(Error::new(E_INVALIDARG, HSTRING::from("没有可拖出的文件")));
    }
    let shell_item_array = get_shell_item_array(paths)?;
    unsafe { shell_item_array.BindToHandler(None, &BHID_DataObject) }
}

fn get_shell_item_array(paths: &[PathBuf]) -> Result<IShellItemArray> {
    let item_ids: Vec<OwnedPidl> = paths
        .iter()
        .map(get_file_item_id)
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
            HSTRING::from(format!("无法解析拖出路径：{}", path.display())),
        ));
    }
    Ok(OwnedPidl(item_id))
}
