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
