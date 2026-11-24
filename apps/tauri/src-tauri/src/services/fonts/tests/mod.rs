use super::{
    ensure_font_file, font_family_from_file_name, font_metadata_from_bytes,
    font_metadata_from_file_name, font_weight_from_label, is_safe_font_id, is_valid_content_hash,
    migrate_legacy_font_metadata, FontPaths, ImportedFont, ImportedFontIndex,
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
    assert_eq!(
        font_family_from_file_name("MesloLGS NF Regular.ttf"),
        "MesloLGS NF"
    );
    assert_eq!(
        font_family_from_file_name("MesloLGS-NF-Bold.ttf"),
        "MesloLGS-NF"
    );
    assert_eq!(font_weight_from_label("MesloLGS NF Regular"), Some(400));
    assert_eq!(font_weight_from_label("MesloLGS NF Bold"), Some(700));
    assert_eq!(font_weight_from_label("Example Semi Bold"), Some(600));
}

#[test]
fn reads_family_and_weight_from_font_metadata() {
    let regular = font_metadata_from_bytes(
        include_bytes!("../../../../../src/renderer/assets/fonts/text/geist/Geist-400.ttf"),
        "regular.ttf",
    );
    let bold = font_metadata_from_bytes(
        include_bytes!("../../../../../src/renderer/assets/fonts/text/geist/Geist-700.ttf"),
        "bold.ttf",
    );

    assert_eq!(regular.family, bold.family);
    assert_eq!(regular.font_weight, 400);
    assert_eq!(bold.font_weight, 700);
    assert_eq!(
        font_metadata_from_file_name("fallback-Bold.ttf").font_weight,
        700
    );
}

#[test]
fn migrates_legacy_font_family_and_weight_from_stored_file() {
    let root =
        std::env::temp_dir().join(format!("fileterm-font-metadata-{}", uuid::Uuid::new_v4()));
    let paths = FontPaths {
        index_path: root.join("fonts.json"),
        fonts_dir: root.join("fonts"),
    };
    let bytes = include_bytes!("../../../../../src/renderer/assets/fonts/text/geist/Geist-700.ttf");
    let hash = format!("{:x}", Sha256::digest(bytes));
    let mut index = ImportedFontIndex {
        version: 1,
        fonts: vec![ImportedFont {
            id: format!("font-{hash}"),
            family: "Geist-700".to_string(),
            font_weight: 400,
            format: "ttf".to_string(),
            file_name: "Geist-Bold.ttf".to_string(),
            content_hash: hash,
            imported_at: 1,
        }],
    };
    fs::create_dir_all(&paths.fonts_dir).unwrap();
    fs::write(
        paths.fonts_dir.join(format!("{}.ttf", index.fonts[0].id)),
        bytes,
    )
    .unwrap();

    assert!(migrate_legacy_font_metadata(&paths, &mut index));
    assert_eq!(index.fonts[0].family, "Geist");
    assert_eq!(index.fonts[0].font_weight, 700);
    assert_eq!(index.version, 2);

    fs::remove_dir_all(root).unwrap();
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
    let root = std::env::temp_dir().join(format!("fileterm-font-repair-{}", uuid::Uuid::new_v4()));
    let paths = FontPaths {
        index_path: root.join("fonts.json"),
        fonts_dir: root.join("fonts"),
    };
    let bytes = b"font-fixture";
    let hash = format!("{:x}", Sha256::digest(bytes));
    let font = ImportedFont {
        id: format!("font-{hash}"),
        family: "Fixture".to_string(),
        font_weight: 400,
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
