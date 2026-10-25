//! aion-copilot: the documents the player writes — the checklist, the auras,
//! the alerts — one file each (`copilot-<name>.json`, beside the other
//! copilot files), shared by every window.
//!
//! On disk a document is `{"rev": n, "doc": ...}`. A write names the rev it
//! started from and is refused when the file has moved on since, so two
//! windows editing at once cannot silently undo each other: the loser reads
//! again and retries. Every write tells all windows (`copilot-store-changed`).
//!
//! A file that does not parse is put aside as `copilot-<name>.json.bad`
//! (kept, for a look by hand) and the document starts over empty.

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The documents there are. Any other name is refused, so a page cannot
/// write files of its choosing.
pub const NAMES: [&str; 3] = ["checklist", "auras", "alerts"];

/// A document larger than this is refused (it would be a bug, not a list).
const MAX_BYTES: usize = 1 << 20;

/// The event every window hears after a write: `{name, rev}`.
pub const CHANGED_EVENT: &str = "copilot-store-changed";

/// A document and its revision. Rev 0 with a null doc: never written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stored {
    pub rev: u64,
    pub doc: Value,
}

impl Stored {
    fn empty() -> Self {
        Stored { rev: 0, doc: Value::Null }
    }
}

/// Reads and writes in turn: a write is read, compare, replace, and two at
/// once would both pass the compare.
static LOCK: Mutex<()> = Mutex::new(());

fn file(dir: &Path, name: &str) -> Result<PathBuf, String> {
    if !NAMES.contains(&name) {
        return Err(format!("unknown document: {name}"));
    }
    Ok(dir.join(format!("copilot-{name}.json")))
}

/// The document in `path`. Missing: empty. Unreadable as a document: put
/// aside as `.bad`, then empty. A read error (other than missing) is an
/// error, so a write cannot replace a file that is only locked for a moment.
fn load(path: &Path) -> Result<Stored, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Stored::empty()),
        Err(e) => return Err(e.to_string()),
    };
    match serde_json::from_str::<Stored>(&text) {
        Ok(stored) => Ok(stored),
        Err(e) => {
            let bad = path.with_extension("json.bad");
            tracing::warn!("aion-copilot: {} is not a document ({e}); kept as {}", path.display(), bad.display());
            if let Err(e) = std::fs::rename(path, &bad) {
                tracing::warn!("aion-copilot: could not put {} aside: {e}", path.display());
            }
            Ok(Stored::empty())
        }
    }
}

/// Document `name` in `dir`.
pub fn get(dir: &Path, name: &str) -> Result<Stored, String> {
    let path = file(dir, name)?;
    let _turn = LOCK.lock();
    load(&path)
}

/// Replace document `name` in `dir` with `doc`, if it is still at `base_rev`.
/// Returns the new rev.
pub fn put(dir: &Path, name: &str, doc: Value, base_rev: u64) -> Result<u64, String> {
    let path = file(dir, name)?;
    let _turn = LOCK.lock();
    let current = load(&path)?.rev;
    if base_rev != current {
        return Err(format!("stale rev: {name} is at {current}, not {base_rev}"));
    }
    let stored = Stored { rev: current + 1, doc };
    let json = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    if json.len() > MAX_BYTES {
        return Err(format!("document too large: {name}"));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    crate::atomic_file::write(&path, json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(stored.rev)
}

fn app_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// aion-copilot: a document and its rev (rev 0 and null when never written).
/// `async`, off the main thread: a write waits on `LOCK` and on
/// `atomic_file`'s rename retries (an antivirus holding the file), and as a
/// sync command every window would stop painting meanwhile (as
/// `read_cached_icon` did).
#[tauri::command]
pub async fn copilot_store_get(app: tauri::AppHandle, name: String) -> Result<Stored, String> {
    let dir = app_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || get(&dir, &name)).await.map_err(|e| e.to_string())?
}

/// aion-copilot: write a document over rev `base_rev`; the new rev, or an
/// error when the document has changed since (read it again and retry).
#[tauri::command]
pub async fn copilot_store_put(app: tauri::AppHandle, name: String, doc: Value, base_rev: u64) -> Result<u64, String> {
    use tauri::Emitter;
    let dir = app_dir(&app)?;
    let key = name.clone();
    let rev = tauri::async_runtime::spawn_blocking(move || put(&dir, &key, doc, base_rev)).await.map_err(|e| e.to_string())??;
    let _ = app.emit(CHANGED_EVENT, serde_json::json!({ "name": name, "rev": rev }));
    Ok(rev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("copilot-store-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_document_comes_back_as_written_with_its_rev() {
        let dir = temp("round-trip");
        assert_eq!(get(&dir, "checklist").unwrap(), Stored::empty());
        let doc = json!({ "version": 2, "tasks": [{ "id": "a", "label": "Ódile" }] });
        assert_eq!(put(&dir, "checklist", doc.clone(), 0).unwrap(), 1);
        assert_eq!(get(&dir, "checklist").unwrap(), Stored { rev: 1, doc: doc.clone() });
        assert_eq!(put(&dir, "checklist", json!(null), 1).unwrap(), 2);
        assert_eq!(get(&dir, "checklist").unwrap(), Stored { rev: 2, doc: Value::Null });
        let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("copilot-checklist.json")).unwrap()).unwrap();
        assert_eq!(on_disk, json!({ "rev": 2, "doc": null }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_name_is_refused() {
        let dir = temp("unknown");
        assert!(get(&dir, "settings").is_err());
        assert!(put(&dir, "../settings", json!({}), 0).is_err());
        assert!(put(&dir, "", json!({}), 0).is_err());
        assert!(!dir.exists(), "nothing written");
    }

    #[test]
    fn a_write_from_an_old_rev_is_refused() {
        let dir = temp("stale");
        put(&dir, "auras", json!({ "n": 1 }), 0).unwrap();
        put(&dir, "auras", json!({ "n": 2 }), 1).unwrap();
        let err = put(&dir, "auras", json!({ "n": 3 }), 1).unwrap_err();
        assert!(err.starts_with("stale rev"), "{err}");
        assert!(put(&dir, "auras", json!({ "n": 3 }), 7).is_err(), "a rev from the future too");
        assert_eq!(get(&dir, "auras").unwrap(), Stored { rev: 2, doc: json!({ "n": 2 }) });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_file_reads_empty_and_is_kept_aside() {
        let dir = temp("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("copilot-alerts.json");
        std::fs::write(&path, "{\"rev\": 3, \"doc\": [tru").unwrap();
        assert_eq!(get(&dir, "alerts").unwrap(), Stored::empty());
        assert!(!path.exists());
        assert_eq!(std::fs::read_to_string(dir.join("copilot-alerts.json.bad")).unwrap(), "{\"rev\": 3, \"doc\": [tru");
        // Starts over from rev 0.
        assert_eq!(put(&dir, "alerts", json!({ "volume": 0.5 }), 0).unwrap(), 1);
        assert!(dir.join("copilot-alerts.json.bad").exists(), "still kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_json_but_not_a_document_is_kept_aside_too() {
        let dir = temp("not-a-document");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("copilot-auras.json"), "[1, 2, 3]").unwrap();
        assert_eq!(get(&dir, "auras").unwrap(), Stored::empty());
        assert!(dir.join("copilot-auras.json.bad").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_huge_document_is_refused() {
        let dir = temp("huge");
        let big = "x".repeat(MAX_BYTES + 1);
        assert!(put(&dir, "checklist", json!({ "big": big }), 0).is_err());
        assert_eq!(get(&dir, "checklist").unwrap(), Stored::empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
