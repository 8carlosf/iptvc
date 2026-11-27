// Whether a channel's stream answers, and why not: used to explain a failed channel, and by the channel check.
// Most failures are the channel (offline, region-locked), not the player, and the wording should say which.

export type Health =
  | 'ok' // answers with a stream (it may still be in a format the browser can't play)
  | 'refused' // 401/403: region-locked or subscription
  | 'offline' // 404/410
  | 'server' // 5xx
  | 'unavailable' // another status, or an answer that isn't a stream
  | 'drm' // DASH with ContentProtection
  | 'timeout' // no answer in time
  | 'blocked'; // failed at once: offline, or a server that doesn't let pages read it (CORS); it may still play

export type Probe = { health: Health; status?: number };

/** Each request gives up after this long: a server that never answers is the likeliest failure of all. */
const TIMEOUT = 8_000;

const get = (url: string | URL, signal?: AbortSignal) =>
  fetch(url, { signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(TIMEOUT)]) : AbortSignal.timeout(TIMEOUT) });

function byStatus(status: number): Probe {
  if (status === 401 || status === 403) return { health: 'refused', status };
  if (status === 404 || status === 410) return { health: 'offline', status };
  return { health: status >= 500 ? 'server' : 'unavailable', status };
}

/** Checks a stream (an HLS playlist and its first variant, a DASH manifest) without downloading any video. */
export async function probe(url: string, signal?: AbortSignal): Promise<Probe> {
  try {
    const r = await get(url, signal);
    if (!r.ok) return void r.body?.cancel(), byStatus(r.status);
    const type = r.headers.get('content-type') ?? '';
    if (/\.mpd(\?|$)/i.test(url) || /dash/i.test(type)) {
      return { health: /<ContentProtection/i.test(await r.text()) ? 'drm' : 'ok' };
    }
    if (!/\.m3u8(\?|$)/i.test(url) && !/mpegurl/i.test(type)) return void r.body?.cancel(), { health: 'ok' };
    const text = await r.text();
    if (!text.trimStart().startsWith('#EXTM3U')) return { health: 'unavailable' };
    const variant = text.includes('#EXT-X-STREAM-INF') && text.split(/\r?\n/).find((l) => l.trim() && !l.startsWith('#'));
    if (variant) {
      const v = await get(new URL(variant.trim(), r.url), signal);
      void v.body?.cancel();
      if (!v.ok) return byStatus(v.status);
    }
    return { health: 'ok' };
  } catch (e) {
    if (signal?.aborted) throw e;
    return { health: e instanceof DOMException && e.name === 'TimeoutError' ? 'timeout' : 'blocked' };
  }
}

export type Diagnosis = { title: string; detail: string };

/** The HTTP error status in a player error ("HTTP 403", mediabunny's "Error fetching …: 403 Forbidden"). */
export const statusOf = (message: string) => Number(/(?:HTTP |: )([45]\d\d)\b/.exec(message)?.[1]) || undefined;

/**
 * A failed channel, explained. `seen` is an error status the player itself got: that wins over a fresh probe,
 * which can catch a flaky server (a shared, rate-limited token) on a good moment and blame the format.
 */
export async function diagnose(url: string, seen?: number): Promise<Diagnosis> {
  const { health, status } = seen ? byStatus(seen) : await probe(url);
  switch (health) {
    case 'refused': return { title: 'Access refused', detail: `The stream answered ${status}: it may be region-locked, need a subscription, or be turning viewers away.` };
    case 'offline': return { title: 'Channel offline', detail: `The stream answered ${status}: it isn’t broadcasting at this address anymore.` };
    case 'server': return { title: 'Server problem', detail: `The stream’s server answered ${status}. It may come back later.` };
    case 'drm': return { title: 'Protected channel', detail: 'This channel is encrypted (DRM), which this player can’t decrypt.' };
    case 'timeout': return { title: 'Channel unreachable', detail: 'Its server didn’t answer within 8 seconds. It’s probably offline.' };
    case 'blocked': return { title: 'Channel unreachable', detail: 'Its server didn’t answer, or doesn’t allow browsers to fetch it. It may be offline.' };
    case 'unavailable': return { title: 'Channel unavailable', detail: status ? `The stream answered ${status}.` : 'Its address answered, but not with a stream.' };
    case 'ok': return { title: 'Can’t play this stream', detail: 'The stream is reachable, but its format isn’t playable in this browser.' };
  }
}
