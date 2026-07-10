import path from 'node:path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  root: path.resolve(__dirname, 'ui'),
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    outDir: path.resolve(__dirname, 'dist'),
    emptyOutDir: true,
    sourcemap: false,
    target: ['es2022', 'chrome120'],
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'ui/src'),
    },
  },
});
