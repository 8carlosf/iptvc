// Channel checks for public playlists: politely probes a filtered list (a few at a time, one request per host,
// spaced), keeping each result for an hour. Never used for providers that count connections.
import { SvelteMap } from 'svelte/reactivity';
import { probe, type Health } from './diagnose';

/** The most channels one check may cover. */
export const CHECK_MAX = 300;
const CONCURRENCY = 6;
/** Between two requests to the same host (ms): some hosts serve hundreds of channels. */
const HOST_GAP = 500;
const TTL = 60 * 60_000;

export type Result = { health: Health; id: number; at: number };
/** By stream URL. Reactive, so badges update as results come in. */
export const results = new SvelteMap<string, Result>();

// Results expire after an hour.
setInterval(() => {
  for (const [url, r] of results) if (Date.now() - r.at > TTL) results.delete(url);
}, 60_000);

/** Results that mean the channel won't play (a "blocked" probe may just be CORS, so it doesn't count). */
export const FAILED = new Set<Health>(['refused', 'offline', 'server', 'unavailable', 'drm', 'timeout']);

const host = (url: string) => { try { return new URL(url).host; } catch { return url; } };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Checks `channels` ({id, url}), reporting progress; stops when `signal` aborts. */
export async function checkAll(channels: { id: number; url: string }[], signal: AbortSignal, progress: (done: number) => void) {
  const queue = [...channels];
  const busy = new Set<string>();
  const last = new Map<string, number>();
  let done = 0;
  const worker = async () => {
    while (queue.length && !signal.aborted) {
      const i = queue.findIndex((c) => !busy.has(host(c.url)));
      if (i < 0) { await sleep(100); continue; }
      const { id, url } = queue.splice(i, 1)[0];
      const h = host(url);
      busy.add(h);
      try {
        const wait = (last.get(h) ?? 0) + HOST_GAP - Date.now();
        if (wait > 0) await sleep(wait);
        const { health } = await probe(url, signal);
        results.set(url, { health, id, at: Date.now() });
      } catch { /* aborted */ } finally {
        last.set(h, Date.now());
        busy.delete(h);
        progress(++done);
      }
    }
  };
  await Promise.all(Array.from({ length: CONCURRENCY }, worker));
}
