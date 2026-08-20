use std::fs;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::AppHandle;

use crate::AppError;

const FONT_INDEX_VERSION: u32 = 1;
const MAX_FONT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFont {
    pub id: String,
    pub family: String,
    pub format: String,
    pub file_name: String,
    pub content_hash: String,
    pub imported_at: u64,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportedFontIndex {
    version: u32,
    fonts: Vec<ImportedFont>,
}

struct FontPaths {
    index_path: PathBuf,
    fonts_dir: PathBuf,
}

impl FontPaths {
    fn for_app(app: &AppHandle) -> Result<Self, AppError> {
        let index_path = crate::storage::workspace_file(app, "fonts.json")?;
        let storage_root = index_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::Storage("无法解析字体存储目录".to_string()))?;
        Ok(Self {
            index_path,
            fonts_dir: storage_root.join("fonts"),
        })
    }

    fn font_path(&self, font: &ImportedFont) -> Result<PathBuf, AppError> {
        if !is_safe_font_id(&font.id) || !matches!(font.format.as_str(), "ttf" | "otf") {
            return Err(AppError::Storage("字体索引包含无效文件名".to_string()));
        }
        Ok(self.fonts_dir.join(format!("{}.{}", font.id, font.format)))
    }
}

pub fn list(app: &AppHandle) -> Result<Vec<ImportedFont>, AppError> {
    let paths = FontPaths::for_app(app)?;
    Ok(read_index(&paths)?.fonts)
}

pub async fn import(app: &AppHandle) -> Result<Option<ImportedFont>, AppError> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("导入字体")
        .add_filter("字体文件", &["ttf", "otf"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };

    let source_path = file.path().to_path_buf();
    let format = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|extension| matches!(extension.as_str(), "ttf" | "otf"))
        .ok_or_else(|| AppError::Command("只支持导入 .ttf 或 .otf 字体文件。".to_string()))?;
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::Command("无法读取字体文件名。".to_string()))?;
    let bytes = fs::read(&source_path).map_err(|error| AppError::Storage(error.to_string()))?;
    if bytes.is_empty() || bytes.len() > MAX_FONT_BYTES {
        return Err(AppError::Command(
            "字体文件大小必须在 1B 到 32MB 之间。".to_string(),
        ));
    }

    let content_hash = format!("{:x}", Sha256::digest(&bytes));
    let paths = FontPaths::for_app(app)?;
    let mut index = read_index(&paths)?;
    if let Some(existing) = index
        .fonts
        .iter()
        .find(|font| font.content_hash.eq_ignore_ascii_case(&content_hash))
    {
        return Ok(Some(existing.clone()));
    }

    fs::create_dir_all(&paths.fonts_dir).map_err(|error| AppError::Storage(error.to_string()))?;
    let family = font_family_from_file_name(&file_name);
    let font = ImportedFont {
        id: format!("font-{content_hash}"),
        family,
        format,
        file_name,
        content_hash,
        imported_at: now_millis(),
    };
    let font_path = paths.font_path(&font)?;
    write_bytes_atomic(&font_path, &bytes)?;
    index.version = FONT_INDEX_VERSION;
    index.fonts.insert(0, font.clone());
    if let Err(error) = write_index(&paths, &index) {
        let _ = fs::remove_file(&font_path);
        return Err(error);
    }

    Ok(Some(font))
}

pub fn data_url(app: &AppHandle, font_id: &str) -> Result<Option<String>, AppError> {
    if !is_safe_font_id(font_id) {
        return Err(AppError::Command("无效的字体 ID。".to_string()));
    }
    let paths = FontPaths::for_app(app)?;
    let Some(font) = read_index(&paths)?
        .fonts
        .into_iter()
        .find(|font| font.id == font_id)
    else {
        return Ok(None);
    };
    let path = paths.font_path(&font)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mime = if font.format == "otf" {
        "font/otf"
    } else {
        "font/ttf"
    };
    Ok(Some(format!(
        "data:{mime};base64,{}",
        STANDARD.encode(bytes)
    )))
}

pub fn delete(app: &AppHandle, font_id: &str) -> Result<bool, AppError> {
    if !is_safe_font_id(font_id) {
        return Err(AppError::Command("无效的字体 ID。".to_string()));
    }
    let paths = FontPaths::for_app(app)?;
    let mut index = read_index(&paths)?;
    let Some(position) = index.fonts.iter().position(|font| font.id == font_id) else {
        return Ok(false);
    };

    let font = index.fonts.remove(position);
    write_index(&paths, &index)?;
    if let Ok(font_path) = paths.font_path(&font) {
        if font_path.exists() {
            let _ = fs::remove_file(font_path);
        }
    }
    Ok(true)
}

fn read_index(paths: &FontPaths) -> Result<ImportedFontIndex, AppError> {
    if !paths.index_path.exists() {
        return Ok(ImportedFontIndex {
            version: FONT_INDEX_VERSION,
            fonts: Vec::new(),
        });
    }
    let content = fs::read_to_string(&paths.index_path)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    let mut index: ImportedFontIndex = serde_json::from_str(&content)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    index
        .fonts
        .retain(|font| is_safe_font_id(&font.id) && matches!(font.format.as_str(), "ttf" | "otf"));
    Ok(index)
}

fn write_index(paths: &FontPaths, index: &ImportedFontIndex) -> Result<(), AppError> {
    let content = serde_json::to_vec_pretty(index)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    write_bytes_atomic(&paths.index_path, &content)
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
    }
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    crate::storage::write_restricted_file(&temporary, bytes)?;
    if let Err(error) = crate::storage::replace_file_atomically(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

fn is_safe_font_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn font_family_from_file_name(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Imported Font");
    let mut family = String::with_capacity(stem.len());
    let mut previous_was_space = false;
    for character in stem.chars() {
        let allowed =
            character.is_alphanumeric() || matches!(character, ' ' | '.' | '-' | '_' | '\'');
        if allowed {
            if character.is_whitespace() {
                if !previous_was_space {
                    family.push(' ');
                }
                previous_was_space = true;
            } else {
                family.push(character);
                previous_was_space = false;
            }
        } else if !previous_was_space {
            family.push(' ');
            previous_was_space = true;
        }
    }
    let family = family.trim().trim_matches('.').trim().to_string();
    if family.is_empty() {
        "Imported Font".to_string()
    } else {
        family.chars().take(96).collect()
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{font_family_from_file_name, is_safe_font_id};

    #[test]
    fn derives_safe_family_from_filename() {
        assert_eq!(
            font_family_from_file_name("JetBrains-Mono_v2.ttf"),
            "JetBrains-Mono_v2"
        );
        assert_eq!(
            font_family_from_file_name("font_with?name.otf"),
            "font_with name"
        );
    }

    #[test]
    fn accepts_only_safe_ids() {
        assert!(is_safe_font_id("font-abc_123"));
        assert!(is_safe_font_id("font-1234567890abcdef"));
        assert!(!is_safe_font_id("../font"));
        assert!(!is_safe_font_id("/root/font"));
        assert!(!is_safe_font_id("font/evil"));
        assert!(!is_safe_font_id(""));
    }
}
