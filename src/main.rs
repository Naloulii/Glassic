#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod lyrics;
mod server;
mod smtc;
mod tray;
mod ui;

use config::{cleanup_legacy_startup_files, AppConfig};
use lyrics::LyricsManager;
use server::ObsWebServer;
use smtc::{MediaInfo, SmtcController};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tray::load_egui_window_icon;
use ui::GlassicApp;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

fn get_default_widget_position() -> (f32, f32) {
    let mut work_rect = RECT::default();
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work_rect as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_ok() && work_rect.bottom > 200 {
        (30.0, (work_rect.bottom - 156) as f32)
    } else {
        (30.0, 860.0)
    }
}

fn main() -> eframe::Result<()> {
    cleanup_legacy_startup_files();

    let mut cfg = AppConfig::load();
    let (def_x, def_y) = get_default_widget_position();
    if cfg.window_x.is_none() {
        cfg.window_x = Some(def_x);
    }
    if cfg.window_y.is_none() {
        cfg.window_y = Some(def_y);
    }

    let smtc = Arc::new(SmtcController::new());
    let latest_media: Arc<Mutex<Option<MediaInfo>>> = Arc::new(Mutex::new(None));
    let lyrics_mgr = Arc::new(LyricsManager::new());
    let obs_server = Arc::new(ObsWebServer::new(
        cfg.effective_obs_payload(),
        cfg.web_port,
        cfg.web_server,
    ));

    // Background Windows SMTC Polling Thread
    {
        let smtc_bg = smtc.clone();
        let media_bg = latest_media.clone();
        let obs_bg = obs_server.clone();

        thread::spawn(move || {
            let mut last_broadcast_key = String::new();
            loop {
                let polled = smtc_bg.poll_active_session();
                let current_key = if let Some(ref m) = polled {
                    format!(
                        "{}|{}|{}|{}|{}|{}",
                        m.source, m.title, m.artist, m.status, m.total_sessions, m.session_index
                    )
                } else {
                    "null".to_string()
                };

                if let Ok(mut guard) = media_bg.lock() {
                    *guard = Some(polled.clone().unwrap_or_else(|| MediaInfo {
                        title: String::new(),
                        artist: String::new(),
                        album: String::new(),
                        app_id: String::new(),
                        status: "stopped".to_string(),
                        duration: 0,
                        position: 0,
                        thumbnail: None,
                        source: String::new(),
                        source_meta: smtc::SourceMeta {
                            icon: String::new(),
                            color: String::new(),
                            label: String::new(),
                        },
                        total_sessions: 0,
                        session_index: 0,
                        raw_art_bytes: None,
                        dominant_rgb: [168, 56, 245],
                        title_is_dark: false,
                    }));
                }

                if current_key != last_broadcast_key {
                    last_broadcast_key = current_key;
                }
                obs_bg.update_media(polled);

                thread::sleep(Duration::from_millis(1200));
            }
        });
    }

    let base_size = GlassicApp::base_card_size_with_lyrics(&cfg.layout, false);
    let factor = (cfg.scale as f32) / 100.0;
    let initial_size = egui::vec2(base_size.x * factor, base_size.y * factor);

    let initial_pos = egui::pos2(cfg.window_x.unwrap_or(def_x), cfg.window_y.unwrap_or(def_y));

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Glassic")
        .with_inner_size(initial_size)
        .with_position(initial_pos)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false);

    if cfg.always_on_top {
        viewport = viewport.with_always_on_top();
    }
    if let Some(icon) = load_egui_window_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Glassic",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(GlassicApp::new(
                cc,
                cfg,
                smtc,
                latest_media,
                lyrics_mgr,
                obs_server,
            )))
        }),
    )
}
