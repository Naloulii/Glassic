/**
 * Media Detector — single source via Windows SMTC (native exe)
 *
 * No Spotify API credentials needed.
 * Reads ALL media from Windows directly:
 *   Spotify, YouTube, Twitch, VLC, WMP, local players…
 *
 * Same approach as the Stream Deck "Current Media" plugin.
 */

const { getActiveSession } = require('./smtc');

class MediaDetector {
  constructor(config = {}) {
    this.currentMedia = null;
    this.listeners = new Set();
    this._interval = null;
    this._lastMediaKey = null;
    this.pollMs = config.pollMs || 1500;
  }

  /** Start polling */
  start() {
    this._poll();
    this._interval = setInterval(() => this._poll(), this.pollMs);
  }

  stop() {
    if (this._interval) {
      clearInterval(this._interval);
      this._interval = null;
    }
  }

  /** Subscribe to media change events */
  on(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  _emit(media) {
    const key = media
      ? `${media.source}|${media.title}|${media.artist}|${media.status}|${media.position}`
      : 'null';

    // Only emit if something actually changed (source, title, artist, status, sessions)
    const changeKey = media
      ? `${media.source}|${media.title}|${media.artist}|${media.status}|${media.totalSessions}|${media.sessionIndex}`
      : 'null';

    if (changeKey === this._lastMediaKey) {
      // Position updated only — update stored media silently without broadcasting
      if (media) this.currentMedia = media;
      return;
    }

    this._lastMediaKey = changeKey;
    this.currentMedia = media;
    this.listeners.forEach((fn) => fn(media));
  }

  async _poll() {
    try {
      const { session } = await getActiveSession();
      this._emit(session || null);
    } catch {
      // silently ignore
    }
  }

  // Stubs kept for API compatibility with server.js
  get spotifyAuthUrl()      { return null; }
  get spotifyAuthenticated() { return false; }
  async handleSpotifyCallback() { throw new Error('Not used — Spotify handled via SMTC'); }
}

module.exports = { MediaDetector };
