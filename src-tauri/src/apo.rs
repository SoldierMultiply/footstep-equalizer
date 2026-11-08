//! System-wide apply through Equalizer APO: install detection (registry),
//! output device enumeration (MMDevices), and the managed `config.txt` block.

use crate::dsp::{Band, FilterKind};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
use winreg::RegKey;

pub const BLOCK_BEGIN: &str = "# --- Earshot begin (generated, edits inside are overwritten) ---";
pub const BLOCK_END: &str = "# --- Earshot end ---";

const APO_KEY: &str = r"SOFTWARE\EqualizerAPO";
const APO_UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\EqualizerAPO";
const RENDER_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render";
const PKEY_DEVICE_DESC: &str = "{a45c254e-df1c-4efd-8020-67d146a850e0},2";
const PKEY_ADAPTER_NAME: &str = "{b3f8fa53-0004-438e-9003-51a46e139bfc},6";
/// CLSID Equalizer APO registers into a device's FxProperties when it is enabled for it.
const APO_CLSID: &str = "{EACD2258-FCAC-4FF4-B36D-419E924A6D79}";
const DEVICE_STATE_ACTIVE: u32 = 1;

#[derive(Debug, Error)]
pub enum ApoError {
    #[error("Equalizer APO is not installed")]
    NotInstalled,
    #[error("config folder not found: {0}")]
    MissingConfig(String),
    #[error("cannot write Equalizer APO config: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApoStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub install_path: Option<String>,
    pub config_path: Option<String>,
    /// True when `config.txt` already contains an Earshot block.
    pub block_present: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub adapter: String,
    pub active: bool,
    pub apo_attached: bool,
    /// Generated sample row (no MMDevices data readable).
    pub sample: bool,
}

impl OutputDevice {
    /// Pattern for the APO `Device:` command, e.g. "Headphones Arctis Nova 7".
    pub fn apo_pattern(&self) -> String {
        format!("{} {}", self.name, self.adapter).trim().to_string()
    }
}

fn hklm(path: &str) -> Option<RegKey> {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY)
        .ok()
}

/// Read Equalizer APO install state from the registry.
pub fn detect() -> ApoStatus {
    let Some(key) = hklm(APO_KEY) else {
        return ApoStatus::default();
    };
    let install_path: Option<String> = key.get_value("InstallPath").ok();
    let config_path: Option<String> = key
        .get_value("ConfigPath")
        .ok()
        .or_else(|| install_path.as_ref().map(|p| format!(r"{p}\config")));
    let version = hklm(APO_UNINSTALL).and_then(|k| k.get_value::<String, _>("DisplayVersion").ok());
    let block_present = config_path
        .as_ref()
        .and_then(|p| fs::read_to_string(Path::new(p).join("config.txt")).ok())
        .map(|s| s.contains(BLOCK_BEGIN))
        .unwrap_or(false);
    ApoStatus { installed: install_path.is_some(), version, install_path, config_path, block_present }
}

/// Enumerate render endpoints from the MMDevices registry tree.
pub fn devices() -> Vec<OutputDevice> {
    let mut out = read_render_devices();
    if out.is_empty() {
        out = sample_devices();
    }
    out.sort_by(|a, b| b.active.cmp(&a.active).then(a.name.cmp(&b.name)));
    out
}

fn read_render_devices() -> Vec<OutputDevice> {
    let Some(render) = hklm(RENDER_KEY) else {
        return Vec::new();
    };
    render
        .enum_keys()
        .filter_map(Result::ok)
        .filter_map(|id| {
            let dev = render.open_subkey_with_flags(&id, KEY_READ).ok()?;
            let state: u32 = dev.get_value("DeviceState").unwrap_or(0);
            let props = dev.open_subkey_with_flags("Properties", KEY_READ).ok()?;
            let name: String = props.get_value(PKEY_DEVICE_DESC).ok()?;
            let adapter: String = props.get_value(PKEY_ADAPTER_NAME).unwrap_or_default();
            let apo_attached = dev
                .open_subkey_with_flags("FxProperties", KEY_READ)
                .map(|fx| {
                    fx.enum_values()
                        .filter_map(Result::ok)
                        .any(|(_, v)| v.to_string().to_ascii_uppercase().contains(APO_CLSID))
                })
                .unwrap_or(false);
            Some(OutputDevice {
                id,
                name,
                adapter,
                active: state & 0xF == DEVICE_STATE_ACTIVE,
                apo_attached,
                sample: false,
            })
        })
        .collect()
}

pub fn sample_devices() -> Vec<OutputDevice> {
    let row = |id: &str, name: &str, adapter: &str, active: bool, apo: bool| OutputDevice {
        id: id.into(),
        name: name.into(),
        adapter: adapter.into(),
        active,
        apo_attached: apo,
        sample: true,
    };
    vec![
        row("{0.0.0.00000000}.{6a1f}", "Headphones", "Arctis Nova 7", true, true),
        row("{0.0.0.00000000}.{91c4}", "Speakers", "Realtek(R) Audio", true, true),
        row("{0.0.0.00000000}.{2be0}", "Headset Earphone", "HyperX Cloud II Wireless", true, false),
        row("{0.0.0.00000000}.{f7d2}", "LG ULTRAGEAR", "NVIDIA High Definition Audio", false, false),
    ]
}

fn fmt_hz(f: f64) -> String {
    if f >= 1000.0 && (f % 1000.0).abs() < 1e-9 {
        format!("{:.0}", f)
    } else if f >= 100.0 {
        format!("{:.0}", f.round())
    } else {
        format!("{:.1}", f).trim_end_matches(".0").to_string()
    }
}

/// One Equalizer APO filter line, e.g. `Filter 7: ON PK Fc 2400 Hz Gain 4.5 dB Q 1.40`.
pub fn filter_line(n: usize, b: &Band, bypass: bool) -> String {
    let state = if b.enabled && !bypass { "ON" } else { "OFF" };
    let code = b.kind.apo_code();
    if b.kind.has_gain() {
        format!("Filter {n}: {state} {code} Fc {} Hz Gain {:.1} dB Q {:.2}", fmt_hz(b.freq), b.gain_db, b.q)
    } else {
        format!("Filter {n}: {state} {code} Fc {} Hz Q {:.2}", fmt_hz(b.freq), b.q)
    }
}

/// Render the managed block for a profile.
pub fn render_block(profile_label: &str, bands: &[Band], preamp_db: f64, bypass: bool, device: Option<&str>) -> String {
    let mut lines = vec![BLOCK_BEGIN.to_string(), format!("# Profile: {profile_label}")];
    if bypass {
        lines.push("# A/B bypass engaged: filters OFF, preamp 0 dB".into());
    }
    if let Some(d) = device {
        lines.push(format!("Device: {d}"));
    }
    lines.push(format!("Preamp: {:.1} dB", if bypass { 0.0 } else { preamp_db }));
    lines.extend(bands.iter().enumerate().map(|(i, b)| filter_line(i + 1, b, bypass)));
    if device.is_some() {
        // Restore device selection so user lines after the block keep their scope.
        lines.push("Device: all".into());
    }
    lines.push(BLOCK_END.to_string());
    lines.join("\r\n")
}

/// Replace an existing Earshot block in `existing`, or append it. Lines
/// outside the markers (user filters, Include: lines) are preserved.
pub fn merge_block(existing: &str, block: &str) -> String {
    let norm = existing.replace("\r\n", "\n");
    let block = block.replace("\r\n", "\n");
    let merged = match (norm.find(BLOCK_BEGIN), norm.find(BLOCK_END)) {
        (Some(s), Some(e)) if e > s => {
            let end = e + BLOCK_END.len();
            format!("{}{}{}", &norm[..s], block, &norm[end..])
        }
        _ => {
            let mut base = norm.trim_end().to_string();
            if !base.is_empty() {
                base.push_str("\n\n");
            }
            format!("{base}{block}\n")
        }
    };
    merged.replace('\n', "\r\n")
}

/// Write the block into `<config_dir>\config.txt`. A one-time backup of the
/// user's original file is kept as `config.txt.earshot-backup`. Equalizer APO
/// watches the file and reloads it within a few milliseconds.
pub fn write_config(config_dir: &Path, block: &str) -> Result<PathBuf, ApoError> {
    if !config_dir.is_dir() {
        return Err(ApoError::MissingConfig(config_dir.display().to_string()));
    }
    let path = config_dir.join("config.txt");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let backup = config_dir.join("config.txt.earshot-backup");
    if !existing.is_empty() && !backup.exists() {
        fs::write(&backup, &existing)?;
    }
    let tmp = config_dir.join("config.txt.earshot-tmp");
    fs::write(&tmp, merge_block(&existing, block))?;
    fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Remove the Earshot block, leaving the rest of the user's config untouched.
pub fn remove_block(config_dir: &Path) -> Result<(), ApoError> {
    let path = config_dir.join("config.txt");
    let existing = fs::read_to_string(&path)?;
    let cleaned = merge_block(&existing, "").replace("\r\n\r\n\r\n", "\r\n");
    fs::write(path, cleaned.trim_end().to_string() + "\r\n")?;
    Ok(())
}

pub fn config_dir(status: &ApoStatus) -> Result<PathBuf, ApoError> {
    status.config_path.as_ref().map(PathBuf::from).ok_or(ApoError::NotInstalled)
}

/// A preset parsed from Equalizer APO / AutoEQ "ParametricEQ.txt" text.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedPreset {
    pub preamp_db: Option<f64>,
    pub bands: Vec<Band>,
    /// Lines that looked like filters but could not be understood.
    pub skipped: Vec<String>,
}

fn kind_from_code(code: &str) -> Option<FilterKind> {
    Some(match code.to_ascii_uppercase().as_str() {
        "PK" | "PEQ" | "MODAL" => FilterKind::Peaking,
        "LS" | "LSC" | "LSQ" => FilterKind::LowShelf,
        "HS" | "HSC" | "HSQ" => FilterKind::HighShelf,
        "HP" | "HPQ" => FilterKind::HighPass,
        "LP" | "LPQ" => FilterKind::LowPass,
        _ => return None,
    })
}

/// Value following `key` in a whitespace-tokenised filter line, e.g. `Fc 2000`.
fn value_after(tokens: &[&str], key: &str) -> Option<f64> {
    let i = tokens.iter().position(|t| t.eq_ignore_ascii_case(key))?;
    tokens.get(i + 1)?.replace(',', ".").parse().ok()
}

/// Parse one `Filter[ n]: ON PK Fc 2000 Hz Gain 4.0 dB Q 1.40` line.
pub fn parse_filter_line(line: &str) -> Option<Band> {
    let (head, rest) = line.split_once(':')?;
    if !head.trim_start().to_ascii_lowercase().starts_with("filter") {
        return None;
    }
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let enabled = tokens.first()?.eq_ignore_ascii_case("ON");
    let kind = kind_from_code(tokens.get(1)?)?;
    let freq = value_after(&tokens, "Fc")?;
    let gain = if kind.has_gain() { value_after(&tokens, "Gain")? } else { 0.0 };
    // Shelves exported without Q use the 12 dB/oct default of 0.71.
    let q = value_after(&tokens, "Q").unwrap_or(if kind == FilterKind::Peaking { 1.41 } else { 0.71 });
    let mut band = Band::new(kind, freq, gain, q);
    band.enabled = enabled;
    Some(band)
}

/// Parse a whole preset file; at most `max_bands` filters are kept.
pub fn parse_preset(text: &str, max_bands: usize) -> ImportedPreset {
    let mut out = ImportedPreset { preamp_db: None, bands: Vec::new(), skipped: Vec::new() };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("preamp:") {
            out.preamp_db = line[7..].split_whitespace().next().and_then(|v| v.parse().ok());
        } else if lower.starts_with("filter") {
            match parse_filter_line(line) {
                Some(b) if out.bands.len() < max_bands => out.bands.push(b),
                _ => out.skipped.push(line.to_string()),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::{Band, FilterKind};

    #[test]
    fn parses_autoeq_style_preset() {
        let text = "Preamp: -6.2 dB\nFilter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0.70\nFilter 2: ON PK Fc 2400 Hz Gain -3.1 dB Q 1.41\nFilter 3: OFF HPQ Fc 20 Hz Q 0.71\nFilter 4: ON XYZ Fc 1 Hz\n";
        let p = parse_preset(text, 10);
        assert_eq!(p.preamp_db, Some(-6.2));
        assert_eq!(p.bands.len(), 3);
        assert_eq!(p.bands[0].kind, FilterKind::LowShelf);
        assert_eq!((p.bands[1].freq, p.bands[1].gain_db), (2400.0, -3.1));
        assert!(!p.bands[2].enabled);
        assert_eq!(p.skipped.len(), 1);
    }

    #[test]
    fn rendered_lines_round_trip() {
        let b = Band::peak(3150.0, 5.0, 1.4);
        assert_eq!(parse_filter_line(&filter_line(7, &b, false)), Some(b));
    }

    #[test]
    fn peaking_line_matches_apo_syntax() {
        let l = filter_line(1, &Band::peak(2000.0, 4.0, 1.4), false);
        assert_eq!(l, "Filter 1: ON PK Fc 2000 Hz Gain 4.0 dB Q 1.40");
    }

    #[test]
    fn shelf_and_pass_lines() {
        let ls = filter_line(2, &Band::new(FilterKind::LowShelf, 31.0, -2.0, 0.71), false);
        assert_eq!(ls, "Filter 2: ON LSC Fc 31 Hz Gain -2.0 dB Q 0.71");
        let hp = filter_line(3, &Band::new(FilterKind::HighPass, 25.0, 0.0, 0.71), true);
        assert_eq!(hp, "Filter 3: OFF HPQ Fc 25 Hz Q 0.71");
    }

    #[test]
    fn block_has_preamp_and_markers() {
        let b = render_block("Footsteps+ / CS2", &[Band::peak(2400.0, 4.5, 1.4)], -5.0, false, Some("Headphones"));
        assert!(b.starts_with(BLOCK_BEGIN) && b.ends_with(BLOCK_END));
        assert!(b.contains("Preamp: -5.0 dB"));
        assert!(b.contains("Device: Headphones\r\n"));
        assert!(b.contains("Device: all"));
    }

    #[test]
    fn bypass_zeroes_preamp() {
        let b = render_block("x", &[Band::peak(2400.0, 4.5, 1.4)], -5.0, true, None);
        assert!(b.contains("Preamp: 0.0 dB") && b.contains(": OFF PK"));
    }

    #[test]
    fn merge_appends_then_replaces_preserving_user_lines() {
        let user = "Preamp: -1 dB\r\nInclude: example.txt\r\n";
        let first = merge_block(user, &render_block("A", &[], -2.0, false, None));
        assert!(first.starts_with("Preamp: -1 dB\r\nInclude: example.txt\r\n\r\n"));
        let second = merge_block(&first, &render_block("B", &[], -3.0, false, None));
        assert_eq!(second.matches(BLOCK_BEGIN).count(), 1);
        assert!(second.contains("# Profile: B") && !second.contains("# Profile: A"));
        assert!(second.contains("Include: example.txt"));
    }

    #[test]
    fn write_config_round_trip() {
        let dir = std::env::temp_dir().join(format!("earshot-apo-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.txt"), "Include: example.txt\r\n").unwrap();
        let block = render_block("T", &[Band::peak(3000.0, 3.0, 1.0)], -3.5, false, None);
        let p = write_config(&dir, &block).unwrap();
        let text = fs::read_to_string(p).unwrap();
        assert!(text.contains("Filter 1: ON PK Fc 3000 Hz Gain 3.0 dB Q 1.00"));
        assert!(dir.join("config.txt.earshot-backup").exists());
        remove_block(&dir).unwrap();
        assert!(!fs::read_to_string(dir.join("config.txt")).unwrap().contains(BLOCK_BEGIN));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hz_formatting() {
        assert_eq!(fmt_hz(31.0), "31");
        assert_eq!(fmt_hz(62.5), "62.5");
        assert_eq!(fmt_hz(3720.4), "3720");
    }
}
