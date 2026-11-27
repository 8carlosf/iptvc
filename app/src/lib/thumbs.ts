// Timeline previews, grabbed from what's already playing: no extra requests. In memory only (gone with the tab),
// capped, oldest dropped first, so they never pile up anywhere.

const MAX_BYTES = 10 << 20; // ~1,500 thumbnails at ~7 KB
const W = 192;
const H = 108;

type Thumb = { key: string; t: number; url: string; size: number };
const byKey = new Map<string, Thumb[]>(); // per source, sorted by t
const order: Thumb[] = []; // insertion order, for eviction
let bytes = 0;
const canvas = new OffscreenCanvas(W, H);
const ctx = canvas.getContext('2d')!;

/** Saves a preview of `frame` (a <video>, or a decoded VideoFrame) at time `t` of source `key`. */
export async function grab(key: string, t: number, frame: CanvasImageSource) {
  ctx.drawImage(frame, 0, 0, W, H);
  const blob = await canvas.convertToBlob({ type: 'image/jpeg', quality: 0.7 });
  const thumb = { key, t, url: URL.createObjectURL(blob), size: blob.size };
  const list = byKey.get(key) ?? [];
  list.splice(list.findIndex((x) => x.t > t) >>> 0, 0, thumb); // sorted insert (-1 >>> 0 appends)
  byKey.set(key, list);
  order.push(thumb);
  bytes += thumb.size;
  while (bytes > MAX_BYTES) drop(order.shift()!);
}

function drop(thumb: Thumb) {
  const list = byKey.get(thumb.key)!;
  list.splice(list.indexOf(thumb), 1);
  if (!list.length) byKey.delete(thumb.key);
  URL.revokeObjectURL(thumb.url);
  bytes -= thumb.size;
}

/** The preview closest to `t`, if one is within `within` seconds. */
export function near(key: string, t: number, within: number): string | undefined {
  let best: Thumb | undefined;
  for (const x of byKey.get(key) ?? []) if (!best || Math.abs(x.t - t) < Math.abs(best.t - t)) best = x;
  return best && Math.abs(best.t - t) <= within ? best.url : undefined;
}
