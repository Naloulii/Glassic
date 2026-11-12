use crate::config::{create_desktop_shortcut, set_windows_auto_start, AppConfig};
use crate::lyrics::LyricsManager;
use crate::server::ObsWebServer;
use crate::smtc::{MediaCommand, MediaInfo, SmtcController};
use crate::tray::{
    check_github_update, is_foreground_fullscreen, GlobalHotkeyWatcher, SystemTrayManager,
    TrayAction, UpdateInfo,
};
use egui::{
    Align2, Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Key,
    Mesh, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, Vec2,
    ViewportCommand,
};
use image::{ImageBuffer, Rgba, RgbaImage};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Vibrant purple used for active toggles, slider tracks, and buttons
pub const ACCENT_PURPLE: Color32 = Color32::from_rgb(168, 56, 245); // #A838F5
/// High-contrast luminous purple for text labels, headers, and percentages on dark cards
pub const ACCENT_TEXT_PURPLE: Color32 = Color32::from_rgb(206, 132, 255); // #CE84FF

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Desktop,
    ObsStudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTarget {
    None,
    SettingsKey,
    GhostKey,
}

pub struct GlassicApp {
    pub config: AppConfig,
    pub smtc: Arc<SmtcController>,
    pub latest_media: Arc<Mutex<Option<MediaInfo>>>,
    pub current_media: Option<MediaInfo>,
    pub lyrics_mgr: Arc<LyricsManager>,
    pub obs_server: Arc<ObsWebServer>,
    pub tray: SystemTrayManager,
    pub hotkeys: GlobalHotkeyWatcher,
    pub update_checker: Arc<Mutex<Option<UpdateInfo>>>,
    pub pending_update: Option<UpdateInfo>,

    // UI Modal state
    pub settings_open: bool,
    pub settings_focus_requested: bool,
    pub settings_dwm_applied: bool,
    pub onboard_dwm_applied: bool,
    pub update_dwm_applied: bool,
    pub settings_tab: SettingsTab,
    pub onboard_open: bool,
    pub confirm_reset_open: bool,
    pub capture_target: CaptureTarget,
    pub status_toast: Option<(String, Instant)>,
    pub shortcut_created_toast: bool,

    // Window & visibility state
    pub manually_hidden: bool,
    pub last_applied_size: Vec2,
    pub last_click_through: bool,
    pub paused_since: Option<Instant>,
    pub fullscreen_hidden: bool,
    pub last_fullscreen_check: Instant,
    pub marquee_active: bool,

    // Animation & rendering caches
    pub cached_track_key: String,
    pub art_rounded_tex: Option<TextureHandle>,
    pub art_circle_tex: Option<TextureHandle>,
    pub blurred_bg_tex: Option<TextureHandle>,
    pub dark_glass_bg_tex: Option<TextureHandle>,
    pub dark_normal_bg_tex: Option<TextureHandle>,
    pub border_glass_tex: Option<TextureHandle>,
    pub border_normal_tex: Option<TextureHandle>,
    pub cached_card_tex_size: [u32; 2],
    pub default_art_tex: Option<TextureHandle>,
    pub vinyl_angle: f32,
    pub last_frame_time: Instant,
    pub hover_last_seen: Option<Instant>,
    pub local_pos_secs: f64,
    pub last_pos_sync: Instant,

    // Smooth Lyric Transition State
    pub last_lyric_curr: String,
    pub prev_lyric_curr: Option<String>,
    pub lyric_transition_start: Option<Instant>,

    // Onboarding temp state
    pub onboard_lang: String,
    pub onboard_shortcut: bool,
    pub onboard_startup: bool,
}

impl GlassicApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: AppConfig,
        smtc: Arc<SmtcController>,
        latest_media: Arc<Mutex<Option<MediaInfo>>>,
        lyrics_mgr: Arc<LyricsManager>,
        obs_server: Arc<ObsWebServer>,
    ) -> Self {
        // Load Windows system fonts (Segoe UI + Segoe UI Emoji + Segoe UI Symbol) so all icons/emojis render cleanly
        Self::install_windows_fonts(&cc.egui_ctx);

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::TRANSPARENT;
        visuals.window_fill = Color32::from_rgb(20, 20, 28);
        visuals.override_text_color = Some(Color32::WHITE);
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(28, 28, 36);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(24, 24, 32);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 42, 56);
        visuals.widgets.active.bg_fill = ACCENT_PURPLE;
        visuals.selection.bg_fill = ACCENT_PURPLE;
        cc.egui_ctx.set_visuals(visuals);

        let onboard_open = !config.first_run_completed;
        let onboard_lang = config.lang.clone();

        let mut app = Self {
            last_applied_size: Vec2::ZERO,
            last_click_through: false,
            config,
            smtc,
            latest_media,
            current_media: None,
            lyrics_mgr,
            obs_server,
            tray: SystemTrayManager::new(),
            hotkeys: GlobalHotkeyWatcher::new(),
            update_checker: check_github_update("2.0.0"),
            pending_update: None,
            settings_open: false,
            settings_focus_requested: false,
            settings_dwm_applied: false,
            onboard_dwm_applied: false,
            update_dwm_applied: false,
            settings_tab: SettingsTab::Desktop,
            onboard_open,
            confirm_reset_open: false,
            capture_target: CaptureTarget::None,
            status_toast: None,
            shortcut_created_toast: false,
            manually_hidden: false,
            paused_since: None,
            fullscreen_hidden: false,
            last_fullscreen_check: Instant::now(),
            marquee_active: false,
            cached_track_key: String::new(),
            art_rounded_tex: None,
            art_circle_tex: None,
            blurred_bg_tex: None,
            dark_glass_bg_tex: None,
            dark_normal_bg_tex: None,
            border_glass_tex: None,
            border_normal_tex: None,
            cached_card_tex_size: [0, 0],
            default_art_tex: None,
            vinyl_angle: 0.0,
            last_frame_time: Instant::now(),
            hover_last_seen: None,
            local_pos_secs: 0.0,
            last_pos_sync: Instant::now(),
            last_lyric_curr: String::new(),
            prev_lyric_curr: None,
            lyric_transition_start: None,
            onboard_lang,
            onboard_shortcut: true,
            onboard_startup: true,
        };
        app.last_applied_size = app.current_card_size();
        app
    }

    fn install_windows_fonts(ctx: &Context) {
        let mut fonts = FontDefinitions::default();
        let font_files = [
            ("segoeui", r"C:\Windows\Fonts\segoeui.ttf"),
            ("seguisym", r"C:\Windows\Fonts\seguisym.ttf"),
            ("seguiemj", r"C:\Windows\Fonts\seguiemj.ttf"),
        ];
        for (idx, (name, path)) in font_files.iter().enumerate() {
            if let Ok(bytes) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert((*name).to_string(), Arc::new(FontData::from_owned(bytes)));
                if let Some(fam) = fonts.families.get_mut(&FontFamily::Proportional) {
                    if idx == 0 {
                        fam.insert(0, (*name).to_string());
                    } else {
                        fam.push((*name).to_string());
                    }
                }
                if let Some(fam) = fonts.families.get_mut(&FontFamily::Monospace) {
                    fam.push((*name).to_string());
                }
            }
        }
        ctx.set_fonts(fonts);
    }

    pub fn base_card_size_with_lyrics(layout: &str, show_lyrics: bool) -> Vec2 {
        let extra_h = if show_lyrics { 14.0 } else { 0.0 };
        match layout {
            "compact" => Vec2::new(358.0, 92.0 + extra_h),
            "minimal" => Vec2::new(338.0, 82.0 + extra_h),
            _ => Vec2::new(458.0, 126.0 + extra_h),
        }
    }

    pub fn current_card_size(&self) -> Vec2 {
        let has_active_lyrics = self.config.show_lyrics
            && self
                .lyrics_mgr
                .current_and_next_lines(self.local_pos_secs)
                .is_some();
        let base = Self::base_card_size_with_lyrics(&self.config.layout, has_active_lyrics);
        let factor = (self.config.scale as f32) / 100.0;
        base * factor
    }

    pub fn is_modal_active(&self) -> bool {
        self.settings_open || self.onboard_open || self.pending_update.is_some()
    }

    pub fn sync_all_settings(&mut self) {
        self.config.save();
        self.obs_server
            .set_enabled(self.config.web_server, self.config.web_port);
        self.obs_server
            .update_obs_config(self.config.effective_obs_payload());
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.status_toast = Some((msg.into(), Instant::now()));
    }
}

impl eframe::App for GlassicApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let dt = now
            .duration_since(self.last_frame_time)
            .as_secs_f32()
            .min(0.1);
        self.last_frame_time = now;

        // Check if an update was found on GitHub
        if self.pending_update.is_none() {
            if let Ok(mut guard) = self.update_checker.try_lock() {
                if let Some(info) = guard.take() {
                    self.pending_update = Some(info);
                }
            }
        }

        // Handle System Tray events
        if let Some(action) = self.tray.poll_action() {
            match action {
                TrayAction::ToggleVisible => {
                    self.manually_hidden = !self.manually_hidden;
                }
                TrayAction::ToggleClickThrough => {
                    self.config.click_through = !self.config.click_through;
                    self.config.save();
                    let msg = if self.config.click_through {
                        "👻 Mode Fantôme activé"
                    } else {
                        "👻 Mode Fantôme désactivé"
                    };
                    self.show_toast(msg);
                }
                TrayAction::OpenSettings => {
                    self.manually_hidden = false;
                    self.settings_open = true;
                    self.settings_focus_requested = true;
                }
                TrayAction::PlayPause => {
                    self.smtc.execute_command(MediaCommand::PlayPause);
                }
                TrayAction::NextTrack => {
                    self.smtc.execute_command(MediaCommand::Next);
                }
                TrayAction::CopyObsUrl => {
                    let url = format!("http://localhost:{}/widget.html", self.config.web_port);
                    ctx.copy_text(url);
                }
                TrayAction::Quit => {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }

        // Global Hotkeys (Settings keybind & Ghost keybind)
        if self.capture_target == CaptureTarget::None {
            let (settings_hit, ghost_hit) = self
                .hotkeys
                .check_pressed(&self.config.keybind, &self.config.ghost_keybind);
            if settings_hit {
                self.settings_open = !self.settings_open;
                if self.settings_open {
                    self.manually_hidden = false;
                    self.settings_focus_requested = true;
                }
            }
            if ghost_hit {
                self.config.click_through = !self.config.click_through;
                self.config.save();
                let msg = if self.config.click_through {
                    "👻 Mode Fantôme activé"
                } else {
                    "👻 Mode Fantôme désactivé"
                };
                self.show_toast(msg);
            }
        } else {
            self.capture_keybind_from_egui(ctx);
        }

        // Sync media from background SMTC poller (take() only when a new poll result arrives to avoid cloning strings & base64 on every frame!)
        let polled_opt = self.latest_media.try_lock().ok().and_then(|mut g| g.take());
        if let Some(m) = polled_opt {
            if !m.title.is_empty() {
                let is_new_track = {
                    let key = format!("{}|{}|{}", m.title, m.artist, m.album);
                    key != self.cached_track_key
                };
                let prev_pos = self.current_media.as_ref().map(|c| c.position).unwrap_or(-1);
                if is_new_track || (m.position - prev_pos).abs() >= 2 {
                    self.local_pos_secs = m.position as f64;
                    self.last_pos_sync = now;
                }
                if is_new_track {
                    self.update_cover_textures(ctx, &m);
                    if self.config.show_lyrics || self.config.obs.show_lyrics {
                        self.lyrics_mgr.request_for_track(&m.title, &m.artist);
                    }
                }
                self.current_media = Some(m);
            } else {
                self.current_media = None;
                self.cached_track_key.clear();
                self.art_rounded_tex = None;
                self.art_circle_tex = None;
                self.blurred_bg_tex = None;
            }
        }

        // Advance smooth local playback timer when playing
        let is_playing = self
            .current_media
            .as_ref()
            .map(|m| m.status == "playing")
            .unwrap_or(false);
        if is_playing {
            self.local_pos_secs += dt as f64;
            if let Some(ref m) = self.current_media {
                if m.duration > 0 && self.local_pos_secs > m.duration as f64 {
                    self.local_pos_secs = m.duration as f64;
                }
            }
            self.paused_since = None;
        } else if self.paused_since.is_none() {
            self.paused_since = Some(now);
        }

        // Update lyrics broadcast for OBS (current + next line)
        if self.config.show_lyrics || self.config.obs.show_lyrics {
            let lines = self
                .lyrics_mgr
                .current_and_next_lines(self.local_pos_secs);
            self.obs_server.update_lyric(lines);
        }

        // Check fullscreen auto-hide (throttled to every 250ms to avoid Win32 syscall overhead on every frame)
        if self.config.hide_on_fullscreen && !self.is_modal_active() {
            if now.duration_since(self.last_fullscreen_check) >= Duration::from_millis(250) {
                self.last_fullscreen_check = now;
                self.fullscreen_hidden = is_foreground_fullscreen();
            }
        } else {
            self.fullscreen_hidden = false;
        }

        // Manage widget viewport size, position, always-on-top, and click-through
        self.manage_viewport(ctx);

        let widget_hidden = self.should_hide_widget(now);

        // Render main desktop widget card in the primary viewport
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                if !widget_hidden {
                    self.draw_widget_card(ctx, ui, dt, is_playing);
                } else {
                    self.marquee_active = false;
                }
            });

        // Render independent movable OS windows for Settings, Onboarding, and Updates!
        if self.settings_open {
            self.show_settings_viewport(ctx);
        } else {
            self.settings_dwm_applied = false;
        }
        if self.onboard_open {
            self.show_onboarding_viewport(ctx);
        } else {
            self.onboard_dwm_applied = false;
        }
        if let Some(info) = self.pending_update.clone() {
            self.show_update_viewport(ctx, &info);
        } else {
            self.update_dwm_applied = false;
        }

        // Smart adaptive frame rate:
        // - 60 FPS (16ms) when playing, scrolling marquee text, animating lyric transition, hovered, or modal open
        // - 10 FPS (100ms) when paused & static (0% CPU/GPU while remaining instant on mouse hover)
        // - 4 FPS (250ms) when hidden
        let lyric_animating = self
            .lyric_transition_start
            .map(|t| t.elapsed() < Duration::from_millis(450))
            .unwrap_or(false);
        let pointer_near = self
            .hover_last_seen
            .map(|t| t.elapsed() < Duration::from_secs(5))
            .unwrap_or(false);

        if widget_hidden && !self.is_modal_active() {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else if is_playing
            || self.marquee_active
            || lyric_animating
            || pointer_near
            || self.is_modal_active()
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

impl GlassicApp {
    fn should_hide_widget(&self, now: Instant) -> bool {
        if self.manually_hidden || self.fullscreen_hidden {
            return true;
        }
        if self.config.auto_hide_pause_secs > 0 {
            let is_playing = self
                .current_media
                .as_ref()
                .map(|m| m.status == "playing")
                .unwrap_or(false);
            if !is_playing {
                if self.config.auto_hide_pause_secs == 1 {
                    return true;
                }
                if let Some(since) = self.paused_since {
                    if now.duration_since(since).as_secs() >= self.config.auto_hide_pause_secs as u64
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn manage_viewport(&mut self, ctx: &Context) {
        // Click-through (Ghost mode) on the desktop widget
        let effective_click_through = self.config.click_through;
        if effective_click_through != self.last_click_through {
            self.last_click_through = effective_click_through;
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(effective_click_through));
        }

        // Update widget window size if scale, layout, or lyrics visibility changed
        let desired_size = self.current_card_size();
        if (desired_size - self.last_applied_size).length() > 0.5 {
            self.last_applied_size = desired_size;
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(desired_size));
        }

        // Remember dragged widget window position
        if let Some(outer) = ctx.input(|i| i.viewport().outer_rect) {
            let x = outer.min.x;
            let y = outer.min.y;
            if (self.config.window_x.unwrap_or(-999.0) - x).abs() > 2.0
                || (self.config.window_y.unwrap_or(-999.0) - y).abs() > 2.0
            {
                self.config.window_x = Some(x);
                self.config.window_y = Some(y);
            }
        }
    }

    fn capture_keybind_from_egui(&mut self, ctx: &Context) {
        let keys = [
            (Key::F1, "F1"),
            (Key::F2, "F2"),
            (Key::F3, "F3"),
            (Key::F4, "F4"),
            (Key::F5, "F5"),
            (Key::F6, "F6"),
            (Key::F7, "F7"),
            (Key::F8, "F8"),
            (Key::F9, "F9"),
            (Key::F10, "F10"),
            (Key::F11, "F11"),
            (Key::F12, "F12"),
            (Key::Insert, "Insert"),
            (Key::Delete, "Delete"),
            (Key::Home, "Home"),
            (Key::End, "End"),
            (Key::PageUp, "PageUp"),
            (Key::PageDown, "PageDown"),
        ];
        ctx.input(|i| {
            for (k, name) in keys {
                if i.key_pressed(k) {
                    match self.capture_target {
                        CaptureTarget::SettingsKey => self.config.keybind = name.to_string(),
                        CaptureTarget::GhostKey => self.config.ghost_keybind = name.to_string(),
                        CaptureTarget::None => {}
                    }
                    self.capture_target = CaptureTarget::None;
                    self.config.save();
                    return;
                }
            }
        });
    }

    fn update_cover_textures(&mut self, ctx: &Context, media: &MediaInfo) {
        let key = format!("{}|{}|{}", media.title, media.artist, media.album);
        self.cached_track_key = key;

        let Some(ref raw_bytes) = media.raw_art_bytes else {
            self.art_rounded_tex = None;
            self.art_circle_tex = None;
            self.blurred_bg_tex = None;
            return;
        };

        let Ok(img) = image::load_from_memory(raw_bytes) else {
            return;
        };

        // 1. Rounded square art (136x136 with 20px SDF rounded corners)
        let sq = img
            .resize_to_fill(136, 136, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        let mut sq_rounded = sq.clone();
        apply_sdf_rounded_mask(&mut sq_rounded, 20.0);
        let sq_color = egui::ColorImage::from_rgba_unmultiplied(
            [136, 136],
            sq_rounded.as_flat_samples().as_slice(),
        );
        self.art_rounded_tex =
            Some(ctx.load_texture("art_rounded", sq_color, TextureOptions::LINEAR));

        // 2. Circular art for Vinyl & Circle modes (136x136 with 68px radius circle SDF mask)
        let mut cir = sq;
        apply_sdf_rounded_mask(&mut cir, 68.0);
        let cir_color = egui::ColorImage::from_rgba_unmultiplied(
            [136, 136],
            cir.as_flat_samples().as_slice(),
        );
        self.art_circle_tex =
            Some(ctx.load_texture("art_circle", cir_color, TextureOptions::LINEAR));

        // 3. Blurred Cover Background matching HTML `filter: blur(42px) saturate(200%)`
        let card_sz = self.current_card_size();
        let bw = (card_sz.x.round() as u32).max(64);
        let bh = (card_sz.y.round() as u32).max(48);
        let br = (20.0 * (self.config.scale as f32 / 100.0)).round();
        let blurred_rgba = build_blurred_card_background(&img, bw, bh, br);
        let bg_color = egui::ColorImage::from_rgba_unmultiplied(
            [bw as usize, bh as usize],
            blurred_rgba.as_flat_samples().as_slice(),
        );
        self.blurred_bg_tex =
            Some(ctx.load_texture("art_blurred_bg", bg_color, TextureOptions::LINEAR));
    }

    fn ensure_default_art(&mut self, ctx: &Context) -> TextureHandle {
        if let Some(ref tex) = self.default_art_tex {
            return tex.clone();
        }
        let mut img: RgbaImage = ImageBuffer::new(136, 136);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let dx = x as f32 - 68.0;
            let dy = y as f32 - 68.0;
            let r = (dx * dx + dy * dy).sqrt();
            let ring = ((r * 0.35).sin() * 12.0) as i32;
            let v = (42 + ring).clamp(24, 65) as u8;
            *p = Rgba([v, v, v + 8, 255]);
        }
        apply_sdf_rounded_mask(&mut img, 68.0);
        let color_img = egui::ColorImage::from_rgba_unmultiplied(
            [136, 136],
            img.as_flat_samples().as_slice(),
        );
        let handle = ctx.load_texture("default_art", color_img, TextureOptions::LINEAR);
        self.default_art_tex = Some(handle.clone());
        handle
    }

    fn ensure_card_shape_textures(&mut self, ctx: &Context, w: u32, h: u32, radius: f32) {
        if self.cached_card_tex_size == [w, h] && self.border_glass_tex.is_some() {
            return;
        }
        self.cached_card_tex_size = [w, h];

        // Regenerate blurred cover background at the exact new card size & radius so corners never stretch!
        let maybe_raw = self
            .current_media
            .as_ref()
            .and_then(|m| m.raw_art_bytes.clone());
        if let Some(raw_bytes) = maybe_raw {
            if let Ok(img) = image::load_from_memory(&raw_bytes) {
                let blurred_rgba = build_blurred_card_background(&img, w, h, radius);
                let bg_color = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    blurred_rgba.as_flat_samples().as_slice(),
                );
                self.blurred_bg_tex =
                    Some(ctx.load_texture("art_blurred_bg", bg_color, TextureOptions::LINEAR));
            }
        }

        // 1. Dark glassmorphic & normal fallback card textures (SDF rounded mask)
        let mut glass_bg = ImageBuffer::from_pixel(w, h, Rgba([34, 34, 40, 195]));
        apply_sdf_rounded_mask(&mut glass_bg, radius);
        self.dark_glass_bg_tex = Some(ctx.load_texture(
            "card_dark_glass",
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], glass_bg.as_flat_samples().as_slice()),
            TextureOptions::LINEAR,
        ));

        let mut norm_bg = ImageBuffer::from_pixel(w, h, Rgba([14, 14, 18, 220]));
        apply_sdf_rounded_mask(&mut norm_bg, radius);
        self.dark_normal_bg_tex = Some(ctx.load_texture(
            "card_dark_norm",
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], norm_bg.as_flat_samples().as_slice()),
            TextureOptions::LINEAR,
        ));

        // 2. Crisp SDF Rounded Border Ring Textures (follows the exact SDF curve of the blurred background!)
        // Outer radius matches card radius; thickness 1.0px inside the card edge
        let b_glass = build_sdf_border_image(w, h, radius, 1.0, 52);
        self.border_glass_tex = Some(ctx.load_texture(
            "card_border_glass",
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], b_glass.as_flat_samples().as_slice()),
            TextureOptions::LINEAR,
        ));

        let b_norm = build_sdf_border_image(w, h, radius, 1.0, 32);
        self.border_normal_tex = Some(ctx.load_texture(
            "card_border_norm",
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], b_norm.as_flat_samples().as_slice()),
            TextureOptions::LINEAR,
        ));
    }

    fn draw_widget_card(&mut self, ctx: &Context, ui: &mut Ui, dt: f32, is_playing: bool) {
        let scale = (self.config.scale as f32) / 100.0;
        let opacity = (self.config.opacity as f32) / 100.0;
        let card_size = self.current_card_size();
        let rect = Rect::from_min_size(Pos2::ZERO, card_size);
        let radius_px = (20.0 * scale).round();

        let tex_w = (card_size.x.round() as u32).max(64);
        let tex_h = (card_size.y.round() as u32).max(48);
        self.ensure_card_shape_textures(ctx, tex_w, tex_h, radius_px);

        let pointer_pos = ctx.input(|i| i.pointer.hover_pos().or(i.pointer.interact_pos()));
        let is_hovered = pointer_pos.map(|p| rect.contains(p)).unwrap_or(false);
        if is_hovered {
            self.hover_last_seen = Some(Instant::now());
        }
        let show_gear = self
            .hover_last_seen
            .map(|t| t.elapsed() < Duration::from_secs(5))
            .unwrap_or(false);

        // Track if cursor is over any interactive button so window dragging never steals clicks!
        let mut pointer_over_interactive = false;

        let painter = ui.painter_at(rect);
        let uv_full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        let full_tint =
            Color32::from_rgba_unmultiplied(255, 255, 255, (255.0 * opacity).round() as u8);

        // 1. Background (Colorful blurred cover OR dark glassmorphic card — both use exact SDF rounded mask)
        let colorful_active = self.config.color && self.blurred_bg_tex.is_some();
        if colorful_active {
            if let Some(ref bg_tex) = self.blurred_bg_tex {
                let tint = Color32::from_rgba_unmultiplied(
                    255,
                    255,
                    255,
                    (230.0 * opacity).round() as u8,
                );
                painter.image(bg_tex.id(), rect, uv_full, tint);
            }
        } else {
            let fallback_tex = if self.config.glassy {
                self.dark_glass_bg_tex.as_ref()
            } else {
                self.dark_normal_bg_tex.as_ref()
            };
            if let Some(tex) = fallback_tex {
                painter.image(tex.id(), rect, uv_full, full_tint);
            }
        }

        // Exact SDF Rounded Border Ring (matches the card's curve pixel-for-pixel without any gap or corner mismatch!)
        let border_tex = if self.config.glassy {
            self.border_glass_tex.as_ref()
        } else {
            self.border_normal_tex.as_ref()
        };
        if let Some(b_tex) = border_tex {
            painter.image(b_tex.id(), rect, uv_full, full_tint);
        }

        // Determine active colors
        let title_dark = colorful_active
            && !self.config.source_color
            && self
                .current_media
                .as_ref()
                .map(|m| m.title_is_dark)
                .unwrap_or(false);

        let title_col = if title_dark {
            Color32::from_rgba_unmultiplied(18, 18, 22, (255.0 * opacity) as u8)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, (255.0 * opacity) as u8)
        };

        let artist_col = if title_dark {
            Color32::from_rgba_unmultiplied(35, 35, 42, (190.0 * opacity) as u8)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, (165.0 * opacity) as u8)
        };

        let time_col = if title_dark {
            Color32::from_rgba_unmultiplied(40, 40, 48, (150.0 * opacity) as u8)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, (115.0 * opacity) as u8)
        };

        let accent_col = if self.config.source_color {
            with_opacity(ACCENT_PURPLE, opacity)
        } else if colorful_active {
            title_col
        } else if let Some(ref m) = self.current_media {
            with_opacity(parse_hex_color(&m.source_meta.color), opacity)
        } else {
            with_opacity(ACCENT_PURPLE, opacity)
        };

        // 2. Album Art / Rotating Vinyl (unless minimal layout)
        let pad_left = 18.0 * scale;
        let mut info_left = pad_left;

        if self.config.layout != "minimal" {
            let art_size = if self.config.layout == "compact" {
                54.0 * scale
            } else {
                68.0 * scale
            };
            let art_center = Pos2::new(pad_left + art_size * 0.5, card_size.y * 0.5);
            let art_rect = Rect::from_center_size(art_center, Vec2::splat(art_size));

            let is_vinyl = self.config.vinyl && self.config.art_style == "vinyl";
            if is_vinyl && is_playing {
                self.vinyl_angle =
                    (self.vinyl_angle + dt * std::f32::consts::TAU / 8.0) % std::f32::consts::TAU;
            }

            let tex_handle = if self.config.art_style == "rounded" && !self.config.vinyl {
                self.art_rounded_tex
                    .clone()
                    .unwrap_or_else(|| self.ensure_default_art(ctx))
            } else {
                self.art_circle_tex
                    .clone()
                    .unwrap_or_else(|| self.ensure_default_art(ctx))
            };

            let tint =
                Color32::from_rgba_unmultiplied(255, 255, 255, (255.0 * opacity).round() as u8);

            if is_vinyl {
                draw_rotated_textured_quad(
                    &painter,
                    tex_handle.id(),
                    art_center,
                    art_size * 0.5,
                    self.vinyl_angle,
                    tint,
                );
                // Vinyl center spindle ring & hole
                painter.circle_filled(
                    art_center,
                    8.5 * scale,
                    Color32::from_rgba_unmultiplied(17, 17, 17, (255.0 * opacity) as u8),
                );
                painter.circle_stroke(
                    art_center,
                    8.5 * scale,
                    Stroke::new(
                        1.5 * scale,
                        Color32::from_rgba_unmultiplied(255, 255, 255, (65.0 * opacity) as u8),
                    ),
                );
                painter.circle_filled(
                    art_center,
                    3.2 * scale,
                    Color32::from_rgba_unmultiplied(0, 0, 0, (255.0 * opacity) as u8),
                );
            } else {
                painter.image(
                    tex_handle.id(),
                    art_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    tint,
                );
            }

            info_left = art_rect.max.x + 16.0 * scale;
        }

        // 3. Equalizer Bars on the right (shifted left to leave a clean margin around the session swap buttons!)
        let eq_right = card_size.x - 36.0 * scale;
        let eq_width = 18.0 * scale;
        let eq_left = eq_right - eq_width;
        let info_right = eq_left - 14.0 * scale;

        if self.current_media.is_some() {
            let t = ctx.input(|i| i.time) as f32;
            let bar_w = (3.0 * scale).max(2.0);
            let gap = 2.5 * scale;
            let max_h = 14.0 * scale;
            let base_y = card_size.y * 0.5 + max_h * 0.5;
            let phases = [0.0, 1.1, 2.3, 0.6];

            for (idx, phase) in phases.iter().enumerate() {
                let ratio = if is_playing {
                    0.3 + 0.7 * ((t * 4.2 + phase).sin() * 0.5 + 0.5)
                } else {
                    0.3
                };
                let bh = (max_h * ratio).max(3.0 * scale);
                let bx = eq_left + idx as f32 * (bar_w + gap);
                let bar_rect =
                    Rect::from_min_size(Pos2::new(bx, base_y - bh), Vec2::new(bar_w, bh));
                painter.rect_filled(bar_rect, CornerRadius::same(2), accent_col);
            }
        }

        // 4. Info Column (Source, Title, Artist, Controls, Progress Bar, and Spotify Lyrics below!)
        let show_controls = match self.config.controls_mode.as_str() {
            "always" => true,
            "hover" => is_hovered,
            _ => false,
        };
        let active_lyrics = if self.config.show_lyrics {
            self.lyrics_mgr.current_and_next_lines(self.local_pos_secs)
        } else {
            None
        };

        // Measure exact vertical content height to center the info column perfectly (eliminates any bottom gap!)
        let progress_block_h = if self.config.layout != "compact" {
            16.0 * scale
        } else {
            0.0
        };
        let controls_block_h = if show_controls { 25.0 * scale } else { 0.0 };
        let lyrics_block_h = if active_lyrics.is_some() {
            28.0 * scale
        } else {
            0.0
        };
        let total_info_h =
            (14.5 + 18.0 + 16.0) * scale + controls_block_h + progress_block_h + lyrics_block_h;
        let mut cursor_y = ((card_size.y - total_info_h) * 0.5).max(8.0 * scale);

        // Source dot & label
        let source_label = self
            .current_media
            .as_ref()
            .map(|m| m.source_meta.label.to_uppercase())
            .unwrap_or_else(|| "GLASSIC".to_string());

        let dot_center = Pos2::new(info_left + 3.5 * scale, cursor_y + 6.0 * scale);
        painter.circle_filled(dot_center, 3.5 * scale, accent_col);

        painter.text(
            Pos2::new(info_left + 12.0 * scale, cursor_y),
            Align2::LEFT_TOP,
            &source_label,
            FontId::new(10.0 * scale, FontFamily::Proportional),
            accent_col,
        );
        cursor_y += 14.5 * scale;

        // Title & Artist
        let title_text = self
            .current_media
            .as_ref()
            .map(|m| m.title.as_str())
            .unwrap_or(if self.config.lang == "fr" {
                "Aucune lecture en cours"
            } else {
                "Nothing playing"
            });
        let artist_text = self
            .current_media
            .as_ref()
            .map(|m| {
                if m.album.is_empty() || m.album == m.title {
                    m.artist.clone()
                } else if m.artist.is_empty() {
                    m.album.clone()
                } else {
                    format!("{} · {}", m.artist, m.album)
                }
            })
            .unwrap_or_else(|| "–".to_string());

        let max_text_w = (info_right - info_left).max(40.0);
        let title_scrolling = draw_clipped_or_scrolling_text(
            ctx,
            &painter,
            Pos2::new(info_left, cursor_y),
            max_text_w,
            19.0 * scale,
            title_text,
            FontId::new(14.0 * scale, FontFamily::Proportional),
            title_col,
        );
        cursor_y += 18.0 * scale;

        let artist_scrolling = draw_clipped_or_scrolling_text(
            ctx,
            &painter,
            Pos2::new(info_left, cursor_y),
            max_text_w,
            16.0 * scale,
            &artist_text,
            FontId::new(12.0 * scale, FontFamily::Proportional),
            artist_col,
        );
        self.marquee_active = title_scrolling || artist_scrolling;
        cursor_y += 16.0 * scale;

        // Interactive Playback Controls Bar
        if show_controls {
            let ctrl_w = (150.0 * scale).min(max_text_w);
            let ctrl_h = 22.0 * scale;
            let ctrl_rect = Rect::from_min_size(
                Pos2::new(info_left, cursor_y + 1.0 * scale),
                Vec2::new(ctrl_w, ctrl_h),
            );
            if pointer_pos.map(|p| ctrl_rect.contains(p)).unwrap_or(false) {
                pointer_over_interactive = true;
            }
            painter.rect_filled(
                ctrl_rect,
                CornerRadius::same(99),
                Color32::from_rgba_unmultiplied(0, 0, 0, (135.0 * opacity) as u8),
            );

            let btns = [
                ("⏮", MediaCommand::Previous),
                (if is_playing { "⏸" } else { "▶" }, MediaCommand::PlayPause),
                ("⏭", MediaCommand::Next),
                ("🔉", MediaCommand::VolumeDown),
                ("🔊", MediaCommand::VolumeUp),
            ];
            let step = ctrl_w / (btns.len() as f32);
            for (i, (label, cmd)) in btns.into_iter().enumerate() {
                let r = Rect::from_min_size(
                    Pos2::new(ctrl_rect.min.x + i as f32 * step, ctrl_rect.min.y),
                    Vec2::new(step, ctrl_h),
                );
                let resp = ui.interact(r, ui.id().with(("ctrl_btn", i)), Sense::click());
                if resp.hovered() || resp.is_pointer_button_down_on() {
                    pointer_over_interactive = true;
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let col = if resp.hovered() {
                    ACCENT_TEXT_PURPLE
                } else {
                    Color32::from_rgba_unmultiplied(255, 255, 255, (230.0 * opacity) as u8)
                };
                painter.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::new(11.5 * scale, FontFamily::Proportional),
                    col,
                );
                if resp.clicked() {
                    self.smtc.execute_command(cmd);
                }
            }
            cursor_y += ctrl_h + 4.0 * scale;
        }

        // 5. Progress Bar (with interactive Seek on click!)
        if self.config.layout != "compact" {
            let dur = self
                .current_media
                .as_ref()
                .map(|m| m.duration.max(0))
                .unwrap_or(0);
            let pos = (self.local_pos_secs.round() as i64).clamp(0, dur.max(0));
            let pos_str = format_mm_ss(pos);
            let dur_str = if dur > 0 {
                format_mm_ss(dur)
            } else {
                "0:00".to_string()
            };

            let prog_y = cursor_y + 6.5 * scale;
            painter.text(
                Pos2::new(info_left, prog_y),
                Align2::LEFT_CENTER,
                &pos_str,
                FontId::new(10.0 * scale, FontFamily::Proportional),
                time_col,
            );
            painter.text(
                Pos2::new(info_right, prog_y),
                Align2::RIGHT_CENTER,
                &dur_str,
                FontId::new(10.0 * scale, FontFamily::Proportional),
                time_col,
            );

            let bar_left = info_left + 28.0 * scale;
            let bar_right = info_right - 30.0 * scale;
            if bar_right > bar_left + 20.0 {
                let bar_h = (3.5 * scale).max(3.0);
                let bar_rect = Rect::from_min_max(
                    Pos2::new(bar_left, prog_y - bar_h * 0.5),
                    Pos2::new(bar_right, prog_y + bar_h * 0.5),
                );
                let hit_rect = bar_rect.expand2(Vec2::new(0.0, 5.5 * scale));
                if pointer_pos.map(|p| hit_rect.contains(p)).unwrap_or(false) {
                    pointer_over_interactive = true;
                }
                let seek_resp = ui.interact(hit_rect, ui.id().with("seek_bar"), Sense::click());

                painter.rect_filled(
                    bar_rect,
                    CornerRadius::same(99),
                    Color32::from_rgba_unmultiplied(255, 255, 255, (40.0 * opacity) as u8),
                );

                let pct = if dur > 0 {
                    (self.local_pos_secs as f32 / dur as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                if pct > 0.0 {
                    let fill_w = (bar_rect.width() * pct).max(bar_h);
                    let fill_rect =
                        Rect::from_min_size(bar_rect.min, Vec2::new(fill_w, bar_rect.height()));
                    painter.rect_filled(fill_rect, CornerRadius::same(99), accent_col);
                }

                if (seek_resp.hovered() || seek_resp.is_pointer_button_down_on()) && dur > 0 {
                    pointer_over_interactive = true;
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if seek_resp.clicked() && dur > 0 {
                    if let Some(click_pos) = seek_resp.interact_pointer_pos() {
                        let ratio =
                            ((click_pos.x - bar_rect.min.x) / bar_rect.width()).clamp(0.0, 1.0);
                        let target_secs = (ratio * dur as f32).round() as i64;
                        self.local_pos_secs = target_secs as f64;
                        self.smtc
                            .execute_command(MediaCommand::SeekSeconds(target_secs));
                    }
                }
            }
            cursor_y = prog_y + 8.5 * scale;
        }

        // 6. Spotify-Style Synchronized Lyrics BELOW the Progress Bar with Smooth Natural Roll-Up Transition
        // - Current line is clearly visible (bright).
        // - If current line is too long for 1 line, it wraps onto 2 lines.
        // - If current line fits on 1 line and there is a next line, the next line is shown below it with lower visibility.
        // - When the lyric line advances, the previous line glides upward & fades out while the new line glides up from row 2 into row 1!
        if let Some((curr_line, next_line_opt)) = active_lyrics {
            if curr_line != self.last_lyric_curr {
                if !self.last_lyric_curr.is_empty() {
                    self.prev_lyric_curr = Some(self.last_lyric_curr.clone());
                    self.lyric_transition_start = Some(Instant::now());
                }
                self.last_lyric_curr = curr_line.clone();
            }

            let anim_dur = 0.38_f32;
            let raw_t = self
                .lyric_transition_start
                .map(|t| (t.elapsed().as_secs_f32() / anim_dur).clamp(0.0, 1.0))
                .unwrap_or(1.0);
            // Smooth cubic ease-out: 1 - (1 - t)^3
            let ease = 1.0 - (1.0 - raw_t).powi(3);
            let row_step = 13.5 * scale;
            let slide_y = (1.0 - ease) * row_step;

            let lyric_font_cur = FontId::new(11.0 * scale, FontFamily::Proportional);
            let lyric_font_nxt = FontId::new(10.5 * scale, FontFamily::Proportional);

            let clip_rect = Rect::from_min_size(
                Pos2::new(info_left, cursor_y - 2.0 * scale),
                Vec2::new(max_text_w, 31.0 * scale),
            );
            let clipped = painter.with_clip_rect(clip_rect);

            // 1) Outgoing previous line sliding upward and fading out
            if raw_t < 1.0 {
                if let Some(ref prev_str) = self.prev_lyric_curr {
                    let out_alpha = (1.0 - ease).powi(2);
                    let out_col = with_opacity(title_col, out_alpha);
                    let out_y = cursor_y - ease * row_step;
                    let prev_galley = painter.layout_no_wrap(
                        prev_str.clone(),
                        lyric_font_cur.clone(),
                        out_col,
                    );
                    clipped.galley(Pos2::new(info_left, out_y), prev_galley, out_col);
                }
            }

            // 2) Active current line (interpolating brightness & vertical position from row 2 -> row 1)
            let cur_alpha_factor = 0.46 + 0.54 * ease;
            let cur_col = with_opacity(title_col, cur_alpha_factor);
            let cur_y = cursor_y + slide_y;

            let single_galley =
                painter.layout_no_wrap(curr_line.clone(), lyric_font_cur.clone(), cur_col);

            if single_galley.size().x <= max_text_w {
                // Fits on 1 line: draw active line on row 1, and upcoming line dimmed on row 2
                clipped.galley(Pos2::new(info_left, cur_y), single_galley, cur_col);
                if let Some(next_str) = next_line_opt {
                    if !next_str.is_empty() {
                        // Next line gently rises and fades in
                        let nxt_alpha = 0.46 * ease;
                        let nxt_col = with_opacity(artist_col, nxt_alpha);
                        let nxt_y = cursor_y + row_step + (1.0 - ease) * (row_step * 0.65);
                        let nxt_galley =
                            painter.layout_no_wrap(next_str, lyric_font_nxt, nxt_col);
                        clipped.galley(Pos2::new(info_left, nxt_y), nxt_galley, nxt_col);
                    }
                }
            } else {
                // Current lyric is long: wrap onto 2 lines (Spotify style)
                let mut job = egui::text::LayoutJob::simple(
                    curr_line,
                    lyric_font_cur,
                    cur_col,
                    max_text_w,
                );
                job.wrap.max_rows = 2;
                job.wrap.break_anywhere = false;
                let wrapped_galley = ctx.fonts(|f| f.layout_job(job));
                clipped.galley(Pos2::new(info_left, cur_y), wrapped_galley, cur_col);
            }
        } else {
            self.last_lyric_curr.clear();
            self.prev_lyric_curr = None;
        }

        // 7. Settings Gear Button (top-right on hover)
        if show_gear {
            let gear_size = 24.0 * scale;
            let gear_rect = Rect::from_min_size(
                Pos2::new(card_size.x - 12.0 * scale - gear_size, 12.0 * scale),
                Vec2::splat(gear_size),
            );
            let gear_hit = gear_rect.expand(3.0 * scale);
            if pointer_pos.map(|p| gear_hit.contains(p)).unwrap_or(false) {
                pointer_over_interactive = true;
            }
            let gear_resp = ui.interact(gear_hit, ui.id().with("btn_gear"), Sense::click());
            let gear_bg = if gear_resp.hovered() {
                ACCENT_PURPLE
            } else {
                Color32::from_rgba_unmultiplied(0, 0, 0, 145)
            };
            painter.circle_filled(gear_rect.center(), gear_size * 0.5, gear_bg);
            painter.circle_stroke(
                gear_rect.center(),
                gear_size * 0.5,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 65)),
            );
            painter.text(
                gear_rect.center(),
                Align2::CENTER_CENTER,
                "⚙",
                FontId::new(12.5 * scale, FontFamily::Proportional),
                Color32::WHITE,
            );
            if gear_resp.hovered() || gear_resp.is_pointer_button_down_on() {
                pointer_over_interactive = true;
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if gear_resp.clicked() {
                self.settings_open = true;
                self.settings_focus_requested = true;
            }
        }

        // 8. Session selector arrows (▲ / ▼) when multiple media sessions are active
        let total_sessions = self
            .current_media
            .as_ref()
            .map(|m| m.total_sessions)
            .unwrap_or(0);
        if is_hovered && total_sessions > 1 {
            let btn_sz = 18.0 * scale;
            let arrow_x = card_size.x - 18.0 * scale;
            let up_rect = Rect::from_center_size(
                Pos2::new(arrow_x, card_size.y * 0.5 - 12.0 * scale),
                Vec2::splat(btn_sz),
            );
            let dn_rect = Rect::from_center_size(
                Pos2::new(arrow_x, card_size.y * 0.5 + 12.0 * scale),
                Vec2::splat(btn_sz),
            );
            for (r, sym, delta, id_str) in [
                (up_rect, "▲", -1, "sess_up"),
                (dn_rect, "▼", 1, "sess_dn"),
            ] {
                if pointer_pos.map(|p| r.contains(p)).unwrap_or(false) {
                    pointer_over_interactive = true;
                }
                let resp = ui.interact(r, ui.id().with(id_str), Sense::click());
                let bg = if resp.hovered() {
                    ACCENT_PURPLE
                } else {
                    Color32::from_rgba_unmultiplied(0, 0, 0, 145)
                };
                painter.circle_filled(r.center(), btn_sz * 0.5, bg);
                painter.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    sym,
                    FontId::new(9.0 * scale, FontFamily::Proportional),
                    Color32::WHITE,
                );
                if resp.hovered() || resp.is_pointer_button_down_on() {
                    pointer_over_interactive = true;
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    self.smtc.execute_command(MediaCommand::CycleSession(delta));
                }
            }
        }

        // 9. Window Dragging ONLY when pointer is NOT pressing an interactive button
        if !pointer_over_interactive && is_hovered {
            let start_drag = ctx.input(|i| {
                i.pointer.primary_down() && i.pointer.delta().length_sq() > 1.5
            });
            if start_drag {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
        }
    }

    fn show_settings_viewport(&mut self, ctx: &Context) {
        let is_fr = self.config.lang == "fr";
        let title = if is_fr {
            "Paramètres Glassic"
        } else {
            "Glassic Settings"
        };
        let vp_id = egui::ViewportId::from_hash_of("glassic_settings_viewport");
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([840.0, 600.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_window_level(egui::WindowLevel::Normal);

        ctx.show_viewport_immediate(vp_id, builder, |vp_ctx, _class| {
            if !self.settings_dwm_applied {
                self.settings_dwm_applied = apply_win11_native_rounded_window(title);
            }
            if self.settings_focus_requested {
                self.settings_focus_requested = false;
                vp_ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
            if vp_ctx.input(|i| i.viewport().close_requested()) {
                self.settings_open = false;
                self.settings_dwm_applied = false;
                self.capture_target = CaptureTarget::None;
                self.sync_all_settings();
            }
            if self.capture_target != CaptureTarget::None {
                self.capture_keybind_from_egui(vp_ctx);
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(Color32::TRANSPARENT))
                .show(vp_ctx, |ui| {
                    self.draw_settings_modal(vp_ctx, ui);
                });
        });
    }

    fn show_onboarding_viewport(&mut self, ctx: &Context) {
        let title = "Bienvenue sur Glassic";
        let vp_id = egui::ViewportId::from_hash_of("glassic_onboarding_viewport");
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([580.0, 360.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_window_level(egui::WindowLevel::Normal);

        ctx.show_viewport_immediate(vp_id, builder, |vp_ctx, _class| {
            if !self.onboard_dwm_applied {
                self.onboard_dwm_applied = apply_win11_native_rounded_window(title);
            }
            if vp_ctx.input(|i| i.viewport().close_requested()) {
                self.onboard_open = false;
                self.onboard_dwm_applied = false;
                self.config.first_run_completed = true;
                self.sync_all_settings();
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(Color32::TRANSPARENT))
                .show(vp_ctx, |ui| {
                    self.draw_onboarding_modal(vp_ctx, ui);
                });
        });
    }

    fn show_update_viewport(&mut self, ctx: &Context, info: &UpdateInfo) {
        let title = "Mise à jour Glassic";
        let vp_id = egui::ViewportId::from_hash_of("glassic_update_viewport");
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([500.0, 240.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_window_level(egui::WindowLevel::Normal);

        ctx.show_viewport_immediate(vp_id, builder, |vp_ctx, _class| {
            if !self.update_dwm_applied {
                self.update_dwm_applied = apply_win11_native_rounded_window(title);
            }
            if vp_ctx.input(|i| i.viewport().close_requested()) {
                self.pending_update = None;
                self.update_dwm_applied = false;
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(Color32::TRANSPARENT))
                .show(vp_ctx, |ui| {
                    self.draw_update_modal(vp_ctx, ui, info);
                });
        });
    }

    fn draw_settings_modal(&mut self, ctx: &Context, ui: &mut Ui) {
        let is_fr = self.config.lang == "fr";
        let modal_rect = ui.max_rect();
        let painter = ui.painter();

        // Fill the entire window cleanly; DWM native hardware rounding (`DWMWCP_ROUND`) clips the corners with sub-pixel anti-aliasing and draws the native outer border!
        painter.rect_filled(
            modal_rect,
            CornerRadius::ZERO,
            Color32::from_rgb(15, 15, 20),
        );

        let inner_rect = modal_rect.shrink2(Vec2::new(20.0, 16.0));
        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner_rect));

        // Top Header (Draggable anywhere on the top bar) + Tabs + Minimize / Close Buttons
        let header_resp = child_ui.horizontal(|ui| {
            let title = if is_fr {
                "⚙ PARAMÈTRES GLASSIC"
            } else {
                "⚙ GLASSIC SETTINGS"
            };
            let title_resp = ui.add(
                egui::Label::new(
                    egui::RichText::new(title)
                        .size(14.5)
                        .strong()
                        .color(ACCENT_TEXT_PURPLE),
                )
                .sense(Sense::click_and_drag()),
            );
            if title_resp.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }

            ui.add_space(18.0);

            // Tab Buttons: Desktop vs OBS Web Studio
            let tab_desktop_label = if is_fr {
                "🖥 Widget Bureau"
            } else {
                "🖥 Desktop Widget"
            };
            let tab_obs_label = if is_fr {
                "🌐 Studio Web / HTML (OBS)"
            } else {
                "🌐 Web / HTML Studio (OBS)"
            };

            if draw_tab_pill(ui, tab_desktop_label, self.settings_tab == SettingsTab::Desktop) {
                self.settings_tab = SettingsTab::Desktop;
            }
            ui.add_space(6.0);
            if draw_tab_pill(ui, tab_obs_label, self.settings_tab == SettingsTab::ObsStudio) {
                self.settings_tab = SettingsTab::ObsStudio;
            }

            // Draggable spacer in header so user can grab anywhere in the empty top bar to move the Settings window
            let rem_w = (ui.available_width() - 62.0).max(10.0);
            let (_, drag_space_resp) = ui.allocate_exact_size(Vec2::new(rem_w, 26.0), Sense::click_and_drag());
            if drag_space_resp.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close_btn = ui.add_sized(
                    [26.0, 26.0],
                    egui::Button::new(egui::RichText::new("✕").size(12.5).color(Color32::WHITE))
                        .fill(Color32::from_rgb(38, 38, 50))
                        .corner_radius(CornerRadius::same(13)),
                );
                if close_btn.clicked() {
                    self.settings_open = false;
                    self.capture_target = CaptureTarget::None;
                    self.sync_all_settings();
                }

                ui.add_space(4.0);
                let min_btn = ui.add_sized(
                    [26.0, 26.0],
                    egui::Button::new(egui::RichText::new("—").size(11.5).color(Color32::WHITE))
                        .fill(Color32::from_rgb(32, 32, 42))
                        .corner_radius(CornerRadius::same(13)),
                );
                if min_btn.on_hover_text(if is_fr { "Réduire" } else { "Minimize" }).clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                }
            });
        });
        let _ = header_resp;

        child_ui.add_space(6.0);
        child_ui.separator();
        child_ui.add_space(6.0);

        let mut changed = false;

        // Scrollable Settings Body
        let body_h = (inner_rect.height() - 86.0).max(200.0);
        egui::ScrollArea::vertical()
            .max_height(body_h)
            .auto_shrink([false, false])
            .show(&mut child_ui, |ui| match self.settings_tab {
                SettingsTab::Desktop => {
                    changed |= self.draw_desktop_settings_tab(ctx, ui, is_fr);
                }
                SettingsTab::ObsStudio => {
                    changed |= self.draw_obs_studio_tab(ctx, ui, is_fr);
                }
            });

        if changed {
            self.sync_all_settings();
        }

        // Footer Bar
        child_ui.add_space(6.0);
        child_ui.separator();
        child_ui.add_space(6.0);

        child_ui.horizontal(|ui| {
            let reset_lbl = if is_fr {
                "🔄 Réinitialiser"
            } else {
                "🔄 Reset Defaults"
            };
            let reset_btn = ui.add(
                egui::Button::new(
                    egui::RichText::new(reset_lbl)
                        .size(11.5)
                        .strong()
                        .color(Color32::from_rgb(255, 92, 105)),
                )
                .fill(Color32::from_rgb(48, 22, 28))
                .stroke(Stroke::new(1.0, Color32::from_rgb(130, 42, 52)))
                .corner_radius(CornerRadius::same(6)),
            );
            if reset_btn.clicked() {
                self.confirm_reset_open = true;
            }

            ui.add_space(8.0);

            let exp_lbl = if is_fr {
                "📤 Exporter Config"
            } else {
                "📤 Export Config"
            };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(exp_lbl).size(11.0).color(Color32::WHITE))
                        .fill(Color32::from_rgb(34, 34, 44))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(56, 56, 72)))
                        .corner_radius(CornerRadius::same(6)),
                )
                .clicked()
            {
                let backup_path = AppConfig::app_data_dir().join("glassic_backup.json");
                if self.config.export_to_file(&backup_path).is_ok() {
                    self.show_toast(if is_fr {
                        "✅ Config exportée dans AppData/Glassic/glassic_backup.json"
                    } else {
                        "✅ Config exported to AppData/Glassic/glassic_backup.json"
                    });
                }
            }

            let imp_lbl = if is_fr {
                "📥 Importer Config"
            } else {
                "📥 Import Config"
            };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(imp_lbl).size(11.0).color(Color32::WHITE))
                        .fill(Color32::from_rgb(34, 34, 44))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(56, 56, 72)))
                        .corner_radius(CornerRadius::same(6)),
                )
                .clicked()
            {
                let backup_path = AppConfig::app_data_dir().join("glassic_backup.json");
                if let Ok(loaded) = AppConfig::import_from_file(&backup_path) {
                    self.config = loaded;
                    self.sync_all_settings();
                    self.show_toast(if is_fr {
                        "✅ Config importée avec succès !"
                    } else {
                        "✅ Config imported successfully!"
                    });
                } else {
                    self.show_toast(if is_fr {
                        "⚠ Aucun fichier glassic_backup.json trouvé"
                    } else {
                        "⚠ No glassic_backup.json found"
                    });
                }
            }

            if let Some((ref msg, ts)) = self.status_toast {
                if ts.elapsed() < Duration::from_secs(4) {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(msg)
                            .size(11.5)
                            .strong()
                            .color(ACCENT_TEXT_PURPLE),
                    );
                }
            }
        });

        // Reset Confirmation Dialog Overlay
        if self.confirm_reset_open {
            self.draw_reset_confirm_dialog(ui, is_fr);
        }
    }

    fn draw_desktop_settings_tab(&mut self, ctx: &Context, ui: &mut Ui, is_fr: bool) -> bool {
        let mut changed = false;
        let col_w = (ui.available_width() - 12.0) * 0.5;

        // 2-column grid of toggle switches
        egui::Grid::new("desktop_toggles_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .min_col_width(col_w)
            .max_col_width(col_w)
            .show(ui, |ui| {
                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr { "💿 Vinyle tournant" } else { "💿 Spinning Vinyl" },
                    &mut self.config.vinyl,
                ) {
                    if self.config.vinyl {
                        self.config.art_style = "vinyl".to_string();
                    }
                    changed = true;
                }
                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr { "🌈 Couleurs de la Cover" } else { "🌈 Cover Colors" },
                    &mut self.config.color,
                ) {
                    changed = true;
                }
                ui.end_row();

                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr { "✨ Effet Glassy" } else { "✨ Glassy Effect" },
                    &mut self.config.glassy,
                ) {
                    changed = true;
                }
                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr { "📌 Toujours au premier plan" } else { "📌 Always on Top" },
                    &mut self.config.always_on_top,
                ) {
                    let level = if self.config.always_on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    };
                    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
                    changed = true;
                }
                ui.end_row();

                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr { "✨ Couleur d'application" } else { "✨ App Accent Color" },
                    &mut self.config.source_color,
                ) {
                    changed = true;
                }
                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr {
                        "👻 Mode Fantôme (Click-through)"
                    } else {
                        "👻 Ghost Mode (Click-through)"
                    },
                    &mut self.config.click_through,
                ) {
                    changed = true;
                }
                ui.end_row();

                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr {
                        "🎤 Paroles synchronisées (LRCLIB)"
                    } else {
                        "🎤 Synced Lyrics (LRCLIB)"
                    },
                    &mut self.config.show_lyrics,
                ) {
                    if self.config.show_lyrics {
                        if let Some(ref m) = self.current_media {
                            self.lyrics_mgr.request_for_track(&m.title, &m.artist);
                        }
                    }
                    changed = true;
                }
                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr {
                        "🎬 Masquer en plein écran (Jeux)"
                    } else {
                        "🎬 Hide on Fullscreen (Games)"
                    },
                    &mut self.config.hide_on_fullscreen,
                ) {
                    changed = true;
                }
                ui.end_row();
            });

        ui.add_space(8.0);
        let full_w = ui.available_width();

        // Scale Slider
        if draw_slider_card(
            ui,
            full_w,
            if is_fr { "🔍 Taille du Widget" } else { "🔍 Widget Size" },
            &mut self.config.scale,
            50..=200,
            "%",
        ) {
            changed = true;
        }
        ui.add_space(6.0);

        // Opacity Slider
        if draw_slider_card(
            ui,
            full_w,
            if is_fr {
                "💧 Transparence du Widget"
            } else {
                "💧 Widget Opacity"
            },
            &mut self.config.opacity,
            10..=100,
            "%",
        ) {
            changed = true;
        }
        ui.add_space(6.0);

        // Layout & Art Style selectors
        egui::Grid::new("desktop_combos_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .min_col_width(col_w)
            .max_col_width(col_w)
            .show(ui, |ui| {
                changed |= draw_combo_card(
                    ui,
                    col_w,
                    "combo_layout",
                    if is_fr { "📐 Format du Widget" } else { "📐 Widget Layout" },
                    &mut self.config.layout,
                    &[
                        ("standard", if is_fr { "Standard (440×126)" } else { "Standard (440×126)" }),
                        ("compact", if is_fr { "Compact / Pilule" } else { "Compact / Pill" }),
                        ("minimal", if is_fr { "Minimal (Sans Cover)" } else { "Minimal (No Art)" }),
                    ],
                );

                if draw_combo_card(
                    ui,
                    col_w,
                    "combo_art_style",
                    if is_fr { "🎨 Style de Pochette" } else { "🎨 Cover Art Style" },
                    &mut self.config.art_style,
                    &[
                        ("vinyl", if is_fr { "Vinyle tournant" } else { "Spinning Vinyl" }),
                        ("rounded", if is_fr { "Carré arrondi" } else { "Rounded Square" }),
                        ("circle", if is_fr { "Cercle simple" } else { "Simple Circle" }),
                    ],
                ) {
                    self.config.vinyl = self.config.art_style == "vinyl";
                    changed = true;
                }
                ui.end_row();

                changed |= draw_combo_card(
                    ui,
                    col_w,
                    "combo_ctrl_mode",
                    if is_fr { "🎮 Contrôles de lecture" } else { "🎮 Playback Controls" },
                    &mut self.config.controls_mode,
                    &[
                        ("hover", if is_fr { "Au survol" } else { "On Hover" }),
                        ("always", if is_fr { "Toujours visibles" } else { "Always On" }),
                        ("off", if is_fr { "Désactivé" } else { "Disabled" }),
                    ],
                );

                // Auto-hide when paused selector
                let mut pause_str = self.config.auto_hide_pause_secs.to_string();
                if draw_combo_card(
                    ui,
                    col_w,
                    "combo_pause_hide",
                    if is_fr { "⏸ Masquer en pause" } else { "⏸ Hide when Paused" },
                    &mut pause_str,
                    &[
                        ("0", if is_fr { "Désactivé" } else { "Disabled" }),
                        ("1", if is_fr { "Immédiatement" } else { "Immediately" }),
                        ("5", if is_fr { "Après 5 secondes" } else { "After 5 seconds" }),
                        ("10", if is_fr { "Après 10 secondes" } else { "After 10 seconds" }),
                        ("30", if is_fr { "Après 30 secondes" } else { "After 30 seconds" }),
                    ],
                ) {
                    self.config.auto_hide_pause_secs = pause_str.parse().unwrap_or(0);
                    changed = true;
                }
                ui.end_row();
            });

        ui.add_space(6.0);

        // Keybinds & Language
        egui::Grid::new("desktop_keys_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .min_col_width(col_w)
            .max_col_width(col_w)
            .show(ui, |ui| {
                draw_keybind_card(
                    ui,
                    col_w,
                    if is_fr { "⌨ Raccourci Paramètres" } else { "⌨ Settings Keybind" },
                    &self.config.keybind,
                    self.capture_target == CaptureTarget::SettingsKey,
                    is_fr,
                    || self.capture_target = CaptureTarget::SettingsKey,
                );

                draw_keybind_card(
                    ui,
                    col_w,
                    if is_fr { "👻 Raccourci Mode Fantôme" } else { "👻 Ghost Mode Keybind" },
                    &self.config.ghost_keybind,
                    self.capture_target == CaptureTarget::GhostKey,
                    is_fr,
                    || self.capture_target = CaptureTarget::GhostKey,
                );
                ui.end_row();

                changed |= draw_combo_card(
                    ui,
                    col_w,
                    "combo_lang",
                    "🌐 Language / Langue",
                    &mut self.config.lang,
                    &[("fr", "Français"), ("en", "English")],
                );

                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr {
                        "🚀 Lancer au démarrage Windows"
                    } else {
                        "🚀 Launch on Windows Startup"
                    },
                    &mut self.config.auto_start,
                ) {
                    set_windows_auto_start(self.config.auto_start);
                    changed = true;
                }
                ui.end_row();
            });

        ui.add_space(6.0);

        // Desktop Shortcut Row + Quick OBS Toggle
        egui::Grid::new("desktop_bottom_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .min_col_width(col_w)
            .max_col_width(col_w)
            .show(ui, |ui| {
                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr {
                        "🌐 Serveur Web / OBS (Port 3000)"
                    } else {
                        "🌐 Web / OBS Server (Port 3000)"
                    },
                    &mut self.config.web_server,
                ) {
                    changed = true;
                }

                draw_action_button_card(
                    ui,
                    col_w,
                    if is_fr { "🖥 Raccourci Bureau" } else { "🖥 Desktop Shortcut" },
                    if self.shortcut_created_toast {
                        "✅ Créé !"
                    } else if is_fr {
                        "Créer"
                    } else {
                        "Create"
                    },
                    || {
                        if create_desktop_shortcut() {
                            self.shortcut_created_toast = true;
                        }
                    },
                );
                ui.end_row();
            });

        changed
    }

    fn draw_obs_studio_tab(&mut self, ctx: &Context, ui: &mut Ui, is_fr: bool) -> bool {
        let mut changed = false;
        let full_w = ui.available_width();
        let col_w = (full_w - 12.0) * 0.5;

        // Top OBS Server Status & Link Card
        let box_frame = egui::Frame::NONE
            .fill(Color32::from_rgb(30, 20, 44))
            .stroke(Stroke::new(1.0, ACCENT_PURPLE))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(12.0);

        box_frame.show(ui, |ui| {
            ui.set_width(full_w - 24.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if is_fr {
                        "🌐 STUDIO DE PERSONNALISATION WEB / HTML (OBS)"
                    } else {
                        "🌐 WEB / HTML CUSTOMIZATION STUDIO (OBS)"
                    })
                    .size(12.5)
                    .strong()
                    .color(ACCENT_TEXT_PURPLE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if draw_pill_switch(ui, &mut self.config.web_server) {
                        changed = true;
                    }
                    ui.label(
                        egui::RichText::new(if self.config.web_server {
                            if is_fr { "Serveur Actif" } else { "Server Active" }
                        } else if is_fr {
                            "Serveur Arrêté"
                        } else {
                            "Server Stopped"
                        })
                        .size(11.5)
                        .strong(),
                    );
                });
            });

            ui.add_space(6.0);
            let url = format!("http://localhost:{}/widget.html", self.config.web_port);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("🔗 URL OBS Browser Source :")
                        .size(11.0)
                        .color(Color32::from_gray(210)),
                );
                ui.label(
                    egui::RichText::new(&url)
                        .monospace()
                        .size(11.5)
                        .strong()
                        .color(ACCENT_TEXT_PURPLE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(if is_fr {
                                    "📋 Copier le lien"
                                } else {
                                    "📋 Copy Link"
                                })
                                .size(11.0)
                                .strong()
                                .color(Color32::WHITE),
                            )
                            .fill(ACCENT_PURPLE)
                            .corner_radius(CornerRadius::same(6)),
                        )
                        .clicked()
                    {
                        ctx.copy_text(url.clone());
                        self.show_toast(if is_fr {
                            "✅ Lien OBS copié dans le presse-papiers !"
                        } else {
                            "✅ OBS link copied to clipboard!"
                        });
                    }
                });
            });
        });

        ui.add_space(8.0);

        // Sync with Desktop toggle
        if draw_setting_switch_card(
            ui,
            full_w,
            if is_fr {
                "🔄 Synchroniser automatiquement le style OBS avec le Widget Bureau"
            } else {
                "🔄 Automatically sync OBS style with Desktop Widget"
            },
            &mut self.config.obs.sync_with_desktop,
        ) {
            changed = true;
        }

        if self.config.obs.sync_with_desktop {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(if is_fr {
                    "ℹ Le widget HTML/OBS utilise actuellement exactement les mêmes réglages que votre widget bureau. Désactivez l'option ci-dessus pour personnaliser OBS de manière totalement indépendante."
                } else {
                    "ℹ The HTML/OBS widget currently mirrors your Desktop widget settings. Disable the toggle above to customize OBS independently."
                })
                .size(11.5)
                .color(Color32::from_gray(195)),
            );
            return changed;
        }

        ui.add_space(8.0);

        // Independent OBS Customization Controls
        egui::Grid::new("obs_custom_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .min_col_width(col_w)
            .max_col_width(col_w)
            .show(ui, |ui| {
                changed |= draw_combo_card(
                    ui,
                    col_w,
                    "obs_layout_combo",
                    if is_fr { "📐 Format Carte OBS" } else { "📐 OBS Card Layout" },
                    &mut self.config.obs.layout,
                    &[
                        ("standard", "Standard (440×126)"),
                        ("compact", "Compact / Pilule (340×92)"),
                        ("minimal", "Minimal (320×82)"),
                    ],
                );

                if draw_combo_card(
                    ui,
                    col_w,
                    "obs_art_combo",
                    if is_fr { "💿 Style Pochette OBS" } else { "💿 OBS Art Style" },
                    &mut self.config.obs.art_style,
                    &[
                        ("vinyl", if is_fr { "Vinyle tournant" } else { "Spinning Vinyl" }),
                        ("rounded", if is_fr { "Carré arrondi" } else { "Rounded Square" }),
                        ("circle", if is_fr { "Cercle simple" } else { "Simple Circle" }),
                    ],
                ) {
                    self.config.obs.vinyl = self.config.obs.art_style == "vinyl";
                    changed = true;
                }
                ui.end_row();

                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr { "🌈 Couleurs Cover OBS (Flou)" } else { "🌈 OBS Cover Colors (Blur)" },
                    &mut self.config.obs.color,
                ) {
                    changed = true;
                }
                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr { "✨ Effet Glassy OBS" } else { "✨ OBS Glassy Effect" },
                    &mut self.config.obs.glassy,
                ) {
                    changed = true;
                }
                ui.end_row();

                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr { "🌑 Ombre portée sous la carte OBS" } else { "🌑 OBS Drop Shadow" },
                    &mut self.config.obs.drop_shadow,
                ) {
                    changed = true;
                }
                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr { "🎤 Afficher les paroles sur OBS" } else { "🎤 Show Synced Lyrics on OBS" },
                    &mut self.config.obs.show_lyrics,
                ) {
                    if self.config.obs.show_lyrics {
                        if let Some(ref m) = self.current_media {
                            self.lyrics_mgr.request_for_track(&m.title, &m.artist);
                        }
                    }
                    changed = true;
                }
                ui.end_row();

                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr { "⏸ Masquer sur OBS en pause" } else { "⏸ Hide on OBS when Paused" },
                    &mut self.config.obs.hide_when_paused,
                ) {
                    changed = true;
                }
                if draw_setting_switch_card(
                    ui,
                    col_w,
                    if is_fr { "🎨 Forcer couleur d'accentuation OBS" } else { "🎨 Custom OBS Accent Color" },
                    &mut self.config.obs.source_color,
                ) {
                    changed = true;
                }
                ui.end_row();
            });

        ui.add_space(6.0);

        // Custom Accent Color Presets for OBS
        let preset_frame = setting_card_frame();
        preset_frame.show(ui, |ui| {
            ui.set_width(full_w - 20.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if is_fr {
                        "🎨 Palette Couleur d'accentuation HTML/OBS :"
                    } else {
                        "🎨 HTML/OBS Accent Color Palette:"
                    })
                    .size(11.5)
                    .color(Color32::WHITE),
                );
                let presets = [
                    ("Violet Glassic", "#9A22E2", Color32::from_rgb(154, 34, 226)),
                    ("Vert Spotify", "#1DB954", Color32::from_rgb(29, 185, 84)),
                    ("Rose Néon", "#FF2A85", Color32::from_rgb(255, 42, 133)),
                    ("Cyan Stream", "#00E5FF", Color32::from_rgb(0, 229, 255)),
                    ("Or Ambré", "#FFB800", Color32::from_rgb(255, 184, 0)),
                    ("Blanc Pur", "#FFFFFF", Color32::from_rgb(255, 255, 255)),
                ];
                for (name, hex, col) in presets {
                    let selected = self.config.obs.custom_accent.eq_ignore_ascii_case(hex);
                    let stroke = if selected {
                        Stroke::new(2.0, Color32::WHITE)
                    } else {
                        Stroke::new(1.0, Color32::from_gray(90))
                    };
                    let btn = ui.add_sized(
                        [22.0, 22.0],
                        egui::Button::new("")
                            .fill(col)
                            .stroke(stroke)
                            .corner_radius(CornerRadius::same(11)),
                    );
                    if btn.on_hover_text(format!("{name} ({hex})")).clicked() {
                        self.config.obs.custom_accent = hex.to_string();
                        self.config.obs.source_color = true;
                        changed = true;
                    }
                }
            });
        });

        ui.add_space(6.0);

        // OBS Sliders: Scale, Opacity, Border Radius
        changed |= draw_slider_card(
            ui,
            full_w,
            if is_fr { "🔍 Taille / Échelle OBS" } else { "🔍 OBS Widget Scale" },
            &mut self.config.obs.scale,
            50..=200,
            "%",
        );
        ui.add_space(6.0);

        changed |= draw_slider_card(
            ui,
            full_w,
            if is_fr { "💧 Opacité OBS" } else { "💧 OBS Widget Opacity" },
            &mut self.config.obs.opacity,
            10..=100,
            "%",
        );
        ui.add_space(6.0);

        changed |= draw_slider_card(
            ui,
            full_w,
            if is_fr {
                "⬜ Rayon des bords arrondis OBS"
            } else {
                "⬜ OBS Corner Radius"
            },
            &mut self.config.obs.border_radius,
            0..=40,
            " px",
        );

        changed
    }

    fn draw_onboarding_modal(&mut self, ctx: &Context, ui: &mut Ui) {
        let modal_rect = ui.max_rect();
        let painter = ui.painter();
        painter.rect_filled(
            modal_rect,
            CornerRadius::ZERO,
            Color32::from_rgb(16, 16, 22),
        );

        let inner = modal_rect.shrink(26.0);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        let is_fr = self.onboard_lang == "fr";

        let header = child.add(
            egui::Label::new(
                egui::RichText::new(if is_fr {
                    "✨ Bienvenue sur Glassic"
                } else {
                    "✨ Welcome to Glassic"
                })
                .size(20.0)
                .strong()
                .color(ACCENT_TEXT_PURPLE),
            )
            .sense(Sense::click_and_drag()),
        );
        if header.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        child.add_space(6.0);
        child.label(
            egui::RichText::new(if is_fr {
                "Configurez vos préférences pour votre premier lancement :"
            } else {
                "Set up your preferences for your first launch:"
            })
            .size(13.0)
            .color(Color32::from_gray(200)),
        );
        child.add_space(16.0);

        let w = inner.width();
        draw_combo_card(
            &mut child,
            w,
            "onboard_lang_combo",
            "🌐 Language / Langue",
            &mut self.onboard_lang,
            &[("fr", "Français"), ("en", "English")],
        );
        child.add_space(8.0);
        draw_setting_switch_card(
            &mut child,
            w,
            if is_fr {
                "🖥 Créer un raccourci sur le Bureau"
            } else {
                "🖥 Create Desktop Shortcut"
            },
            &mut self.onboard_shortcut,
        );
        child.add_space(8.0);
        draw_setting_switch_card(
            &mut child,
            w,
            if is_fr {
                "🚀 Lancer au démarrage de Windows"
            } else {
                "🚀 Launch on Windows Startup"
            },
            &mut self.onboard_startup,
        );

        child.add_space(20.0);
        child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let btn = ui.add(
                egui::Button::new(
                    egui::RichText::new(if is_fr {
                        "Démarrer Glassic 🚀"
                    } else {
                        "Start Glassic 🚀"
                    })
                    .size(13.5)
                    .strong()
                    .color(Color32::WHITE),
                )
                .fill(ACCENT_PURPLE)
                .corner_radius(CornerRadius::same(10)),
            );
            if btn.clicked() {
                self.config.lang = self.onboard_lang.clone();
                self.config.auto_start = self.onboard_startup;
                self.config.first_run_completed = true;
                set_windows_auto_start(self.onboard_startup);
                if self.onboard_shortcut {
                    create_desktop_shortcut();
                }
                self.onboard_open = false;
                self.sync_all_settings();
            }
        });
    }

    fn draw_update_modal(&mut self, ctx: &Context, ui: &mut Ui, info: &UpdateInfo) {
        let is_fr = self.config.lang == "fr";
        let modal_rect = ui.max_rect();
        let painter = ui.painter();
        painter.rect_filled(
            modal_rect,
            CornerRadius::ZERO,
            Color32::from_rgb(16, 16, 22),
        );

        let inner = modal_rect.shrink(24.0);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));

        let header = child.add(
            egui::Label::new(
                egui::RichText::new(if is_fr {
                    "🚀 Nouvelle mise à jour disponible !"
                } else {
                    "🚀 New Update Available!"
                })
                .size(18.0)
                .strong()
                .color(ACCENT_TEXT_PURPLE),
            )
            .sense(Sense::click_and_drag()),
        );
        if header.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        child.add_space(10.0);
        child.label(
            egui::RichText::new(if is_fr {
                format!(
                    "Une nouvelle version de Glassic ({}) est disponible. Tous vos paramètres seront conservés !",
                    info.version
                )
            } else {
                format!(
                    "A new version of Glassic ({}) is ready to install. All your settings will be preserved!",
                    info.version
                )
            })
            .size(12.5)
            .color(Color32::from_gray(205)),
        );

        child.add_space(28.0);
        child.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(if is_fr { "Plus tard" } else { "Later" })
                            .size(12.0)
                            .color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(38, 38, 50))
                    .corner_radius(CornerRadius::same(8)),
                )
                .clicked()
            {
                self.pending_update = None;
            }
            ui.add_space(10.0);
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(if is_fr {
                            "Mettre à jour maintenant"
                        } else {
                            "Update Now"
                        })
                        .size(12.0)
                        .strong()
                        .color(Color32::WHITE),
                    )
                    .fill(ACCENT_PURPLE)
                    .corner_radius(CornerRadius::same(8)),
                )
                .clicked()
            {
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "", &info.url])
                    .spawn();
                self.pending_update = None;
            }
        });
    }

    fn draw_reset_confirm_dialog(&mut self, ui: &mut Ui, is_fr: bool) {
        let dialog_rect = Rect::from_center_size(ui.max_rect().center(), Vec2::new(320.0, 150.0));
        let painter = ui.painter();
        painter.rect_filled(
            ui.max_rect(),
            CornerRadius::same(18),
            Color32::from_rgba_unmultiplied(0, 0, 0, 185),
        );
        painter.rect_filled(
            dialog_rect,
            CornerRadius::same(14),
            Color32::from_rgb(24, 24, 32),
        );
        painter.rect_stroke(
            dialog_rect,
            CornerRadius::same(14),
            Stroke::new(1.0, Color32::from_rgb(60, 60, 78)),
            StrokeKind::Inside,
        );

        let inner = dialog_rect.shrink(18.0);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        child.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(if is_fr {
                    "Réinitialiser les paramètres"
                } else {
                    "Reset Settings"
                })
                .size(14.0)
                .strong()
                .color(Color32::WHITE),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(if is_fr {
                    "Voulez-vous vraiment remettre tous les réglages par défaut ?"
                } else {
                    "Are you sure you want to reset all settings to default values?"
                })
                .size(11.5)
                .color(Color32::from_gray(195)),
            );
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        [125.0, 28.0],
                        egui::Button::new(if is_fr { "Annuler" } else { "Cancel" })
                            .fill(Color32::from_rgb(42, 42, 54))
                            .corner_radius(CornerRadius::same(6)),
                    )
                    .clicked()
                {
                    self.confirm_reset_open = false;
                }
                ui.add_space(8.0);
                if ui
                    .add_sized(
                        [125.0, 28.0],
                        egui::Button::new(
                            egui::RichText::new(if is_fr { "Oui, réinitialiser" } else { "Yes, Reset" })
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(255, 71, 87))
                        .corner_radius(CornerRadius::same(6)),
                    )
                    .clicked()
                {
                    let lang = self.config.lang.clone();
                    self.config = AppConfig::default();
                    self.config.first_run_completed = true;
                    self.config.lang = lang;
                    self.sync_all_settings();
                    self.confirm_reset_open = false;
                }
            });
        });
    }
}

// ── UI Component Helpers ──────────────────────────────────────────────────────

fn setting_card_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(Color32::from_rgb(26, 26, 34))
        .stroke(Stroke::new(1.0, Color32::from_rgb(48, 48, 62)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 8))
}

fn draw_tab_pill(ui: &mut Ui, label: &str, active: bool) -> bool {
    let fill = if active {
        ACCENT_PURPLE
    } else {
        Color32::from_rgb(30, 30, 40)
    };
    let stroke = if active {
        Stroke::new(1.0, ACCENT_TEXT_PURPLE)
    } else {
        Stroke::new(1.0, Color32::from_rgb(52, 52, 68))
    };
    let text_col = if active {
        Color32::WHITE
    } else {
        Color32::from_gray(205)
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(11.5).strong().color(text_col))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(8)),
    )
    .clicked()
}

fn draw_setting_switch_card(ui: &mut Ui, width: f32, label: &str, value: &mut bool) -> bool {
    let mut changed = false;
    setting_card_frame().show(ui, |ui| {
        ui.set_width((width - 24.0).max(80.0));
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).size(11.5).color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if draw_pill_switch(ui, value) {
                    changed = true;
                }
            });
        });
    });
    changed
}

fn draw_pill_switch(ui: &mut Ui, on: &mut bool) -> bool {
    let desired_size = Vec2::new(38.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool(response.id, *on);
        let bg = if *on {
            ACCENT_PURPLE
        } else {
            Color32::from_rgb(58, 58, 72)
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), bg);
        let circle_x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), how_on);
        let knob_col = if *on { Color32::BLACK } else { Color32::WHITE };
        ui.painter()
            .circle_filled(Pos2::new(circle_x, rect.center().y), 7.0, knob_col);
    }
    response.changed()
}

fn draw_slider_card(
    ui: &mut Ui,
    width: f32,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    suffix: &str,
) -> bool {
    let mut changed = false;
    setting_card_frame().show(ui, |ui| {
        let inner_w = (width - 24.0).max(100.0);
        ui.set_width(inner_w);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).size(11.5).color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{value}{suffix}"))
                        .size(12.0)
                        .strong()
                        .color(ACCENT_TEXT_PURPLE),
                );
            });
        });
        ui.add_space(4.0);

        // Custom filled purple slider bar (matching the HTML Glassic slider aesthetic)
        let min_v = *range.start() as f32;
        let max_v = (*range.end() as f32).max(min_v + 1.0);
        let slider_h = 18.0;
        let (slider_rect, resp) =
            ui.allocate_exact_size(Vec2::new(inner_w, slider_h), Sense::click_and_drag());

        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if (resp.clicked() || resp.dragged()) && slider_rect.width() > 16.0 {
            if let Some(p) = resp.interact_pointer_pos() {
                let pad = 8.0;
                let t = ((p.x - (slider_rect.left() + pad)) / (slider_rect.width() - pad * 2.0))
                    .clamp(0.0, 1.0);
                let new_v = (min_v + t * (max_v - min_v)).round() as u32;
                if new_v != *value {
                    *value = new_v;
                    changed = true;
                }
            }
        }

        let pad = 8.0;
        let rail_left = slider_rect.left() + pad;
        let rail_right = slider_rect.right() - pad;
        let rail_w = (rail_right - rail_left).max(10.0);
        let rail_y = slider_rect.center().y;
        let rail_h = 6.0;
        let rail_rect = Rect::from_min_size(
            Pos2::new(rail_left, rail_y - rail_h * 0.5),
            Vec2::new(rail_w, rail_h),
        );

        let painter = ui.painter();
        // Background track
        painter.rect_filled(
            rail_rect,
            CornerRadius::same(3),
            Color32::from_rgb(52, 52, 66),
        );
        painter.rect_stroke(
            rail_rect,
            CornerRadius::same(3),
            Stroke::new(1.0, Color32::from_rgb(75, 75, 92)),
            StrokeKind::Inside,
        );

        // Filled purple portion
        let ratio = ((*value as f32 - min_v) / (max_v - min_v)).clamp(0.0, 1.0);
        let fill_w = (rail_w * ratio).max(rail_h);
        let fill_rect = Rect::from_min_size(rail_rect.min, Vec2::new(fill_w, rail_h));
        painter.rect_filled(fill_rect, CornerRadius::same(3), ACCENT_PURPLE);

        // Glowing thumb knob
        let thumb_x = rail_left + rail_w * ratio;
        let thumb_center = Pos2::new(thumb_x, rail_y);
        painter.circle_filled(thumb_center, 7.5, ACCENT_TEXT_PURPLE);
        painter.circle_stroke(thumb_center, 7.5, Stroke::new(1.5, Color32::WHITE));
    });
    changed
}

fn draw_combo_card(
    ui: &mut Ui,
    width: f32,
    id_salt: &str,
    label: &str,
    current: &mut String,
    options: &[(&str, &str)],
) -> bool {
    let mut changed = false;
    setting_card_frame().show(ui, |ui| {
        ui.set_width((width - 24.0).max(80.0));
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).size(11.5).color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let selected_text = options
                    .iter()
                    .find(|(k, _)| *k == current.as_str())
                    .map(|(_, v)| *v)
                    .unwrap_or(current.as_str());
                egui::ComboBox::from_id_salt(id_salt)
                    .selected_text(
                        egui::RichText::new(selected_text)
                            .size(11.0)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .show_ui(ui, |ui| {
                        for (val, text) in options {
                            if ui.selectable_label(current == *val, *text).clicked() {
                                *current = (*val).to_string();
                                changed = true;
                            }
                        }
                    });
            });
        });
    });
    changed
}

fn draw_keybind_card(
    ui: &mut Ui,
    width: f32,
    label: &str,
    current_key: &str,
    is_capturing: bool,
    is_fr: bool,
    on_click: impl FnOnce(),
) {
    setting_card_frame().show(ui, |ui| {
        ui.set_width((width - 24.0).max(80.0));
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).size(11.5).color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn_text = if is_capturing {
                    if is_fr { "Appuie..." } else { "Press key..." }
                } else {
                    current_key
                };
                let fill = if is_capturing {
                    ACCENT_PURPLE
                } else {
                    Color32::from_rgb(16, 16, 22)
                };
                let stroke = if is_capturing {
                    Stroke::new(1.0, ACCENT_TEXT_PURPLE)
                } else {
                    Stroke::new(1.0, Color32::from_rgb(62, 62, 78))
                };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(btn_text)
                                .size(11.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(fill)
                        .stroke(stroke)
                        .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    on_click();
                }
            });
        });
    });
}

fn draw_action_button_card(
    ui: &mut Ui,
    width: f32,
    label: &str,
    btn_label: &str,
    on_click: impl FnOnce(),
) {
    setting_card_frame().show(ui, |ui| {
        ui.set_width((width - 24.0).max(80.0));
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).size(11.5).color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(btn_label)
                                .size(11.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(16, 16, 22))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(62, 62, 78)))
                        .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    on_click();
                }
            });
        });
    });
}

// ── Image & Geometry Helpers ──────────────────────────────────────────────────

fn draw_rotated_textured_quad(
    painter: &egui::Painter,
    tex_id: egui::TextureId,
    center: Pos2,
    half_size: f32,
    angle: f32,
    tint: Color32,
) {
    let (s, c) = angle.sin_cos();
    let rot = |dx: f32, dy: f32| -> Pos2 {
        Pos2::new(center.x + dx * c - dy * s, center.y + dx * s + dy * c)
    };

    let mut mesh = Mesh::with_texture(tex_id);
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rot(-half_size, -half_size),
        uv: Pos2::new(0.0, 0.0),
        color: tint,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rot(half_size, -half_size),
        uv: Pos2::new(1.0, 0.0),
        color: tint,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rot(half_size, half_size),
        uv: Pos2::new(1.0, 1.0),
        color: tint,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rot(-half_size, half_size),
        uv: Pos2::new(0.0, 1.0),
        color: tint,
    });
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(Shape::mesh(mesh));
}

fn draw_clipped_or_scrolling_text(
    ctx: &Context,
    painter: &egui::Painter,
    pos: Pos2,
    max_width: f32,
    height: f32,
    text: &str,
    font: FontId,
    color: Color32,
) -> bool {
    let clip_rect = Rect::from_min_size(pos, Vec2::new(max_width, height));
    let clipped = painter.with_clip_rect(clip_rect);
    let galley = painter.layout_no_wrap(text.to_string(), font, color);
    let text_w = galley.size().x;

    if text_w <= max_width {
        clipped.galley(pos, galley, color);
        false
    } else {
        // Smooth 60 FPS marquee scrolling (active both when playing and when paused!)
        let gap = 48.0;
        let cycle = text_w + gap;
        let t = ctx.input(|i| i.time) as f32;
        let offset = (t * 28.0) % cycle;
        clipped.galley(Pos2::new(pos.x - offset, pos.y), galley.clone(), color);
        clipped.galley(Pos2::new(pos.x - offset + cycle, pos.y), galley, color);
        true
    }
}

pub fn apply_sdf_rounded_mask(img: &mut RgbaImage, radius: f32) {
    let w = img.width() as f32;
    let h = img.height() as f32;
    let half_w = w * 0.5;
    let half_h = h * 0.5;
    let r = radius.min(half_w).min(half_h);

    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let px = (x as f32 + 0.5 - half_w).abs() - (half_w - r);
        let py = (y as f32 + 0.5 - half_h).abs() - (half_h - r);
        let qx = px.max(0.0);
        let qy = py.max(0.0);
        let dist = (qx * qx + qy * qy).sqrt() + px.max(py).min(0.0) - r;

        let alpha = (0.5 - dist).clamp(0.0, 1.0);
        pixel.0[3] = ((pixel.0[3] as f32) * alpha).round() as u8;
    }
}

/// Generates an exact anti-aliased SDF rounded border ring matching `apply_sdf_rounded_mask` pixel-for-pixel
pub fn build_sdf_border_image(
    w: u32,
    h: u32,
    radius: f32,
    thickness: f32,
    max_alpha: u8,
) -> RgbaImage {
    let mut img = ImageBuffer::new(w, h);
    let wf = w as f32;
    let hf = h as f32;
    let half_w = wf * 0.5;
    let half_h = hf * 0.5;
    let r = radius.min(half_w).min(half_h);

    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let px = (x as f32 + 0.5 - half_w).abs() - (half_w - r);
        let py = (y as f32 + 0.5 - half_h).abs() - (half_h - r);
        let qx = px.max(0.0);
        let qy = py.max(0.0);
        let dist = (qx * qx + qy * qy).sqrt() + px.max(py).min(0.0) - r;

        let outer_cov = (0.5 - dist).clamp(0.0, 1.0);
        let inner_cov = (0.5 - (dist + thickness)).clamp(0.0, 1.0);
        let ring_cov = (outer_cov - inner_cov).clamp(0.0, 1.0);
        let a = ((max_alpha as f32) * ring_cov).round() as u8;
        *pixel = Rgba([255, 255, 255, a]);
    }
    img
}

/// Builds the diffuse blurred & saturated cover background (`blur(42px) saturate(200%)`)
pub fn build_blurred_card_background(
    src: &image::DynamicImage,
    out_w: u32,
    out_h: u32,
    corner_radius: f32,
) -> RgbaImage {
    // Work at quarter resolution for fast, ultra-smooth 42px-equivalent Gaussian blur
    let small_w = 110u32;
    let small_h = 36u32;
    let mut small = src
        .resize_to_fill(small_w, small_h, image::imageops::FilterType::Triangle)
        .to_rgba8();

    // Boost saturation (200%)
    for p in small.pixels_mut() {
        let r = p.0[0] as f32 / 255.0;
        let g = p.0[1] as f32 / 255.0;
        let b = p.0[2] as f32 / 255.0;
        let luma = 0.299 * r + 0.587 * g + 0.114 * b;
        let sat_factor = 2.05;
        let nr = (luma + (r - luma) * sat_factor).clamp(0.0, 1.0);
        let ng = (luma + (g - luma) * sat_factor).clamp(0.0, 1.0);
        let nb = (luma + (b - luma) * sat_factor).clamp(0.0, 1.0);
        p.0[0] = (nr * 255.0) as u8;
        p.0[1] = (ng * 255.0) as u8;
        p.0[2] = (nb * 255.0) as u8;
        p.0[3] = 255;
    }

    let blurred_small = image::imageops::blur(&small, 9.5);
    let mut upscaled = image::imageops::resize(
        &blurred_small,
        out_w,
        out_h,
        image::imageops::FilterType::CatmullRom,
    );
    // Blend subtle dark glass tint (`rgba(12, 12, 16, 0.22)`) directly into the cover pixels so no extra rect is needed
    for p in upscaled.pixels_mut() {
        p.0[0] = ((p.0[0] as f32) * 0.80 + 12.0 * 0.20) as u8;
        p.0[1] = ((p.0[1] as f32) * 0.80 + 12.0 * 0.20) as u8;
        p.0[2] = ((p.0[2] as f32) * 0.80 + 16.0 * 0.20) as u8;
    }
    apply_sdf_rounded_mask(&mut upscaled, corner_radius);
    upscaled
}

fn format_mm_ss(secs: i64) -> String {
    let s = secs.max(0);
    format!("{}:{:02}", s / 60, s % 60)
}

fn parse_hex_color(hex: &str) -> Color32 {
    let s = hex.trim().trim_start_matches('#');
    if s.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) {
            return Color32::from_rgb(r, g, b);
        }
    }
    ACCENT_PURPLE
}

fn with_opacity(col: Color32, opacity: f32) -> Color32 {
    let [r, g, b, a] = col.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, ((a as f32) * opacity).round() as u8)
}

/// Applies native Windows 11 DWM hardware rounded corners (`DWMWCP_ROUND`) and subtle border color
/// to secondary borderless viewports. By NOT using GDI `SetWindowRgn`, DWM renders silky-smooth
/// hardware anti-aliased rounded corners and a crisp 1px outer border with zero jagged pixels!
fn apply_win11_native_rounded_window(target_title: &str) -> bool {
    use windows::Win32::Foundation::{BOOL, COLORREF, HWND, LPARAM};
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId,
    };

    struct SearchData {
        pid: u32,
        target: String,
        found_hwnd: HWND,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &mut *(lparam.0 as *mut SearchData);
        let mut win_pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut win_pid));
        if win_pid == data.pid {
            let mut buf = [0u16; 256];
            let len = GetWindowTextW(hwnd, &mut buf);
            if len > 0 {
                let title = String::from_utf16_lossy(&buf[..len as usize]);
                if title == data.target {
                    data.found_hwnd = hwnd;
                    return BOOL(0);
                }
            }
        }
        BOOL(1)
    }

    unsafe {
        let mut search = SearchData {
            pid: GetCurrentProcessId(),
            target: target_title.to_string(),
            found_hwnd: HWND::default(),
        };
        let _ = EnumWindows(
            Some(enum_proc),
            LPARAM(&mut search as *mut SearchData as isize),
        );
        if search.found_hwnd.0.is_null() {
            return false;
        }
        let hwnd = search.found_hwnd;

        // 1. Enable Windows 11 DWM smooth hardware anti-aliased rounded corners
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const _,
            std::mem::size_of_val(&pref) as u32,
        );

        // 2. Set DWM native 1px border color (`0x00BBGGRR` -> RGB(62, 62, 78)) so the border follows the curve perfectly
        let border_col = COLORREF(0x004E3E3E);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border_col as *const _ as *const _,
            std::mem::size_of_val(&border_col) as u32,
        );

        true
    }
}


