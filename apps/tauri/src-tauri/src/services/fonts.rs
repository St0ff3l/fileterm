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
    let paths = match FontPaths::for_app(app) {
        Ok(paths) => paths,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!("list path resolution failed: {error}"),
            );
            return Err(error);
        }
    };
    let mut index = match read_index(&paths) {
        Ok(index) => index,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!(
                    "list index read failed path={}: {error}",
                    paths.index_path.display()
                ),
            );
            return Err(error);
        }
    };

    let before = index.fonts.len();
    let mut stale_ids = Vec::new();
    index.fonts.retain(|font| match paths.font_path(font) {
        Ok(path) if path.is_file() => true,
        Ok(_) | Err(_) => {
            stale_ids.push(font.id.clone());
            false
        }
    });
    if index.fonts.len() != before {
        index.version = FONT_INDEX_VERSION;
        if let Err(error) = write_index(&paths, &index) {
            crate::services::logging::error(
                app,
                "fonts",
                format!(
                    "stale index cleanup failed path={} removed={} error={error}",
                    paths.index_path.display(),
                    stale_ids.len()
                ),
            );
            return Err(error);
        }
        crate::services::logging::warn(
            app,
            "fonts",
            format!(
                "removed stale font index entries path={} count={} ids={}",
                paths.index_path.display(),
                stale_ids.len(),
                stale_ids.join(",")
            ),
        );
    }
    crate::services::logging::debug(
        app,
        "fonts",
        format!(
            "list completed index={} fonts={} stale_removed={}",
            paths.index_path.display(),
            index.fonts.len(),
            before.saturating_sub(index.fonts.len())
        ),
    );
    Ok(index.fonts)
}

pub async fn import(app: &AppHandle) -> Result<Option<ImportedFont>, AppError> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("导入字体")
        .add_filter("字体文件", &["ttf", "otf"])
        .pick_file()
        .await
    else {
        crate::services::logging::debug(app, "fonts", "import canceled");
        return Ok(None);
    };

    let source_path = file.path().to_path_buf();
    let format = match source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|extension| matches!(extension.as_str(), "ttf" | "otf"))
    {
        Some(format) => format,
        None => {
            crate::services::logging::warn(
                app,
                "fonts",
                format!(
                    "import rejected unsupported extension source={}",
                    source_path.display()
                ),
            );
            return Err(AppError::Command(
                "只支持导入 .ttf 或 .otf 字体文件。".to_string(),
            ));
        }
    };
    let file_name = match source_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
    {
        Some(file_name) => file_name,
        None => {
            crate::services::logging::error(
                app,
                "fonts",
                format!(
                    "import could not read source filename source={}",
                    source_path.display()
                ),
            );
            return Err(AppError::Command("无法读取字体文件名。".to_string()));
        }
    };
    let bytes = match fs::read(&source_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!(
                    "import source read failed source={} error={error}",
                    source_path.display()
                ),
            );
            return Err(AppError::Storage(error.to_string()));
        }
    };
    if bytes.is_empty() || bytes.len() > MAX_FONT_BYTES {
        crate::services::logging::warn(
            app,
            "fonts",
            format!(
                "import rejected invalid size source={} bytes={} max_bytes={MAX_FONT_BYTES}",
                source_path.display(),
                bytes.len()
            ),
        );
        return Err(AppError::Command(
            "字体文件大小必须在 1B 到 32MB 之间。".to_string(),
        ));
    }

    let content_hash = format!("{:x}", Sha256::digest(&bytes));
    crate::services::logging::debug(
        app,
        "fonts",
        format!(
            "import source accepted file={} format={} bytes={} hash={}",
            file_name,
            format,
            bytes.len(),
            content_hash
        ),
    );
    let paths = match FontPaths::for_app(app) {
        Ok(paths) => paths,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!("import path resolution failed: {error}"),
            );
            return Err(error);
        }
    };
    let mut index = match read_index(&paths) {
        Ok(index) => index,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!(
                    "import index read failed path={} error={error}",
                    paths.index_path.display()
                ),
            );
            return Err(error);
        }
    };
    if let Some(existing) = index
        .fonts
        .iter()
        .find(|font| font.content_hash.eq_ignore_ascii_case(&content_hash))
    {
        let existing_path = match paths.font_path(existing) {
            Ok(path) => path,
            Err(error) => {
                crate::services::logging::error(
                    app,
                    "fonts",
                    format!(
                        "duplicate entry has invalid path id={} error={error}",
                        existing.id
                    ),
                );
                return Err(error);
            }
        };
        let repaired = match ensure_font_file(&paths, existing, &bytes) {
            Ok(repaired) => repaired,
            Err(error) => {
                crate::services::logging::error(
                    app,
                    "fonts",
                    format!(
                        "duplicate font repair failed id={} path={} error={error}",
                        existing.id,
                        existing_path.display()
                    ),
                );
                return Err(error);
            }
        };
        crate::services::logging::info(
            app,
            "fonts",
            format!(
                "duplicate import reused id={} path={} repaired={repaired}",
                existing.id,
                existing_path.display()
            ),
        );
        return Ok(Some(existing.clone()));
    }

    let family = font_family_from_file_name(&file_name);
    let font = ImportedFont {
        id: format!("font-{content_hash}"),
        family,
        format,
        file_name,
        content_hash,
        imported_at: now_millis(),
    };
    let font_path = match paths.font_path(&font) {
        Ok(path) => path,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!(
                    "new font path construction failed id={} error={error}",
                    font.id
                ),
            );
            return Err(error);
        }
    };
    if let Err(error) = ensure_font_file(&paths, &font, &bytes) {
        crate::services::logging::error(
            app,
            "fonts",
            format!(
                "new font file write failed id={} path={} error={error}",
                font.id,
                font_path.display()
            ),
        );
        return Err(error);
    }
    index.version = FONT_INDEX_VERSION;
    index.fonts.insert(0, font.clone());
    if let Err(error) = write_index(&paths, &index) {
        let _ = fs::remove_file(&font_path);
        crate::services::logging::error(
            app,
            "fonts",
            format!(
                "new font index write failed id={} index={} error={error}",
                font.id,
                paths.index_path.display()
            ),
        );
        return Err(error);
    }

    crate::services::logging::info(
        app,
        "fonts",
        format!(
            "font imported id={} family={} format={} bytes={} path={}",
            font.id,
            font.family,
            font.format,
            bytes.len(),
            font_path.display()
        ),
    );
    Ok(Some(font))
}

pub fn data_url(app: &AppHandle, font_id: &str) -> Result<Option<String>, AppError> {
    if !is_safe_font_id(font_id) {
        crate::services::logging::warn(app, "fonts", format!("data request rejected id={font_id}"));
        return Err(AppError::Command("无效的字体 ID。".to_string()));
    }
    let paths = match FontPaths::for_app(app) {
        Ok(paths) => paths,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!("data path resolution failed: {error}"),
            );
            return Err(error);
        }
    };
    let index = match read_index(&paths) {
        Ok(index) => index,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!(
                    "data index read failed id={} path={} error={error}",
                    font_id,
                    paths.index_path.display()
                ),
            );
            return Err(error);
        }
    };
    let Some(font) = index.fonts.into_iter().find(|font| font.id == font_id) else {
        crate::services::logging::debug(app, "fonts", format!("data request unknown id={font_id}"));
        return Ok(None);
    };
    let path = match paths.font_path(&font) {
        Ok(path) => path,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!("data path construction failed id={font_id} error={error}"),
            );
            return Err(error);
        }
    };
    if !path.is_file() {
        crate::services::logging::warn(
            app,
            "fonts",
            format!(
                "font data file missing id={font_id} path={}",
                path.display()
            ),
        );
        return Ok(None);
    }
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!(
                    "font data read failed id={font_id} path={} error={error}",
                    path.display()
                ),
            );
            return Err(AppError::Storage(error.to_string()));
        }
    };
    let mime = if font.format == "otf" {
        "font/otf"
    } else {
        "font/ttf"
    };
    crate::services::logging::debug(
        app,
        "fonts",
        format!(
            "font data loaded id={font_id} bytes={} path={}",
            bytes.len(),
            path.display()
        ),
    );
    Ok(Some(format!(
        "data:{mime};base64,{}",
        STANDARD.encode(bytes)
    )))
}

pub fn delete(app: &AppHandle, font_id: &str) -> Result<bool, AppError> {
    if !is_safe_font_id(font_id) {
        crate::services::logging::warn(
            app,
            "fonts",
            format!("delete request rejected id={font_id}"),
        );
        return Err(AppError::Command("无效的字体 ID。".to_string()));
    }
    let paths = match FontPaths::for_app(app) {
        Ok(paths) => paths,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!("delete path resolution failed: {error}"),
            );
            return Err(error);
        }
    };
    let mut index = match read_index(&paths) {
        Ok(index) => index,
        Err(error) => {
            crate::services::logging::error(
                app,
                "fonts",
                format!(
                    "delete index read failed id={} path={} error={error}",
                    font_id,
                    paths.index_path.display()
                ),
            );
            return Err(error);
        }
    };
    let Some(position) = index.fonts.iter().position(|font| font.id == font_id) else {
        crate::services::logging::debug(app, "fonts", format!("delete unknown id={font_id}"));
        return Ok(false);
    };

    let font = index.fonts.remove(position);
    if let Err(error) = write_index(&paths, &index) {
        crate::services::logging::error(
            app,
            "fonts",
            format!(
                "delete index write failed id={} path={} error={error}",
                font_id,
                paths.index_path.display()
            ),
        );
        return Err(error);
    }
    match paths.font_path(&font) {
        Ok(font_path) if font_path.is_file() => {
            if let Err(error) = fs::remove_file(&font_path) {
                crate::services::logging::warn(
                    app,
                    "fonts",
                    format!(
                        "font file delete failed id={} path={} error={error}",
                        font_id,
                        font_path.display()
                    ),
                );
            }
        }
        Ok(_) => {}
        Err(error) => {
            crate::services::logging::warn(
                app,
                "fonts",
                format!("deleted font has invalid file path id={font_id} error={error}"),
            );
        }
    }
    crate::services::logging::info(app, "fonts", format!("font deleted id={font_id}"));
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
    index.fonts.retain(|font| {
        is_safe_font_id(&font.id)
            && matches!(font.format.as_str(), "ttf" | "otf")
            && is_valid_content_hash(&font.content_hash)
    });
    Ok(index)
}

fn ensure_font_file(
    paths: &FontPaths,
    font: &ImportedFont,
    bytes: &[u8],
) -> Result<bool, AppError> {
    let path = paths.font_path(font)?;
    let needs_write = match fs::read(&path) {
        Ok(existing) => {
            let existing_hash = format!("{:x}", Sha256::digest(existing));
            !existing_hash.eq_ignore_ascii_case(&font.content_hash)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => return Err(AppError::Storage(error.to_string())),
    };
    if !needs_write {
        return Ok(false);
    }

    fs::create_dir_all(&paths.fonts_dir).map_err(|error| AppError::Storage(error.to_string()))?;
    write_bytes_atomic(&path, bytes)?;
    Ok(true)
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

fn is_valid_content_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
    use super::{
        ensure_font_file, font_family_from_file_name, is_safe_font_id, is_valid_content_hash,
        FontPaths, ImportedFont,
    };
    use sha2::{Digest, Sha256};
    use std::fs;

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

    #[test]
    fn accepts_only_sha256_content_hashes() {
        assert!(is_valid_content_hash(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(is_valid_content_hash(
            "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789"
        ));
        assert!(!is_valid_content_hash("not-a-hash"));
        assert!(!is_valid_content_hash(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeg"
        ));
    }

    #[test]
    fn repairs_missing_or_corrupted_duplicate_font_files() {
        let root =
            std::env::temp_dir().join(format!("fileterm-font-repair-{}", uuid::Uuid::new_v4()));
        let paths = FontPaths {
            index_path: root.join("fonts.json"),
            fonts_dir: root.join("fonts"),
        };
        let bytes = b"font-fixture";
        let hash = format!("{:x}", Sha256::digest(bytes));
        let font = ImportedFont {
            id: format!("font-{hash}"),
            family: "Fixture".to_string(),
            format: "ttf".to_string(),
            file_name: "fixture.ttf".to_string(),
            content_hash: hash,
            imported_at: 1,
        };

        assert!(ensure_font_file(&paths, &font, bytes).unwrap());
        assert_eq!(
            fs::read(root.join("fonts").join(format!("{}.ttf", font.id))).unwrap(),
            bytes
        );
        assert!(!ensure_font_file(&paths, &font, bytes).unwrap());

        fs::write(
            root.join("fonts").join(format!("{}.ttf", font.id)),
            b"corrupt",
        )
        .unwrap();
        assert!(ensure_font_file(&paths, &font, bytes).unwrap());
        assert_eq!(
            fs::read(root.join("fonts").join(format!("{}.ttf", font.id))).unwrap(),
            bytes
        );

        fs::remove_dir_all(root).unwrap();
    }
}
