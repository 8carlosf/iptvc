// The Rust core (core/, built by `mise run wasm`): catalog search, EPG, H.264 field detection, MPEG audio decoding.
import init from '../../../core/pkg/iptvc.wasm?init';

type Exports = {
  memory: WebAssembly.Memory;
  ic_alloc(n: number): number;
  ic_free(p: number, n: number): void;
  ic_call(op: number, p: number, n: number): number;
  ic_out_ptr(): number;
  ic_out_len(): number;
  mpa_open(): number;
  mpa_close(h: number): void;
  mpa_decode(h: number, p: number, n: number): number;
  mpa_rate(h: number): number;
  mpa_channels(h: number): number;
  h264_reset(): void;
  h264_field(p: number, n: number): number;
  h264_interlaced(): number;
};

export type Kind = 'live' | 'vod' | 'series';
export type Item = { id: number; name: string; icon: string; cat: string; ext: string; archive: number; rating: number };
export type Programme = { start: number; end: number; title: string; desc: string; archive: boolean };

const x = (await init({})).exports as unknown as Exports;
const enc = new TextEncoder();
const dec = new TextDecoder();

function put(bytes: Uint8Array): number {
  const p = x.ic_alloc(bytes.length);
  new Uint8Array(x.memory.buffer, p, bytes.length).set(bytes);
  return p;
}

function call(op: number, ...fields: (string | number)[]): string {
  const req = enc.encode(fields.join('\n'));
  const p = put(req);
  const code = x.ic_call(op, p, req.length);
  x.ic_free(p, req.length);
  const out = dec.decode(new Uint8Array(x.memory.buffer, x.ic_out_ptr(), x.ic_out_len()));
  if (code !== 0) throw new Error(out);
  return out;
}

export const core = {
  load: (kind: Kind, json: string) => Number(call(0, kind, json)),
  query: (kind: Kind, o: { cat?: string; offset?: number; limit?: number; ids?: number[]; q?: string }) =>
    JSON.parse(call(1, kind, o.cat ?? '', o.offset ?? 0, o.limit ?? 120, (o.ids ?? []).join(','), o.q ?? '')) as { total: number; items: Item[] },
  epg: (json: string, now = Date.now() / 1000) => JSON.parse(call(2, Math.floor(now), json)) as Programme[],
};

/** H.264 field pictures (PAFF 1080i): the remuxer sends those streams to the software decoder. */
export const h264 = {
  reset: () => x.h264_reset(),
  /** The current stream's SPS allows interlaced coding. */
  interlaced: () => !!x.h264_interlaced(),
  /** -1 for a frame, else `frame_num << 1 | bottom`. */
  field(au: Uint8Array): number {
    const p = put(au);
    const r = x.h264_field(p, au.length);
    x.ic_free(p, au.length);
    return r;
  },
};

/** One MPEG audio stream (layers I–III). */
export class Mpa {
  private h = x.mpa_open();
  /** Interleaved f32 samples, or null for a skipped frame. */
  decode(frame: Uint8Array): { pcm: Float32Array; rate: number; channels: number } | null {
    const p = put(frame);
    const frames = x.mpa_decode(this.h, p, frame.length);
    x.ic_free(p, frame.length);
    if (!frames) return null;
    const pcm = new Float32Array(new Uint8Array(x.memory.buffer, x.ic_out_ptr(), x.ic_out_len()).slice().buffer);
    return { pcm, rate: x.mpa_rate(this.h), channels: x.mpa_channels(this.h) };
  }
  close() { x.mpa_close(this.h); }
}
