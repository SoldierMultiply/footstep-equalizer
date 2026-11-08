//! Background threads: the game process watcher (auto-activate) and the
//! global A/B bypass hotkey. Neither touches game processes: the watcher only
//! reads the process list, the hotkey is a standard RegisterHotKey binding.

use crate::profiles::match_running;
use crate::state::{self, AppState, Detection, EVT_DETECTED};
use std::thread;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager};

/// What the watcher should do after a poll, decided purely from data.
#[derive(Debug, Clone, PartialEq)]
pub enum Transition {
    None,
    Started { game_id: String, exe: String },
    Exited { game_id: String },
    Switched { from: String, game_id: String, exe: String },
}

pub fn decide(current: Option<&str>, found: Option<(&str, &str)>) -> Transition {
    match (current, found) {
        (None, None) => Transition::None,
        (None, Some((g, exe))) => Transition::Started { game_id: g.into(), exe: exe.into() },
        (Some(c), None) => Transition::Exited { game_id: c.into() },
        (Some(c), Some((g, _))) if c == g => Transition::None,
        (Some(c), Some((g, exe))) => Transition::Switched { from: c.into(), game_id: g.into(), exe: exe.into() },
    }
}

pub fn spawn_process_watcher(app: AppHandle) {
    let res = thread::Builder::new()
        .name("earshot-process-watcher".into())
        .spawn(move || watch_loop(app));
    if let Err(e) = res {
        log::error!("cannot start process watcher: {e}");
    }
}

fn running_exes(sys: &mut System) -> Vec<String> {
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let mut names: Vec<String> = sys
        .processes()
        .values()
        .map(|p| p.name().to_string_lossy().to_ascii_lowercase())
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

fn watch_loop(app: AppHandle) {
    let mut sys = System::new();
    let mut current: Option<String> = None;
    loop {
        let (enabled, poll_ms) = {
            let st = app.state::<AppState>();
            let inner = st.lock();
            (inner.settings.auto_switch, inner.settings.poll_ms.clamp(250, 10_000))
        };
        if enabled {
            let running = running_exes(&mut sys);
            let found = {
                let st = app.state::<AppState>();
                let inner = st.lock();
                let auto = |gid: &str| inner.profiles.iter().any(|p| p.game_id.as_deref() == Some(gid) && p.auto);
                match_running(&inner.games, &running, auto).map(|(g, exe)| (g.id.clone(), exe))
            };
            let t = decide(current.as_deref(), found.as_ref().map(|(g, e)| (g.as_str(), e.as_str())));
            handle(&app, &t);
            current = found.map(|(g, _)| g);
        } else if current.take().is_some() {
            handle(&app, &Transition::Exited { game_id: String::new() });
        }
        thread::sleep(Duration::from_millis(poll_ms));
    }
}

fn handle(app: &AppHandle, t: &Transition) {
    match t {
        Transition::None => {}
        Transition::Started { game_id, exe } | Transition::Switched { game_id, exe, .. } => {
            let detection = {
                let st = app.state::<AppState>();
                let mut inner = st.lock();
                let name = inner.game(game_id).map(|g| g.name.clone()).unwrap_or_default();
                let now = state::unix_now();
                if let Ok(p) = inner.profile_mut(game_id) {
                    p.last_seen_unix = Some(now);
                }
                let d = Detection { game_id: game_id.clone(), game_name: name, exe: exe.clone(), since_unix: now };
                inner.detected = Some(d.clone());
                d
            };
            log::info!("detected {} ({})", detection.game_name, detection.exe);
            let _ = app.emit(EVT_DETECTED, &detection);
            if let Err(e) = state::activate(app, game_id) {
                log::warn!("auto-activate failed: {e}");
            }
        }
        Transition::Exited { .. } => {
            let fallback = {
                let st = app.state::<AppState>();
                let mut inner = st.lock();
                inner.detected = None;
                inner.persist();
                inner.settings.revert_on_exit.then(|| inner.settings.fallback_profile.clone())
            };
            match fallback {
                Some(id) => {
                    let _ = state::activate(app, &id);
                }
                None => state::broadcast(app),
            }
        }
    }
}

/// A parsed global hotkey: RegisterHotKey modifier flags + virtual key code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub mods: u32,
    pub vk: u32,
}

/// Parse "Ctrl+Alt+B", "Shift+F9", "Ctrl+Pause" into RegisterHotKey arguments.
pub fn parse_hotkey(s: &str) -> Option<Hotkey> {
    let (mut mods, mut vk) = (0u32, None);
    for part in s.split('+').map(|p| p.trim().to_ascii_uppercase()) {
        match part.as_str() {
            "CTRL" | "CONTROL" => mods |= 0x2,
            "ALT" => mods |= 0x1,
            "SHIFT" => mods |= 0x4,
            "WIN" => mods |= 0x8,
            "PAUSE" => vk = Some(0x13),
            "SPACE" => vk = Some(0x20),
            k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphanumeric()) => vk = Some(k.as_bytes()[0] as u32),
            k if k.starts_with('F') => {
                let n: u32 = k[1..].parse().ok()?;
                if !(1..=24).contains(&n) {
                    return None;
                }
                vk = Some(0x6F + n);
            }
            _ => return None,
        }
    }
    // Require a modifier unless it is a function key, so typing never triggers it.
    let vk = vk?;
    if mods == 0 && !(0x70..=0x87).contains(&vk) {
        return None;
    }
    Some(Hotkey { mods, vk })
}

// TODO(owner): re-register when the user changes the hotkey in Settings
// (post WM_QUIT to this thread via PostThreadMessageW, then spawn again).
pub fn spawn_hotkey(app: AppHandle, combo: String) {
    let Some(hk) = parse_hotkey(&combo) else {
        log::warn!("invalid bypass hotkey '{combo}'");
        return;
    };
    let _ = thread::Builder::new().name("earshot-hotkey".into()).spawn(move || hotkey_loop(app, hk));
}

#[cfg(windows)]
fn hotkey_loop(app: AppHandle, hk: Hotkey) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};
    const ID: i32 = 0xEA51;
    // SAFETY: plain Win32 calls on this thread's own message queue; MSG is POD.
    unsafe {
        if RegisterHotKey(std::ptr::null_mut(), ID, hk.mods | MOD_NOREPEAT, hk.vk) == 0 {
            log::warn!("hotkey already in use by another app");
            return;
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.message == WM_HOTKEY && msg.wParam == ID as usize {
                state::toggle_bypass(&app);
            }
        }
        UnregisterHotKey(std::ptr::null_mut(), ID);
    }
}

#[cfg(not(windows))]
fn hotkey_loop(_app: AppHandle, _hk: Hotkey) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions() {
        assert_eq!(decide(None, None), Transition::None);
        assert_eq!(decide(Some("cs2"), Some(("cs2", "cs2.exe"))), Transition::None);
        assert!(matches!(decide(None, Some(("cs2", "cs2.exe"))), Transition::Started { .. }));
        assert!(matches!(decide(Some("cs2"), None), Transition::Exited { .. }));
        assert!(matches!(decide(Some("cs2"), Some(("r6", "rainbowsix.exe"))), Transition::Switched { .. }));
    }

    #[test]
    fn hotkey_parsing() {
        assert_eq!(parse_hotkey("Ctrl+Alt+B"), Some(Hotkey { mods: 0x3, vk: 0x42 }));
        assert_eq!(parse_hotkey("shift + f9"), Some(Hotkey { mods: 0x4, vk: 0x78 }));
        assert_eq!(parse_hotkey("F10"), Some(Hotkey { mods: 0, vk: 0x79 }));
        assert_eq!(parse_hotkey("B"), None);
        assert_eq!(parse_hotkey("Ctrl+Banana"), None);
    }
}
