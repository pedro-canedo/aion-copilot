//! aion-copilot: what its own features need from Windows — the alert sounds,
//! knowing whether the game is open (and in exclusive full screen), whether
//! Alt is held, running the installer of an update, and starting with Windows.

use std::path::Path;

/// The user's Run key: starting with Windows needs no admin.
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
/// The value under `RUN_KEY` that starts the app.
const RUN_VALUE: &str = "aion-copilot";
/// No console window flashing up for reg.exe or the installer.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// The installer outlives the app it replaces.
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// Play a WAV held in memory, and return at once (it plays in the
/// background, over any window, the game's included).
///
/// The bytes are `'static` because Windows reads them while the sound plays,
/// after this returns: with SND_ASYNC, PlaySoundW keeps the pointer and
/// nothing else. A temporary would be freed mid-sound. A later call stops the
/// sound before it, so the earlier buffer must still be valid then too.
pub fn play_wav(bytes: &'static [u8]) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    // SAFETY: with SND_MEMORY the "name" is the WAV image itself, which is
    // 'static, so it outlives the call and the playback that follows.
    let ok = unsafe { PlaySoundW(PCWSTR(bytes.as_ptr().cast()), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
    if ok.as_bool() { Ok(()) } else { Err("o Windows não tocou o som".to_string()) }
}

/// The process id of the running game (its window), if any.
pub fn aion2_pid() -> Option<u32> {
    super::window_detector::aion2_pid()
}

/// Whether Alt is held down right now, wherever the focus is (the overlays'
/// "hold Alt to interact", for the 1.4.0 overlays).
pub fn alt_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
    // SAFETY: plain query; the high bit says the key is down now.
    unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) < 0 }
}

/// Whether a program runs in exclusive full screen (Direct3D), where no
/// overlay can show above it: the overlays say so instead of hiding silently.
pub fn game_fullscreen_exclusive() -> bool {
    use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
    // SAFETY: plain query, no pointers of ours.
    unsafe { SHQueryUserNotificationState() }.is_ok_and(|state| state == QUNS_RUNNING_D3D_FULL_SCREEN)
}

/// Whether this install can update itself (the NSIS installer below).
pub fn self_update_supported() -> bool {
    true
}

/// Run the downloaded NSIS installer over this install: passive (a progress
/// bar, no questions), per user so no UAC prompt, and with `relaunch` it opens
/// the app again when done (not when the player was quitting anyway). Returns
/// once it has started; the caller exits so the installer can replace the
/// running files.
pub fn run_nsis_update(path: &Path, relaunch: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let args: &[&str] = if relaunch { &["/P", "/R", "/UPDATE"] } else { &["/P", "/UPDATE"] };
    std::process::Command::new(path)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Start with Windows, or stop. What reg.exe answers is not checked here
/// (deleting a value that is not there fails, and that is not an error):
/// `autostart` is what tells whether it is on.
pub fn set_autostart(on: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let quoted = format!("\"{}\"", exe.display());
    let mut cmd = std::process::Command::new("reg");
    if on {
        cmd.args(["add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &quoted, "/f"]);
    } else {
        cmd.args(["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]);
    }
    let _ = cmd.creation_flags(CREATE_NO_WINDOW).status();
    Ok(())
}

/// Whether the app starts with Windows.
pub fn autostart() -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("reg")
        .args(["query", RUN_KEY, "/v", RUN_VALUE])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
