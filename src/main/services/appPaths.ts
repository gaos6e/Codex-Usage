import fs from 'fs';
import os from 'os';
import path from 'path';

export const APP_DIRECTORY_NAME = 'CodexUsage';

export interface AppPaths {
  baseDir: string;
  settingsPath: string;
  cacheDir: string;
  cachePath: string;
  logsDir: string;
  logFilePath: string;
  exportsDir: string;
}

export function resolveLocalAppData(): string {
  return process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local');
}

export function resolveAppPaths(): AppPaths {
  const baseDir = path.join(resolveLocalAppData(), APP_DIRECTORY_NAME);
  const cacheDir = path.join(baseDir, 'cache');
  const logsDir = path.join(baseDir, 'logs');
  const exportsDir = path.join(baseDir, 'exports');
  fs.mkdirSync(cacheDir, { recursive: true });
  fs.mkdirSync(logsDir, { recursive: true });
  fs.mkdirSync(exportsDir, { recursive: true });
  return {
    baseDir,
    settingsPath: path.join(baseDir, 'settings.json'),
    cacheDir,
    cachePath: path.join(cacheDir, 'summary-cache.json'),
    logsDir,
    logFilePath: path.join(logsDir, 'main.log'),
    exportsDir,
  };
}
