import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// The port `dageist bridge` listens on unless it is given one; `DAGEIST_BRIDGE_PORT` names another
// when the bridge is started with `--port`.
const bridgePort = process.env.DAGEIST_BRIDGE_PORT ?? '7878';

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
    // The page calls its own origin, so no other origin needs a cross-origin answer. Vite
    // would otherwise answer one from any page on localhost, and with it the bridge's path.
    cors: false,
    proxy: {
      // The view calls its own origin, and the bridge's path goes to the bridge. The bridge
      // serves only a request whose Host is its own loopback address and port, so the proxy
      // sends the target's host and not the page's.
      '/bridge': { target: `http://127.0.0.1:${bridgePort}`, changeOrigin: true },
    },
  },
});
