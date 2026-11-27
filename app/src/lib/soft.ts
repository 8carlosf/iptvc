// FFmpeg's H.264 decoder (libav.js, `mise run decoder`) for what no browser path decodes: PAFF 1080i, where
// each field is its own picture. Runs in libav.js's worker; frames queue up as VideoFrames for the screen.
import type { EncodedPacket } from 'mediabunny';

/** Decoded frames buffered ahead of the clock; decoding waits beyond this (1080p I420 ≈ 3 MB each). */
const AHEAD = 24;

type LibAV = {
  ff_init_decoder(name: string): Promise<[number, number, number, number]>;
  ff_decode_multi(c: number, pkt: number, frame: number, packets: object[], opts: object): Promise<LibAVFrame[]>;
  ff_free_decoder(c: number, pkt: number, frame: number): Promise<void>;
  terminate(): void;
};
type LibAVFrame = { data: Uint8Array; format: number; width: number; height: number; pts: number; layout: { offset: number; stride: number }[] };

const YUV420P = 0;
const base = () => new URL('libav', document.baseURI).href;

/** A new libav.js instance (its own worker). */
async function loadLibAV(): Promise<LibAV> {
  const mod = await import(/* @vite-ignore */ `${base()}/libav-iptvc.mjs`);
  return mod.default.LibAV({ base: base() });
}

export class SoftVideo {
  private queue: VideoFrame[] = [];
  private closed = false;
  /** Presentation time (s) of the newest frame decoded so far. */
  private latest = -Infinity;
  /** Video skipped while the tab is hidden (nobody sees it); decoding resumes at the next keyframe. */
  resyncing = false;

  /** `clock`: the playback position (s), so frames already late are dropped as they're decoded. */
  private constructor(private lib: LibAV, private ctx: [number, number, number, number], private clock: () => number) {}

  static async open(clock: () => number): Promise<SoftVideo> {
    const lib = await loadLibAV();
    return new SoftVideo(lib, await lib.ff_init_decoder('h264'), clock);
  }

  /** Whether decoding trails the clock at `t` (the picture would lag the sound). */
  behind(t: number) {
    return this.latest < t - 0.2;
  }

  /** Decodes one access unit (a frame or a single field, as the stream carries them). */
  async decode(p: EncodedPacket) {
    // Hidden tab: no frames get drawn, so don't decode any (a full queue would stall the pipeline otherwise).
    if (document.hidden) {
      if (!this.resyncing) { this.resyncing = true; for (const f of this.queue) f.close(); this.queue = []; }
      // Keep pace with the clock even so: running ahead to the newest data would leave nothing to show on return.
      while (document.hidden && !this.closed && p.timestamp > this.clock() + 1) await new Promise((r) => setTimeout(r, 100));
      if (document.hidden) return;
    }
    if (this.resyncing) {
      if (p.type !== 'key') return;
      this.resyncing = false;
    }
    while (this.queue.length >= AHEAD && !this.closed) await new Promise((r) => setTimeout(r, 20));
    if (this.closed) return;
    const [, c, pkt, frame] = this.ctx;
    // Milliseconds keep pts within 32 bits (no ptshi) for streams whose clock reads days.
    const frames = await this.lib.ff_decode_multi(c, pkt, frame,
      [{ data: p.data, pts: Math.round(p.timestamp * 1000), ptshi: 0, flags: p.type === 'key' ? 1 : 0 }],
      { ignoreErrors: true, copyoutFrame: 'video' });
    for (const f of frames) {
      if (f.format !== YUV420P || this.closed) continue;
      this.latest = Math.max(this.latest, f.pts / 1000);
      // Already late (after a jump): don't queue it, so decoding races to the clock instead of waiting on the screen.
      if (f.pts / 1000 < this.clock() - 0.05) continue;
      this.queue.push(new VideoFrame(f.data, {
        format: 'I420', codedWidth: f.width, codedHeight: f.height, timestamp: f.pts * 1000, layout: f.layout,
      }));
    }
    this.queue.sort((a, b) => a.timestamp - b.timestamp); // decode order → presentation order
  }

  /** The latest frame due by `t` seconds (the caller closes it); frames it skips over are dropped. */
  take(t: number): VideoFrame | null {
    let due: VideoFrame | null = null;
    while (this.queue.length && this.queue[0].timestamp / 1e6 <= t) {
      due?.close();
      due = this.queue.shift()!;
    }
    return due;
  }

  close() {
    this.closed = true;
    for (const f of this.queue) f.close();
    this.queue = [];
    const [, c, pkt, frame] = this.ctx;
    void this.lib.ff_free_decoder(c, pkt, frame).finally(() => this.lib.terminate());
  }
}
