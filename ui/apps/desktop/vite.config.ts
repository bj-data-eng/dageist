import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Vite reads its configuration from this file's default export.
// oxlint-disable-next-line import/no-default-export
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Keep Vite's cache beside the workspace's other tooling caches.
  cacheDir: '../../node_modules/.vite',
  build: {
    // The Tauri host embeds ui/dist, so the bundle is written there.
    outDir: '../../dist',
    emptyOutDir: true,
    target: 'es2022',
  },
  server: {
    port: 5173,
    strictPort: true,
  },
});
