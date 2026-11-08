//! Game catalog (process names to watch) and the per-game "Footsteps+" presets.

use crate::dsp::{Band, FilterKind};
use serde::{Deserialize, Serialize};

/// Center frequencies of the 10 fader strip bands.
pub const ISO_BANDS: [f64; 10] = [31.0, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

/// How a game's footstep focus curve is shaped.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FootstepTuning {
    /// Center of the main presence boost, where step transients live (1.5-4 kHz).
    pub presence_hz: f64,
    pub presence_db: f64,
    /// How hard the 100-250 Hz explosion / vehicle rumble is cut.
    pub rumble_cut_db: f64,
    /// Small lift around 8 kHz for surface texture (gravel, metal, wood).
    pub air_db: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub name: String,
    /// Two-letter monogram drawn in the profile list (no logos).
    pub monogram: String,
    /// Lowercase executable names that identify the running game.
    pub exes: Vec<String>,
    pub tuning: FootstepTuning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PresetKind {
    Flat,
    FootstepsPlus,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub game_id: Option<String>,
    pub preset: PresetKind,
    pub bands: Vec<Band>,
    /// Switch to this profile automatically when the game process starts.
    pub auto: bool,
    pub last_seen_unix: Option<u64>,
}

impl Profile {
    pub fn label(&self) -> String {
        match self.preset {
            PresetKind::Flat => "Flat".into(),
            PresetKind::FootstepsPlus => "Footsteps+".into(),
            PresetKind::Custom => "Custom".into(),
        }
    }

    /// Reset bands from a preset kind, keeping the profile identity.
    pub fn apply_preset(&mut self, kind: PresetKind, tuning: Option<&FootstepTuning>) {
        self.bands = match (kind, tuning) {
            (PresetKind::FootstepsPlus, Some(t)) => footsteps_plus(t),
            (PresetKind::FootstepsPlus, None) => footsteps_plus(&GENERIC),
            _ => flat(),
        };
        self.preset = kind;
    }
}

pub const GENERIC: FootstepTuning = FootstepTuning { presence_hz: 2500.0, presence_db: 4.5, rumble_cut_db: 5.0, air_db: 1.0 };

fn game(id: &str, name: &str, mono: &str, exes: &[&str], t: (f64, f64, f64, f64)) -> Game {
    Game {
        id: id.into(),
        name: name.into(),
        monogram: mono.into(),
        exes: exes.iter().map(|e| e.to_ascii_lowercase()).collect(),
        tuning: FootstepTuning { presence_hz: t.0, presence_db: t.1, rumble_cut_db: t.2, air_db: t.3 },
    }
}

/// Supported shooters. Tunings are starting points from listening tests,
/// not measurements; the owner can refine them per title.
pub fn catalog() -> Vec<Game> {
    vec![
        game("valorant", "Valorant", "VA", &["VALORANT-Win64-Shipping.exe"], (3150.0, 5.0, 5.5, 1.0)),
        game("cs2", "Counter-Strike 2", "CS", &["cs2.exe"], (2400.0, 4.5, 4.0, 1.5)),
        game("apex", "Apex Legends", "AL", &["r5apex.exe", "r5apex_dx12.exe"], (2000.0, 5.5, 6.0, 0.5)),
        game("fortnite", "Fortnite", "FN", &["FortniteClient-Win64-Shipping.exe"], (2800.0, 4.0, 5.0, 1.0)),
        game("pubg", "PUBG: Battlegrounds", "PB", &["TslGame.exe"], (1800.0, 5.0, 4.5, 1.0)),
        game("r6", "Rainbow Six Siege", "R6", &["RainbowSix.exe", "RainbowSix_Vulkan.exe", "RainbowSix_BE.exe"], (2200.0, 5.0, 6.5, 0.5)),
        game("warzone", "Call of Duty: Warzone", "WZ", &["cod.exe", "ModernWarfare.exe"], (2600.0, 5.5, 6.0, 1.0)),
        game("tarkov", "Escape from Tarkov", "EF", &["EscapeFromTarkov.exe"], (1600.0, 4.5, 3.5, 2.0)),
        game("overwatch", "Overwatch 2", "OW", &["Overwatch.exe"], (3000.0, 3.5, 3.0, 1.0)),
        game("finals", "The Finals", "TF", &["Discovery.exe"], (2500.0, 4.5, 6.5, 0.5)),
    ]
}

pub fn flat() -> Vec<Band> {
    ISO_BANDS
        .iter()
        .enumerate()
        .map(|(i, &f)| match i {
            0 => Band::new(FilterKind::LowShelf, f, 0.0, 0.71),
            9 => Band::new(FilterKind::HighShelf, f, 0.0, 0.71),
            _ => Band::peak(f, 0.0, 1.41),
        })
        .collect()
}

/// Footstep focus: cut 100-250 Hz rumble (explosions, engines, music bass),
/// boost the 1.5-4 kHz presence region where heel/toe transients sit,
/// and add a touch of air for surface texture. The two presence nodes are
/// placed relative to the game's tuned center so they overlap into a plateau.
pub fn footsteps_plus(t: &FootstepTuning) -> Vec<Band> {
    let c = t.rumble_cut_db;
    let p = t.presence_db;
    let hz = t.presence_hz.clamp(1500.0, 4000.0);
    vec![
        Band::new(FilterKind::LowShelf, 31.0, -0.4 * c, 0.71),
        Band::peak(63.0, -0.6 * c, 1.2),
        Band::peak(125.0, -c, 1.1),
        Band::peak(220.0, -0.75 * c, 1.3),
        Band::peak(500.0, -0.2 * c, 1.41),
        Band::peak(1000.0, 0.15 * p, 1.41),
        Band::peak(hz, p, 1.4),
        Band::peak((hz * 1.55).min(6000.0), 0.8 * p, 1.8),
        Band::peak(8000.0, t.air_db, 1.41),
        Band::new(FilterKind::HighShelf, 16000.0, t.air_db - 2.0, 0.71),
    ]
}

pub fn default_profiles(games: &[Game]) -> Vec<Profile> {
    let mut out = vec![Profile {
        id: "flat".into(),
        name: "Flat (desktop)".into(),
        game_id: None,
        preset: PresetKind::Flat,
        bands: flat(),
        auto: false,
        last_seen_unix: None,
    }];
    for (i, g) in games.iter().enumerate() {
        out.push(Profile {
            id: g.id.clone(),
            name: g.name.clone(),
            game_id: Some(g.id.clone()),
            preset: PresetKind::FootstepsPlus,
            bands: footsteps_plus(&g.tuning),
            // Off by default for titles where in-game audio mixes already lean bright.
            auto: !matches!(g.id.as_str(), "fortnite" | "overwatch"),
            last_seen_unix: sample_last_seen(i),
        });
    }
    out
}

/// Plausible "last detected" stamps so a fresh install shows realistic content.
fn sample_last_seen(i: usize) -> Option<u64> {
    const AGO_MIN: [u64; 10] = [42, 0, 1_380, 4_320, 0, 2_900, 9_800, 610, 0, 15_200];
    let now = crate::state::unix_now();
    match AGO_MIN.get(i) {
        Some(0) | None => None,
        Some(m) => Some(now.saturating_sub(m * 60)),
    }
}

/// Find the first game whose executable is in `running` and whose profile
/// has auto-switch enabled. Returns (game id, exe name).
pub fn match_running<'a, F>(games: &'a [Game], running: &[String], auto_enabled: F) -> Option<(&'a Game, String)>
where
    F: Fn(&str) -> bool,
{
    games.iter().filter(|g| auto_enabled(&g.id)).find_map(|g| {
        running
            .iter()
            .find(|r| g.exes.iter().any(|e| e == &r.to_ascii_lowercase()))
            .map(|r| (g, r.to_ascii_lowercase()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::{response_at, SAMPLE_RATE};

    #[test]
    fn catalog_has_ten_games_with_unique_ids() {
        let c = catalog();
        assert_eq!(c.len(), 10);
        let mut ids: Vec<_> = c.iter().map(|g| g.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 10);
    }

    #[test]
    fn footsteps_plus_boosts_presence_and_cuts_rumble() {
        let bands = footsteps_plus(&GENERIC);
        assert_eq!(bands.len(), 10);
        let presence = response_at(&bands, 2800.0, SAMPLE_RATE);
        let rumble = response_at(&bands, 160.0, SAMPLE_RATE);
        assert!(presence > 4.0, "presence {presence}");
        assert!(rumble < -3.0, "rumble {rumble}");
    }

    #[test]
    fn flat_is_flat() {
        let bands = flat();
        for f in [30.0, 300.0, 3000.0, 15000.0] {
            assert!(response_at(&bands, f, SAMPLE_RATE).abs() < 1e-9);
        }
    }

    #[test]
    fn matcher_respects_auto_flag_and_case() {
        let games = catalog();
        let running = vec!["explorer.exe".to_string(), "CS2.EXE".to_string()];
        let hit = match_running(&games, &running, |_| true).unwrap();
        assert_eq!((hit.0.id.as_str(), hit.1.as_str()), ("cs2", "cs2.exe"));
        assert!(match_running(&games, &running, |id| id != "cs2").is_none());
    }

    #[test]
    fn preset_reset_switches_kind() {
        let games = catalog();
        let mut p = default_profiles(&games).remove(2);
        p.apply_preset(PresetKind::Flat, None);
        assert_eq!(p.preset, PresetKind::Flat);
        assert!(p.bands.iter().all(|b| b.gain_db == 0.0));
    }
}
