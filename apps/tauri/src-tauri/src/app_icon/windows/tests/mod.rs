use super::*;
use windows::{
    core::w,
    Win32::{
        Graphics::Gdi::{DeleteObject, GetObjectW, BITMAP, HGDIOBJ},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetClassLongPtrW, GetIconInfo, SetClassLongPtrW,
            ICONINFO, WINDOW_EX_STYLE, WM_GETICON, WS_OVERLAPPED,
        },
    },
};

fn native_icon_size(handle: isize) -> (i32, i32) {
    let mut info = ICONINFO::default();
    unsafe { GetIconInfo(HICON(handle as *mut _), &mut info) }.unwrap();
    let mut bitmap = BITMAP::default();
    let copied = unsafe {
        GetObjectW(
            HGDIOBJ(info.hbmColor.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some((&mut bitmap as *mut BITMAP).cast()),
        )
    };
    unsafe {
        let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
    }
    assert_eq!(copied, std::mem::size_of::<BITMAP>() as i32);
    (bitmap.bmWidth, bitmap.bmHeight)
}

#[test]
fn native_window_updates_both_icon_sizes_and_can_restore_classic() {
    // A hidden native window exercises real WM_SETICON/WM_GETICON handling.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("FileTerm icon test"),
            WS_OVERLAPPED,
            0,
            0,
            100,
            100,
            None,
            None,
            None,
            None,
        )
    }
    .unwrap();
    let old_class_big = unsafe { GetClassLongPtrW(hwnd, GCLP_HICON) };
    let old_class_small = unsafe { GetClassLongPtrW(hwnd, GCLP_HICONSM) };
    let mut handles = Vec::new();
    for variant in ["classic", "material", "clean", "classic"] {
        apply_window_icon(hwnd, variant).unwrap();
        let get =
            |kind| unsafe { SendMessageW(hwnd, WM_GETICON, Some(WPARAM(kind as usize)), None).0 };
        let big = get(ICON_BIG);
        let small = get(ICON_SMALL);
        assert_ne!(big, 0);
        assert_ne!(small, 0);
        assert_ne!(small, big);
        assert_eq!(native_icon_size(small), (1024, 1024));
        assert_eq!(native_icon_size(big), (1024, 1024));
        assert_eq!(unsafe { GetClassLongPtrW(hwnd, GCLP_HICON) }, big as usize);
        assert_eq!(
            unsafe { GetClassLongPtrW(hwnd, GCLP_HICONSM) },
            small as usize
        );
        handles.push((small, big));
    }
    assert_ne!(handles[0], handles[1]);
    assert_ne!(handles[1], handles[2]);
    assert_ne!(handles[0], handles[2]);
    assert_eq!(handles[0], handles[3]);
    unsafe {
        SetClassLongPtrW(hwnd, GCLP_HICON, old_class_big as isize);
        SetClassLongPtrW(hwnd, GCLP_HICONSM, old_class_small as isize);
    }
    unsafe { DestroyWindow(hwnd) }.unwrap();
}

#[test]
fn shell_link_icon_refresh_only_changes_fileterm_links_and_keeps_targets() {
    let _apartment = ComApartment::initialize().unwrap();
    let temp_dir = env::temp_dir().join(format!(
        "fileterm-shell-link-test-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();
    let shortcut_path = temp_dir.join("FileTerm.lnk");
    let icon_directory = temp_dir.join("icons");
    fs::create_dir_all(&icon_directory).unwrap();
    let icon_path = icon_directory.join("fileterm-test.ico");
    fs::write(&icon_path, shell_icon_bytes("classic")).unwrap();
    let executable = env::current_exe().unwrap();

    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap() };
    let executable_name = HSTRING::from(executable.to_string_lossy().as_ref());
    unsafe { link.SetPath(&executable_name).unwrap() };
    let persist: IPersistFile = link.cast().unwrap();
    let shortcut_name = HSTRING::from(shortcut_path.to_string_lossy().as_ref());
    unsafe { persist.Save(&shortcut_name, true).unwrap() };

    assert!(!refresh_shortcut_icon(
        &shortcut_path,
        &normalized_path(&temp_dir.join("some-other-app.exe")),
        &normalized_path(&icon_directory),
        &icon_path
    )
    .unwrap());
    assert!(refresh_shortcut_icon(
        &shortcut_path,
        &normalized_path(&executable),
        &normalized_path(&icon_directory),
        &icon_path
    )
    .unwrap());

    let check_link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap() };
    let check_persist: IPersistFile = check_link.cast().unwrap();
    unsafe { check_persist.Load(&shortcut_name, STGM_READWRITE).unwrap() };
    let mut target = [0u16; 32768];
    unsafe {
        check_link
            .GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)
            .unwrap();
    }
    assert_eq!(
        normalized_path(Path::new(&wide_path(&target))),
        normalized_path(&executable)
    );
    let mut actual_icon = [0u16; 32768];
    let mut actual_icon_index = 0;
    unsafe {
        check_link
            .GetIconLocation(&mut actual_icon, &mut actual_icon_index)
            .unwrap();
    }
    assert_eq!(
        normalized_path(Path::new(&wide_path(&actual_icon))),
        normalized_path(&icon_path)
    );
    assert!(!refresh_shortcut_icon(
        &shortcut_path,
        &normalized_path(&executable),
        &normalized_path(&icon_directory),
        &icon_path
    )
    .unwrap());

    let custom_icon_path = temp_dir.join("user-icon.ico");
    fs::write(&custom_icon_path, shell_icon_bytes("clean")).unwrap();
    let custom_icon_name = HSTRING::from(custom_icon_path.to_string_lossy().as_ref());
    unsafe {
        check_link.SetIconLocation(&custom_icon_name, 0).unwrap();
        check_persist.Save(&shortcut_name, true).unwrap();
    }
    assert!(!refresh_shortcut_icon(
        &shortcut_path,
        &normalized_path(&executable),
        &normalized_path(&icon_directory),
        &icon_path
    )
    .unwrap());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn shell_icon_variants_are_valid_ico_payloads_with_distinct_assets() {
    use windows::{
        core::HSTRING,
        Win32::{UI::Shell::ExtractIconExW, UI::WindowsAndMessaging::DestroyIcon},
    };

    let temp_dir = env::temp_dir().join(format!(
        "fileterm-icon-payload-test-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();
    let classic = shell_icon_bytes("classic");
    let material = shell_icon_bytes("material");
    let clean = shell_icon_bytes("clean");

    for (variant, icon) in [
        ("classic", &classic),
        ("material", &material),
        ("clean", &clean),
    ] {
        assert_eq!(&icon[0..2], &[0, 0]);
        assert_eq!(&icon[2..4], &[1, 0]);
        let frame_count = u16::from_le_bytes([icon[4], icon[5]]);
        assert_eq!(frame_count, 9);
        let frame_sizes = (0..frame_count)
            .map(|index| {
                let offset = 6 + usize::from(index) * 16;
                let width = if icon[offset] == 0 {
                    256
                } else {
                    u32::from(icon[offset])
                };
                let height = if icon[offset + 1] == 0 {
                    256
                } else {
                    u32::from(icon[offset + 1])
                };
                (width, height)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            frame_sizes,
            [16, 20, 24, 32, 40, 48, 64, 128, 256]
                .into_iter()
                .map(|size| (size, size))
                .collect::<Vec<_>>()
        );
        let icon_path = temp_dir.join(format!("{variant}.ico"));
        fs::write(&icon_path, icon).unwrap();
        let icon_name = HSTRING::from(icon_path.to_string_lossy().as_ref());
        let mut large = unsafe { std::mem::zeroed() };
        let count = unsafe { ExtractIconExW(&icon_name, 0, Some(&mut large), None, 1) };
        assert_eq!(
            count, 1,
            "Windows Shell must load the {variant} ICO payload"
        );
        assert_ne!(large.0, std::ptr::null_mut());
        unsafe { DestroyIcon(large) }.unwrap();
    }
    assert_ne!(classic, material);
    assert_ne!(material, clean);
    assert_ne!(classic, clean);
    fs::remove_dir_all(&temp_dir).unwrap();
}
