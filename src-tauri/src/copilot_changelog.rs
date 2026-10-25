//! aion-copilot: what each version brought, for the Painel's Novidades page
//! (`src/data/copilot_changelog.json`, embedded). Newest first; the first entry
//! is the version being built (a test holds it to `tauri.conf.json`).
//!
//! `copilot.lastSeenVersion` is the version the page was last read at. A new
//! install starts there silently, so only a version installed later counts as
//! news (an upgrade from 1.0.0, which did not keep it, starts at 1.0.0).

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const LAST_SEEN_KEY: &str = "copilot.lastSeenVersion";

/// What an item is, shown as a chip.
pub const TAGS: [&str; 3] = ["novo", "melhorado", "corrigido"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Changelog {
    pub versions: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub version: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub title: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// One of `TAGS`.
    pub tag: String,
    pub text: String,
}

const SOURCE: &str = include_str!("../../src/data/copilot_changelog.json");

pub fn changelog() -> &'static Changelog {
    static PARSED: OnceLock<Changelog> = OnceLock::new();
    PARSED.get_or_init(|| serde_json::from_str(SOURCE).unwrap_or_default())
}

/// What the Novidades page shows.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatsNew {
    pub current: String,
    pub last_seen: Option<String>,
    pub versions: &'static [Entry],
}

fn current_version(app: &tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

/// What `copilot.lastSeenVersion` starts at when it is unset: the version
/// running for a new install, but the oldest one in the changelog when other
/// `copilot.*` settings are already there. 1.0.0 never wrote the key, so an
/// upgrade from it looks like that, and its notes are news.
fn first_last_seen<'a>(keys: impl IntoIterator<Item = &'a str>, current: &'a str) -> &'a str {
    let upgraded = keys.into_iter().any(|k| k.starts_with("copilot.") && k != LAST_SEEN_KEY);
    match changelog().versions.last() {
        Some(oldest) if upgraded => &oldest.version,
        _ => current,
    }
}

/// aion-copilot: at start, a new install takes the version it is at as read
/// (from `setup`, before anything else writes a `copilot.*` setting).
pub fn init(app: &tauri::AppHandle) {
    use tauri::Manager;
    if crate::copilot_update::setting(app, LAST_SEEN_KEY).is_some() {
        return;
    }
    let Some(state) = app.try_state::<crate::app::AppState>() else { return };
    let keys = state.settings.get_all();
    let current = current_version(app);
    let first = first_last_seen(keys.keys().map(String::as_str), &current);
    crate::copilot_update::save_setting(app, LAST_SEEN_KEY, Some(first), false);
}

/// aion-copilot: the changelog, the version running and the one the page was
/// last read at.
#[tauri::command]
pub fn copilot_whats_new(app: tauri::AppHandle) -> WhatsNew {
    WhatsNew {
        current: current_version(&app),
        last_seen: crate::copilot_update::setting(&app, LAST_SEEN_KEY),
        versions: &changelog().versions,
    }
}

/// aion-copilot: the Novidades of this version have been read.
#[tauri::command]
pub fn copilot_whats_new_seen(app: tauri::AppHandle) {
    crate::copilot_update::save_setting(&app, LAST_SEEN_KEY, Some(&current_version(&app)), true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed() -> Changelog {
        serde_json::from_str(SOURCE).expect("copilot_changelog.json parses")
    }

    #[test]
    fn the_changelog_parses_and_is_not_empty() {
        let log = parsed();
        assert!(!log.versions.is_empty());
        for entry in &log.versions {
            assert!(!entry.title.trim().is_empty(), "{} has no title", entry.version);
            assert!(!entry.items.is_empty(), "{} has no items", entry.version);
            assert!(
                chrono::NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d").is_ok(),
                "{} has date {:?}, not YYYY-MM-DD",
                entry.version,
                entry.date
            );
        }
    }

    #[test]
    fn versions_go_strictly_down() {
        let versions: Vec<semver::Version> = parsed()
            .versions
            .iter()
            .map(|e| semver::Version::parse(&e.version).unwrap_or_else(|_| panic!("{:?} is not a version", e.version)))
            .collect();
        for pair in versions.windows(2) {
            assert!(pair[0] > pair[1], "{} comes before {}, newest first", pair[0], pair[1]);
        }
    }

    #[test]
    fn the_first_entry_is_the_version_being_built() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
        let conf: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let version = conf["version"].as_str().unwrap();
        assert_eq!(parsed().versions[0].version, version, "write the changelog entry for {version}");
    }

    #[test]
    fn an_upgrade_from_1_0_0_has_its_news_unread() {
        let oldest = parsed().versions.last().unwrap().version.clone();
        assert_eq!(first_last_seen(["window.x", "dpsMeter.language"], "1.1.0"), "1.1.0", "a new install");
        assert_eq!(first_last_seen([], "1.1.0"), "1.1.0");
        assert_eq!(first_last_seen(["window.x", "copilot.hubOpen"], "1.1.0"), oldest, "1.0.0 was here");
    }

    #[test]
    fn every_item_has_a_known_tag_and_text() {
        for entry in parsed().versions {
            for item in entry.items {
                assert!(TAGS.contains(&item.tag.as_str()), "{}: tag {:?}", entry.version, item.tag);
                assert!(!item.text.trim().is_empty(), "{}: an item without text", entry.version);
            }
        }
    }
}
