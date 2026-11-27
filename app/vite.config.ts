import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// Dev only: `IPTVC_PROXY=http://host:port mise run dev`, then sign in with server `/xt`. For providers that send
// no CORS header on movie/series files, which only the wasm player (AC-3/DTS/MKV files) needs.
const target = process.env.IPTVC_PROXY;

export default defineConfig({
  base: './',
  plugins: [svelte()],
  // The player chunk inlines mediabunny's AC-3/DTS wasm; it's loaded on first play, not with the app.
  build: { target: 'es2022', chunkSizeWarningLimit: 4000 },
  server: target ? { proxy: { '/xt': { target, changeOrigin: true, rewrite: (p) => p.replace(/^\/xt/, '') } } } : {},
});
