<script lang="ts" module>
  import type { Programme } from './lib/core';

  export type Media = {
    title: string;
    url: string;
    live?: boolean;
    /** Live with catch-up: the stream from unix time `t` up to now, and how far back the archive goes (s). */
    rewind?: { at: (t: number) => string; window: number };
    /** Start rewound to this unix time (a programme picked in the guide). */
    from?: number;
    /** The channel's EPG, drawn on the timeline as chapters. */
    guide?: Programme[];
  };

  /** The live/catch-up timeline shows this much either side of where you are (up to now). */
  const HALF = 2 * 3600;
  /** Landing within this fraction of either end of the bar re-centres it there. */
  const RECENTER = 0.25;
  /** A timeline preview is grabbed every this many seconds of playback. */
  const THUMB_EVERY = 60;
  /** A channel that hasn't started playing by then is given up on and checked (servers that never answer hang). */
  const START_DEADLINE = 15_000;
  /** The browser's own HLS player sometimes stalls without an error (e.g. on field-coded video): ours takes over. */
  const NATIVE_DEADLINE = 8_000;
  /** Controls hide after this long without pointer movement, while playing. */
  const IDLE_MS = 2500;
</script>

<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { UrlSource, type EncodedPacket } from 'mediabunny';
  import { h264 } from './lib/core';
  import { mirror, route, Screen } from './lib/player';
  import { remux, streamSource } from './lib/remux';
  import { SoftVideo } from './lib/soft';
  import { grab, near } from './lib/thumbs';
  import { diagnose, statusOf } from './lib/diagnose';
  import Icon from './Icon.svelte';

  /** `onmove`: live/catch-up position changes (the catch-up start, null at live), for the page's link. */
  let { media, onclose, onmove }: { media: Media; onclose: () => void; onmove?: (from: number | null) => void } = $props();

  let box: HTMLDivElement;
  let video: HTMLVideoElement;
  let canvas: HTMLCanvasElement;
  let mode = $state<'loading' | 'native' | 'remux'>('loading');
  let error = $state('');
  let errorTitle = $state(''); // set when a channel's failure was diagnosed
  let dash: { destroy(): void } | null = null; // dash.js, while a DASH channel plays
  let openedAt = Date.now(); // when the current stream was asked for
  let started = $state(false); // it has played at least a moment
  let reconnecting = $state(false);
  let buffering = $state(true);
  let retries = 0; // consecutive reconnects; reset once playing again
  let now = $state(0);
  let playing = $state(false);
  let volume = $state(Number(localStorage.volume ?? 0.8));
  let muted = $state(localStorage.muted === '1');
  let deinterlace = $state(false);
  let soft = $state(false); // video decoded by FFmpeg-in-wasm (PAFF), drawn on the canvas; <video> plays the audio
  let screen: Screen;
  let decoder: Promise<SoftVideo> | null = null;
  let held = $state(false); // paused by us, not the user: the software-decoded picture is catching up with the sound
  let raf = 0;
  let autoDeinterlace = true; // until the user picks
  let length = $state(0); // files
  let idle = $state(false);
  let idleTimer: ReturnType<typeof setTimeout>;
  let settings = $state(false);
  let fullscreen = $state(false);
  let picture = $state(''); // "1920×1080", for the settings menu

  let abort = new AbortController();
  let origin = 0; // video time where the current pipeline starts
  let from = $state<number | null>(null); // live: unix time the catch-up stream starts at; null at the live edge
  let edge = $state(Date.now() / 1000);
  let timer: ReturnType<typeof setInterval>;
  const stream = $derived(!!media.live);
  const hls = $derived(/\.m3u8(\?|$)/i.test(media.url));
  let pausedLive = $state(false); // the user paused at the live edge: from then on they're behind it
  const atLive = $derived(stream && from === null && !pausedLive);
  // The live window: centred where you last jumped to (null: anchored at now), re-centred only past its edges.
  let center = $state<number | null>(null);
  const oldest = $derived(edge - (media.rewind?.window ?? 0));
  const viewStart = $derived(Math.max(oldest, center === null ? edge - HALF : center - HALF));
  const viewEnd = $derived(center === null ? edge : Math.min(edge, center + HALF));
  const span = $derived(viewEnd - viewStart);
  const fmt = (s: number) => new Date(Math.max(0, s) * 1000).toISOString().slice(s >= 3600 ? 11 : 14, 19);
  const clock = (t: number) => new Date(t * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  /** Timeline position (%) of unix time `t`. */
  const pct = (t: number) => ((t - viewStart) / span) * 100;
  const chapters = $derived((media.guide ?? []).filter((p) => p.end > viewStart && p.start < viewEnd));
  const onAir = $derived(stream ? media.guide?.find((p) => p.start <= now && now < p.end) : undefined);
  let tip = $state<{ x: number; text: string; p?: Programme; img?: string } | null>(null);

  /** Grabs a timeline preview at `now` from `frame` unless one is near already (watched before). */
  // Pixels are only readable where we decode the stream ourselves (remux); a cross-origin <video> playing directly
  // is "tainted", so drawing it (thumbnails, the deinterlace mirror) would yield nothing.
  function maybeGrab(frame: CanvasImageSource) {
    if (mode !== 'remux' || buffering || near(media.url, now, THUMB_EVERY / 2)) return;
    void grab(media.url, now, frame);
  }

  // Scrubbing: the bar follows the pointer at once; the seek happens on release, and the target stays shown
  // until playback gets there (a catch-up stream takes a second or two to start).
  let bar = $state<HTMLDivElement>();
  let scrub = $state<number | null>(null);
  let pending = $state<number | null>(null);
  const shown = $derived(scrub ?? pending ?? now);
  const fraction = $derived(stream ? pct(shown) / 100 : length ? shown / length : 0);
  let wokeFromIdle = false; // a click on hidden controls only brings them back

  function timeAt(e: PointerEvent) {
    const r = bar!.getBoundingClientRect();
    const x = Math.min(Math.max(e.clientX - r.left, 0), r.width);
    return { r, x, t: stream ? viewStart + (x / r.width) * span : (x / r.width) * length };
  }

  function press(e: PointerEvent) {
    bar!.setPointerCapture(e.pointerId);
    scrub = timeAt(e).t;
    hover(e);
  }

  function release() {
    if (scrub === null) return;
    const t = scrub;
    scrub = null;
    pending = t;
    seek(t);
  }

  function hover(e: PointerEvent) {
    const { r, x, t } = timeAt(e);
    if (scrub !== null) scrub = t;
    const img = near(media.url, t, THUMB_EVERY);
    const at = Math.min(Math.max(x, 100), r.width - 100); // keep the bubble inside the bar
    if (!stream) return void (tip = { x: at, text: fmt(t), img });
    const p = chapters.find((c) => c.start <= t && t < c.end);
    tip = { x: at, text: `${clock(t)}${p ? ` · ${p.title}` : ''}`, p, img };
  }

  /**
   * (Re)starts the remux pipeline. Files: `at` seconds in. Live: `at` is the unix time to rewind to (catch-up up
   * to now), or null for the live edge. `now` is then seconds into the file, or the unix time being watched.
   */
  async function start(at: number | null = null) {
    abort.abort();
    abort = new AbortController();
    const { signal } = abort;
    error = '';
    errorTitle = '';
    openedAt = Date.now();
    started = false;
    buffering = true;
    pausedLive = false;
    void decoder?.then((d) => d.close());
    decoder = null;
    // Opened on the first packet that needs it; each pipeline (a seek starts a fresh stream) gets its own.
    const softVideo = async (p: EncodedPacket) => {
      if (signal.aborted) return;
      await (await (decoder ??= SoftVideo.open(() => video.currentTime))).decode(p);
    };
    const onerror = (e: unknown) => {
      if (signal.aborted) return;
      // Connections drop mid-play; pick up again (catch-up where it was) before giving up. A stream that never
      // started isn't retried: it's explained.
      if (stream && started && retries++ < 5) {
        error = '';
        reconnecting = true;
        const at = from === null ? null : now;
        setTimeout(() => !signal.aborted && start(at), 2000);
        return;
      }
      error = e instanceof Error ? e.message : String(e);
      if (hls) void explain(statusOf(error));
    };
    try {
      if (stream) {
        from = media.rewind && at !== null ? Math.floor(at / 60) * 60 : null; // timeshift URLs go by the minute
        onmove?.(from);
        const url = from === null ? media.url : media.rewind!.at(from);
        // HLS (where the browser can't play it itself) is read by mediabunny, segment by segment.
        ({ soft } = await remux(video, hls ? new UrlSource(url) : streamSource(url, signal), signal, { onerror, softVideo }));
      } else {
        const r = await remux(video, new UrlSource(media.url), signal, { start: fileStart + (at ?? 0), seekable: true, onerror, softVideo });
        soft = r.soft;
        fileStart = r.first;
        length = r.end - r.first;
      }
      // Timestamps are the source's own: jump to where the data starts.
      while (!video.buffered.length && !signal.aborted) await new Promise((r) => setTimeout(r, 100));
      if (signal.aborted) return;
      origin = stream ? video.buffered.start(0) : fileStart;
      video.currentTime = Math.max(video.buffered.start(0), stream ? 0 : fileStart + (at ?? 0));
      await video.play();
    } catch (e) {
      if (!signal.aborted) onerror(e);
    }
  }
  let fileStart = 0;

  onMount(async () => {
    screen = new Screen(canvas);
    const frame = async () => {
      raf = requestAnimationFrame(frame);
      const f = soft && decoder ? (await decoder).take(video.currentTime) : null;
      if (f) { screen.draw(f, f.displayWidth, f.displayHeight); maybeGrab(f); f.close(); } // grab copies before close
    };
    frame();
    timer = setInterval(tick, 250);
    wake();
    if (/\.mpd(\?|$)/i.test(media.url)) {
      // DASH: dash.js (loaded only now) drives the same <video>, so the controls work as for any direct stream.
      mode = 'native';
      const { MediaPlayer } = await import('dashjs');
      const player = MediaPlayer().create();
      player.on('error', () => void (error || explain()));
      player.initialize(video, media.url, true);
      dash = player;
      return;
    }
    // Live HLS plays natively where the browser can (Chrome, Safari); files when they need no conversion.
    if (stream ? hls && !!video.canPlayType('application/vnd.apple.mpegurl') : (await route(media.url)) === 'native') {
      mode = 'native';
      video.src = media.url;
      video.play().catch(() => {});
      return;
    }
    mode = 'remux';
    if (media.from !== undefined && media.from < Date.now() / 1000 - HALF) center = media.from;
    await start(media.from ?? null);
  });

  onDestroy(() => {
    clearInterval(timer);
    clearTimeout(idleTimer);
    cancelAnimationFrame(raf);
    dash?.destroy();
    abort.abort();
    void decoder?.then((d) => d.close());
    video?.pause();
    video?.removeAttribute('src');
    video?.load();
  });

  async function tick() {
    if (!video.paused && video.readyState >= 3 && video.currentTime > 0) started = true;
    // Never started in the browser's own HLS player: hand the stream to ours (which has its own deadline).
    if (!started && !error && mode === 'native' && stream && hls && !dash && !ownTried && Date.now() - openedAt > NATIVE_DEADLINE)
      return void ownPipeline();
    // Never started: stop waiting on a silent server and say what's wrong.
    if (!started && !error && !reconnecting && mode !== 'loading' && Date.now() - openedAt > START_DEADLINE) {
      abort.abort();
      dash?.destroy();
      dash = null;
      video.pause();
      video.removeAttribute('src');
      video.load();
      buffering = false;
      return void explain();
    }
    // Sound waits for picture: never let the software decoder fall behind the audio clock.
    // (Not while hidden or resyncing after it: then video is skipped on purpose and sound must play on.)
    if (soft && decoder && !document.hidden && (!video.paused || held)) {
      const d = await decoder;
      const behind = !d.resyncing && d.behind(video.currentTime);
      if (behind && !held) { held = true; buffering = true; video.pause(); }
      else if (!behind && held) { held = false; buffering = false; void video.play(); }
    }
    playing = !video.paused || held;
    if (pending !== null && !video.paused && !buffering) pending = null;
    picture = soft ? (canvas.width ? `${canvas.width}×${canvas.height}` : '') : video.videoWidth ? `${video.videoWidth}×${video.videoHeight}` : '';
    if (!video.paused && video.readyState >= 3) { retries = 0; reconnecting = false; }
    if (mode === 'native') {
      now = video.currentTime;
      length = Number.isFinite(video.duration) ? video.duration : 0;
      // Sound but no picture: the browser's own HLS player can't decode this video (HEVC in TS, in Chrome).
      if (stream && hls && !video.paused && video.currentTime > 2 && !video.videoWidth) return void ownPipeline();
      // Chrome-only counter: a file with video but no decodable audio (AC-3/DTS) that we couldn't probe.
      if (video.currentTime > 3 && video.webkitAudioDecodedByteCount === 0 && !error)
        error = 'No sound: this audio format needs the server to allow cross-origin access to movie and series files.';
      return;
    }
    if (mode !== 'remux') return;
    edge = Date.now() / 1000;
    // Paused at live: the position stays where it stopped while now moves on.
    if (!pausedLive) now = stream ? (from === null ? edge : from + video.currentTime - origin) : video.currentTime - origin;
    // Played past the window (only once settled: while a new stream starts, `now` still uses the old one's timing).
    if (center !== null && !buffering && pending === null && (now > viewEnd || now < viewStart)) center = now;
    if (autoDeinterlace && (video.videoWidth || soft)) deinterlace = h264.interlaced();
    if (!soft && !video.paused && video.readyState >= 2) maybeGrab(video);
  }

  /** Files: seconds in. Live: a unix time; the last minute counts as live. */
  function seek(t: number) {
    if (mode === 'native') return void (video.currentTime = Math.min(Math.max(t, 0), length));
    if (stream && !media.rewind) return;
    if (stream) {
      t = Math.max(t, oldest);
      if (t > edge - 60) center = null;
      // Re-centre only from the outer quarter of the bar (the right one only when the bar doesn't already end at now).
      else if (t < viewStart + span * RECENTER || (viewEnd < edge - 60 && t > viewEnd - span * RECENTER)) center = t;
    }
    void start(stream && t > edge - 60 ? null : Math.max(0, t));
  }

  function toggle() {
    if (held) { held = false; buffering = false; return; } // a pause while catching up stays paused
    if (mode === 'native') {
      pausedLive = stream && !video.paused;
      return void (video.paused ? video.play() : video.pause());
    }
    if (!video.paused) { pausedLive = stream && from === null; return video.pause(); }
    // Paused live: with catch-up, carry on from the paused moment; without, rejoin the live edge.
    if (stream && from === null) void start(media.rewind ? now : null);
    else void video.play();
  }

  let ownTried = false;
  /**
   * The browser's own HLS player gave up (a playlist served with the wrong type, a codec it lacks): try once more
   * with ours, which reads the playlist itself and hands the video to MSE. Only if that fails is the channel checked.
   */
  function ownPipeline() {
    if (ownTried) return void explain();
    ownTried = true;
    mode = 'remux';
    video.removeAttribute('src');
    void start(null);
  }

  function retry() {
    error = '';
    errorTitle = '';
    openedAt = Date.now();
    started = false;
    if (mode !== 'native') return void start(from);
    if (!video.getAttribute('src') && !dash) video.src = media.url; // dropped by the start deadline
    video.load();
    void video.play().catch(() => {});
  }

  /** A channel failed: find out whether it's the channel (offline, refused) or the browser, and say so. */
  async function explain(seen?: number) {
    errorTitle = '';
    error = 'Checking the channel…';
    const d = await diagnose(media.url, seen);
    errorTitle = d.title;
    error = d.detail;
  }

  function goLive() {
    if (mode === 'native') {
      // Native HLS: the newest point the stream offers.
      if (video.seekable.length) video.currentTime = video.seekable.end(video.seekable.length - 1);
      pausedLive = false;
      void video.play();
      return;
    }
    center = null;
    void start(null);
  }

  /** Shows the controls, and hides them again after a pause in pointer movement while playing. */
  function wake() {
    idle = false;
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => (idle = playing && !settings && !tip), IDLE_MS);
  }

  function key(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement && e.target.type !== 'range') return;
    wake();
    const step = stream ? 60 : 10;
    const k = e.key.toLowerCase();
    if (k === 'escape') { if (settings) settings = false; else if (!document.fullscreenElement) onclose(); }
    else if (k === ' ' || k === 'k') { e.preventDefault(); toggle(); }
    else if (k === 'f') toggleFullscreen();
    else if (k === 'm') muted = !muted;
    else if (k === 'arrowleft') { e.preventDefault(); seek(now - step); }
    else if (k === 'arrowright' && !(stream && from === null)) { e.preventDefault(); seek(now + step); }
    else return;
  }

  const toggleFullscreen = () => (document.fullscreenElement ? document.exitFullscreen() : box.requestFullscreen());

  $effect(() => {
    localStorage.volume = volume;
    localStorage.muted = muted ? '1' : '0';
    if (video) { video.volume = volume; video.muted = muted; }
  });
  $effect(() => {
    if (screen) screen.deinterlace = deinterlace;
    if (deinterlace && !soft && screen && mode === 'remux') return mirror(video, screen);
  });
</script>

<svelte:window onkeydown={key} />
<svelte:document onfullscreenchange={() => (fullscreen = !!document.fullscreenElement)} />

<div class="player" class:idle bind:this={box} onpointermove={wake} onpointerdown={() => { wokeFromIdle = idle; wake(); }} role="application" aria-label="Video player">
  <div class="stage">
    <!-- svelte-ignore a11y_media_has_caption -->
    <video bind:this={video} playsinline onclick={() => !wokeFromIdle && toggle()} ondblclick={toggleFullscreen}
      onwaiting={() => (buffering = true)} onplaying={() => (buffering = false)} oncanplay={() => (buffering = false)}
      onended={() => from !== null && start(null)}
      onerror={() => mode === 'native' && (stream ? (hls && !dash ? ownPipeline() : explain()) : (error = 'This file can’t be played in the browser.'))}></video>
    <canvas bind:this={canvas} hidden={mode !== 'remux' || (!deinterlace && !soft)} onclick={() => !wokeFromIdle && toggle()} ondblclick={toggleFullscreen}></canvas>

    <div class="center">
      {#if error}
        <div class="notice">{#if errorTitle}<h2>{errorTitle}</h2>{/if}<p>{error}</p><button class="btn" onclick={retry}>Try again</button></div>
      {:else if reconnecting}
        <div class="notice"><span class="spinner"></span><p>Reconnecting…</p></div>
      {:else if held || buffering || mode === 'loading' || (stream && !started)}
        <span class="spinner big"></span>
      {:else if !playing}
        <button class="bigplay" onclick={toggle} aria-label="Play"><Icon name="play" size={34} /></button>
      {/if}
    </div>
  </div>

  <header class="chrome top">
    <button class="icon-btn" onclick={onclose} aria-label="Close player"><Icon name="back" size={22} /></button>
    <div class="titles">
      <h1>{media.title}</h1>
      {#if onAir}<p>{onAir.title}<span> · {clock(onAir.start)}–{clock(onAir.end)}</span></p>{/if}
    </div>
  </header>

  <footer class="chrome bottom">
    {#if (stream && media.rewind) || (!stream && length)}
      <div class="bar" bind:this={bar} role="slider" tabindex="-1" aria-label="Position" aria-valuenow={Math.round(shown)}
        aria-valuemin={Math.round(stream ? viewStart : 0)} aria-valuemax={Math.round(stream ? viewEnd : length)}
        onpointerdown={press} onpointermove={hover} onpointerup={release} onpointercancel={() => (scrub = null)}
        onpointerleave={() => scrub === null && (tip = null)}>
        {#if tip?.p}<b class="hl" style:left="{Math.max(0, pct(tip.p.start))}%" style:right="{Math.max(0, 100 - pct(tip.p.end))}%"></b>{/if}
        {#if stream}
          {#each chapters as c (c.start)}{#if c.start > viewStart}<i style:left="{pct(c.start)}%"></i>{/if}{/each}
        {/if}
        <span class="fill" style:width="{fraction * 100}%"></span>
        <span class="knob" style:left="{fraction * 100}%"></span>
        {#if tip}<span class="tip" style:left="{tip.x}px">{#if tip.img}<img src={tip.img} alt="" />{/if}<span>{tip.text}</span></span>{/if}
      </div>
    {/if}
    <div class="row">
      <button class="icon-btn" onclick={toggle} aria-label={playing ? 'Pause' : 'Play'}><Icon name={playing ? 'pause' : 'play'} size={22} /></button>
      <div class="volume">
        <button class="icon-btn" onclick={() => (muted = !muted)} aria-label={muted ? 'Unmute' : 'Mute'}><Icon name={muted || !volume ? 'mute' : 'volume'} size={21} /></button>
        <input type="range" min="0" max="1" step="0.02" bind:value={volume} oninput={() => (muted = false)} aria-label="Volume"
          style:--level="{(muted ? 0 : volume) * 100}%" />
      </div>
      <span class="time">
        {#if stream}
          {#if mode === 'remux' && (!atLive || pending !== null || scrub !== null)}{clock(shown)} <span>· {fmt(edge - shown)} behind</span>{/if}
        {:else if length}{fmt(shown)} <span>/ {fmt(length)}</span>{/if}
      </span>
      {#if stream}
        <button class="live" class:on={atLive} disabled={atLive} onclick={goLive}><span></span>LIVE</button>
      {/if}
      <div class="menu">
        <button class="icon-btn" class:active={settings} onclick={() => (settings = !settings)} aria-label="Settings" aria-expanded={settings}><Icon name="sliders" size={20} /></button>
        {#if settings}
          <div class="popover" role="dialog" aria-label="Playback settings">
            <label class="switch" class:off={mode !== 'remux'}>
              <span>Deinterlace<small>{mode === 'remux' ? 'Smooths combing on 1080i broadcasts' : 'Not available for this stream'}</small></span>
              <input type="checkbox" bind:checked={deinterlace} disabled={mode !== 'remux'} onchange={() => (autoDeinterlace = false)} />
            </label>
            <dl>
              <dt>Decoder</dt><dd>{soft ? 'FFmpeg (WebAssembly)' : mode === 'native' ? 'Browser, direct' : 'Browser, remuxed'}</dd>
              {#if picture}<dt>Picture</dt><dd>{picture}</dd>{/if}
            </dl>
          </div>
        {/if}
      </div>
      <button class="icon-btn" onclick={toggleFullscreen} aria-label={fullscreen ? 'Exit full screen' : 'Full screen'}><Icon name={fullscreen ? 'shrink' : 'expand'} size={20} /></button>
    </div>
  </footer>
</div>

<style>
  .player { position: relative; height: 100%; background: #000; color: #fff; overflow: hidden; }
  .player.idle { cursor: none; }
  .stage { position: absolute; inset: 0; }
  video, canvas { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; }
  [hidden] { display: none; }

  .center { position: absolute; inset: 0; display: grid; place-items: center; pointer-events: none; }
  .center > * { pointer-events: auto; }
  .spinner.big { width: 3rem; height: 3rem; border-width: 3px; pointer-events: none; }
  .bigplay { display: grid; place-items: center; width: 5rem; height: 5rem; padding-left: 0.3rem; border-radius: 50%;
    background: rgb(255 255 255 / 0.14); backdrop-filter: blur(10px); transition: transform 0.2s var(--ease), background 0.2s var(--ease); }
  .bigplay:hover { transform: scale(1.06); background: rgb(255 255 255 / 0.22); }
  .notice h2 { font-size: 1.05rem; font-weight: 600; }
  .notice p { color: rgb(255 255 255 / 0.75); }
  .notice { display: grid; justify-items: center; gap: 0.9rem; max-width: 26rem; padding: 1.25rem 1.5rem; border-radius: 14px;
    background: rgb(20 22 27 / 0.85); backdrop-filter: blur(10px); text-align: center; }

  .chrome { position: absolute; left: 0; right: 0; z-index: 2; transition: opacity 0.3s var(--ease), transform 0.3s var(--ease); }
  .idle .chrome { opacity: 0; pointer-events: none; }
  .idle .top { transform: translateY(-0.5rem); }
  .idle .bottom { transform: translateY(0.5rem); }
  .chrome .icon-btn { color: rgb(255 255 255 / 0.85); }
  .chrome .icon-btn:hover, .chrome .icon-btn.active { background: rgb(255 255 255 / 0.12); color: #fff; }

  .top { top: 0; display: flex; align-items: center; gap: 0.75rem; padding: 1rem 1.25rem 3rem;
    background: linear-gradient(rgb(0 0 0 / 0.7), transparent); }
  .titles { min-width: 0; }
  h1 { font-size: 1.1rem; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .titles p { color: rgb(255 255 255 / 0.75); font-size: 0.9rem; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .titles p span { color: rgb(255 255 255 / 0.5); }

  .bottom { bottom: 0; padding: 3.5rem 1.25rem 0.75rem; background: linear-gradient(transparent, rgb(0 0 0 / 0.8)); }
  .row { display: flex; align-items: center; gap: 0.35rem; }
  .time { flex: 1; margin-left: 0.5rem; font-size: 0.9rem; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .time span { color: rgb(255 255 255 / 0.6); }

  .bar { position: relative; height: 1.4rem; margin: 0 0.4rem 0.35rem; display: flex; align-items: center; }
  .bar::before { content: ''; position: absolute; left: 0; right: 0; height: 4px; border-radius: 4px; background: rgb(255 255 255 / 0.22); transition: height 0.15s var(--ease); }
  .bar:hover::before { height: 6px; }
  .fill { position: absolute; left: 0; height: 4px; border-radius: 4px; background: #fff; pointer-events: none; transition: height 0.15s var(--ease); }
  .bar:hover .fill { height: 6px; }
  .bar i { position: absolute; z-index: 1; width: 3px; height: 6px; margin-left: -1.5px; background: rgb(0 0 0 / 0.75); pointer-events: none; }
  .bar .hl { position: absolute; height: 6px; border-radius: 4px; background: rgb(255 255 255 / 0.45); pointer-events: none; }
  .bar { cursor: pointer; touch-action: none; }
  .knob { position: absolute; z-index: 2; width: 14px; height: 14px; margin-left: -7px; border-radius: 50%; background: #fff; pointer-events: none;
    transform: scale(0); transition: transform 0.15s var(--ease); }
  .bar:hover .knob, .bar:active .knob { transform: scale(1); }
  .tip { position: absolute; z-index: 3; bottom: 1.9rem; transform: translateX(-50%); display: grid; justify-items: center; gap: 0.35rem; max-width: 22rem;
    padding: 0.3rem 0.6rem; border-radius: 8px; background: rgb(20 22 27 / 0.92); font-size: 0.85rem; font-weight: 500; pointer-events: none; }
  .tip img { width: 192px; height: 108px; margin: 0.15rem -0.35rem 0; border-radius: 5px; object-fit: cover; background: #000; }
  .tip span { max-width: 100%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .volume { display: flex; align-items: center; }
  .volume input { width: 0; opacity: 0; transition: width 0.2s var(--ease), opacity 0.2s var(--ease); appearance: none; height: 4px; border-radius: 4px; cursor: pointer;
    background: linear-gradient(to right, #fff var(--level), rgb(255 255 255 / 0.25) var(--level)); }
  .volume:hover input, .volume:focus-within input { width: 5.5rem; opacity: 1; margin: 0 0.5rem 0 0.25rem; }
  .volume input::-webkit-slider-thumb { appearance: none; width: 12px; height: 12px; border-radius: 50%; background: #fff; }
  .volume input::-moz-range-thumb { width: 12px; height: 12px; border: 0; border-radius: 50%; background: #fff; }

  .live { display: inline-flex; align-items: center; gap: 0.4rem; height: 1.9rem; padding: 0 0.7rem; border-radius: 6px; font-size: 0.78rem; font-weight: 700;
    letter-spacing: 0.06em; color: rgb(255 255 255 / 0.75); background: rgb(255 255 255 / 0.1); }
  .live span { width: 0.45rem; height: 0.45rem; border-radius: 50%; background: rgb(255 255 255 / 0.5); }
  .live:hover:not(:disabled) { background: rgb(255 255 255 / 0.18); color: #fff; }
  .live.on { color: #fff; background: none; }
  .live.on span { background: var(--live); box-shadow: 0 0 0 3px rgb(240 72 72 / 0.3); }

  .menu { position: relative; }
  .popover { position: absolute; right: 0; bottom: calc(100% + 0.6rem); width: 17rem; padding: 0.9rem; border-radius: 12px;
    background: rgb(20 22 27 / 0.95); backdrop-filter: blur(12px); border: 1px solid rgb(255 255 255 / 0.08); display: grid; gap: 0.9rem; }
  .switch { display: flex; align-items: center; justify-content: space-between; gap: 1rem; cursor: pointer; }
  .switch span { display: grid; font-weight: 500; }
  .switch small { color: var(--muted); font-weight: 400; font-size: 0.8rem; }
  .switch input { appearance: none; flex: none; width: 2.3rem; height: 1.35rem; border-radius: 999px; background: var(--surface-3); position: relative; cursor: pointer; transition: background 0.2s var(--ease); }
  .switch input::after { content: ''; position: absolute; top: 3px; left: 3px; width: calc(1.35rem - 6px); aspect-ratio: 1; border-radius: 50%; background: #fff; transition: translate 0.2s var(--ease); }
  .switch input:checked { background: var(--accent); }
  .switch input:checked::after { translate: 0.95rem 0; }
  .switch.off { cursor: default; opacity: 0.55; }
  .switch.off input { cursor: default; }
  dl { display: grid; grid-template-columns: auto 1fr; gap: 0.3rem 1rem; margin: 0; padding-top: 0.75rem; border-top: 1px solid rgb(255 255 255 / 0.08); font-size: 0.82rem; }
  dt { color: var(--muted); }
  dd { margin: 0; text-align: right; }

  @media (max-width: 700px) {
    .top { padding: 0.75rem 0.75rem 2.5rem; }
    .bottom { padding: 3rem 0.5rem 0.5rem; }
    .volume input { display: none; }
  }
</style>
