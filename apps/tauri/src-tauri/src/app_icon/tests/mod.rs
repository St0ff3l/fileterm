use super::platform_icon_bytes;

#[cfg(target_os = "windows")]
#[test]
fn windows_tray_uses_256px_artwork() {
    for variant in ["classic", "material", "clean"] {
        let bytes = super::tray_icon_bytes(variant);
        assert_eq!(bytes, platform_icon_bytes(variant, 256));
        let icon = tauri::image::Image::from_bytes(bytes).unwrap();
        assert_eq!((icon.width(), icon.height()), (256, 256));
    }
}

fn assert_icon_fills_canvas(bytes: &[u8], size: u32) {
    let icon = tauri::image::Image::from_bytes(bytes).unwrap();
    assert_eq!(icon.width(), size);
    assert_eq!(icon.height(), size);

    let mut bounds = [size, size, 0, 0];
    for y in 0..size {
        for x in 0..size {
            let alpha = icon.rgba()[((y * size + x) * 4 + 3) as usize];
            if alpha > 0 {
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x);
                bounds[3] = bounds[3].max(y);
            }
        }
    }
    assert!(
        bounds[0] <= 1 && bounds[1] <= 1 && bounds[2] >= size - 2 && bounds[3] >= size - 2,
        "icon artwork should fill its platform canvas, got alpha bounds {bounds:?} for {size}px"
    );
}

#[test]
fn linux_static_icons_include_the_classic_silver_rim() {
    for bytes in [
        &include_bytes!("../../../icons/app/linux/32x32.png")[..],
        &include_bytes!("../../../icons/app/linux/128x128.png")[..],
        &include_bytes!("../../../icons/app/linux/128x128@2x.png")[..],
    ] {
        let icon = tauri::image::Image::from_bytes(bytes).unwrap();
        assert_icon_fills_canvas(bytes, icon.width());
        let offset = ((icon.height() / 64 * icon.width() + icon.width() / 2) * 4) as usize;
        let rim = &icon.rgba()[offset..offset + 4];
        assert!(
            rim[0] > 100 && rim[1] > 100 && rim[2] > 100 && rim[3] > 200,
            "classic artwork must retain its visible silver top rim"
        );
    }
}

#[cfg(target_os = "windows")]
#[test]
fn windows_packaged_icon_matches_classic_runtime_icon() {
    let packaged =
        tauri::image::Image::from_bytes(include_bytes!("../../../icons/app/windows/classic.ico"))
            .unwrap();
    let classic = tauri::image::Image::from_bytes(platform_icon_bytes("classic", 256)).unwrap();
    assert_eq!(packaged.width(), classic.width());
    assert_eq!(packaged.height(), classic.height());
    assert_eq!(packaged.rgba(), classic.rgba());
}

#[test]
fn each_app_icon_variant_is_an_rgba_png() {
    for variant in ["classic", "material", "clean"] {
        let bytes = super::icon_bytes(variant);
        assert_eq!(bytes, platform_icon_bytes(variant, 1024));
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&bytes[12..16], b"IHDR");
        assert_eq!(bytes[24], 8);
        assert_eq!(bytes[25], 6);
        assert_icon_fills_canvas(bytes, 1024);
        assert_icon_fills_canvas(platform_icon_bytes(variant, 512), 512);
        assert_icon_fills_canvas(platform_icon_bytes(variant, 256), 256);
    }
}

#[test]
fn platform_tray_icon_assets_keep_native_sizes_and_rgba_format() {
    for (icon, size) in [
        (
            &include_bytes!("../../../icons/tray/linux/classic-32.png")[..],
            32,
        ),
        (
            &include_bytes!("../../../icons/tray/linux/material-32.png")[..],
            32,
        ),
        (
            &include_bytes!("../../../icons/tray/linux/clean-32.png")[..],
            32,
        ),
        (
            &include_bytes!("../../../icons/tray/windows/classic-32.png")[..],
            32,
        ),
        (
            &include_bytes!("../../../icons/tray/windows/material-32.png")[..],
            32,
        ),
        (
            &include_bytes!("../../../icons/tray/windows/clean-32.png")[..],
            32,
        ),
    ] {
        assert_eq!(&icon[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&icon[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(icon[16..20].try_into().unwrap()), size);
        assert_eq!(u32::from_be_bytes(icon[20..24].try_into().unwrap()), size);
        assert_eq!(icon[24], 8, "tray PNGs must use 8-bit channels");
        assert_eq!(icon[25], 6, "tray PNGs must use RGBA pixels");
        assert_icon_fills_canvas(icon, size);
    }
}
