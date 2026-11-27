// Why a channel won't play, checked once the player has given up: most failures are the channel (offline,
// region-locked), not the player, and the message should say which.

export type Diagnosis = { title: string; detail: string };

function byStatus(status: number): Diagnosis {
  if (status === 401 || status === 403)
    return { title: 'Access refused', detail: `The stream answered ${status}: it may be region-locked or need a subscription.` };
  if (status === 404 || status === 410)
    return { title: 'Channel offline', detail: `The stream answered ${status}: it isn’t broadcasting at this address anymore.` };
  if (status >= 500) return { title: 'Server problem', detail: `The stream’s server answered ${status}. It may come back later.` };
  return { title: 'Channel unavailable', detail: `The stream answered ${status}.` };
}

/** Each check gives up after this long: a server that never answers is the likeliest failure of all. */
const TIMEOUT = 10_000;

/** Checks the stream (and, for HLS, its first variant) without downloading it. */
export async function diagnose(url: string): Promise<Diagnosis> {
  try {
    const r = await fetch(url, { signal: AbortSignal.timeout(TIMEOUT) });
    if (!r.ok) return void r.body?.cancel(), byStatus(r.status);
    const playlist = /\.m3u8(\?|$)/i.test(url) || /mpegurl/i.test(r.headers.get('content-type') ?? '');
    if (!playlist) {
      void r.body?.cancel();
    } else {
      const text = await r.text();
      if (!text.trimStart().startsWith('#EXTM3U'))
        return { title: 'Channel unavailable', detail: 'Its address answered, but not with a stream.' };
      const variant = text.includes('#EXT-X-STREAM-INF') && text.split(/\r?\n/).find((l) => l.trim() && !l.startsWith('#'));
      if (variant) {
        const v = await fetch(new URL(variant.trim(), r.url), { signal: AbortSignal.timeout(TIMEOUT) });
        void v.body?.cancel();
        if (!v.ok) return byStatus(v.status);
      }
    }
    return { title: 'Can’t play this stream', detail: 'The stream is reachable, but its format isn’t playable in this browser.' };
  } catch (e) {
    if (e instanceof DOMException && e.name === 'TimeoutError')
      return { title: 'Channel unreachable', detail: 'Its server didn’t answer within 10 seconds. It’s probably offline.' };
    return { title: 'Channel unreachable', detail: 'Its server didn’t answer, or doesn’t allow browsers to fetch it. It may be offline.' };
  }
}
