import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// The port `dageist bridge` listens on unless it is given one; `DAGEIST_BRIDGE_PORT` names another
// when the bridge is started with `--port`.
const bridgePort = process.env.DAGEIST_BRIDGE_PORT ?? '7878';

// The names a request to the development server or its preview may carry in its `Host` header.
// The forwarded request carries the bridge's own host, so the bridge cannot tell a page on
// another site from this one; this list is what stops a page that reaches the server under its
// own name, as DNS rebinding does. Vite also always accepts an IP address and a name under
// `.localhost`, which a page cannot point at another machine.
const LOOPBACK_NAMES = ['localhost', '127.0.0.1'];

// The view calls its own origin, and the bridge's path goes to the bridge. The bridge serves only
// a request whose Host is its own loopback address and port, so the proxy sends the target's host
// and not the page's.
const BRIDGE_PROXY = {
  '/bridge': { target: `http://127.0.0.1:${bridgePort}`, changeOrigin: true },
};

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
    allowedHosts: LOOPBACK_NAMES,
    // The page calls its own origin, so no other origin needs a cross-origin answer. Vite
    // would otherwise answer one from any page on localhost, and with it the bridge's path.
    cors: false,
    proxy: BRIDGE_PROXY,
  },
  // `vite preview` serves the built page and forwards the same path, so it holds the same
  // lists.
  preview: {
    allowedHosts: LOOPBACK_NAMES,
    cors: false,
    proxy: BRIDGE_PROXY,
  },
});
