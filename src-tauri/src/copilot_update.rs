//! aion-copilot: updates from the fork's own GitHub releases (tags
//! `copilot-vX.Y.Z`), not upstream's manifest.
//!
//! The latest release is asked for 20 s after start and then every 6 h. Its
//! installer is `aion-copilot_<ver>_x64-setup.exe`, published beside a
//! `.sha256`; it is downloaded, checked against that hash and run over this
//! install (`platform::copilot::run_nsis_update`). The installer closes the
//! app, so it never runs while the game is open: a checked installer waits
//! for the game to close, or for the app to quit.
//!
//! `copilot.update.mode`: "auto" (the default) downloads by itself, "notify"
//! only says a version is out and installs on the click, "off" asks only when
//! told to. `copilot.update.skip` is a version the player said no to.
//!
//! Built without the `selfupdate` feature the commands are still there and
//! say "indisponível".

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

/// Every change of state, with the whole `Status`.
pub const STATUS_EVENT: &str = "copilot-update-status";
/// While the installer downloads: `{pct}`.
pub const PROGRESS_EVENT: &str = "copilot-update-progress";

const MODE_KEY: &str = "copilot.update.mode";
const SKIP_KEY: &str = "copilot.update.skip";
#[cfg_attr(not(feature = "selfupdate"), allow(dead_code))]
const ETAG_KEY: &str = "copilot.update.etag";
/// The release the ETag stands for: a 304 after a restart needs it.
#[cfg_attr(not(feature = "selfupdate"), allow(dead_code))]
const RELEASE_KEY: &str = "copilot.update.release";

/// What `Status::state` can say.
pub const UNAVAILABLE: &str = "indisponível";
pub const IDLE: &str = "ocioso";
pub const CHECKING: &str = "verificando";
pub const UP_TO_DATE: &str = "atualizado";
pub const AVAILABLE: &str = "disponível";
pub const SKIPPED: &str = "ignorada";
pub const DOWNLOADING: &str = "baixando";
/// Downloaded and checked: installs once the game is closed.
pub const READY: &str = "pronta";
pub const INSTALLING: &str = "instalando";
pub const FAILED: &str = "erro";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Notify,
    Off,
}

impl Mode {
    /// Anything unknown (or unset) is the default, "auto".
    pub fn parse(value: Option<&str>) -> Mode {
        match value {
            Some("notify") => Mode::Notify,
            Some("off") => Mode::Off,
            _ => Mode::Auto,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Auto => "auto",
            Mode::Notify => "notify",
            Mode::Off => "off",
        }
    }

    fn of(app: &tauri::AppHandle) -> Mode {
        Mode::parse(setting(app, MODE_KEY).as_deref())
    }
}

/// A published release this app can install.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub exe_url: String,
    pub sha_url: String,
}

/// What the Painel shows.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub current: String,
    /// The newer version found, if any (also when skipped).
    pub latest: Option<String>,
    pub state: &'static str,
    /// Download percent, while `state` is "baixando".
    pub progress: Option<u8>,
    pub error: Option<String>,
    pub mode: &'static str,
    /// The player clicked "Atualizar agora": a "pronta" installer goes in
    /// when the game closes even outside "auto".
    pub asked: bool,
}

// ── pure parts ─────────────────────────────────────────────────────────────

/// The version in a release tag: `copilot-v1.2.0`, `v1.2.0` or `1.2.0`. A
/// pre-release version (`1.2.1-test`) is not a release.
pub fn parse_tag(tag: &str) -> Option<semver::Version> {
    let tag = tag.trim();
    let bare = tag.strip_prefix("copilot-v").or_else(|| tag.strip_prefix('v')).unwrap_or(tag);
    semver::Version::parse(bare).ok().filter(|v| v.pre.is_empty())
}

/// The installer of `version` and its hash file, as published.
pub fn asset_names(version: &str) -> (String, String) {
    let exe = format!("aion-copilot_{version}_x64-setup.exe");
    let sha = format!("{exe}.sha256");
    (exe, sha)
}

/// The SHA-256 in a `.sha256` file ("<hex>  <file name>", as `sha256sum`
/// writes it): its first word, when that is 64 hex digits. A BOM in front is
/// skipped (PowerShell writes one unless told not to).
pub fn parse_sha256(text: &str) -> Option<String> {
    let word = text.trim_start_matches('\u{feff}').split_whitespace().next()?;
    (word.len() == 64 && word.bytes().all(|b| b.is_ascii_hexdigit())).then(|| word.to_ascii_lowercase())
}

/// Whether a download is the file the release vouches for: its SHA-256, in
/// hex, equals the published one. An empty or malformed hash matches nothing.
/// (The same rule as upstream's `package_hash_matches`, which is `online` only.)
pub fn hash_matches(expected: &str, actual_hex: &str) -> bool {
    let expected = expected.trim();
    expected.len() == 64
        && expected.bytes().all(|b| b.is_ascii_hexdigit())
        && expected.eq_ignore_ascii_case(actual_hex)
}

/// Whether a release's installer already failed its hash in this session:
/// same version, same published SHA-256. Then it is not downloaded again
/// (15 MB every retry otherwise); a release uploaded again with another
/// `.sha256` is a new try.
pub fn known_bad(bad: Option<&(String, String)>, version: &str, expected: &str) -> bool {
    bad.is_some_and(|(v, sha)| v == version && sha.eq_ignore_ascii_case(expected))
}

/// The installable release in GitHub's answer for `releases/latest`. `None`
/// for a draft, a pre-release, a tag that is not a version or one not newer
/// than `current` (whatever its assets: 1.0.0 went out without a `.sha256`);
/// an error when a newer one lacks the installer or its hash, which is a
/// release published wrong.
pub fn pick_release(json: &str, current: &str) -> Result<Option<Release>, String> {
    #[derive(Deserialize)]
    struct GhAsset {
        name: String,
        browser_download_url: String,
    }
    #[derive(Deserialize)]
    struct GhRelease {
        tag_name: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
        #[serde(default)]
        assets: Vec<GhAsset>,
    }
    let release: GhRelease = serde_json::from_str(json).map_err(|e| format!("resposta do GitHub ilegível: {e}"))?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let Some(version) = parse_tag(&release.tag_name) else { return Ok(None) };
    if semver::Version::parse(current).is_ok_and(|current| version <= current) {
        return Ok(None);
    }
    let version = version.to_string();
    let (exe, sha) = asset_names(&version);
    let url = |name: &str| release.assets.iter().find(|a| a.name == name).map(|a| a.browser_download_url.clone());
    match (url(&exe), url(&sha)) {
        (Some(exe_url), Some(sha_url)) => Ok(Some(Release { version, exe_url, sha_url })),
        (None, Some(_)) => Err(format!("a versão {version} foi publicada sem {exe}")),
        (Some(_), None) => Err(format!("a versão {version} foi publicada sem {sha}")),
        (None, None) => Err(format!("a versão {version} foi publicada sem {exe} e {sha}")),
    }
}

/// What a release found means for this app: up to date (not newer, or not a
/// version), skipped by the player, or available.
pub fn decide(latest: &str, current: &str, skip: Option<&str>) -> &'static str {
    let (Ok(latest_v), Ok(current_v)) = (semver::Version::parse(latest), semver::Version::parse(current)) else {
        return UP_TO_DATE;
    };
    if latest_v <= current_v {
        UP_TO_DATE
    } else if skip == Some(latest) {
        SKIPPED
    } else {
        AVAILABLE
    }
}

// ── settings ───────────────────────────────────────────────────────────────

/// A copilot setting from the Rust settings map; blank counts as unset.
pub(crate) fn setting(app: &tauri::AppHandle, key: &str) -> Option<String> {
    app.try_state::<crate::app::AppState>()
        .and_then(|state| state.settings.get(key))
        .filter(|v| !v.is_empty())
}

/// Store a setting (`None` removes it). With `tell`, the windows hear it as
/// `update_settings` would say it (`setting-changed`).
pub(crate) fn save_setting(app: &tauri::AppHandle, key: &str, value: Option<&str>, tell: bool) {
    let Some(state) = app.try_state::<crate::app::AppState>() else { return };
    let changed = match value {
        Some(v) => state.settings.set(key, v),
        None => {
            state.settings.remove(key);
            true
        }
    };
    if changed && tell {
        let _ = app.emit("setting-changed", serde_json::json!({ "key": key, "value": value.unwrap_or("") }));
    }
}

fn current_version(app: &tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

// ── the updater ────────────────────────────────────────────────────────────

#[cfg(feature = "selfupdate")]
mod engine {
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    use parking_lot::Mutex;
    use tauri::Emitter;

    use super::*;

    const LATEST_URL: &str = "https://api.github.com/repos/pedro-canedo/aion-copilot/releases/latest";
    /// The first look, after start has settled.
    const FIRST_CHECK: Duration = Duration::from_secs(20);
    /// Between looks: GitHub allows 60 unauthenticated asks an hour.
    const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
    /// After a look, download or installer start that failed (no network yet
    /// at autostart, a 403, an antivirus holding the file): a session is
    /// shorter than `CHECK_EVERY`.
    const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);
    /// How often a checked installer looks whether the game has closed.
    const TICK: Duration = Duration::from_secs(30);
    /// Under the system's temp folder; emptied before each download.
    const DOWNLOAD_DIR: &str = "aion-copilot-update";

    struct Engine {
        state: &'static str,
        /// The newer release found (also when skipped).
        latest: Option<Release>,
        progress: Option<u8>,
        error: Option<String>,
        /// `latest`'s installer, downloaded and checked.
        ready: Option<PathBuf>,
        /// The player clicked "Atualizar agora" for `ready`: it installs when
        /// the game closes in any mode. Without it, only "auto" installs by
        /// itself.
        asked: bool,
        /// A check or a download is under way.
        busy: bool,
        /// The installer has been started: nothing else may start one.
        installing: bool,
        /// (version, published SHA-256) of an installer that did not match it.
        bad: Option<(String, String)>,
    }

    static ENGINE: Mutex<Engine> = Mutex::new(Engine {
        state: IDLE,
        latest: None,
        progress: None,
        error: None,
        ready: None,
        asked: false,
        busy: false,
        installing: false,
        bad: None,
    });

    /// One check or download at a time; let go when dropped.
    struct Busy;

    impl Busy {
        fn take() -> Option<Busy> {
            let mut engine = ENGINE.lock();
            if engine.busy {
                return None;
            }
            engine.busy = true;
            Some(Busy)
        }
    }

    impl Drop for Busy {
        fn drop(&mut self) {
            ENGINE.lock().busy = false;
        }
    }

    pub fn status(app: &tauri::AppHandle) -> Status {
        let current = current_version(app);
        let mode = Mode::of(app).name();
        if !crate::platform::copilot::self_update_supported() {
            return Status { current, latest: None, state: UNAVAILABLE, progress: None, error: None, mode, asked: false };
        }
        let engine = ENGINE.lock();
        Status {
            current,
            latest: engine.latest.as_ref().map(|r| r.version.clone()),
            state: engine.state,
            progress: engine.progress,
            error: engine.error.clone(),
            mode,
            asked: engine.asked,
        }
    }

    /// Change the state, then tell the windows.
    fn update(app: &tauri::AppHandle, change: impl FnOnce(&mut Engine)) {
        change(&mut ENGINE.lock());
        let _ = app.emit(STATUS_EVENT, status(app));
    }

    fn fail(app: &tauri::AppHandle, error: &str) {
        tracing::warn!("aion-copilot: update: {error}");
        update(app, |e| {
            e.state = FAILED;
            e.progress = None;
            e.error = Some(error.to_string());
        });
    }

    fn client(app: &tauri::AppHandle) -> &'static reqwest::Client {
        static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
        CLIENT.get_or_init(|| {
            reqwest::Client::builder()
                .user_agent(format!("aion-copilot/{}", current_version(app)))
                .build()
                .unwrap_or_default()
        })
    }

    fn offline(e: reqwest::Error) -> String {
        format!("sem conexão com o GitHub: {e}")
    }

    /// The latest release, or `None` when there is none to install. Sends the
    /// ETag of the last answer, so an unchanged release costs a bodiless 304.
    async fn fetch_latest(app: &tauri::AppHandle) -> Result<Option<Release>, String> {
        use reqwest::header::{ACCEPT, ETAG, IF_NONE_MATCH};
        use reqwest::StatusCode;
        let cached: Option<Release> = setting(app, RELEASE_KEY).and_then(|json| serde_json::from_str(&json).ok());
        let mut request = client(app)
            .get(LATEST_URL)
            .header(ACCEPT, "application/vnd.github+json")
            .timeout(Duration::from_secs(30));
        if let (Some(_), Some(tag)) = (&cached, setting(app, ETAG_KEY)) {
            request = request.header(IF_NONE_MATCH, tag);
        }
        let response = request.send().await.map_err(offline)?;
        match response.status() {
            StatusCode::NOT_MODIFIED => return Ok(cached),
            // Nothing published yet.
            StatusCode::NOT_FOUND => return Ok(None),
            status if !status.is_success() => return Err(format!("o GitHub respondeu {status}")),
            _ => {}
        }
        let tag = response.headers().get(ETAG).and_then(|v| v.to_str().ok()).map(str::to_string);
        let body = response.text().await.map_err(offline)?;
        let release = pick_release(&body, &current_version(app))?;
        match (&release, tag) {
            (Some(r), Some(tag)) => {
                let json = serde_json::to_string(r).unwrap_or_default();
                save_setting(app, RELEASE_KEY, Some(&json), false);
                save_setting(app, ETAG_KEY, Some(&tag), false);
            }
            _ => {
                save_setting(app, RELEASE_KEY, None, false);
                save_setting(app, ETAG_KEY, None, false);
            }
        }
        Ok(release)
    }

    /// Ask GitHub now. A check already under way answers for this one.
    pub async fn check(app: &tauri::AppHandle) -> Result<(), String> {
        if !crate::platform::copilot::self_update_supported() {
            return Ok(());
        }
        let Some(_busy) = Busy::take() else { return Ok(()) };
        update(app, |e| {
            e.state = CHECKING;
            e.error = None;
        });
        let release = match fetch_latest(app).await {
            Ok(release) => release,
            // An installer already here is still good: it stays "pronta"
            // (and the Painel keeps saying so); the look is tried again later.
            Err(error) if ENGINE.lock().ready.is_some() => {
                tracing::warn!("aion-copilot: update: {error}");
                update(app, |e| e.state = READY);
                return Ok(());
            }
            Err(error) => {
                fail(app, &error);
                return Err(error);
            }
        };
        let current = current_version(app);
        let skip = setting(app, SKIP_KEY);
        let state = release.as_ref().map_or(UP_TO_DATE, |r| decide(&r.version, &current, skip.as_deref()));
        if let Some(r) = release.as_ref().filter(|_| state == AVAILABLE) {
            tracing::info!("aion-copilot: update {} available (running {current})", r.version);
        }
        update(app, |e| {
            let newer = release.filter(|_| state != UP_TO_DATE);
            if e.latest.as_ref().map(|r| &r.version) != newer.as_ref().map(|r| &r.version) || state != AVAILABLE {
                e.ready = None;
                e.asked = false;
            }
            e.latest = newer;
            e.progress = None;
            e.state = if state == AVAILABLE && e.ready.is_some() { READY } else { state };
        });
        Ok(())
    }

    /// Whether the player said no to `latest` (which can happen while it
    /// downloads). Its installer, if here, is dropped, so it never installs.
    fn drop_if_skipped(app: &tauri::AppHandle) -> bool {
        let skip = setting(app, SKIP_KEY);
        let dropped = {
            let mut engine = ENGINE.lock();
            let skipped = skip.is_some() && engine.latest.as_ref().map(|r| &r.version) == skip.as_ref();
            if !skipped {
                return false;
            }
            let had = engine.ready.take().is_some();
            engine.asked = false;
            if had {
                engine.state = SKIPPED;
            }
            had
        };
        if dropped {
            let _ = app.emit(STATUS_EVENT, status(app));
        }
        true
    }

    /// Download `latest`'s installer and check its hash, unless that is done.
    /// The caller holds `Busy`.
    async fn fetch_installer(app: &tauri::AppHandle) -> Result<PathBuf, String> {
        let (release, ready) = {
            let engine = ENGINE.lock();
            (engine.latest.clone(), engine.ready.clone())
        };
        if let Some(path) = ready {
            return Ok(path);
        }
        let release = release.ok_or_else(|| "nenhuma atualização disponível".to_string())?;
        update(app, |e| {
            e.state = DOWNLOADING;
            e.progress = Some(0);
            e.error = None;
        });
        match download(app, &release).await {
            Ok(path) => {
                tracing::info!("aion-copilot: update {} downloaded and checked: {}", release.version, path.display());
                update(app, |e| {
                    if e.latest.as_ref() == Some(&release) {
                        e.ready = Some(path.clone());
                    }
                    e.state = READY;
                    e.progress = None;
                });
                drop_if_skipped(app);
                Ok(path)
            }
            Err(error) => {
                fail(app, &error);
                Err(error)
            }
        }
    }

    /// The installer, into `%TEMP%\aion-copilot-update\`, hashed as it comes
    /// (the pattern of upstream's `download_and_install_update`). A file that
    /// does not match the published hash is deleted.
    async fn download(app: &tauri::AppHandle, release: &Release) -> Result<PathBuf, String> {
        use futures_util::StreamExt;
        use sha2::{Digest, Sha256};
        use tokio::io::AsyncWriteExt;

        let http = client(app);
        let sums = http.get(&release.sha_url).timeout(Duration::from_secs(30)).send().await.map_err(offline)?;
        if !sums.status().is_success() {
            return Err(format!("o SHA-256 da versão {} não baixou (HTTP {})", release.version, sums.status()));
        }
        let expected = parse_sha256(&sums.text().await.map_err(offline)?)
            .ok_or_else(|| format!("o arquivo .sha256 da versão {} não traz um SHA-256", release.version))?;
        if known_bad(ENGINE.lock().bad.as_ref(), &release.version, &expected) {
            return Err(format!("o instalador da versão {} não confere com o SHA-256 publicado; espere uma nova publicação", release.version));
        }

        let dir = std::env::temp_dir().join(DOWNLOAD_DIR);
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
        let path = dir.join(asset_names(&release.version).0);

        let response = http.get(&release.exe_url).timeout(Duration::from_secs(600)).send().await.map_err(offline)?;
        if !response.status().is_success() {
            return Err(format!("o instalador da versão {} não baixou (HTTP {})", release.version, response.status()));
        }
        let total = response.content_length().unwrap_or(0);
        let mut hasher = Sha256::new();
        let written: Result<(), String> = async {
            let mut file = tokio::fs::File::create(&path).await.map_err(|e| e.to_string())?;
            let mut stream = response.bytes_stream();
            let mut downloaded: u64 = 0;
            let mut last_pct: u64 = 0;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(offline)?;
                hasher.update(&chunk);
                file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                downloaded += chunk.len() as u64;
                // No percent without a length.
                if let Some(pct) = (downloaded * 100).checked_div(total).map(|p| p.min(100))
                    && pct != last_pct
                {
                    last_pct = pct;
                    ENGINE.lock().progress = Some(pct as u8);
                    let _ = app.emit(PROGRESS_EVENT, serde_json::json!({ "pct": pct }));
                }
            }
            file.flush().await.map_err(|e| e.to_string())
        }
        .await;
        if let Err(error) = written {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(error);
        }

        let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if !hash_matches(&expected, &actual) {
            let _ = tokio::fs::remove_file(&path).await;
            tracing::warn!("aion-copilot: update {} has SHA-256 {actual}, the release says {expected}; deleted", path.display());
            ENGINE.lock().bad = Some((release.version.clone(), expected));
            return Err("o instalador baixado não confere com o SHA-256 publicado; foi apagado".into());
        }
        Ok(path)
    }

    fn flush_settings(app: &tauri::AppHandle) {
        if let Some(state) = app.try_state::<crate::app::AppState>()
            && let Err(error) = state.settings.flush()
        {
            tracing::warn!("aion-copilot: could not save settings before updating: {error}");
        }
    }

    /// Start the installer, which opens the app again when done, and quit.
    async fn install(app: &tauri::AppHandle, path: PathBuf) -> Result<(), String> {
        {
            let mut engine = ENGINE.lock();
            if engine.installing {
                return Ok(());
            }
            engine.installing = true;
            engine.state = INSTALLING;
        }
        let _ = app.emit(STATUS_EVENT, status(app));
        tracing::info!("aion-copilot: installing the update: {}", path.display());
        // What quitting saves (the fights on the meter, the map's places,
        // the settings), first: the installer closes the app as soon as it
        // starts, maybe before the exit events run.
        let handle = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || crate::app::copilot_save_before_exit(&handle)).await;
        if let Err(error) = crate::platform::copilot::run_nsis_update(&path, true) {
            // Downloaded again on the next check or click, not retried every tick.
            {
                let mut engine = ENGINE.lock();
                engine.installing = false;
                engine.ready = None;
                engine.asked = false;
            }
            fail(app, &error);
            return Err(error);
        }
        // Give the installer time to start, then make way for it.
        tokio::time::sleep(Duration::from_secs(2)).await;
        flush_settings(app);
        app.exit(0);
        Ok(())
    }

    /// Whether a checked installer may go in without a click now: in "auto",
    /// or when the player asked for it. One downloaded in "auto" waits for
    /// the click after a switch to "notify" or "off".
    fn may_install(app: &tauri::AppHandle, engine: &Engine) -> bool {
        engine.asked || Mode::of(app) == Mode::Auto
    }

    /// A checked installer, installed now if the game is closed.
    async fn install_if_idle(app: &tauri::AppHandle) {
        if drop_if_skipped(app) {
            return;
        }
        let ready = {
            let engine = ENGINE.lock();
            if engine.busy || engine.installing || !may_install(app, &engine) { None } else { engine.ready.clone() }
        };
        if let Some(path) = ready
            && crate::copilot::game_pid().is_none()
        {
            let _ = install(app, path).await;
        }
    }

    /// In "auto", a version found is downloaded straight away.
    pub async fn auto_download(app: &tauri::AppHandle) {
        if Mode::of(app) != Mode::Auto || ENGINE.lock().state != AVAILABLE {
            return;
        }
        let Some(_busy) = Busy::take() else { return };
        let _ = fetch_installer(app).await;
    }

    /// The player's "Atualizar agora": download and check, then install, or,
    /// with the game open, wait for it to close (the installer closes the app).
    pub async fn install_now(app: &tauri::AppHandle) -> Result<(), String> {
        if !crate::platform::copilot::self_update_supported() {
            return Err("atualização automática indisponível neste sistema".into());
        }
        {
            let mut engine = ENGINE.lock();
            if engine.installing {
                return Ok(());
            }
            // Also for a download already under way (below): it installs
            // when the game closes, whatever the mode.
            engine.asked = true;
        }
        // Asking for it takes back a no to this version.
        let latest = ENGINE.lock().latest.as_ref().map(|r| r.version.clone());
        if latest.is_some() && setting(app, SKIP_KEY) == latest {
            save_setting(app, SKIP_KEY, None, true);
        }
        let path = {
            let Some(_busy) = Busy::take() else {
                // A download already under way installs when it is done.
                return if ENGINE.lock().state == DOWNLOADING {
                    Ok(())
                } else {
                    Err("aguarde: a atualização está sendo verificada".into())
                };
            };
            fetch_installer(app).await?
        };
        if crate::copilot::game_pid().is_some() {
            tracing::info!("aion-copilot: update ready; installs when the game closes");
            return Ok(());
        }
        install(app, path).await
    }

    /// Say no to `version` (blank: take back a no). A downloaded installer of
    /// it is dropped, so it can never install by itself (one still downloading
    /// is dropped when it lands: `drop_if_skipped`).
    pub fn skip(app: &tauri::AppHandle, version: &str) {
        let version = version.trim();
        save_setting(app, SKIP_KEY, (!version.is_empty()).then_some(version), true);
        let current = current_version(app);
        update(app, |e| {
            let Some(latest) = e.latest.as_ref() else { return };
            if !matches!(e.state, AVAILABLE | READY | SKIPPED) {
                return;
            }
            let state = decide(&latest.version, &current, (!version.is_empty()).then_some(version));
            if state == SKIPPED {
                e.ready = None;
                e.asked = false;
            }
            e.state = if state == AVAILABLE && e.ready.is_some() { READY } else { state };
        });
    }

    /// The background updater: the first look 20 s after start, then every
    /// 6 h (10 min after a failure); in between, every 30 s, a checked
    /// installer goes in as soon as the game is closed.
    pub fn start(app: &tauri::AppHandle) {
        if !crate::platform::copilot::self_update_supported() {
            return;
        }
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(FIRST_CHECK).await;
            let mut checked: Option<Instant> = None;
            loop {
                let mode = Mode::of(&app);
                let wait = if ENGINE.lock().state == FAILED { RETRY_AFTER } else { CHECK_EVERY };
                if mode != Mode::Off && checked.is_none_or(|at| at.elapsed() >= wait) {
                    checked = Some(Instant::now());
                    if check(&app).await.is_ok() {
                        auto_download(&app).await;
                    }
                }
                install_if_idle(&app).await;
                tokio::time::sleep(TICK).await;
            }
        });
    }

    /// The app is quitting: a checked installer runs now (in "auto", or when
    /// asked for), without opening the app again afterwards. Once only (the
    /// exit events come twice).
    pub fn on_exit(app: &tauri::AppHandle) {
        if drop_if_skipped(app) {
            return;
        }
        let path = {
            let mut engine = ENGINE.lock();
            if engine.installing || !may_install(app, &engine) {
                return;
            }
            let Some(path) = engine.ready.clone() else { return };
            engine.installing = true;
            path
        };
        tracing::info!("aion-copilot: installing the update on the way out: {}", path.display());
        if let Err(error) = crate::platform::copilot::run_nsis_update(&path, false) {
            tracing::warn!("aion-copilot: update: {error}");
        }
    }
}

#[cfg(not(feature = "selfupdate"))]
mod engine {
    //! Built without `selfupdate`: no HTTP client, so no updates.

    use super::*;

    pub fn status(app: &tauri::AppHandle) -> Status {
        Status {
            current: current_version(app),
            latest: None,
            state: UNAVAILABLE,
            progress: None,
            error: None,
            mode: Mode::of(app).name(),
            asked: false,
        }
    }

    pub async fn check(_app: &tauri::AppHandle) -> Result<(), String> {
        Ok(())
    }

    pub async fn auto_download(_app: &tauri::AppHandle) {}

    pub async fn install_now(_app: &tauri::AppHandle) -> Result<(), String> {
        Err("atualização automática indisponível nesta versão".into())
    }

    pub fn skip(app: &tauri::AppHandle, version: &str) {
        let version = version.trim();
        save_setting(app, SKIP_KEY, (!version.is_empty()).then_some(version), true);
    }

    pub fn start(_app: &tauri::AppHandle) {}

    pub fn on_exit(_app: &tauri::AppHandle) {}
}

/// aion-copilot: start the background updater (from `setup`).
pub fn start(app: &tauri::AppHandle) {
    engine::start(app);
}

/// aion-copilot: the app is quitting; a downloaded update installs now.
pub fn on_exit(app: &tauri::AppHandle) {
    engine::on_exit(app);
}

/// aion-copilot: the version running, the newer one found and what the
/// updater is doing.
#[tauri::command]
pub fn copilot_update_status(app: tauri::AppHandle) -> Status {
    engine::status(&app)
}

/// aion-copilot: look for a new version now (in any mode); in "auto" a
/// version found starts downloading.
#[tauri::command]
pub async fn copilot_update_check(app: tauri::AppHandle) -> Result<Status, String> {
    engine::check(&app).await?;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { engine::auto_download(&handle).await });
    Ok(engine::status(&app))
}

/// aion-copilot: "Atualizar agora". With the game open it downloads and
/// waits for the game to close (state "pronta").
#[tauri::command]
pub async fn copilot_update_install(app: tauri::AppHandle) -> Result<(), String> {
    engine::install_now(&app).await
}

/// aion-copilot: never offer `version` again (blank: offer it again).
#[tauri::command]
pub fn copilot_update_skip(app: tauri::AppHandle, version: String) -> Status {
    engine::skip(&app, &version);
    engine::status(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_gives_its_version() {
        assert_eq!(parse_tag("copilot-v1.2.0").unwrap().to_string(), "1.2.0");
        assert_eq!(parse_tag("v1.10.3").unwrap().to_string(), "1.10.3");
        assert_eq!(parse_tag(" 1.1.0 ").unwrap().to_string(), "1.1.0");
        // A test release and upstream's own tags of another kind are not ours.
        assert!(parse_tag("copilot-v1.2.1-test").is_none());
        assert!(parse_tag("copilot-1.2.0").is_none());
        assert!(parse_tag("latest").is_none());
        assert!(parse_tag("").is_none());
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert_eq!(decide("1.10.0", "1.9.0", None), AVAILABLE);
        assert_eq!(decide("1.1.0", "1.1.0", None), UP_TO_DATE);
        assert_eq!(decide("1.0.9", "1.1.0", None), UP_TO_DATE);
        assert_eq!(decide("2.0.0", "1.1.0", Some("2.0.0")), SKIPPED);
        // Skipping one version does not skip the next.
        assert_eq!(decide("2.0.1", "1.1.0", Some("2.0.0")), AVAILABLE);
        assert_eq!(decide("nonsense", "1.1.0", None), UP_TO_DATE);
    }

    #[test]
    fn asset_names_are_the_installer_and_its_hash() {
        let (exe, sha) = asset_names("1.2.0");
        assert_eq!(exe, "aion-copilot_1.2.0_x64-setup.exe");
        assert_eq!(sha, "aion-copilot_1.2.0_x64-setup.exe.sha256");
    }

    #[test]
    fn a_sha256_file_gives_its_first_word() {
        let hex = "9F86D081884C7D659A2FEAA0C55AD015A3BF4F1B2B0B822CD15D6C15B0F00A08";
        let lower = hex.to_ascii_lowercase();
        assert_eq!(parse_sha256(&format!("{hex}  aion-copilot_1.2.0_x64-setup.exe\n")).as_deref(), Some(lower.as_str()));
        assert_eq!(parse_sha256(&format!("\u{feff}{lower}")).as_deref(), Some(lower.as_str()));
        assert_eq!(parse_sha256(&format!("  {lower}\r\n")).as_deref(), Some(lower.as_str()));
        assert!(parse_sha256("").is_none());
        assert!(parse_sha256("9f86d081  file.exe").is_none());
        assert!(parse_sha256(&format!("{}g  file.exe", &lower[..63])).is_none());
    }

    #[test]
    fn only_the_published_hash_matches() {
        let actual = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        assert!(hash_matches(actual, actual));
        assert!(hash_matches(&format!(" {} ", actual.to_uppercase()), actual));
        assert!(!hash_matches(&actual.replace('9', "8"), actual));
        assert!(!hash_matches("", actual));
        assert!(!hash_matches("9f86d081", "9f86d081"));
    }

    fn github(tag: &str, draft: bool, prerelease: bool, assets: &[&str]) -> String {
        let assets: Vec<_> = assets
            .iter()
            .map(|n| serde_json::json!({ "name": n, "browser_download_url": format!("https://example.test/{n}") }))
            .collect();
        serde_json::json!({ "tag_name": tag, "draft": draft, "prerelease": prerelease, "assets": assets }).to_string()
    }

    #[test]
    fn a_release_is_picked_by_its_exact_assets() {
        let both = ["aion-copilot_x64-setup.exe", "aion-copilot_1.2.0_x64-setup.exe", "aion-copilot_1.2.0_x64-setup.exe.sha256"];
        let release = pick_release(&github("copilot-v1.2.0", false, false, &both), "1.1.0").unwrap().unwrap();
        assert_eq!(release.version, "1.2.0");
        assert_eq!(release.exe_url, "https://example.test/aion-copilot_1.2.0_x64-setup.exe");
        assert_eq!(release.sha_url, "https://example.test/aion-copilot_1.2.0_x64-setup.exe.sha256");

        assert_eq!(pick_release(&github("copilot-v1.2.0", true, false, &both), "1.1.0").unwrap(), None);
        assert_eq!(pick_release(&github("copilot-v1.2.0", false, true, &both), "1.1.0").unwrap(), None);
        assert_eq!(pick_release(&github("copilot-v1.2.1-test", false, false, &both), "1.1.0").unwrap(), None);
        // Without the hash beside it, the installer is not trusted, and the
        // error names what is missing.
        let err = pick_release(&github("copilot-v1.2.0", false, false, &both[..2]), "1.1.0").unwrap_err();
        assert!(err.ends_with("sem aion-copilot_1.2.0_x64-setup.exe.sha256"), "{err}");
        assert!(pick_release("not json", "1.1.0").is_err());
    }

    #[test]
    fn a_release_not_newer_is_nothing_to_install_whatever_its_assets() {
        // v1.0.0 as published: the installer, no .sha256.
        let old = github("v1.0.0", false, false, &["aion-copilot_1.0.0_x64-setup.exe"]);
        assert_eq!(pick_release(&old, "1.1.0").unwrap(), None);
        let same = github("copilot-v1.1.0", false, false, &[]);
        assert_eq!(pick_release(&same, "1.1.0").unwrap(), None);
        assert!(pick_release(&same, "1.0.0").is_err());
    }

    #[test]
    fn an_installer_that_failed_its_hash_is_not_downloaded_again() {
        let sha = "ab".repeat(32);
        let bad = ("1.2.0".to_string(), sha.clone());
        assert!(known_bad(Some(&bad), "1.2.0", &sha));
        assert!(known_bad(Some(&bad), "1.2.0", &sha.to_ascii_uppercase()));
        // Uploaded again with another hash, or a newer version: try again.
        assert!(!known_bad(Some(&bad), "1.2.0", &"cd".repeat(32)));
        assert!(!known_bad(Some(&bad), "1.2.1", &sha));
        assert!(!known_bad(None, "1.2.0", &sha));
    }

    #[test]
    fn an_unknown_mode_is_auto() {
        assert_eq!(Mode::parse(None), Mode::Auto);
        assert_eq!(Mode::parse(Some("auto")), Mode::Auto);
        assert_eq!(Mode::parse(Some("notify")), Mode::Notify);
        assert_eq!(Mode::parse(Some("off")), Mode::Off);
        assert_eq!(Mode::parse(Some("sometimes")), Mode::Auto);
    }
}
