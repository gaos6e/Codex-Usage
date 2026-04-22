import { app, BrowserWindow, shell } from 'electron';
import started from 'electron-squirrel-startup';
import { resolveAppPaths } from './services/appPaths';
import { configureLogger } from './services/logger';
import { SettingsStore } from './services/settings';
import { UsageService } from './services/usageService';
import { registerIpcHandlers } from './ipc';

declare const MAIN_WINDOW_WEBPACK_ENTRY: string;
declare const MAIN_WINDOW_PRELOAD_WEBPACK_ENTRY: string;

if (started) {
  app.quit();
}

const paths = resolveAppPaths();
app.setPath('userData', paths.baseDir);
app.setPath('logs', paths.logsDir);
app.setAppLogsPath(paths.logsDir);

const logger = configureLogger(paths);
const settingsStore = new SettingsStore(paths);
const usageService = new UsageService(paths, () => settingsStore.load());
registerIpcHandlers(paths, settingsStore, usageService);

const createWindow = async (): Promise<void> => {
  const mainWindow = new BrowserWindow({
    width: 1280,
    height: 780,
    minWidth: 960,
    minHeight: 640,
    backgroundColor: '#f5f5f7',
    title: 'Codex Usage',
    webPreferences: {
      preload: MAIN_WINDOW_PRELOAD_WEBPACK_ENTRY,
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
    },
  });

  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith('https://') || url.startsWith('http://')) {
      shell.openExternal(url);
    }
    return { action: 'deny' };
  });

  mainWindow.webContents.on('will-navigate', (event, url) => {
    if (url !== MAIN_WINDOW_WEBPACK_ENTRY) {
      event.preventDefault();
    }
  });

  await mainWindow.loadURL(MAIN_WINDOW_WEBPACK_ENTRY);

  if (!app.isPackaged) {
    mainWindow.webContents.openDevTools({ mode: 'detach' });
  }
};

app.whenReady().then(() => {
  app.setAppUserModelId('com.local.codexusage');
  createWindow().catch((error) => logger.error('Window creation failed', error));
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

app.on('activate', () => {
  if (BrowserWindow.getAllWindows().length === 0) {
    createWindow().catch((error) => logger.error('Window recreation failed', error));
  }
});
