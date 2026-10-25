//! aion-copilot: the same functions as `../win32/copilot.rs`, doing the safe
//! nothing: no sound, no game found, no installer, no starting with the system.

use std::path::Path;

/// There is no sound output for this platform yet.
pub fn play_wav(_bytes: &'static [u8]) -> Result<(), String> {
    Err("não suportado".to_string())
}

/// The game is never found here.
pub fn aion2_pid() -> Option<u32> {
    None
}

/// No key state is read here: Alt is never down.
pub fn alt_down() -> bool {
    false
}

/// Never known here, so never said.
pub fn game_fullscreen_exclusive() -> bool {
    false
}

/// Updates here come from elsewhere: the release is a Windows installer.
pub fn self_update_supported() -> bool {
    false
}

/// There is no installer format for this platform yet.
pub fn run_nsis_update(_path: &Path, _relaunch: bool) -> Result<(), String> {
    Err("não suportado".to_string())
}

/// Starting with the system is not supported here yet: asking does nothing,
/// and `autostart` says it is off.
pub fn set_autostart(_on: bool) -> Result<(), String> {
    Ok(())
}

/// Never on here.
pub fn autostart() -> bool {
    false
}
