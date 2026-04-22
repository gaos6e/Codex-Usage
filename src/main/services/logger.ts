import log from 'electron-log/main';
import type { AppPaths } from './appPaths';

export function configureLogger(paths: AppPaths): typeof log {
  log.initialize();
  log.transports.file.resolvePathFn = () => paths.logFilePath;
  log.transports.file.level = 'info';
  log.transports.console.level = process.env.NODE_ENV === 'development' ? 'debug' : 'warn';
  return log;
}
