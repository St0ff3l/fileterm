//! Signed portable updates use a copy of the running binary as a short-lived
//! helper. It runs before Tauri/CLI initialization and never opens a workspace.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

const PREFIX: &str = ".fileterm-update-";
const HELPER: &str = "helper.exe";
const PAYLOAD: &str = "payload.exe";
const BACKUP: &str = "previous.exe";

#[derive(Serialize, Deserialize)]
struct Transaction {
    target: String,
    parent_pid: u32,
    payload_hash: String,
    original_hash: String,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_target(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['/', '\\', ':'])
        && name.to_ascii_lowercase().ends_with(".exe")
}

fn valid_stage(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(PREFIX))
        .is_some_and(|suffix| uuid::Uuid::parse_str(suffix).is_ok())
        && fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("无法创建更新文件: {error}"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("无法保存更新文件: {error}"))
}

fn prepare(executable: &Path, bytes: &[u8]) -> Result<std::path::PathBuf, String> {
    let target = executable
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| valid_target(name))
        .ok_or("便携版程序文件名无效")?;
    let root = executable.parent().ok_or("便携版程序目录无效")?;
    let stage = root.join(format!("{PREFIX}{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage).map_err(|error| format!("程序目录不可写，无法准备更新: {error}"))?;
    let result = (|| {
        let original = fs::read(executable).map_err(|error| error.to_string())?;
        write_synced(&stage.join(HELPER), &original)?;
        write_synced(&stage.join(PAYLOAD), bytes)?;
        let transaction = Transaction {
            target: target.to_string(),
            parent_pid: std::process::id(),
            payload_hash: hash(bytes),
            original_hash: hash(&original),
        };
        let json = serde_json::to_vec(&transaction).map_err(|error| error.to_string())?;
        write_synced(&stage.join("transaction.json"), &json)
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&stage);
        return Err(error);
    }
    Ok(stage)
}

fn read_transaction(stage: &Path) -> Result<Transaction, String> {
    if !valid_stage(stage) {
        return Err("便携版更新目录无效".to_string());
    }
    let transaction: Transaction = serde_json::from_slice(
        &fs::read(stage.join("transaction.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if !valid_target(&transaction.target) {
        return Err("便携版更新目标无效".to_string());
    }
    Ok(transaction)
}

// Only program files participate in the transaction. config/ and the portable
// marker remain in the original directory, including when the EXE was renamed.
fn replace_and_launch(
    stage: &Path,
    transaction: &Transaction,
    launch: impl Fn(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let target = stage
        .parent()
        .ok_or("更新根目录无效")?
        .join(&transaction.target);
    let payload = stage.join(PAYLOAD);
    let backup = stage.join(BACKUP);
    if hash(&fs::read(&payload).map_err(|error| error.to_string())?) != transaction.payload_hash {
        return Err("已验证的更新文件发生变化，拒绝替换".to_string());
    }
    if hash(&fs::read(&target).map_err(|error| error.to_string())?) != transaction.original_hash {
        return Err("原程序发生变化，拒绝覆盖".to_string());
    }
    fs::rename(&target, &backup).map_err(|error| format!("无法备份原程序: {error}"))?;
    let result = fs::rename(&payload, &target)
        .map_err(|error| format!("无法替换程序: {error}"))
        .and_then(|()| launch(&target));
    if let Err(error) = result {
        // Preserve the failed new payload and restore the original EXE. Never
        // discard the backup if rollback itself fails.
        if target.exists() {
            fs::rename(&target, &payload)
                .map_err(|rollback| format!("{error}; 无法移走新程序: {rollback}"))?;
        }
        fs::rename(&backup, &target).map_err(|rollback| {
            format!("{error}; 原程序保留在 {}: {rollback}", backup.display())
        })?;
        return Err(error);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub struct PreparedUpdate {
    child: std::process::Child,
    stage: std::path::PathBuf,
    committed: bool,
}

#[cfg(target_os = "windows")]
impl PreparedUpdate {
    pub fn commit(mut self) {
        self.committed = true;
    }
}

#[cfg(target_os = "windows")]
impl Drop for PreparedUpdate {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = fs::remove_dir_all(&self.stage);
        }
    }
}

#[cfg(target_os = "windows")]
pub async fn install(bytes: &[u8]) -> Result<PreparedUpdate, String> {
    use std::os::windows::process::CommandExt;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let stage = prepare(&executable, bytes)?;
    let child = match std::process::Command::new(stage.join(HELPER))
        .arg("--fileterm-portable-update")
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_dir_all(&stage);
            return Err(format!("无法启动便携版更新程序: {error}"));
        }
    };
    // Confirm the helper owns a handle to this exact process before requesting
    // exit. A helper startup failure must leave the user's sessions running.
    let mut prepared = PreparedUpdate {
        child,
        stage,
        committed: false,
    };
    for _ in 0..100 {
        if prepared.stage.join("ready").is_file() {
            return Ok(prepared);
        }
        if prepared
            .child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("便携版更新程序未就绪，请重试".to_string())
}

#[cfg(target_os = "windows")]
fn wait_for_parent(stage: &Path, pid: u32) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    // The parent is alive until it observes ready. Holding its kernel handle
    // avoids PID reuse and does not rely on a guessed delay or taskkill.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return Err(format!(
            "无法等待原程序退出: {}",
            std::io::Error::last_os_error()
        ));
    }
    let ready = write_synced(&stage.join("ready"), b"");
    let wait = if ready.is_ok() {
        unsafe { WaitForSingleObject(handle, 60_000) }
    } else {
        1
    };
    unsafe { CloseHandle(handle) };
    ready?;
    if wait != WAIT_OBJECT_0 {
        return Err("原程序未退出，取消更新".to_string());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn run_helper() -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let stage = executable.parent().ok_or("更新程序路径无效")?;
    if executable.file_name().is_none_or(|name| name != HELPER) {
        return Err("仅允许从更新事务目录运行辅助程序".to_string());
    }
    let transaction = read_transaction(stage)?;
    // If waiting fails the parent may still be alive: never start a duplicate.
    wait_for_parent(stage, transaction.parent_pid)?;
    let launch = |target: &Path| {
        std::process::Command::new(target)
            .current_dir(target.parent().ok_or("程序目录无效")?)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("无法启动程序: {error}"))
    };
    if let Err(error) = replace_and_launch(stage, &transaction, launch) {
        let _ = fs::write(stage.join("error.txt"), &error);
        let target = stage
            .parent()
            .ok_or("更新根目录无效")?
            .join(&transaction.target);
        // Launch only a restored/unchanged original, never a partially replaced EXE.
        if fs::read(&target).is_ok_and(|bytes| hash(&bytes) == transaction.original_hash) {
            let _ = launch(&target);
        }
        return Err(error);
    }
    let _ = fs::write(stage.join("completed"), b"");
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn cleanup_completed() {
    // The new app can start before the helper exits. Cleanup runs in the
    // background with a bounded retry so Windows can release helper.exe.
    std::thread::spawn(|| {
        let Ok(executable) = std::env::current_exe() else {
            return;
        };
        let Some(root) = executable.parent() else {
            return;
        };
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let stage = entry.path();
            if !valid_stage(&stage) {
                continue;
            }
            for _ in 0..50 {
                if stage.join("completed").is_file() && fs::remove_dir_all(&stage).is_ok() {
                    break;
                }
                // Only retry the current transaction, not old failed updates.
                if stage.join("error.txt").is_file() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    });
}

#[cfg(target_os = "windows")]
pub fn previous_error() -> Option<String> {
    let executable = std::env::current_exe().ok()?;
    let root = executable.parent()?;
    for entry in fs::read_dir(root).ok()?.flatten() {
        let stage = entry.path();
        if !valid_stage(&stage) {
            continue;
        }
        let path = stage.join("error.txt");
        if let Ok(error) = fs::read_to_string(&path) {
            let _ = fs::remove_file(path);
            return Some(format!(
                "便携版更新失败，请重试（原程序或备份已保留）: {error}"
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests;
