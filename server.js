require('dotenv').config();
const express = require('express');
const { WebSocketServer } = require('ws');
const cors = require('cors');
const http = require('http');
const path = require('path');
const { MediaDetector } = require('./src/mediaDetector');

const PORT = process.env.PORT || 3000;
const app = express();
const server = http.createServer(app);

// ─── Middleware ────────────────────────────────────────────────────────────────
app.use(cors());
app.use(express.json());
app.use(express.static(path.join(__dirname, 'public')));

// ─── Media Detector (Windows SMTC native exe — no credentials needed) ─────────
const detector = new MediaDetector({
  pollMs: parseInt(process.env.SMTC_POLL_MS || '1500'),
});

// ─── WebSocket Server ──────────────────────────────────────────────────────────
const wss = new WebSocketServer({ server, path: '/ws' });
const clients = new Set();

wss.on('connection', (ws) => {
  clients.add(ws);

  // Send current state immediately on connect
  ws.send(JSON.stringify({ type: 'media', data: detector.currentMedia }));

  ws.on('close', () => clients.delete(ws));
  ws.on('error', () => clients.delete(ws));
});

function broadcast(media) {
  const msg = JSON.stringify({ type: 'media', data: media });
  clients.forEach((ws) => {
    if (ws.readyState === ws.OPEN) ws.send(msg);
  });
}

detector.on((media) => broadcast(media));

// ─── API Routes ────────────────────────────────────────────────────────────────

/** Current media (REST polling fallback for widget) */
app.get('/api/now-playing', (_req, res) => {
  res.json({ data: detector.currentMedia, timestamp: Date.now() });
});

/** Health check */
app.get('/api/health', (_req, res) => {
  res.json({ status: 'ok', uptime: Math.floor(process.uptime()) });
});

/**
 * Convert HEX color (#1DB954) to RGB string (29,185,84) for Rainmeter
 */
function hexToRgb(hex) {
  if (!hex || !hex.startsWith('#')) return '136,136,136';
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return `${r},${g},${b}`;
}

/**
 * Rainmeter-friendly endpoint — JSON without the huge base64 thumbnail field.
 * WebParser can read this cleanly with regex.
 */
app.get('/api/simple', (_req, res) => {
  const m = detector.currentMedia;
  if (!m) {
    return res.json({
      title: '', artist: '', album: '', source: 'none',
      label: '', color: '136,136,136', status: 'stopped',
      position: 0, duration: 0, hasArt: false,
    });
  }
  res.json({
    title:    m.title    || '',
    artist:   m.artist   || '',
    album:    m.album    || '',
    source:   m.source   || 'unknown',
    label:    m.sourceMeta?.label || m.source || '',
    color:    hexToRgb(m.sourceMeta?.color),
    status:   m.status   || 'stopped',
    position: m.position || 0,
    duration: m.duration || 0,
    hasArt:   !!m.thumbnail,
  });
});

/**
 * Album art image endpoint for Rainmeter Image measure.
 * Decodes the base64 thumbnail and serves it as raw image bytes.
 * Rainmeter polls this URL directly in [MeasureArt].
 */
app.get('/api/art.jpg', (_req, res) => {
  const m = detector.currentMedia;
  if (!m || !m.thumbnail) {
    // Return a 1x1 transparent PNG as placeholder
    const placeholder = Buffer.from(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
      'base64'
    );
    res.set('Content-Type', 'image/png');
    res.set('Cache-Control', 'no-cache, no-store, must-revalidate');
    return res.send(placeholder);
  }

  try {
    // thumbnail is "data:image/jpeg;base64,<data>"
    const [header, b64] = m.thumbnail.split(',');
    const mime = header.replace('data:', '').replace(';base64', '');
    const buf = Buffer.from(b64, 'base64');
    res.set('Content-Type', mime || 'image/jpeg');
    res.set('Cache-Control', 'no-cache, no-store, must-revalidate');
    res.send(buf);
  } catch {
    res.status(500).end();
  }
});

// ─── Start ─────────────────────────────────────────────────────────────────────
detector.start();

server.on('error', (e) => {
  if (e.code === 'EADDRINUSE') {
    console.log(`Port ${PORT} is already in use. Assuming server is already running in another process.`);
  } else {
    console.error(e);
  }
});

server.listen(PORT, () => {
  console.log('');
  console.log('  🎵  Music Widget — Now Playing');
  console.log('  ───────────────────────────────────────────');
  console.log(`  Widget :   http://localhost:${PORT}/widget.html`);
  console.log(`  API    :   http://localhost:${PORT}/api/now-playing`);
  console.log('');
  console.log('  Sources : Spotify · YouTube · Twitch · Local (via SMTC)');
  console.log('  OBS Browser Source → http://localhost:' + PORT + '/widget.html');
  console.log('  ───────────────────────────────────────────');
  console.log('');
});
