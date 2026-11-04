use crate::{storage, AppError};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;

static BOOKMARK_LOCK: Mutex<()> = Mutex::new(());
const STORE: &str = "path-bookmarks.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathBookmark {
    id: String,
    scope: String,
    path: String,
    name: String,
    #[serde(rename = "type")]
    kind: BookmarkKind,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum BookmarkKind {
    File,
    Folder,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BookmarkAction {
    Add,
    Remove,
    Up,
    Down,
}

fn read(app: &AppHandle) -> Result<Vec<PathBookmark>, AppError> {
    storage::read_json_array(app, STORE)?
        .into_iter()
        .map(|value| {
            serde_json::from_value(value).map_err(|e| AppError::Serialization(e.to_string()))
        })
        .collect()
}

fn apply(
    items: &mut Vec<PathBookmark>,
    scope: &str,
    action: BookmarkAction,
    mut bookmark: PathBookmark,
) -> Result<(), AppError> {
    if scope != "local"
        && !scope
            .strip_prefix("remote:")
            .is_some_and(|id| !id.is_empty())
    {
        return Err(AppError::Command("Invalid bookmark scope".into()));
    }
    match action {
        BookmarkAction::Add => {
            if bookmark.path.trim().is_empty()
                || bookmark.path.contains('\0')
                || bookmark.path.len() > 32768
            {
                return Err(AppError::Command("Invalid bookmark path".into()));
            }
            if items
                .iter()
                .any(|item| item.scope == scope && item.path == bookmark.path)
            {
                return Ok(());
            }
            bookmark.id = uuid::Uuid::new_v4().to_string();
            bookmark.scope = scope.to_string();
            items.push(bookmark);
        }
        BookmarkAction::Remove => {
            items.retain(|item| item.scope != scope || item.id != bookmark.id)
        }
        BookmarkAction::Up | BookmarkAction::Down => {
            let indices: Vec<_> = items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.scope == scope)
                .map(|(i, _)| i)
                .collect();
            if let Some(position) = indices.iter().position(|&i| items[i].id == bookmark.id) {
                let next = if matches!(action, BookmarkAction::Up) {
                    position.checked_sub(1)
                } else {
                    position.checked_add(1)
                };
                if let Some(next) = next.filter(|&next| next < indices.len()) {
                    items.swap(indices[position], indices[next]);
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn app_get_path_bookmarks(
    app: AppHandle,
    scope: String,
) -> Result<Vec<PathBookmark>, AppError> {
    let _guard = BOOKMARK_LOCK
        .lock()
        .map_err(|e| AppError::Storage(e.to_string()))?;
    Ok(read(&app)?
        .into_iter()
        .filter(|item| item.scope == scope)
        .collect())
}

#[tauri::command]
pub fn app_update_path_bookmark(
    app: AppHandle,
    scope: String,
    action: BookmarkAction,
    bookmark: PathBookmark,
) -> Result<Vec<PathBookmark>, AppError> {
    let _guard = BOOKMARK_LOCK
        .lock()
        .map_err(|e| AppError::Storage(e.to_string()))?;
    let mut items = read(&app)?;
    apply(&mut items, &scope, action, bookmark)?;
    let values = items
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Serialization(e.to_string()))?;
    storage::write_json_array(&app, STORE, &values)?;
    Ok(items
        .into_iter()
        .filter(|item| item.scope == scope)
        .collect())
}

#[cfg(test)]
mod tests;
