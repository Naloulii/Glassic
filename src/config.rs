use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ObsWebConfig {
    pub sync_with_desktop: bool,
    pub vinyl: bool,
    pub art_style: String,     // "vinyl", "rounded", "circle"
    pub layout: String,        // "standard", "compact", "minimal"
    pub color: bool,           // Cover colors blurred background
    pub glassy: bool,          // Glassmorphism effect
    pub source_color: bool,    // Use custom/app accent color instead of source/cover
    pub custom_accent: String, // Default "#9A22E2"
    pub scale: u32,            // 50..=200
    pub opacity: u32,          // 10..=100
    pub border_radius: u32,    // 0..=40
    pub drop_shadow: bool,     // Box shadow under OBS card
    pub show_lyrics: bool,     // Show synced lyrics in OBS
    pub hide_when_paused: bool,// Auto-hide on OBS when paused
}

impl Default for ObsWebConfig {
    fn default() -> Self {
        Self {
            sync_with_desktop: false,
            vinyl: true,
            art_style: "vinyl".to_string(),
            layout: "standard".to_string(),
            color: true,
            glassy: true,
            source_color: false,
            custom_accent: "#9A22E2".to_string(),
            scale: 100,
            opacity: 100,
            border_radius: 20,
            drop_shadow: true,
            show_lyrics: false,
            hide_when_paused: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub vinyl: bool,
    pub art_style: String,            // "vinyl", "rounded", "circle"
    pub layout: String,               // "standard", "compact", "minimal"
    pub color: bool,                  // Cover colors
    pub glassy: bool,                 // Glassy effect
    pub source_color: bool,           // App accent color (#9A22E2)
    pub always_on_top: bool,
    pub click_through: bool,          // Ghost / Click-through mode
    pub scale: u32,                   // 50..=200
    pub opacity: u32,                 // 10..=100
    pub keybind: String,              // Settings keybind (default "F5")
    pub ghost_keybind: String,        // Click-through toggle keybind (default "F6")
    pub lang: String,                 // "fr" or "en"
    pub controls_mode: String,        // "hover", "always", "off"
    pub show_lyrics: bool,            // Synced lyrics via LRCLIB
    pub auto_hide_pause_secs: u32,    // 0 = disabled, 1 = immediate, 5, 10, 30
    pub hide_on_fullscreen: bool,     // Hide widget when fullscreen app/game is active
    pub web_server: bool,             // Enable OBS Web Server
    pub web_port: u16,                // Default 3000
    pub auto_start: bool,             // Launch on Windows startup
    pub first_run_completed: bool,
    pub window_x: Option<f32>,
    pub window_y: Option<f32>,
    pub obs: ObsWebConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            vinyl: true,
            art_style: "vinyl".to_string(),
            layout: "standard".to_string(),
            color: false,
            glassy: true,
            source_color: false,
            always_on_top: false,
            click_through: false,
            scale: 100,
            opacity: 100,
            keybind: "F5".to_string(),
            ghost_keybind: "F6".to_string(),
            lang: "fr".to_string(),
            controls_mode: "off".to_string(),
            show_lyrics: false,
            auto_hide_pause_secs: 0,
            hide_on_fullscreen: false,
            web_server: false,
            web_port: 3000,
            auto_start: false,
            first_run_completed: false,
            window_x: None,
            window_y: None,
            obs: ObsWebConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn app_data_dir() -> PathBuf {
        let base = env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        let dir = base.join("Glassic");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    pub fn config_path() -> PathBuf {
        Self::app_data_dir().join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&content) {
                cfg.scale = cfg.scale.clamp(50, 200);
                cfg.opacity = cfg.opacity.clamp(10, 100);
                cfg.obs.scale = cfg.obs.scale.clamp(50, 200);
                cfg.obs.opacity = cfg.obs.opacity.clamp(10, 100);
                cfg.obs.border_radius = cfg.obs.border_radius.clamp(0, 40);
                if cfg.web_port == 0 {
                    cfg.web_port = 3000;
                }
                return cfg;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }

    pub fn export_to_file(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn import_from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let cfg: AppConfig = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        cfg.save();
        Ok(cfg)
    }

    /// Returns the effective OBS config (either synced with desktop or independent)
    pub fn effective_obs_payload(&self) -> serde_json::Value {
        if self.obs.sync_with_desktop {
            serde_json::json!({
                "vinyl": self.vinyl && self.art_style == "vinyl",
                "artStyle": self.art_style,
                "layout": self.layout,
                "color": self.color,
                "glassy": self.glassy,
                "sourceColor": self.source_color,
                "customAccent": "#9A22E2",
                "scale": self.scale,
                "opacity": self.opacity,
                "borderRadius": 20,
                "dropShadow": true,
                "showLyrics": self.show_lyrics,
                "hideWhenPaused": self.auto_hide_pause_secs > 0,
            })
        } else {
            serde_json::json!({
                "vinyl": self.obs.vinyl && self.obs.art_style == "vinyl",
                "artStyle": self.obs.art_style,
                "layout": self.obs.layout,
                "color": self.obs.color,
                "glassy": self.obs.glassy,
                "sourceColor": self.obs.source_color,
                "customAccent": self.obs.custom_accent,
                "scale": self.obs.scale,
                "opacity": self.obs.opacity,
                "borderRadius": self.obs.border_radius,
                "dropShadow": self.obs.drop_shadow,
                "showLyrics": self.obs.show_lyrics,
                "hideWhenPaused": self.obs.hide_when_paused,
            })
        }
    }
}

pub fn startup_folder() -> Option<PathBuf> {
    env::var("APPDATA").ok().map(|appdata| {
        PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
    })
}

pub fn cleanup_legacy_startup_files() {
    if let Some(dir) = startup_folder() {
        for name in ["MusicWidget.vbs", "GlassicStartup.vbs", "Glassic.vbs"] {
            let p = dir.join(name);
            if p.exists() {
                let _ = fs::remove_file(p);
            }
        }
    }
}

pub fn set_windows_auto_start(enable: bool) {
    cleanup_legacy_startup_files();
    if let Some(dir) = startup_folder() {
        let lnk_path = dir.join("Glassic.lnk");
        if !enable {
            if lnk_path.exists() {
                let _ = fs::remove_file(&lnk_path);
            }
        } else if let Ok(exe_path) = env::current_exe() {
            create_shortcut(&exe_path, &lnk_path);
        }
    }
}

pub fn create_desktop_shortcut() -> bool {
    let Ok(exe_path) = env::current_exe() else {
        return false;
    };
    let Some(user_profile) = env::var("USERPROFILE").ok() else {
        return false;
    };
    let desktop = PathBuf::from(user_profile).join("Desktop");
    let mut shortcut_path = desktop.join("Glassic.lnk");
    let mut counter = 1;
    while shortcut_path.exists() {
        shortcut_path = desktop.join(format!("Glassic_{counter}.lnk"));
        counter += 1;
    }
    create_shortcut(&exe_path, &shortcut_path)
}

fn create_shortcut(target_exe: &Path, lnk_dest: &Path) -> bool {
    let target_str = target_exe.to_string_lossy().replace('\'', "''");
    let dest_str = lnk_dest.to_string_lossy().replace('\'', "''");
    let work_dir = target_exe
        .parent()
        .map(|p| p.to_string_lossy().replace('\'', "''"))
        .unwrap_or_default();

    let ps_script = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{dest_str}'); \
         $s.TargetPath = '{target_str}'; \
         $s.WorkingDirectory = '{work_dir}'; \
         $s.IconLocation = '{target_str},0'; \
         $s.Description = 'Glassic - Media Widget'; \
         $s.Save()"
    );

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
