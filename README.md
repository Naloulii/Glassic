# Glassic

Glassic is a lightweight, modern desktop media widget for Windows. It displays media information from Spotify, YouTube, TikTok, web browsers, and local media sessions with real-time playback controls and customizable visual themes.

## Features

- Dynamic media detection for Spotify, Web Browsers, YouTube, TikTok, and Local Media (SMTC).
- Customizable widget scaling (from 50% to 200%).
- Customizable keybindings (default F5) to access settings instantly.
- Theme options including Vinyl mode, Glassy effect, and Cover color backgrounds.
- Web/OBS server integration (Port 3000) for streaming overlays.
- Integrated automatic release update checker linked with GitHub Releases.

## Requirements

- Windows 10 / Windows 11
- Node.js (v18 or later recommended for development)

## Installation and Usage

1. Download Glassic.exe from the Releases section.
2. Run Glassic.exe.
3. Press F5 (or click the settings gear icon on widget hover) to configure settings, scaling, and keybindings.

## Building from Source

To build the executable manually from source:

```bash
cd source
npm install
npm run build
```

The compiled binary will be available in dist/Glassic.exe.

## License

MIT License. Free to use and modify.
