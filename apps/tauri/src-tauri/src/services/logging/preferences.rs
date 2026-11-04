use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use super::{LogCategory, LogLevel};

const CATEGORIES: [&str; 14] = [
    "app",
    "workspace",
    "connection",
    "terminal",
    "monitoring",
    "performance",
    "files",
    "transfer",
    "storage",
    "security",
    "updates",
    "integration",
    "logging",
    "other",
];
// Detailed diagnostics are opt-in; basic INFO defaults on and warnings/errors always pass.
const BASIC_LOG_MASK: u32 = 1 << CATEGORIES.len();
const DEFAULT_MASK: u32 = BASIC_LOG_MASK;
static ENABLED_CATEGORIES: AtomicU32 = AtomicU32::new(DEFAULT_MASK);

pub fn default_categories() -> BTreeMap<String, bool> {
    CATEGORIES
        .iter()
        .map(|name| ((*name).to_owned(), false))
        .collect()
}

pub fn migrate_categories(input: BTreeMap<String, bool>) -> BTreeMap<String, bool> {
    // Earlier iterations of this unreleased setting persisted noisy default combinations.
    let previous_defaults = [
        ((1 << CATEGORIES.len()) - 1) & !(1 << 2) & !(1 << 4),
        (1 << 0) | (1 << 9) | (1 << 10) | (1 << 12),
        1 << 5,
    ];
    if input.len() == CATEGORIES.len()
        && previous_defaults.iter().any(|mask| {
            CATEGORIES
                .iter()
                .enumerate()
                .all(|(index, name)| input.get(*name) == Some(&(mask & (1 << index) != 0)))
        })
    {
        default_categories()
    } else {
        input
    }
}

pub fn normalize_categories(input: BTreeMap<String, bool>) -> BTreeMap<String, bool> {
    default_categories()
        .into_iter()
        .map(|(name, default)| {
            let enabled = input.get(&name).copied().unwrap_or(default);
            (name, enabled)
        })
        .collect()
}

pub fn set_preferences(basic_logging_enabled: bool, categories: BTreeMap<String, bool>) {
    let categories = normalize_categories(categories);
    let mask = CATEGORIES.iter().enumerate().fold(
        if basic_logging_enabled {
            BASIC_LOG_MASK
        } else {
            0
        },
        |mask, (index, name)| mask | if categories[*name] { 1 << index } else { 0 },
    );
    ENABLED_CATEGORIES.store(mask, Ordering::Relaxed);
}

fn enabled_in(mask: u32, level: LogLevel, category: LogCategory) -> bool {
    level >= LogLevel::Warn
        || (mask & BASIC_LOG_MASK != 0
            && level == LogLevel::Info
            && matches!(
                category,
                LogCategory::App
                    | LogCategory::Security
                    | LogCategory::Updates
                    | LogCategory::Logging
            ))
        || CATEGORIES
            .iter()
            .position(|name| *name == category.as_str())
            .is_some_and(|index| mask & (1 << index) != 0)
}

pub(super) fn enabled(level: LogLevel, category: LogCategory) -> bool {
    enabled_in(ENABLED_CATEGORIES.load(Ordering::Relaxed), level, category)
}

#[cfg(test)]
mod tests;
