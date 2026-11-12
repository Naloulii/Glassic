use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_VOLUME_DOWN, VK_VOLUME_UP,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMeta {
    pub icon: String,
    pub color: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub app_id: String,
    pub status: String, // "playing", "paused", "stopped"
    pub duration: i64,
    pub position: i64,
    pub thumbnail: Option<String>, // data:image/jpeg;base64,...
    pub source: String,
    pub source_meta: SourceMeta,
    pub total_sessions: usize,
    pub session_index: usize,
    #[serde(skip)]
    pub raw_art_bytes: Option<Arc<Vec<u8>>>,
    #[serde(skip)]
    pub dominant_rgb: [u8; 3],
    #[serde(skip)]
    pub title_is_dark: bool,
}

#[derive(Debug, Clone)]
pub enum MediaCommand {
    PlayPause,
    Play,
    Pause,
    Next,
    Previous,
    SeekSeconds(i64),
    VolumeUp,
    VolumeDown,
    CycleSession(i32),
}

struct CachedCover {
    track_key: String,
    base64_uri: String,
    raw_bytes: Arc<Vec<u8>>,
    dominant_rgb: [u8; 3],
    title_is_dark: bool,
}

pub struct SmtcController {
    active_index: Arc<Mutex<isize>>,
    cached_cover: Arc<Mutex<Option<CachedCover>>>,
}

impl SmtcController {
    pub fn new() -> Self {
        Self {
            active_index: Arc::new(Mutex::new(0)),
            cached_cover: Arc::new(Mutex::new(None)),
        }
    }

    pub fn cycle_session(&self, delta: isize) {
        if let Ok(mut idx) = self.active_index.lock() {
            *idx += delta;
        }
    }

    pub fn execute_command(&self, cmd: MediaCommand) {
        match cmd {
            MediaCommand::VolumeUp => unsafe {
                keybd_event(VK_VOLUME_UP.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
                keybd_event(VK_VOLUME_UP.0 as u8, 0, KEYEVENTF_KEYUP, 0);
            },
            MediaCommand::VolumeDown => unsafe {
                keybd_event(VK_VOLUME_DOWN.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
                keybd_event(VK_VOLUME_DOWN.0 as u8, 0, KEYEVENTF_KEYUP, 0);
            },
            MediaCommand::CycleSession(d) => {
                self.cycle_session(d as isize);
            }
            other => {
                let idx = *self.active_index.lock().unwrap_or_else(|e| e.into_inner());
                let _ = Self::send_smtc_command(other, idx);
            }
        }
    }

    fn send_smtc_command(cmd: MediaCommand, session_idx: isize) -> windows::core::Result<()> {
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.get()?;
        let sessions = manager.GetSessions()?;
        let count = sessions.Size()? as isize;
        if count == 0 {
            return Ok(());
        }
        let target_idx = session_idx.rem_euclid(count) as u32;
        let session: GlobalSystemMediaTransportControlsSession = sessions.GetAt(target_idx)?;

        match cmd {
            MediaCommand::PlayPause => {
                let _ = session.TryTogglePlayPauseAsync()?.get();
            }
            MediaCommand::Play => {
                let _ = session.TryPlayAsync()?.get();
            }
            MediaCommand::Pause => {
                let _ = session.TryPauseAsync()?.get();
            }
            MediaCommand::Next => {
                let _ = session.TrySkipNextAsync()?.get();
            }
            MediaCommand::Previous => {
                let _ = session.TrySkipPreviousAsync()?.get();
            }
            MediaCommand::SeekSeconds(secs) => {
                // Windows Timeline position is in 100-nanosecond ticks (1s = 10_000_000 ticks)
                let ticks = secs.max(0) * 10_000_000;
                let _ = session.TryChangePlaybackPositionAsync(ticks)?.get();
            }
            _ => {}
        }
        Ok(())
    }

    pub fn poll_active_session(&self) -> Option<MediaInfo> {
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .ok()?
            .get()
            .ok()?;
        let sessions = manager.GetSessions().ok()?;
        let total = sessions.Size().ok()? as usize;
        if total == 0 {
            if let Ok(mut cache) = self.cached_cover.lock() {
                *cache = None;
            }
            return None;
        }

        let mut valid_sessions = Vec::new();
        for i in 0..total {
            if let Ok(s) = sessions.GetAt(i as u32) {
                if let Ok(op) = s.TryGetMediaPropertiesAsync() {
                    if let Ok(props) = op.get() {
                        let title = props.Title().map(|h| h.to_string()).unwrap_or_default();
                        if !title.trim().is_empty() {
                            valid_sessions.push((s, props));
                        }
                    }
                }
            }
        }

        if valid_sessions.is_empty() {
            return None;
        }

        let valid_count = valid_sessions.len();
        let active_idx = {
            let mut idx_guard = self.active_index.lock().unwrap_or_else(|e| e.into_inner());
            if *idx_guard < 0 {
                *idx_guard = (valid_count as isize) - 1;
            } else if (*idx_guard as usize) >= valid_count {
                *idx_guard = 0;
            }
            *idx_guard as usize
        };

        let (session, props) = &valid_sessions[active_idx];
        let title = props.Title().map(|h| h.to_string()).unwrap_or_default();
        let artist = props.Artist().map(|h| h.to_string()).unwrap_or_default();
        let album = props.AlbumTitle().map(|h| h.to_string()).unwrap_or_default();
        let app_id = session
            .SourceAppUserModelId()
            .map(|h| h.to_string())
            .unwrap_or_default();

        let status = session
            .GetPlaybackInfo()
            .ok()
            .and_then(|info| info.PlaybackStatus().ok())
            .map(|st| match st {
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => "playing",
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused => "paused",
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped => "stopped",
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Changing => "changing",
                _ => "unknown",
            })
            .unwrap_or("unknown")
            .to_string();

        let (duration, position) = session
            .GetTimelineProperties()
            .ok()
            .map(|tl| {
                let dur = tl
                    .EndTime()
                    .ok()
                    .map(|t| (t.Duration / 10_000_000).max(0))
                    .unwrap_or(0);
                let pos = tl
                    .Position()
                    .ok()
                    .map(|t| (t.Duration / 10_000_000).max(0))
                    .unwrap_or(0);
                (dur, pos)
            })
            .unwrap_or((0, 0));

        let track_key = format!("{app_id}|{title}|{artist}|{album}");

        // Only keep the current track cover in memory; drop previous cover immediately on change
        let (thumbnail, raw_art_bytes, dominant_rgb, title_is_dark) = {
            let mut cache_guard = self.cached_cover.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref cached) = *cache_guard {
                if cached.track_key == track_key {
                    (
                        Some(cached.base64_uri.clone()),
                        Some(cached.raw_bytes.clone()),
                        cached.dominant_rgb,
                        cached.title_is_dark,
                    )
                } else {
                    *cache_guard = None;
                    Self::read_and_cache_thumbnail(&props, &track_key, &mut cache_guard)
                }
            } else {
                Self::read_and_cache_thumbnail(&props, &track_key, &mut cache_guard)
            }
        };

        let mut source = detect_source(&app_id);
        if source == "browser" {
            source = refine_browser_source(&title, &artist);
        }
        let source_meta = get_source_meta(&source);

        Some(MediaInfo {
            title,
            artist,
            album,
            app_id,
            status,
            duration,
            position,
            thumbnail,
            source,
            source_meta,
            total_sessions: valid_count,
            session_index: active_idx,
            raw_art_bytes,
            dominant_rgb,
            title_is_dark,
        })
    }

    fn read_and_cache_thumbnail(
        props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
        track_key: &str,
        cache_guard: &mut Option<CachedCover>,
    ) -> (Option<String>, Option<Arc<Vec<u8>>>, [u8; 3], bool) {
        if let Ok(thumb_ref) = props.Thumbnail() {
            if let Ok(open_op) = thumb_ref.OpenReadAsync() {
                if let Ok(stream) = open_op.get() {
                    let size = stream.Size().unwrap_or(0) as u32;
                    if size > 0 && size < 15 * 1024 * 1024 {
                        if let Ok(input) = stream.GetInputStreamAt(0) {
                            if let Ok(reader) = DataReader::CreateDataReader(&input) {
                                if reader.LoadAsync(size).ok().and_then(|o| o.get().ok()).is_some()
                                {
                                    let mut bytes = vec![0u8; size as usize];
                                    if reader.ReadBytes(&mut bytes).is_ok() {
                                        let mime = stream
                                            .ContentType()
                                            .ok()
                                            .map(|s| s.to_string())
                                            .filter(|s| !s.trim().is_empty())
                                            .unwrap_or_else(|| "image/jpeg".to_string());
                                        let b64 = base64::engine::general_purpose::STANDARD
                                            .encode(&bytes);
                                        let uri = format!("data:{mime};base64,{b64}");
                                        let (dominant_rgb, title_is_dark) =
                                            extract_dominant_color(&bytes);
                                        let arc_bytes = Arc::new(bytes);

                                        *cache_guard = Some(CachedCover {
                                            track_key: track_key.to_string(),
                                            base64_uri: uri.clone(),
                                            raw_bytes: arc_bytes.clone(),
                                            dominant_rgb,
                                            title_is_dark,
                                        });
                                        return (
                                            Some(uri),
                                            Some(arc_bytes),
                                            dominant_rgb,
                                            title_is_dark,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        (None, None, [154, 34, 226], false)
    }
}

pub fn extract_dominant_color(image_bytes: &[u8]) -> ([u8; 3], bool) {
    let Ok(img) = image::load_from_memory(image_bytes) else {
        return ([154, 34, 226], false);
    };
    let small = img.resize_exact(24, 24, image::imageops::FilterType::Triangle).to_rgba8();

    let mut total_r: u64 = 0;
    let mut total_g: u64 = 0;
    let mut total_b: u64 = 0;
    let mut count: u64 = 0;

    let mut best_rgb = [154u8, 34u8, 226u8];
    let mut max_sat: i32 = -1;

    for pixel in small.pixels() {
        let [r, g, b, a] = pixel.0;
        if a < 128 {
            continue;
        }
        total_r += r as u64;
        total_g += g as u64;
        total_b += b as u64;
        count += 1;

        let max_c = r.max(g).max(b) as i32;
        let min_c = r.min(g).min(b) as i32;
        let sat = max_c - min_c;

        if max_c > 40 && min_c < 240 && sat > max_sat {
            max_sat = sat;
            best_rgb = [r, g, b];
        }
    }

    let avg_r = if count > 0 { (total_r / count) as f32 } else { 128.0 };
    let avg_g = if count > 0 { (total_g / count) as f32 } else { 128.0 };
    let avg_b = if count > 0 { (total_b / count) as f32 } else { 128.0 };

    if max_sat < 25 {
        best_rgb = [avg_r as u8, avg_g as u8, avg_b as u8];
    }

    // Perceived luminance for text contrast in colorful mode
    let luminance = (0.299 * avg_r + 0.587 * avg_g + 0.114 * avg_b) / 255.0;
    let title_is_dark = luminance > 0.72;

    (best_rgb, title_is_dark)
}

fn detect_source(app_id: &str) -> String {
    let id = app_id.to_lowercase();
    if id.contains("spotify") {
        return "spotify".to_string();
    }
    if id.contains("vlc")
        || id.contains("wmplayer")
        || id.contains("windows.media")
        || id.contains("groove")
        || id.contains("music.ui")
        || id.contains("foobar")
        || id.contains("aimp")
        || id.contains("winamp")
        || id.contains("musicbee")
    {
        return "local".to_string();
    }
    if id.contains("amazon") || id.contains("amazonmusic") {
        return "amazon".to_string();
    }
    if id.contains("deezer") {
        return "deezer".to_string();
    }
    if id.contains("tidal") {
        return "tidal".to_string();
    }
    if ["chrome", "msedge", "firefox", "brave", "opera", "iexplore", "chromium"]
        .iter()
        .any(|b| id.contains(b))
    {
        return "browser".to_string();
    }
    "other".to_string()
}

fn refine_browser_source(title: &str, artist: &str) -> String {
    let t = title.to_lowercase();
    let a = artist.to_lowercase();
    if t.contains("tiktok") || a.contains("tiktok") {
        return "tiktok".to_string();
    }
    if t.contains("twitch") || a.contains("twitch") {
        return "twitch".to_string();
    }
    if t.contains("soundcloud") || a.contains("soundcloud") {
        return "soundcloud".to_string();
    }
    if t.contains("deezer") || a.contains("deezer") {
        return "deezer".to_string();
    }
    if t.contains("tidal") || a.contains("tidal") {
        return "tidal".to_string();
    }
    if t.contains("youtube") || a.contains("youtube") {
        return "youtube".to_string();
    }
    "browser".to_string()
}

pub fn get_source_meta(source: &str) -> SourceMeta {
    match source {
        "spotify" => SourceMeta {
            icon: "spotify".into(),
            color: "#1DB954".into(),
            label: "Spotify".into(),
        },
        "youtube" => SourceMeta {
            icon: "youtube".into(),
            color: "#FF0000".into(),
            label: "YouTube".into(),
        },
        "twitch" => SourceMeta {
            icon: "twitch".into(),
            color: "#9146FF".into(),
            label: "Twitch".into(),
        },
        "tiktok" => SourceMeta {
            icon: "tiktok".into(),
            color: "#FF0050".into(),
            label: "TikTok".into(),
        },
        "local" => SourceMeta {
            icon: "music".into(),
            color: "#5B9BD5".into(),
            label: "Lecteur Local".into(),
        },
        "soundcloud" => SourceMeta {
            icon: "soundcloud".into(),
            color: "#FF5500".into(),
            label: "SoundCloud".into(),
        },
        "deezer" => SourceMeta {
            icon: "deezer".into(),
            color: "#A238FF".into(),
            label: "Deezer".into(),
        },
        "tidal" => SourceMeta {
            icon: "tidal".into(),
            color: "#00FFFF".into(),
            label: "TIDAL".into(),
        },
        "amazon" => SourceMeta {
            icon: "amazon".into(),
            color: "#FF9900".into(),
            label: "Amazon Music".into(),
        },
        "browser" => SourceMeta {
            icon: "browser".into(),
            color: "#4285F4".into(),
            label: "Navigateur".into(),
        },
        _ => SourceMeta {
            icon: "music".into(),
            color: "#9A22E2".into(),
            label: "Média".into(),
        },
    }
}
