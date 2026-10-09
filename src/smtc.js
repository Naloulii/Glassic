/**
 * SMTC (System Media Transport Controls) module
 * Uses a native C# exe (same technique as the Stream Deck plugin)
 * to read ALL Windows media sessions:
 *   - Spotify desktop
 *   - YouTube / Twitch / SoundCloud via browser (Chrome, Edge, Firefox…)
 *   - VLC, Windows Media Player, Groove, AIMP, foobar2000…
 *   - Any app that registers with Windows media controls
 *
 * NO Spotify API / credentials needed — reads directly from the OS.
 */

const { execFile } = require('child_process');
const path = require('path');

// Native exe (built from smtc-reader/ C# project)
// Falls back to dotnet run if exe not found
const EXE_PATH = path.join(__dirname, '..', 'scripts', 'smtc-reader.exe');
const PROJECT_PATH = path.join(__dirname, '..', 'smtc-reader');

// ── Source detection ──────────────────────────────────────────────────────────
function detectSource(appId) {
  const id = (appId || '').toLowerCase();

  if (id.includes('spotify'))                                       return 'spotify';
  if (id.includes('vlc'))                                          return 'local';
  if (id.includes('wmplayer') || id.includes('windows.media'))     return 'local';
  if (id.includes('groove')   || id.includes('music.ui'))          return 'local';
  if (id.includes('foobar')   || id.includes('aimp'))              return 'local';
  if (id.includes('winamp'))                                        return 'local';
  if (id.includes('musicbee'))                                      return 'local';
  if (id.includes('amazon')   || id.includes('amazonmusic'))       return 'amazon';
  if (id.includes('deezer'))                                        return 'deezer';
  if (id.includes('tidal'))                                         return 'tidal';

  // Browsers — we need the window title to distinguish YouTube vs Twitch etc.
  // The exe provides the appId but not the window title for browser sessions.
  // We return 'browser' and let the title-based re-detection happen in enrichSession().
  if (id.match(/chrome|msedge|firefox|brave|opera|iexplore|chromium/)) {
    return 'browser';
  }

  return 'other';
}

// Refine browser sources by track title / artist patterns
function refineBrowserSource(title, artist) {
  const t = (title  || '').toLowerCase();
  const a = (artist || '').toLowerCase();

  if (t.includes('tiktok') || a.includes('tiktok'))      return 'tiktok';
  if (t.includes('twitch') || a.includes('twitch'))      return 'twitch';
  if (t.includes('soundcloud') || a.includes('soundcloud')) return 'soundcloud';
  if (t.includes('deezer') || a.includes('deezer'))      return 'deezer';
  if (t.includes('tidal')  || a.includes('tidal'))       return 'tidal';
  if (t.includes('youtube') || a.includes('youtube'))    return 'youtube';
  // Default browser media
  return 'other';
}

// ── Source metadata (icon, color, label) ─────────────────────────────────────
const SOURCE_META = {
  spotify:    { icon: 'spotify',    color: '#1DB954', label: 'Spotify' },
  youtube:    { icon: 'youtube',    color: '#FF0000', label: 'YouTube' },
  twitch:     { icon: 'twitch',     color: '#9146FF', label: 'Twitch' },
  tiktok:     { icon: 'tiktok',     color: '#000000', label: 'TikTok' },
  local:      { icon: 'music',      color: '#5B9BD5', label: 'Lecteur Local' },
  soundcloud: { icon: 'soundcloud', color: '#FF5500', label: 'SoundCloud' },
  deezer:     { icon: 'deezer',     color: '#00C7F2', label: 'Deezer' },
  tidal:      { icon: 'tidal',      color: '#00FFFF', label: 'TIDAL' },
  amazon:     { icon: 'amazon',     color: '#FF9900', label: 'Amazon Music' },
  browser:    { icon: 'browser',    color: '#4285F4', label: 'Navigateur' },
  other:      { icon: 'music',      color: '#888888', label: 'Autre' },
  unknown:    { icon: 'music',      color: '#888888', label: 'Inconnu' },
};

function enrichSession(raw) {
  let source = detectSource(raw.appId);

  // Refine browser sessions by title/artist
  if (source === 'browser') {
    source = refineBrowserSource(raw.title, raw.artist);
  }

  return {
    ...raw,
    source,
    sourceMeta: SOURCE_META[source] || SOURCE_META.unknown,
  };
}

// ── Run the native exe ────────────────────────────────────────────────────────
function runReader() {
  return new Promise((resolve) => {
    // Try the pre-built exe first
    execFile(
      EXE_PATH,
      [],
      { timeout: 6000, maxBuffer: 20 * 1024 * 1024 },
      (err, stdout) => {
        if (!err) {
          resolve(stdout.trim());
          return;
        }

        // Fallback: dotnet run (slower but works without the exe)
        execFile(
          'dotnet',
          ['run', '--project', PROJECT_PATH, '-c', 'Release', '--no-build'],
          { timeout: 15000, maxBuffer: 20 * 1024 * 1024 },
          (err2, stdout2) => {
            resolve(err2 ? '' : stdout2.trim());
          }
        );
      }
    );
  });
}

/**
 * Query Windows SMTC via native exe
 * @returns {Promise<{sessions: Array, error: string|null}>}
 */
async function getSMTCSessions() {
  const raw = await runReader();

  if (!raw) return { sessions: [], error: 'SMTC reader returned no output' };

  try {
    const data = JSON.parse(raw);
    const enriched = (data.sessions || []).map(enrichSession);
    return { sessions: enriched, error: data.error || null };
  } catch (e) {
    return { sessions: [], error: `JSON parse error: ${e.message}` };
  }
}

let activeSessionIndex = 0;

/**
 * Send media control commands (playpause, next, previous, volup, voldown)
 */
function sendControlAction(action) {
  const act = (action || '').toLowerCase();

  if (act === 'volup') {
    execFile('powershell', ['-c', '(New-Object -ComObject WScript.Shell).SendKeys([char]175)']);
    return;
  }
  if (act === 'voldown') {
    execFile('powershell', ['-c', '(New-Object -ComObject WScript.Shell).SendKeys([char]174)']);
    return;
  }

  // SMTC native actions: playpause, next, previous
  if (['playpause', 'play', 'pause', 'next', 'previous'].includes(act)) {
    execFile(EXE_PATH, [act], () => {});
  }
}

/**
 * Change session index (Up/Down arrow navigation)
 */
function cycleSession(direction) {
  if (direction === 'up') activeSessionIndex--;
  else if (direction === 'down') activeSessionIndex++;
}

/**
 * Get active session with multi-session awareness
 */
async function getActiveSession() {
  const { sessions, error } = await getSMTCSessions();

  if (!sessions || sessions.length === 0) {
    activeSessionIndex = 0;
    return { session: null, totalSessions: 0, sessionIndex: 0, error };
  }

  // Ensure index stays in bounds
  if (activeSessionIndex < 0) activeSessionIndex = sessions.length - 1;
  if (activeSessionIndex >= sessions.length) activeSessionIndex = 0;

  const session = sessions[activeSessionIndex] || sessions[0];

  return {
    session: {
      ...session,
      totalSessions: sessions.length,
      sessionIndex: activeSessionIndex
    },
    error: null
  };
}

module.exports = { getSMTCSessions, getActiveSession, sendControlAction, cycleSession, SOURCE_META };
