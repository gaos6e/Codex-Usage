import path from 'node:path';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'ui/src'),
    },
  },
  test: {
    environment: 'jsdom',
    include: ['ui/src/**/*.test.{ts,tsx}'],
    setupFiles: ['ui/src/test/setup.ts'],
    restoreMocks: true,
  },
});
