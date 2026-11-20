use eframe::egui::{self, Color32};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RELEASES_URL: &str = "https://github.com/morganlowe/fun-key-synth/releases/latest";
const API_LATEST_URL: &str = "https://api.github.com/repos/morganlowe/fun-key-synth/releases/latest";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StudioTheme {
    #[default]
    Midnight,
    NeonSynthwave,
    AmberAnalog,
    AcidTerminal,
    NordicFrost,
    Oberheim80s,
    Solina70s,
    Daylight,
}

impl StudioTheme {
    pub const ALL: [StudioTheme; 8] = [
        StudioTheme::Midnight,
        StudioTheme::NeonSynthwave,
        StudioTheme::AmberAnalog,
        StudioTheme::AcidTerminal,
        StudioTheme::NordicFrost,
        StudioTheme::Oberheim80s,
        StudioTheme::Solina70s,
        StudioTheme::Daylight,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            StudioTheme::Midnight => "Midnight Studio",
            StudioTheme::NeonSynthwave => "Neon Synthwave",
            StudioTheme::AmberAnalog => "Amber Analog",
            StudioTheme::AcidTerminal => "Acid Terminal",
            StudioTheme::NordicFrost => "Nordic Frost",
            StudioTheme::Oberheim80s => "80s OB-X Pinstripe",
            StudioTheme::Solina70s => "70s Vintage Walnut",
            StudioTheme::Daylight => "Daylight Light",
        }
    }

    pub const fn is_light(self) -> bool {
        matches!(self, StudioTheme::Daylight)
    }

    pub const fn panel_bg(self) -> Color32 {
        match self {
            StudioTheme::Midnight => Color32::from_rgb(12, 14, 20),
            StudioTheme::NeonSynthwave => Color32::from_rgb(16, 10, 26),
            StudioTheme::AmberAnalog => Color32::from_rgb(20, 15, 12),
            StudioTheme::AcidTerminal => Color32::from_rgb(8, 12, 10),
            StudioTheme::NordicFrost => Color32::from_rgb(28, 34, 44),
            StudioTheme::Oberheim80s => Color32::from_rgb(13, 16, 24),
            StudioTheme::Solina70s => Color32::from_rgb(26, 18, 13),
            StudioTheme::Daylight => Color32::from_rgb(236, 240, 246),
        }
    }

    pub const fn bar_bg(self) -> Color32 {
        match self {
            StudioTheme::Midnight => Color32::from_rgb(16, 19, 27),
            StudioTheme::NeonSynthwave => Color32::from_rgb(22, 14, 36),
            StudioTheme::AmberAnalog => Color32::from_rgb(28, 21, 16),
            StudioTheme::AcidTerminal => Color32::from_rgb(12, 18, 14),
            StudioTheme::NordicFrost => Color32::from_rgb(36, 44, 56),
            StudioTheme::Oberheim80s => Color32::from_rgb(18, 23, 36),
            StudioTheme::Solina70s => Color32::from_rgb(36, 25, 18),
            StudioTheme::Daylight => Color32::from_rgb(224, 230, 240),
        }
    }

    pub const fn card_bg(self) -> Color32 {
        match self {
            StudioTheme::Midnight => Color32::from_rgb(18, 22, 32),
            StudioTheme::NeonSynthwave => Color32::from_rgb(26, 16, 42),
            StudioTheme::AmberAnalog => Color32::from_rgb(30, 22, 17),
            StudioTheme::AcidTerminal => Color32::from_rgb(13, 20, 16),
            StudioTheme::NordicFrost => Color32::from_rgb(42, 50, 64),
            StudioTheme::Oberheim80s => Color32::from_rgb(20, 26, 42),
            StudioTheme::Solina70s => Color32::from_rgb(40, 28, 20),
            StudioTheme::Daylight => Color32::from_rgb(246, 248, 252),
        }
    }

    pub const fn border(self) -> Color32 {
        match self {
            StudioTheme::Midnight => Color32::from_rgb(34, 41, 58),
            StudioTheme::NeonSynthwave => Color32::from_rgb(66, 36, 98),
            StudioTheme::AmberAnalog => Color32::from_rgb(68, 48, 32),
            StudioTheme::AcidTerminal => Color32::from_rgb(34, 62, 42),
            StudioTheme::NordicFrost => Color32::from_rgb(62, 74, 94),
            StudioTheme::Oberheim80s => Color32::from_rgb(56, 118, 212),
            StudioTheme::Solina70s => Color32::from_rgb(104, 72, 46),
            StudioTheme::Daylight => Color32::from_rgb(198, 206, 220),
        }
    }

    pub const fn default_accent(self) -> [u8; 3] {
        match self {
            StudioTheme::Midnight => [64, 196, 255],
            StudioTheme::NeonSynthwave => [255, 76, 186],
            StudioTheme::AmberAnalog => [255, 178, 56],
            StudioTheme::AcidTerminal => [168, 255, 58],
            StudioTheme::NordicFrost => [136, 192, 208],
            StudioTheme::Oberheim80s => [82, 168, 255],
            StudioTheme::Solina70s => [244, 196, 92],
            StudioTheme::Daylight => [38, 126, 218],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub check_updates_on_startup: bool,
    pub theme: StudioTheme,
    pub use_vibe_accent: bool,
    pub custom_accent: [u8; 3],
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            check_updates_on_startup: true,
            theme: StudioTheme::Midnight,
            use_vibe_accent: true,
            custom_accent: [64, 196, 255],
        }
    }
}

impl AppSettings {
    fn config_path() -> Option<PathBuf> {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(xdg).join("fun-key-synth").join("settings.json"));
        }
        if let Ok(appdata) = std::env::var("APPDATA") {
            return Some(PathBuf::from(appdata).join("fun-key-synth").join("settings.json"));
        }
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".config").join("fun-key-synth").join("settings.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn apply_to_egui(&self, ctx: &egui::Context) {
        let mut visuals = if self.theme.is_light() {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        visuals.window_fill = self.theme.bar_bg();
        visuals.panel_fill = self.theme.panel_bg();
        if !self.theme.is_light() {
            visuals.widgets.noninteractive.bg_fill = self.theme.card_bg();
            visuals.widgets.inactive.bg_fill = self.theme.bar_bg();
            visuals.widgets.hovered.bg_fill = self.theme.border();
            visuals.widgets.active.bg_fill = self.theme.border();
        }
        let [r, g, b] = self.custom_accent;
        visuals.selection.bg_fill = Color32::from_rgb(r, g, b);
        ctx.set_visuals(visuals);
    }
}

#[derive(Debug, Clone, Default)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    UpdateAvailable { latest_tag: String, url: String },
    OfflineOrError(String),
}

pub fn spawn_update_check(status: Arc<Mutex<UpdateStatus>>) {
    if let Ok(mut s) = status.lock() {
        *s = UpdateStatus::Checking;
    }
    std::thread::spawn(move || {
        let result = check_github_latest();
        if let Ok(mut s) = status.lock() {
            *s = result;
        }
    });
}

fn check_github_latest() -> UpdateStatus {
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "5",
            "-H",
            "User-Agent: fun-key-synth",
            API_LATEST_URL,
        ])
        .output();

    let Ok(out) = output else {
        return UpdateStatus::OfflineOrError("curl unavailable".into());
    };
    if !out.status.success() {
        return UpdateStatus::OfflineOrError("Offline or GitHub unreachable".into());
    }
    let Ok(body) = String::from_utf8(out.stdout) else {
        return UpdateStatus::OfflineOrError("Invalid response".into());
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) else {
        return UpdateStatus::OfflineOrError("Invalid JSON".into());
    };
    let Some(tag) = val.get("tag_name").and_then(|v| v.as_str()) else {
        return UpdateStatus::UpToDate;
    };
    let url = val
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or(RELEASES_URL)
        .to_string();

    if is_newer_version(tag, CURRENT_VERSION) {
        UpdateStatus::UpdateAvailable {
            latest_tag: tag.to_string(),
            url,
        }
    } else {
        UpdateStatus::UpToDate
    }
}

fn parse_semver(s: &str) -> (u32, u32, u32) {
    let clean = s.trim().trim_start_matches('v').trim_start_matches('V');
    let mut parts = clean.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    parse_semver(latest) > parse_semver(current)
}
