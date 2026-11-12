use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub time_secs: f64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct TrackLyrics {
    pub track_key: String,
    pub synced_lines: Vec<LyricLine>,
    pub plain_fallback: Option<String>,
    pub loading: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcLibEntry {
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
}

pub struct LyricsManager {
    state: Arc<Mutex<TrackLyrics>>,
}

impl LyricsManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(TrackLyrics::default())),
        }
    }

    pub fn request_for_track(&self, title: &str, artist: &str) {
        let clean_title = clean_query(title);
        let clean_artist = clean_query(artist);
        if clean_title.is_empty() {
            return;
        }
        let key = format!("{clean_title}|{clean_artist}");

        {
            let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if guard.track_key == key {
                return;
            }
            guard.track_key = key.clone();
            guard.synced_lines.clear();
            guard.plain_fallback = None;
            guard.loading = true;
        }

        let state_clone = self.state.clone();
        thread::spawn(move || {
            let query = if clean_artist.is_empty() {
                clean_title.clone()
            } else {
                format!("{clean_title} {clean_artist}")
            };
            let encoded = url_encode(&query);
            let url = format!("https://lrclib.net/api/search?q={encoded}");

            let resp = ureq::get(&url)
                .set("User-Agent", "Glassic/2.0.0 (https://github.com/Naloulii/Glassic)")
                .timeout(Duration::from_secs(5))
                .call();

            let mut parsed_lines = Vec::new();
            let mut fallback = None;

            if let Ok(r) = resp {
                if let Ok(entries) = r.into_json::<Vec<LrcLibEntry>>() {
                    if let Some(best) = entries
                        .iter()
                        .find(|e| e.synced_lyrics.as_ref().map(|s| !s.is_empty()).unwrap_or(false))
                        .or_else(|| entries.first())
                    {
                        if let Some(ref lrc) = best.synced_lyrics {
                            parsed_lines = parse_lrc(lrc);
                        }
                        if parsed_lines.is_empty() {
                            if let Some(ref plain) = best.plain_lyrics {
                                fallback = plain
                                    .lines()
                                    .map(str::trim)
                                    .find(|l| !l.is_empty())
                                    .map(|s| s.to_string());
                            }
                        }
                    }
                }
            }

            if let Ok(mut guard) = state_clone.lock() {
                if guard.track_key == key {
                    guard.synced_lines = parsed_lines;
                    guard.plain_fallback = fallback;
                    guard.loading = false;
                }
            }
        });
    }

    /// Returns `(current_line, optional_next_line)` for Spotify-style lyrics display
    pub fn current_and_next_lines(&self, pos_secs: f64) -> Option<(String, Option<String>)> {
        let guard = self.state.lock().ok()?;
        if !guard.synced_lines.is_empty() {
            let mut active_idx: Option<usize> = None;
            for (idx, item) in guard.synced_lines.iter().enumerate() {
                if item.time_secs <= pos_secs + 0.35 {
                    active_idx = Some(idx);
                } else {
                    break;
                }
            }
            if let Some(i) = active_idx {
                let curr = guard.synced_lines[i].text.trim().to_string();
                let next = guard
                    .synced_lines
                    .get(i + 1)
                    .map(|l| l.text.trim().to_string())
                    .filter(|s| !s.is_empty());
                if !curr.is_empty() {
                    return Some((curr, next));
                }
            } else if let Some(first) = guard.synced_lines.first() {
                // Before first lyric starts, show upcoming first line dimly
                return Some(("♪ …".to_string(), Some(first.text.trim().to_string())));
            }
            None
        } else {
            guard.plain_fallback.clone().map(|p| (p, None))
        }
    }
}

fn clean_query(input: &str) -> String {
    let mut s = input.to_string();
    for suffix in [
        " - YouTube",
        " - YouTube Music",
        " - Twitch",
        " (Official Video)",
        " [Official Video]",
        " (Official Audio)",
        " (Clip Officiel)",
    ] {
        if let Some(idx) = s.find(suffix) {
            s.truncate(idx);
        }
    }
    s.trim().to_string()
}

fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn parse_lrc(lrc: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    for raw_line in lrc.lines() {
        let line = raw_line.trim();
        if !line.starts_with('[') {
            continue;
        }
        if let Some(close_idx) = line.find(']') {
            let tag = &line[1..close_idx];
            let text = line[close_idx + 1..].trim();
            if let Some(secs) = parse_time_tag(tag) {
                if !text.is_empty() {
                    lines.push(LyricLine {
                        time_secs: secs,
                        text: text.to_string(),
                    });
                }
            }
        }
    }
    lines.sort_by(|a, b| {
        a.time_secs
            .partial_cmp(&b.time_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    lines
}

fn parse_time_tag(tag: &str) -> Option<f64> {
    let mut parts = tag.splitn(2, ':');
    let mins: f64 = parts.next()?.parse().ok()?;
    let secs: f64 = parts.next()?.replace(',', ".").parse().ok()?;
    Some(mins * 60.0 + secs)
}
