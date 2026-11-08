//! Tauri commands invoked from the UI via `window.__TAURI__.core.invoke`.

use crate::analyzer::{SpectrumFrame, Synth};
use crate::apo::{self, ApoStatus, OutputDevice};
use crate::dsp::{self, Band, Curve};
use crate::profiles::PresetKind;
use crate::state::{self, AppState, Settings, Snapshot};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, State};

type CmdResult<T> = Result<T, String>;

fn snap(app: &AppHandle, st: &State<'_, AppState>) -> Snapshot {
    state::broadcast(app);
    st.lock().snapshot()
}

#[tauri::command]
pub fn get_snapshot(st: State<'_, AppState>) -> Snapshot {
    st.lock().snapshot()
}

/// Curve for drawing; defaults to the active profile.
#[tauri::command]
pub fn get_curve(st: State<'_, AppState>, profile_id: Option<String>, points: Option<usize>) -> CmdResult<Curve> {
    let inner = st.lock();
    let p = match profile_id {
        Some(id) => inner.profiles.iter().find(|p| p.id == id).ok_or("unknown profile")?,
        None => inner.active(),
    };
    Ok(dsp::curve(&p.bands, points.unwrap_or(256)))
}

/// Called while a band node or fader is dragged. Does not write APO on every
/// move; the UI calls `commit_edit` on pointer-up.
#[tauri::command]
pub fn set_band(st: State<'_, AppState>, profile_id: String, index: usize, band: Band) -> CmdResult<dsp::GuardResult> {
    let mut inner = st.lock();
    inner.set_band(&profile_id, index, band)?;
    Ok(inner.guard())
}

#[tauri::command]
pub fn commit_edit(app: AppHandle, st: State<'_, AppState>) -> CmdResult<Snapshot> {
    {
        let mut inner = st.lock();
        inner.persist();
        if let Err(e) = inner.apply_system_wide() {
            log::warn!("apply skipped: {e}");
        }
    }
    Ok(snap(&app, &st))
}

#[tauri::command]
pub fn set_preset(app: AppHandle, st: State<'_, AppState>, profile_id: String, preset: PresetKind) -> CmdResult<Snapshot> {
    {
        let mut inner = st.lock();
        inner.reset_preset(&profile_id, preset)?;
        inner.persist();
        let _ = inner.apply_system_wide();
    }
    Ok(snap(&app, &st))
}

#[tauri::command]
pub fn set_auto(app: AppHandle, st: State<'_, AppState>, profile_id: String, enabled: bool) -> CmdResult<Snapshot> {
    {
        let mut inner = st.lock();
        inner.profile_mut(&profile_id)?.auto = enabled;
        inner.persist();
    }
    Ok(snap(&app, &st))
}

#[tauri::command]
pub fn activate_profile(app: AppHandle, st: State<'_, AppState>, profile_id: String) -> CmdResult<Snapshot> {
    state::activate(&app, &profile_id)?;
    Ok(st.lock().snapshot())
}

#[tauri::command]
pub fn toggle_bypass(app: AppHandle) -> bool {
    state::toggle_bypass(&app)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub path: Option<String>,
    pub text: String,
    pub error: Option<String>,
}

/// Force a system-wide write now (Devices screen "Apply to Equalizer APO").
#[tauri::command]
pub fn apply_now(app: AppHandle, st: State<'_, AppState>) -> ApplyReport {
    let report = {
        let mut inner = st.lock();
        let text = inner.render_config();
        match inner.apply_system_wide() {
            Ok(path) => ApplyReport { path: path.map(|p| p.display().to_string()), text, error: None },
            Err(e) => ApplyReport { path: None, text, error: Some(e.to_string()) },
        }
    };
    state::broadcast(&app);
    report
}

#[tauri::command]
pub fn preview_config(st: State<'_, AppState>) -> String {
    st.lock().render_config()
}

#[tauri::command]
pub fn remove_from_apo(app: AppHandle, st: State<'_, AppState>) -> CmdResult<Snapshot> {
    {
        let mut inner = st.lock();
        let dir = apo::config_dir(&inner.apo).map_err(|e| e.to_string())?;
        apo::remove_block(&dir).map_err(|e| e.to_string())?;
        inner.apo.block_present = false;
    }
    Ok(snap(&app, &st))
}

#[tauri::command]
pub fn refresh_apo(st: State<'_, AppState>) -> ApoStatus {
    let status = apo::detect();
    st.lock().apo = status.clone();
    status
}

#[tauri::command]
pub fn list_devices() -> Vec<OutputDevice> {
    apo::devices()
}

#[tauri::command]
pub fn update_settings(app: AppHandle, st: State<'_, AppState>, settings: Settings) -> CmdResult<Snapshot> {
    {
        let mut inner = st.lock();
        let mut s = settings;
        s.poll_ms = s.poll_ms.clamp(250, 10_000);
        s.headroom_db = s.headroom_db.clamp(0.0, 6.0);
        if s.start_with_windows != inner.settings.start_with_windows {
            state::set_autostart(s.start_with_windows).map_err(|e| e.to_string())?;
        }
        inner.settings = s;
        inner.persist();
        let _ = inner.apply_system_wide();
    }
    Ok(snap(&app, &st))
}

/// One analyzer frame. Sample signal today; see analyzer.rs for the capture plan.
#[tauri::command]
pub fn spectrum_frame(synth: State<'_, Mutex<Synth>>, dt_ms: Option<f64>) -> SpectrumFrame {
    synth.lock().unwrap_or_else(|e| e.into_inner()).next(dt_ms.unwrap_or(16.7))
}

/// Import an Equalizer APO / AutoEQ parametric preset into a profile.
/// The file's own preamp is ignored: the no-clip guard recomputes it.
#[tauri::command]
pub fn import_preset(app: AppHandle, st: State<'_, AppState>, profile_id: String, text: String) -> CmdResult<Snapshot> {
    let parsed = apo::parse_preset(&text, 16);
    if parsed.bands.is_empty() {
        return Err("no Filter: lines found in the preset".into());
    }
    {
        let mut inner = st.lock();
        let p = inner.profile_mut(&profile_id)?;
        p.bands = parsed.bands;
        p.preset = PresetKind::Custom;
        inner.persist();
        let _ = inner.apply_system_wide();
    }
    Ok(snap(&app, &st))
}
