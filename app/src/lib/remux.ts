// Feeds <video> through MSE what it can't open itself: MPEG-TS (live, catch-up) and files with audio browsers
// lack (MP2, AC-3, DTS). Video packets are copied untouched, so the browser's own decoder handles interlaced and
// open-GOP broadcast H.264 (WebCodecs refuses interlaced); only audio is decoded (our wasm decoders) and
// re-encoded to AAC. Output is fragmented MP4, appended as it's muxed.
import {
  ALL_FORMATS, AudioSampleSink, AudioSampleSource, canEncodeAudio, EncodedAudioPacketSource, EncodedPacketSink,
  EncodedPacket, EncodedVideoPacketSource, Input, Mp4OutputFormat, Output, ReadableStreamSource, StreamTarget,
  type Source,
} from 'mediabunny';
import { h264 } from './core';
import { laneFetch } from './lane';
import './codecs';

/** Seconds of media kept ahead of the playhead; the muxer waits beyond that (VOD files arrive faster than real time). */
const AHEAD = 60;
/**
 * Streams read their tracks sequentially from one cache, so tracks mustn't drift far apart: catch-up arrives
 * faster than real time, and audio would otherwise run a minute past software-decoded video (held ~1 s ahead).
 */
const STREAM_AHEAD = 15;
/** Seconds kept behind the playhead (live streams would otherwise grow until MSE's quota). */
const BEHIND = 30;

/** An HTTP stream (live TV, catch-up) as a mediabunny source. It holds the request lane for as long as it runs. */
export function streamSource(url: string, signal: AbortSignal): Source {
  const body = new ReadableStream<Uint8Array>({
    start: (c) => laneFetch(url, signal, async (r) => {
      if (!r.ok || !r.body) throw new Error(`HTTP ${r.status}`);
      const reader = r.body.getReader();
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        c.enqueue(value);
      }
      c.close();
    }).catch((e) => { if (!signal.aborted) c.error(e); }),
  });
  return new ReadableStreamSource(body, { maxCacheSize: 64 << 20, handleUnhandledError: () => {} });
}

const updated = (sb: SourceBuffer) => new Promise((r) => sb.addEventListener('updateend', r, { once: true }));

/**
 * Plays `source` in `video` from `start` seconds (files only; streams start where they start), keeping the
 * source's timestamps. Resolves with the media's first timestamp, its end (Infinity for streams) and whether
 * video went to `softVideo` (then MSE carries only audio, and `video` is just the clock) once the pipeline runs;
 * `signal` stops it. Errors after that go to `onerror`.
 */
export async function remux(video: HTMLVideoElement, source: Source, signal: AbortSignal,
  { start = 0, seekable = false, onerror = console.error as (e: unknown) => void,
    softVideo = undefined as ((p: EncodedPacket) => Promise<void>) | undefined } = {}) {
  const input = new Input({ source, formats: ALL_FORMATS });
  const [v, a] = await Promise.all([input.getPrimaryVideoTrack(), input.getPrimaryAudioTrack()]);
  const vcodec = await v?.getCodec();
  const acodec = await a?.getCodec();
  if (!vcodec && !acodec) throw new Error('No playable tracks in this stream');

  const ahead = seekable ? AHEAD : STREAM_AHEAD;
  const ms = new MediaSource();
  video.src = URL.createObjectURL(ms);
  await new Promise((r) => ms.addEventListener('sourceopen', r, { once: true }));
  let sb: SourceBuffer | null = null;
  let mime = '';

  const output = new Output({
    format: new Mp4OutputFormat({ fastStart: 'fragmented', minimumFragmentDuration: 0.5 }),
    target: new StreamTarget(new WritableStream({
      async write({ data }) {
        sb ??= ms.addSourceBuffer((mime ||= await output.getMimeType()));
        while (!signal.aborted && sb.buffered.length && sb.buffered.end(sb.buffered.length - 1) - video.currentTime > ahead)
          await new Promise((r) => setTimeout(r, 500));
        if (signal.aborted) return;
        if (video.currentTime > BEHIND && sb.buffered.length && sb.buffered.start(0) < video.currentTime - BEHIND) {
          sb.remove(0, video.currentTime - BEHIND);
          await updated(sb);
        }
        sb.appendBuffer(data);
        await updated(sb);
      },
    })),
  });

  const jobs: (() => Promise<unknown>)[] = []; // run once the output has started
  const vsink = v && vcodec ? new EncodedPacketSink(v) : null;
  // Streams start after `start` (0): then their first keyframe.
  const firstPacket = await vsink?.getFirstPacket();
  const key = (await vsink?.getKeyPacket(start))
    ?? (firstPacket?.type === 'key' ? firstPacket : firstPacket && (await vsink!.getNextKeyPacket(firstPacket)));
  // PAFF (each field its own picture) decodes in no browser path, so it goes to FFmpeg-in-wasm instead.
  h264.reset();
  const soft = !!(softVideo && vcodec === 'avc' && key && h264.field(key.data) >= 0);
  if (v && vsink) {
    const src = soft ? null : new EncodedVideoPacketSource(vcodec!);
    if (src) output.addVideoTrack(src);
    const decoderConfig = (await v.getDecoderConfig()) ?? undefined;
    jobs.push(async () => {
      let first = true;
      let keyTs = -Infinity;
      for await (const p of vsink.packets(key ?? undefined)) {
        if (signal.aborted) return;
        // Open GOP: pictures shown before the first keyframe reference a GOP we never got; decoders choke on them.
        if (keyTs === -Infinity) keyTs = p.timestamp;
        if (p.timestamp < keyTs) continue;
        if (src) await src.add(p, first ? { decoderConfig } : undefined);
        else await softVideo!(p);
        first = false;
      }
    });
  }
  if (a && acodec && (await a.canDecode())) {
    if (acodec === 'aac') {
      const src = new EncodedAudioPacketSource('aac');
      output.addAudioTrack(src);
      const decoderConfig = (await a.getDecoderConfig()) ?? undefined;
      jobs.push(async () => {
        let first = true;
        const sink = new EncodedPacketSink(a);
        for await (const p of sink.packets(await sink.getPacket(start) ?? undefined)) {
          if (signal.aborted) return;
          await src.add(p, first ? { decoderConfig } : undefined);
          first = false;
        }
      });
    } else {
      const src = new AudioSampleSource({ codec: (await canEncodeAudio('aac')) ? 'aac' : 'opus', bitrate: 192e3 });
      output.addAudioTrack(src);
      jobs.push(async () => {
        for await (const s of new AudioSampleSink(a).samples(start)) {
          if (signal.aborted) return s.close();
          await src.add(s);
          s.close();
        }
      });
    }
  }

  signal.addEventListener('abort', () => {
    void output.cancel().catch(() => {});
    input.dispose();
    URL.revokeObjectURL(video.src);
  }, { once: true });

  const tracks = [v, a].filter((t) => t !== null);
  const first = await input.getFirstTimestamp(tracks);
  const end = seekable ? await input.computeDuration(tracks) : Infinity;
  await output.start();
  Promise.all(jobs.map((job) => job()))
    .then(() => !signal.aborted && output.finalize().then(() => ms.readyState === 'open' && ms.endOfStream()))
    .catch((e) => !signal.aborted && onerror(e));
  return { first, end, soft };
}
