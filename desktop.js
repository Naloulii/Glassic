const { app, BrowserWindow, screen, ipcMain, shell } = require('electron');
const path = require('path');
const os = require('os');
const { MediaDetector } = require('./src/mediaDetector');
const { sendControlAction, cycleSession } = require('./src/smtc');

// Supprimer tous les messages et erreurs de cache / GPU / Chromium dans la console
app.commandLine.appendSwitch('disable-gpu-shader-disk-cache');
app.commandLine.appendSwitch('disable-gpu-program-cache');
app.commandLine.appendSwitch('disable-disk-cache');
app.commandLine.appendSwitch('disable-http-cache');
app.commandLine.appendSwitch('log-level', '3');

// Repertoire userData unique par session pour eviter tout conflit de verrouillage de fichier cache GPU
app.setPath('userData', path.join(os.tmpdir(), `glassic-udata-${process.pid}`));

let mainWindow;
let zoomLevel = 0; // 0 = 100%, 1 = 120%, -1 = 80%, etc.
let isAlwaysOnTop = false;
let webServerActive = false;

// Media detector tournant en local pour l'application Desktop (sans Node server par défaut)
const detector = new MediaDetector({ pollMs: 1500 });
detector.on((media) => {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.executeJavaScript(`if (typeof renderMedia === 'function') renderMedia(${JSON.stringify(media)});`).catch(() => {});
  }
});

function toggleWebServer(enable) {
  if (enable && !webServerActive) {
    webServerActive = true;
    try {
      require('./server.js');
    } catch (e) {
      console.log('Serveur web déjà démarré ou erreur:', e.message);
    }
  }
}

function createWindow() {
  const { width: scrW, height: scrH } = screen.getPrimaryDisplay().workAreaSize;

  mainWindow = new BrowserWindow({
    title: 'Glassic',
    width: 440,
    height: 126,
    x: 30,
    y: Math.max(0, scrH - 156),
    transparent: true,
    frame: false,
    resizable: false,
    alwaysOnTop: isAlwaysOnTop,
    skipTaskbar: false,
    show: true,
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false
    }
  });

  mainWindow.loadFile(path.join(__dirname, 'public', 'widget.html'));
  mainWindow.show();
  mainWindow.focus();

  detector.start();

  mainWindow.webContents.on('before-input-event', (event, input) => {
    if (input.type !== 'keyDown') return;

    if (input.control) {
      if (['+', '=', 'NumpadAdd', 'ArrowUp'].includes(input.key)) {
        zoomLevel += 1;
        mainWindow.webContents.setZoomLevel(zoomLevel);
        const bounds = mainWindow.getBounds();
        mainWindow.setBounds({ width: Math.round(bounds.width * 1.2), height: Math.round(bounds.height * 1.2) });
        event.preventDefault();
      } 
      else if (['-', 'NumpadSubtract', 'ArrowDown'].includes(input.key)) {
        zoomLevel -= 1;
        mainWindow.webContents.setZoomLevel(zoomLevel);
        const bounds = mainWindow.getBounds();
        mainWindow.setBounds({ width: Math.round(bounds.width / 1.2), height: Math.round(bounds.height / 1.2) });
        event.preventDefault();
      }
      else if (input.key === '0') {
        zoomLevel = 0;
        mainWindow.webContents.setZoomLevel(0);
        mainWindow.setSize(560, 200); // Reset fallback
        event.preventDefault();
      }
      else if (input.key.toLowerCase() === 't') {
        isAlwaysOnTop = !isAlwaysOnTop;
        mainWindow.setAlwaysOnTop(isAlwaysOnTop);
        event.preventDefault();
      }
      else if (input.key.toLowerCase() === 'c') {
        mainWindow.webContents.executeJavaScript("document.body.classList.toggle('colorful-mode');");
        event.preventDefault();
      }
      else if (input.key.toLowerCase() === 'g') {
        mainWindow.webContents.executeJavaScript("document.body.classList.toggle('glassy-mode');");
        event.preventDefault();
      }
    } else {
      mainWindow.webContents.executeJavaScript(`if (typeof handleGlobalKeyDown === 'function') handleGlobalKeyDown(${JSON.stringify(input.key)});`).catch(() => {});
    }
  });

  mainWindow.on('closed', function () {
    detector.stop();
    mainWindow = null;
  });
}

let preSettingsBounds = null;

// IPC Handling depuis le widget
ipcMain.on('control-action', (_event, action) => {
  sendControlAction(action);
});

ipcMain.on('cycle-session', (_event, direction) => {
  cycleSession(direction);
  if (detector) detector._poll();
});

ipcMain.on('toggle-web-server', (_event, enable) => {
  toggleWebServer(enable);
});

ipcMain.on('set-always-on-top', (_event, flag) => {
  isAlwaysOnTop = flag;
  if (mainWindow) mainWindow.setAlwaysOnTop(isAlwaysOnTop);
});

ipcMain.on('create-desktop-shortcut', () => {
  try {
    const { exec } = require('child_process');
    const desktopPath = path.join(app.getPath('home'), 'Desktop', 'Glassic.vbs');
    const script = `Set WshShell = CreateObject("WScript.Shell")\nWshShell.CurrentDirectory = "${app.getAppPath()}"\nWshShell.Run "cmd.exe /c start Glassic.exe", 0, False`;
    require('fs').writeFileSync(desktopPath, script);
  } catch (e) {
    console.log('Error creating desktop shortcut:', e.message);
  }
});

ipcMain.on('toggle-auto-start', (_event, enable) => {
  try {
    const startupPath = path.join(app.getPath('appData'), 'Microsoft', 'Windows', 'Start Menu', 'Programs', 'Startup', 'GlassicStartup.vbs');
    if (enable) {
      const script = `Set WshShell = CreateObject("WScript.Shell")\nWshShell.CurrentDirectory = "D:\\Glassic"\nWshShell.Run "D:\\Glassic\\Glassic.exe", 0, False`;
      require('fs').writeFileSync(startupPath, script);
    } else {
      if (require('fs').existsSync(startupPath)) {
        require('fs').unlinkSync(startupPath);
      }
    }
  } catch (e) {
    console.log('Error toggling auto start:', e.message);
  }
});

ipcMain.on('check-update', (event) => {
  const currentVersion = '1.0.0';
  const https = require('https');
  const options = {
    hostname: 'api.github.com',
    path: '/repos/Naloulii/Glassic/releases/latest',
    headers: {
      'User-Agent': 'GlassicApp'
    }
  };

  https.get(options, (res) => {
    let data = '';
    res.on('data', (chunk) => { data += chunk; });
    res.on('end', () => {
      try {
        if (res.statusCode === 200) {
          const release = JSON.parse(data);
          const latestVersion = (release.tag_name || '').replace(/^v/, '');
          if (latestVersion && latestVersion !== currentVersion) {
            event.reply('update-status', {
              hasUpdate: true,
              version: release.tag_name || `v${latestVersion}`,
              url: release.html_url || 'https://github.com/Naloulii/Glassic/releases/latest'
            });
            return;
          }
        }
      } catch (e) {
        console.log('Error parsing update check response:', e.message);
      }
      event.reply('update-status', { hasUpdate: false, version: currentVersion });
    });
  }).on('error', (err) => {
    console.log('Update check error:', err.message);
    event.reply('update-status', { hasUpdate: false, version: currentVersion });
  });
});

ipcMain.on('open-external-url', (_event, url) => {
  if (url && typeof url === 'string') {
    shell.openExternal(url);
  }
});

let currentScalePct = 100;
let isSettingsExpanded = false;
const BASE_W = 440;
const BASE_H = 126;

function applyWindowScale(scalePct) {
  currentScalePct = parseInt(scalePct) || 100;
  if (mainWindow && !mainWindow.isDestroyed()) {
    const factor = currentScalePct / 100;
    mainWindow.webContents.setZoomFactor(factor);
    if (!isSettingsExpanded) {
      const targetW = Math.round(BASE_W * factor);
      const targetH = Math.round(BASE_H * factor);
      const bounds = mainWindow.getBounds();
      mainWindow.setBounds({
        x: bounds.x,
        y: bounds.y,
        width: targetW,
        height: targetH
      });
    }
  }
}

ipcMain.on('set-widget-scale', (_event, scalePct) => {
  applyWindowScale(scalePct);
});

ipcMain.on('expand-settings-window', () => {
  if (mainWindow && !mainWindow.isDestroyed()) {
    isSettingsExpanded = true;
    preSettingsBounds = mainWindow.getBounds();
    mainWindow.webContents.setZoomFactor(1.0);
    const { width: scrW, height: scrH } = screen.getPrimaryDisplay().workAreaSize;
    const targetW = Math.max(820, Math.min(960, Math.round(scrW * 0.5)));
    const targetH = Math.max(560, Math.min(680, Math.round(scrH * 0.6)));
    mainWindow.setBounds({ width: targetW, height: targetH });
    mainWindow.center();
  }
});

ipcMain.on('shrink-settings-window', () => {
  if (mainWindow && !mainWindow.isDestroyed()) {
    isSettingsExpanded = false;
    applyWindowScale(currentScalePct);
  }
});

app.whenReady().then(createWindow);

app.on('window-all-closed', function () {
  detector.stop();
  if (process.platform !== 'darwin') app.quit();
});
