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
    UpdateAvailable {
        latest_tag: String,
        url: String,
        asset_url: Option<String>,
        asset_name: Option<String>,
    },
    Installing {
        latest_tag: String,
        stage: String,
    },
    Installed {
        latest_tag: String,
    },
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

fn find_platform_asset(val: &serde_json::Value) -> (Option<String>, Option<String>) {
    let Some(assets) = val.get("assets").and_then(|v| v.as_array()) else {
        return (None, None);
    };
    for asset in assets {
        let Some(name) = asset.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(dl_url) = asset.get("browser_download_url").and_then(|v| v.as_str()) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        #[cfg(target_os = "linux")]
        let matches = lower.contains("linux") && lower.ends_with(".tar.gz");
        #[cfg(target_os = "windows")]
        let matches = lower.contains("windows") && lower.ends_with(".zip");
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        let matches = false;

        if matches {
            return (Some(dl_url.to_string()), Some(name.to_string()));
        }
    }
    (None, None)
}

fn check_github_latest() -> UpdateStatus {
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "8",
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
        let (asset_url, asset_name) = find_platform_asset(&val);
        UpdateStatus::UpdateAvailable {
            latest_tag: tag.to_string(),
            url,
            asset_url,
            asset_name,
        }
    } else {
        UpdateStatus::UpToDate
    }
}

/// Downloads and installs the release binary in-place on a background thread.
pub fn spawn_install_update(
    status: Arc<Mutex<UpdateStatus>>,
    latest_tag: String,
    asset_url: String,
    asset_name: String,
) {
    let set_stage = |st: &Arc<Mutex<UpdateStatus>>, tag: &str, msg: &str| {
        if let Ok(mut guard) = st.lock() {
            *guard = UpdateStatus::Installing {
                latest_tag: tag.to_string(),
                stage: msg.to_string(),
            };
        }
    };

    set_stage(&status, &latest_tag, "Downloading release...");
    std::thread::spawn(move || {
        match install_update_blocking(&status, &latest_tag, &asset_url, &asset_name) {
            Ok(()) => {
                if let Ok(mut guard) = status.lock() {
                    *guard = UpdateStatus::Installed { latest_tag };
                }
            }
            Err(err) => {
                if let Ok(mut guard) = status.lock() {
                    *guard = UpdateStatus::OfflineOrError(format!("Install failed: {err}"));
                }
            }
        }
    });
}

fn find_file_recursive(dir: &std::path::Path, target_name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() && p.file_name().and_then(|n| n.to_str()) == Some(target_name) {
            return Some(p);
        }
        if p.is_dir() {
            if let Some(found) = find_file_recursive(&p, target_name) {
                return Some(found);
            }
        }
    }
    None
}

fn install_update_blocking(
    status: &Arc<Mutex<UpdateStatus>>,
    latest_tag: &str,
    asset_url: &str,
    asset_name: &str,
) -> Result<(), String> {
    let update_dir = std::env::temp_dir().join(format!("fun-key-synth-update-{latest_tag}"));
    let _ = std::fs::remove_dir_all(&update_dir);
    std::fs::create_dir_all(&update_dir).map_err(|e| format!("temp dir: {e}"))?;

    let archive_path = update_dir.join(asset_name);

    // 1. Download asset
    let dl = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "120",
            "-H",
            "User-Agent: fun-key-synth",
            "-o",
            archive_path.to_str().ok_or("invalid temp path")?,
            asset_url,
        ])
        .output()
        .map_err(|e| format!("curl launch: {e}"))?;
    if !dl.status.success() {
        return Err("download failed".into());
    }

    // 2. Extract archive
    if let Ok(mut guard) = status.lock() {
        *guard = UpdateStatus::Installing {
            latest_tag: latest_tag.to_string(),
            stage: "Extracting...".into(),
        };
    }

    #[cfg(target_os = "linux")]
    {
        let ex = std::process::Command::new("tar")
            .args([
                "-xzf",
                archive_path.to_str().ok_or("invalid archive path")?,
                "-C",
                update_dir.to_str().ok_or("invalid extract dir")?,
            ])
            .output()
            .map_err(|e| format!("tar launch: {e}"))?;
        if !ex.status.success() {
            return Err("tar extraction failed".into());
        }

        let new_bin = find_file_recursive(&update_dir, "fun-key-synth")
            .ok_or("binary not found in archive")?;
        let current_exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;

        if let Ok(mut guard) = status.lock() {
            *guard = UpdateStatus::Installing {
                latest_tag: latest_tag.to_string(),
                stage: "Installing binary...".into(),
            };
        }

        // Make new binary executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&new_bin, std::fs::Permissions::from_mode(0o755));
        }

        // Try direct atomic swap in the current executable's directory
        let staged = current_exe.with_extension("new");
        match std::fs::copy(&new_bin, &staged).and_then(|_| std::fs::rename(&staged, &current_exe)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                // System-wide install (e.g. /usr/bin/fun-key-synth): prompt via pkexec
                let _ = std::fs::remove_file(&staged);
                let pk = std::process::Command::new("pkexec")
                    .args([
                        "install",
                        "-Dm755",
                        new_bin.to_str().ok_or("invalid bin path")?,
                        current_exe.to_str().ok_or("invalid exe path")?,
                    ])
                    .output()
                    .map_err(|err| format!("pkexec unavailable ({err})"))?;
                if !pk.status.success() {
                    return Err("elevated install cancelled or failed".into());
                }
            }
            Err(e) => {
                let _ = std::fs::remove_file(&staged);
                return Err(format!("replace failed: {e}"));
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let ps_cmd = format!(
            "Expand-Archive -Force -Path '{}' -DestinationPath '{}'",
            archive_path.display(),
            update_dir.display()
        );
        let ex = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps_cmd])
            .output()
            .map_err(|e| format!("powershell launch: {e}"))?;
        if !ex.status.success() {
            return Err("zip extraction failed".into());
        }

        let new_bin = find_file_recursive(&update_dir, "fun-key-synth.exe")
            .ok_or("fun-key-synth.exe not found in zip")?;
        let current_exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;

        if let Ok(mut guard) = status.lock() {
            *guard = UpdateStatus::Installing {
                latest_tag: latest_tag.to_string(),
                stage: "Installing binary...".into(),
            };
        }

        let old_exe = current_exe.with_extension("exe.old");
        let _ = std::fs::remove_file(&old_exe);
        std::fs::rename(&current_exe, &old_exe).map_err(|e| format!("rename old exe: {e}"))?;
        if let Err(e) = std::fs::copy(&new_bin, &current_exe) {
            let _ = std::fs::rename(&old_exe, &current_exe);
            return Err(format!("copy new exe: {e}"));
        }
    }

    let _ = std::fs::remove_dir_all(&update_dir);
    Ok(())
}

/// Relaunches the newly installed binary and exits the current process.
pub fn restart_app() {
    if let Ok(exe) = std::env::current_exe() {
        if std::process::Command::new(exe).spawn().is_ok() {
            std::process::exit(0);
        }
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
