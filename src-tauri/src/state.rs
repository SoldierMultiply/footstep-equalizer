//! Application state, persistence under %APPDATA%\Earshot, and the service
//! layer shared by commands, the process watcher, the hotkey and the tray.

use crate::apo::{self, ApoStatus};
use crate::dsp::{self, Band, GuardResult};
use crate::profiles::{self, Game, PresetKind, Profile};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

pub const EVT_SNAPSHOT: &str = "earshot:snapshot";
pub const EVT_DETECTED: &str = "earshot:detected";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub auto_switch: bool,
    pub poll_ms: u64,
    pub revert_on_exit: bool,
    pub fallback_profile: String,
    pub headroom_db: f64,
    pub loudness_guard: bool,
    pub system_wide: bool,
    pub bypass_hotkey: String,
    pub start_with_windows: bool,
    pub minimize_to_tray: bool,
    pub output_device: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            auto_switch: true,
            poll_ms: 1500,
            revert_on_exit: true,
            fallback_profile: "flat".into(),
            headroom_db: 0.5,
            loudness_guard: true,
            system_wide: true,
            bypass_hotkey: "Ctrl+Alt+B".into(),
            start_with_windows: true,
            minimize_to_tray: true,
            output_device: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub game_id: String,
    pub game_name: String,
    pub exe: String,
    pub since_unix: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub games: Vec<Game>,
    pub profiles: Vec<Profile>,
    pub active_id: String,
    pub bypass: bool,
    pub detected: Option<Detection>,
    pub settings: Settings,
    pub guard: GuardResult,
    pub apo: ApoStatus,
    pub last_apply: Option<String>,
}

pub struct Inner {
    pub games: Vec<Game>,
    pub profiles: Vec<Profile>,
    pub active_id: String,
    pub bypass: bool,
    pub settings: Settings,
    pub detected: Option<Detection>,
    pub apo: ApoStatus,
    pub last_apply: Option<String>,
}

pub struct AppState(Mutex<Inner>);

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Earshot")
}

fn read_json<T: DeserializeOwned>(name: &str) -> Option<T> {
    let text = fs::read_to_string(data_dir().join(name)).ok()?;
    serde_json::from_str(&text)
        .map_err(|e| log::warn!("ignoring corrupt {name}: {e}"))
        .ok()
}

fn write_json<T: Serialize>(name: &str, value: &T) -> anyhow::Result<()> {
    let dir = data_dir();
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(name), serde_json::to_string_pretty(value)?)?;
    Ok(())
}

impl AppState {
    pub fn load() -> Self {
        let games = profiles::catalog();
        let settings: Settings = read_json("settings.json").unwrap_or_default();
        let mut stored: Vec<Profile> = read_json("profiles.json").unwrap_or_default();
        // Merge: keep user edits, add profiles for games added in newer versions.
        for p in profiles::default_profiles(&games) {
            if !stored.iter().any(|s| s.id == p.id) {
                stored.push(p);
            }
        }
        let active_id = settings.fallback_profile.clone();
        AppState(Mutex::new(Inner {
            games,
            profiles: stored,
            active_id,
            bypass: false,
            settings,
            detected: None,
            apo: apo::detect(),
            last_apply: None,
        }))
    }

    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Inner {
    pub fn active(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.id == self.active_id)
            .unwrap_or(&self.profiles[0])
    }

    pub fn profile_mut(&mut self, id: &str) -> Result<&mut Profile, String> {
        self.profiles.iter_mut().find(|p| p.id == id).ok_or_else(|| format!("unknown profile '{id}'"))
    }

    pub fn game(&self, id: &str) -> Option<&Game> {
        self.games.iter().find(|g| g.id == id)
    }

    pub fn guard(&self) -> GuardResult {
        let g = dsp::no_clip_preamp(&self.active().bands, self.settings.headroom_db);
        if self.settings.loudness_guard {
            g
        } else {
            GuardResult { preamp_db: 0.0, ..g }
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            games: self.games.clone(),
            profiles: self.profiles.clone(),
            active_id: self.active_id.clone(),
            bypass: self.bypass,
            detected: self.detected.clone(),
            settings: self.settings.clone(),
            guard: self.guard(),
            apo: self.apo.clone(),
            last_apply: self.last_apply.clone(),
        }
    }

    pub fn persist(&self) {
        if let Err(e) = write_json("settings.json", &self.settings).and_then(|_| write_json("profiles.json", &self.profiles)) {
            log::error!("failed to save state: {e}");
        }
    }

    pub fn set_band(&mut self, id: &str, index: usize, band: Band) -> Result<(), String> {
        let p = self.profile_mut(id)?;
        let slot = p.bands.get_mut(index).ok_or("band index out of range")?;
        *slot = band.clamped();
        p.preset = PresetKind::Custom;
        Ok(())
    }

    pub fn reset_preset(&mut self, id: &str, kind: PresetKind) -> Result<(), String> {
        let tuning = {
            let p = self.profiles.iter().find(|p| p.id == id).ok_or("unknown profile")?;
            p.game_id.as_deref().and_then(|g| self.game(g)).map(|g| g.tuning)
        };
        self.profile_mut(id)?.apply_preset(kind, tuning.as_ref());
        Ok(())
    }

    pub fn device_pattern(&self) -> Option<String> {
        let wanted = self.settings.output_device.as_deref()?;
        apo::devices().into_iter().find(|d| d.id == wanted).map(|d| d.apo_pattern())
    }

    /// Text that would be written into Equalizer APO's config.txt.
    pub fn render_config(&self) -> String {
        let p = self.active();
        let label = format!("{} / {}", p.label(), p.name);
        apo::render_block(&label, &p.bands, self.guard().preamp_db, self.bypass, self.device_pattern().as_deref())
    }

    /// Push the active profile system-wide when enabled and APO is present.
    pub fn apply_system_wide(&mut self) -> Result<Option<PathBuf>, apo::ApoError> {
        if !self.settings.system_wide {
            return Ok(None);
        }
        if !self.apo.installed {
            return Err(apo::ApoError::NotInstalled);
        }
        let path = apo::write_config(&apo::config_dir(&self.apo)?, &self.render_config())?;
        self.apo.block_present = true;
        self.last_apply = Some(format!("{} at {}", self.active().name, unix_now()));
        Ok(Some(path))
    }
}

/// Add or remove the per-user Run entry that starts Earshot minimized to tray.
pub fn set_autostart(enabled: bool) -> std::io::Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    let run = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_SET_VALUE)?;
    if enabled {
        let exe = std::env::current_exe()?;
        run.set_value("Earshot", &format!("\"{}\" --minimized", exe.display()))
    } else {
        match run.delete_value("Earshot") {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

/// Emit the full snapshot to the UI and refresh the tray label.
pub fn broadcast(app: &AppHandle) {
    let (snap, tray_text) = {
        let st = app.state::<AppState>();
        let inner = st.lock();
        let p = inner.active();
        let text = if inner.bypass {
            format!("Bypassed (A/B) - {}", p.name)
        } else {
            format!("{} - {}", p.label(), p.name)
        };
        (inner.snapshot(), text)
    };
    if let Err(e) = app.emit(EVT_SNAPSHOT, &snap) {
        log::warn!("emit failed: {e}");
    }
    crate::set_tray_status(app, &tray_text);
}

/// Switch profile, apply it system-wide and notify listeners.
pub fn activate(app: &AppHandle, id: &str) -> Result<(), String> {
    {
        let st = app.state::<AppState>();
        let mut inner = st.lock();
        if !inner.profiles.iter().any(|p| p.id == id) {
            return Err(format!("unknown profile '{id}'"));
        }
        inner.active_id = id.to_string();
        if let Err(e) = inner.apply_system_wide() {
            log::warn!("system-wide apply skipped: {e}");
        }
    }
    broadcast(app);
    Ok(())
}

pub fn toggle_bypass(app: &AppHandle) -> bool {
    let now = {
        let st = app.state::<AppState>();
        let mut inner = st.lock();
        inner.bypass = !inner.bypass;
        if let Err(e) = inner.apply_system_wide() {
            log::warn!("bypass apply skipped: {e}");
        }
        inner.bypass
    };
    broadcast(app);
    now
}
