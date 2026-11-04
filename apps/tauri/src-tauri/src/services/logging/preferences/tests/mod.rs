use super::*;

#[test]
fn defaults_disable_detail_but_keep_basic_info_and_all_errors() {
    for (name, enabled) in default_categories() {
        let category = LogCategory::from_name(&name);
        assert_eq!(category.as_str(), name);
        assert!(!enabled);
        assert_eq!(enabled_in(DEFAULT_MASK, LogLevel::Debug, category), enabled);
        assert_eq!(
            enabled_in(DEFAULT_MASK, LogLevel::Info, category),
            matches!(name.as_str(), "app" | "security" | "updates" | "logging")
        );
        assert!(enabled_in(0, LogLevel::Warn, category));
        assert!(enabled_in(0, LogLevel::Error, category));
        assert!(!enabled_in(0, LogLevel::Debug, category));
    }
    assert!(!enabled_in(
        DEFAULT_MASK,
        LogLevel::Debug,
        LogCategory::from_scope("russh::client")
    ));
    assert!(!enabled_in(
        DEFAULT_MASK,
        LogLevel::Info,
        LogCategory::from_scope("ssh:session")
    ));
    for scope in ["app", "window", "app:connection:tab"] {
        assert!(enabled_in(
            DEFAULT_MASK,
            LogLevel::Info,
            LogCategory::from_scope(scope)
        ));
    }
    for scope in [
        "renderer:error",
        "renderer:performance",
        "terminal:output",
        "ssh:tab",
    ] {
        assert!(enabled_in(
            DEFAULT_MASK,
            LogLevel::Warn,
            LogCategory::from_scope(scope)
        ));
        assert!(enabled_in(
            DEFAULT_MASK,
            LogLevel::Error,
            LogCategory::from_scope(scope)
        ));
    }
    for scope in ["renderer:performance", "app:windows-performance"] {
        assert_eq!(LogCategory::from_scope(scope), LogCategory::Performance);
        assert!(!enabled_in(
            DEFAULT_MASK,
            LogLevel::Info,
            LogCategory::from_scope(scope)
        ));
        assert!(enabled_in(
            DEFAULT_MASK,
            LogLevel::Warn,
            LogCategory::from_scope(scope)
        ));
    }
}

#[test]
fn basic_logs_can_be_disabled_without_disabling_warnings_or_detail() {
    for (index, name) in CATEGORIES.iter().enumerate() {
        let category = LogCategory::from_name(name);
        assert!(!enabled_in(0, LogLevel::Info, category));
        assert!(!enabled_in(0, LogLevel::Debug, category));
        assert!(enabled_in(0, LogLevel::Warn, category));
        assert!(enabled_in(0, LogLevel::Error, category));
        let detail_only = 1 << index;
        assert!(enabled_in(detail_only, LogLevel::Info, category));
        assert!(enabled_in(detail_only, LogLevel::Debug, category));
    }
}

#[test]
fn saved_previous_defaults_migrate_once_without_replacing_custom_choices() {
    for mask in [
        ((1 << CATEGORIES.len()) - 1) & !(1 << 2) & !(1 << 4),
        (1 << 0) | (1 << 9) | (1 << 10) | (1 << 12),
        1 << 5,
    ] {
        let old = CATEGORIES
            .iter()
            .enumerate()
            .map(|(index, name)| ((*name).to_owned(), mask & (1 << index) != 0))
            .collect();
        assert_eq!(migrate_categories(old), default_categories());
    }
    let custom = BTreeMap::from([("connection".into(), true)]);
    assert_eq!(migrate_categories(custom.clone()), custom);
    for name in ["app", "security", "updates", "logging"] {
        assert!(enabled_in(
            DEFAULT_MASK,
            LogLevel::Info,
            LogCategory::from_name(name)
        ));
    }
}

#[test]
fn partial_and_legacy_settings_fill_defaults_and_drop_unknown_categories() {
    let normalized = normalize_categories(BTreeMap::from([
        ("connection".into(), true),
        ("performance".into(), false),
        ("unknown".into(), true),
    ]));
    assert_eq!(normalized.len(), CATEGORIES.len());
    assert!(normalized["connection"]);
    assert!(!normalized["performance"]);
    assert!(!normalized["monitoring"]);
    assert!(!normalized["terminal"]);
    assert!(!normalized["app"]);
}
