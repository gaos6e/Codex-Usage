import type { CodexUsageApi } from '../shared/contracts';

declare global {
  interface Window {
    codexUsage: CodexUsageApi;
  }
}

export {};
