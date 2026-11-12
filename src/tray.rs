use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
};

pub const ICON_PNG_BYTES: &[u8] = include_bytes!("../icon.png");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    ToggleVisible,
    ToggleClickThrough,
    OpenSettings,
    PlayPause,
    NextTrack,
    CopyObsUrl,
    Quit,
}

pub struct SystemTrayManager {
    _tray: Option<TrayIcon>,
    id_visible: MenuId,
    id_ghost: MenuId,
    id_settings: MenuId,
    id_playpause: MenuId,
    id_next: MenuId,
    id_obs: MenuId,
    id_quit: MenuId,
}

impl SystemTrayManager {
    pub fn new() -> Self {
        let menu = Menu::new();

        let item_visible = MenuItem::new("👁 Afficher / Masquer le widget", true, None);
        let item_ghost = MenuItem::new("👻 Mode Fantôme (Click-through)", true, None);
        let item_settings = MenuItem::new("⚙ Ouvrir les Paramètres", true, None);
        let item_playpause = MenuItem::new("⏯ Lecture / Pause", true, None);
        let item_next = MenuItem::new("⏭ Piste suivante", true, None);
        let item_obs = MenuItem::new("🔗 Copier le lien OBS", true, None);
        let item_quit = MenuItem::new("❌ Quitter Glassic", true, None);

        let id_visible = item_visible.id().clone();
        let id_ghost = item_ghost.id().clone();
        let id_settings = item_settings.id().clone();
        let id_playpause = item_playpause.id().clone();
        let id_next = item_next.id().clone();
        let id_obs = item_obs.id().clone();
        let id_quit = item_quit.id().clone();

        let _ = menu.append(&item_visible);
        let _ = menu.append(&item_ghost);
        let _ = menu.append(&item_settings);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_playpause);
        let _ = menu.append(&item_next);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_obs);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&item_quit);

        let icon = load_tray_icon();
        let tray = if let Some(ic) = icon {
            TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("Glassic — Widget Musical & OBS")
                .with_icon(ic)
                .build()
                .ok()
        } else {
            None
        };

        Self {
            _tray: tray,
            id_visible,
            id_ghost,
            id_settings,
            id_playpause,
            id_next,
            id_obs,
            id_quit,
        }
    }

    pub fn poll_action(&self) -> Option<TrayAction> {
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.id_visible {
                return Some(TrayAction::ToggleVisible);
            } else if event.id == self.id_ghost {
                return Some(TrayAction::ToggleClickThrough);
            } else if event.id == self.id_settings {
                return Some(TrayAction::OpenSettings);
            } else if event.id == self.id_playpause {
                return Some(TrayAction::PlayPause);
            } else if event.id == self.id_next {
                return Some(TrayAction::NextTrack);
            } else if event.id == self.id_obs {
                return Some(TrayAction::CopyObsUrl);
            } else if event.id == self.id_quit {
                return Some(TrayAction::Quit);
            }
        }
        None
    }
}

fn load_tray_icon() -> Option<Icon> {
    let img = image::load_from_memory(ICON_PNG_BYTES).ok()?;
    let resized = img
        .resize_exact(64, 64, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let (w, h) = resized.dimensions();
    Icon::from_rgba(resized.into_raw(), w, h).ok()
}

pub fn load_egui_window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(ICON_PNG_BYTES).ok()?;
    let resized = img
        .resize_exact(64, 64, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let (width, height) = resized.dimensions();
    Some(egui::IconData {
        rgba: resized.into_raw(),
        width,
        height,
    })
}

pub struct GlobalHotkeyWatcher {
    settings_was_down: bool,
    ghost_was_down: bool,
}

impl GlobalHotkeyWatcher {
    pub fn new() -> Self {
        Self {
            settings_was_down: false,
            ghost_was_down: false,
        }
    }

    pub fn check_pressed(&mut self, settings_key: &str, ghost_key: &str) -> (bool, bool) {
        let mut settings_triggered = false;
        let mut ghost_triggered = false;

        if let Some(vk) = key_name_to_vk(settings_key) {
            let is_down = unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 };
            if is_down && !self.settings_was_down {
                settings_triggered = true;
            }
            self.settings_was_down = is_down;
        } else {
            self.settings_was_down = false;
        }

        if let Some(vk) = key_name_to_vk(ghost_key) {
            let is_down = unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 };
            if is_down && !self.ghost_was_down {
                ghost_triggered = true;
            }
            self.ghost_was_down = is_down;
        } else {
            self.ghost_was_down = false;
        }

        (settings_triggered, ghost_triggered)
    }
}

pub fn key_name_to_vk(name: &str) -> Option<i32> {
    let upper = name.trim().to_uppercase();
    match upper.as_str() {
        "F1" => Some(0x70),
        "F2" => Some(0x71),
        "F3" => Some(0x72),
        "F4" => Some(0x73),
        "F5" => Some(0x74),
        "F6" => Some(0x75),
        "F7" => Some(0x76),
        "F8" => Some(0x77),
        "F9" => Some(0x78),
        "F10" => Some(0x79),
        "F11" => Some(0x7A),
        "F12" => Some(0x7B),
        "INSERT" => Some(0x2D),
        "DELETE" => Some(0x2E),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PAGEUP" => Some(0x21),
        "PAGEDOWN" => Some(0x22),
        "PAUSE" => Some(0x13),
        "SPACE" => Some(0x20),
        s if s.len() == 1 => {
            let b = s.as_bytes()[0];
            if b.is_ascii_uppercase() || b.is_ascii_digit() {
                Some(b as i32)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Checks if the currently active foreground window is a fullscreen application/game
pub fn is_foreground_fullscreen() -> bool {
    unsafe {
        let fg: HWND = GetForegroundWindow();
        if fg.0.is_null() {
            return false;
        }

        // Ignore our own process
        let mut fg_pid: u32 = 0;
        GetWindowThreadProcessId(fg, Some(&mut fg_pid));
        if fg_pid == std::process::id() {
            return false;
        }

        // Ignore Windows desktop/shell classes
        let mut class_buf = [0u16; 128];
        let len = GetClassNameW(fg, &mut class_buf);
        if len > 0 {
            let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
            if matches!(
                class_name.as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            ) {
                return false;
            }
        }

        let mut win_rect = RECT::default();
        if GetWindowRect(fg, &mut win_rect).is_err() {
            return false;
        }

        let monitor = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        if monitor.is_invalid() {
            return false;
        }

        let mut mon_info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut mon_info).as_bool() {
            return false;
        }

        let m = mon_info.rcMonitor;
        win_rect.left <= m.left
            && win_rect.top <= m.top
            && win_rect.right >= m.right
            && win_rect.bottom >= m.bottom
    }
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: Option<String>,
    html_url: Option<String>,
}

pub fn check_github_update(current_version: &str) -> Arc<Mutex<Option<UpdateInfo>>> {
    let result = Arc::new(Mutex::new(None));
    let res_clone = result.clone();
    let current = current_version.trim_start_matches('v').to_string();

    thread::spawn(move || {
        let resp = ureq::get("https://api.github.com/repos/Naloulii/Glassic/releases/latest")
            .set("User-Agent", "GlassicApp/2.0")
            .timeout(Duration::from_secs(6))
            .call();

        if let Ok(r) = resp {
            if let Ok(rel) = r.into_json::<GithubRelease>() {
                if let Some(tag) = rel.tag_name {
                    let latest = tag.trim_start_matches('v').to_string();
                    if !latest.is_empty() && latest != current && is_newer_version(&latest, &current) {
                        if let Ok(mut guard) = res_clone.lock() {
                            *guard = Some(UpdateInfo {
                                version: tag,
                                url: rel.html_url.unwrap_or_else(|| {
                                    "https://github.com/Naloulii/Glassic/releases/latest".to_string()
                                }),
                            });
                        }
                    }
                }
            }
        }
    });

    result
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    let parse_ver = |s: &str| -> Vec<u32> {
        s.split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    };
    let l = parse_ver(latest);
    let c = parse_ver(current);
    l > c
}
