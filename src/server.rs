use crate::smtc::MediaInfo;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tungstenite::protocol::WebSocket;
use tungstenite::Message;

const WIDGET_HTML: &str = include_str!("../public/widget.html");

const TRANSPARENT_PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x60, 0x00, 0x02, 0x00,
    0x00, 0x05, 0x00, 0x01, 0xE2, 0x26, 0x05, 0x9B, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44,
    0xAE, 0x42, 0x60, 0x82,
];

struct SharedServerState {
    current_media: Option<MediaInfo>,
    obs_config: serde_json::Value,
    current_lyric: Option<(String, Option<String>)>,
    ws_clients: Vec<WebSocket<TcpStream>>,
}

pub struct ObsWebServer {
    state: Arc<Mutex<SharedServerState>>,
    enabled: Arc<AtomicBool>,
    running_port: Arc<Mutex<u16>>,
}

impl ObsWebServer {
    pub fn new(initial_obs_config: serde_json::Value, port: u16, enabled: bool) -> Self {
        let srv = Self {
            state: Arc::new(Mutex::new(SharedServerState {
                current_media: None,
                obs_config: initial_obs_config,
                current_lyric: None,
                ws_clients: Vec::new(),
            })),
            enabled: Arc::new(AtomicBool::new(enabled)),
            running_port: Arc::new(Mutex::new(port)),
        };
        srv.spawn_listener();
        srv
    }

    pub fn set_enabled(&self, enable: bool, port: u16) {
        self.enabled.store(enable, Ordering::SeqCst);
        if let Ok(mut p) = self.running_port.lock() {
            *p = port;
        }
    }

    pub fn update_media(&self, media: Option<MediaInfo>) {
        if !self.enabled.load(Ordering::Relaxed) {
            if let Ok(mut guard) = self.state.lock() {
                guard.current_media = media;
            }
            return;
        }

        let payload = serde_json::json!({
            "type": "media",
            "data": media,
        })
        .to_string();

        if let Ok(mut guard) = self.state.lock() {
            guard.current_media = media;
            if !guard.ws_clients.is_empty() {
                Self::broadcast_raw(&mut guard.ws_clients, &payload);
            }
        }
    }

    pub fn update_obs_config(&self, cfg: serde_json::Value) {
        if !self.enabled.load(Ordering::Relaxed) {
            if let Ok(mut guard) = self.state.lock() {
                guard.obs_config = cfg;
            }
            return;
        }

        let payload = serde_json::json!({
            "type": "config",
            "data": cfg,
        })
        .to_string();

        if let Ok(mut guard) = self.state.lock() {
            guard.obs_config = cfg;
            if !guard.ws_clients.is_empty() {
                Self::broadcast_raw(&mut guard.ws_clients, &payload);
            }
        }
    }

    pub fn update_lyric(&self, pair: Option<(String, Option<String>)>) {
        if let Ok(mut guard) = self.state.lock() {
            if guard.current_lyric == pair {
                return;
            }
            guard.current_lyric = pair.clone();
            if !self.enabled.load(Ordering::Relaxed) || guard.ws_clients.is_empty() {
                return;
            }
            let (curr, next) = match &pair {
                Some((c, n)) => (c.clone(), n.clone().unwrap_or_default()),
                None => (String::new(), String::new()),
            };
            let payload = serde_json::json!({
                "type": "lyrics",
                "line": curr,
                "next": next,
            })
            .to_string();
            Self::broadcast_raw(&mut guard.ws_clients, &payload);
        }
    }

    fn broadcast_raw(clients: &mut Vec<WebSocket<TcpStream>>, msg: &str) {
        clients.retain_mut(|ws| ws.send(Message::Text(msg.to_string().into())).is_ok());
    }

    fn spawn_listener(&self) {
        let state = self.state.clone();
        let enabled = self.enabled.clone();
        let port_lock = self.running_port.clone();

        thread::spawn(move || {
            let mut bound_port: Option<u16> = None;
            let mut listener: Option<TcpListener> = None;

            loop {
                if !enabled.load(Ordering::SeqCst) {
                    listener = None;
                    bound_port = None;
                    thread::sleep(Duration::from_millis(250));
                    continue;
                }

                let target_port = *port_lock.lock().unwrap_or_else(|e| e.into_inner());
                if bound_port != Some(target_port) || listener.is_none() {
                    listener = TcpListener::bind(("127.0.0.1", target_port)).ok();
                    if let Some(ref l) = listener {
                        let _ = l.set_nonblocking(true);
                        bound_port = Some(target_port);
                    } else {
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                }

                if let Some(ref l) = listener {
                    match l.accept() {
                        Ok((stream, _addr)) => {
                            let state_clone = state.clone();
                            thread::spawn(move || {
                                handle_connection(stream, state_clone);
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // Also flush/service any incoming pings on existing WS clients
                            if let Ok(mut guard) = state.lock() {
                                guard.ws_clients.retain_mut(|ws| match ws.read() {
                                    Ok(Message::Ping(d)) => ws.send(Message::Pong(d)).is_ok(),
                                    Ok(Message::Close(_)) => false,
                                    Ok(_) => true,
                                    Err(tungstenite::Error::Io(ref io_e))
                                        if io_e.kind() == std::io::ErrorKind::WouldBlock =>
                                    {
                                        true
                                    }
                                    Err(_) => false,
                                });
                            }
                            thread::sleep(Duration::from_millis(25));
                        }
                        Err(_) => {
                            thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            }
        });
    }
}

fn handle_connection(mut stream: TcpStream, state: Arc<Mutex<SharedServerState>>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));

    let mut peek_buf = [0u8; 2048];
    let n = match stream.peek(&mut peek_buf) {
        Ok(n) if n > 0 => n,
        _ => return,
    };
    let req_preview = String::from_utf8_lossy(&peek_buf[..n]);
    let req_lower = req_preview.to_lowercase();

    // Check if this is a WebSocket upgrade request
    if req_lower.contains("upgrade: websocket") {
        if let Ok(mut ws) = tungstenite::accept(stream) {
            if let Ok(mut guard) = state.lock() {
                let media_msg = serde_json::json!({
                    "type": "media",
                    "data": guard.current_media,
                })
                .to_string();
                let cfg_msg = serde_json::json!({
                    "type": "config",
                    "data": guard.obs_config,
                })
                .to_string();
                let (curr_l, next_l) = match &guard.current_lyric {
                    Some((c, n)) => (c.clone(), n.clone().unwrap_or_default()),
                    None => (String::new(), String::new()),
                };
                let lyric_msg = serde_json::json!({
                    "type": "lyrics",
                    "line": curr_l,
                    "next": next_l,
                })
                .to_string();

                let _ = ws.send(Message::Text(media_msg.into()));
                let _ = ws.send(Message::Text(cfg_msg.into()));
                let _ = ws.send(Message::Text(lyric_msg.into()));

                let _ = ws.get_mut().set_nonblocking(true);
                guard.ws_clients.push(ws);
            }
        }
        return;
    }

    // Standard HTTP request
    let mut read_buf = [0u8; 4096];
    let read_len = match stream.read(&mut read_buf) {
        Ok(n) if n > 0 => n,
        _ => return,
    };
    let req_str = String::from_utf8_lossy(&read_buf[..read_len]);
    let first_line = req_str.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/");

    if method == "OPTIONS" {
        let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(resp.as_bytes());
        return;
    }

    match (method, path) {
        ("GET", "/") => {
            let resp = "HTTP/1.1 302 Found\r\nLocation: /widget.html\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(resp.as_bytes());
        }
        ("GET", "/widget.html") => {
            write_http_response(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                WIDGET_HTML.as_bytes(),
            );
        }
        ("GET", "/api/now-playing") => {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let body = if let Ok(guard) = state.lock() {
                serde_json::json!({
                    "data": guard.current_media,
                    "timestamp": ts,
                })
                .to_string()
            } else {
                "{}".to_string()
            };
            write_http_response(&mut stream, "200 OK", "application/json", body.as_bytes());
        }
        ("GET", "/api/config") => {
            let body = if let Ok(guard) = state.lock() {
                guard.obs_config.to_string()
            } else {
                "{}".to_string()
            };
            write_http_response(&mut stream, "200 OK", "application/json", body.as_bytes());
        }
        ("GET", "/api/health") => {
            let body = r#"{"status":"ok"}"#;
            write_http_response(&mut stream, "200 OK", "application/json", body.as_bytes());
        }
        ("GET", "/api/simple") => {
            let body = if let Ok(guard) = state.lock() {
                if let Some(ref m) = guard.current_media {
                    let [r, g, b] = m.dominant_rgb;
                    serde_json::json!({
                        "title": m.title,
                        "artist": m.artist,
                        "album": m.album,
                        "source": m.source,
                        "label": m.source_meta.label,
                        "color": format!("{r},{g},{b}"),
                        "status": m.status,
                        "position": m.position,
                        "duration": m.duration,
                        "hasArt": m.raw_art_bytes.is_some(),
                    })
                    .to_string()
                } else {
                    serde_json::json!({
                        "title": "",
                        "artist": "",
                        "album": "",
                        "source": "none",
                        "label": "",
                        "color": "136,136,136",
                        "status": "stopped",
                        "position": 0,
                        "duration": 0,
                        "hasArt": false,
                    })
                    .to_string()
                }
            } else {
                "{}".to_string()
            };
            write_http_response(&mut stream, "200 OK", "application/json", body.as_bytes());
        }
        ("GET", "/api/art.jpg") => {
            let art_bytes = state
                .lock()
                .ok()
                .and_then(|g| g.current_media.as_ref().and_then(|m| m.raw_art_bytes.clone()));
            if let Some(bytes) = art_bytes {
                write_http_response(&mut stream, "200 OK", "image/jpeg", &bytes);
            } else {
                write_http_response(&mut stream, "200 OK", "image/png", TRANSPARENT_PNG_1X1);
            }
        }
        _ => {
            write_http_response(&mut stream, "404 Not Found", "text/plain", b"Not Found");
        }
    }
}

fn write_http_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-cache, no-store, must-revalidate\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}
