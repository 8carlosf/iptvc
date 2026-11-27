<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { Item, Programme } from './lib/core';
  import Icon from './Icon.svelte';

  let { channel, guide, loading, logo, onwatch, onclose }: {
    channel: Item; guide: Programme[]; loading: boolean; logo: string;
    onwatch: (from?: number) => void; onclose: () => void;
  } = $props();

  let now = $state(Date.now() / 1000);
  let list: HTMLOListElement;
  onMount(() => {
    const id = setInterval(() => (now = Date.now() / 1000), 30_000);
    return () => clearInterval(id);
  });

  const time = (s: number) => new Date(s * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  const day = (s: number) => {
    const d = new Date(s * 1000), t = new Date();
    const diff = Math.round((new Date(d.toDateString()).getTime() - new Date(t.toDateString()).getTime()) / 864e5);
    return diff === 0 ? 'Today' : diff === -1 ? 'Yesterday' : diff === 1 ? 'Tomorrow'
      : d.toLocaleDateString([], { weekday: 'long', day: 'numeric', month: 'short' });
  };

  // Bring the programme on air into view once the guide arrives.
  $effect(() => {
    if (guide.length) tick().then(() => list?.querySelector('.now')?.scrollIntoView({ block: 'center' }));
  });
</script>

<section>
  <header>
    <div class="logo">{#if logo}<img src={logo} alt="" />{/if}</div>
    <div class="title">
      <h2>{channel.name}</h2>
      {#if channel.archive}<p><Icon name="history" size={14} /> Catch-up · {channel.archive} days</p>{/if}
    </div>
    <button class="icon-btn close" onclick={onclose} aria-label="Close guide"><Icon name="close" /></button>
  </header>
  <button class="btn live" onclick={() => onwatch()}><span class="dot"></span> Watch live</button>

  {#if loading}
    <div class="state"><span class="spinner"></span></div>
  {:else if !guide.length}
    <p class="state">No programme guide for this channel.</p>
  {/if}

  <ol bind:this={list}>
    {#each guide as p, i (p.start)}
      {@const live = p.start <= now && now < p.end}
      {#if i === 0 || day(p.start) !== day(guide[i - 1].start)}<li class="day">{day(p.start)}</li>{/if}
      <li class:now={live} class:past={p.end <= now}>
        <button disabled={!p.archive && !live} title={p.desc || p.title} onclick={() => onwatch(live ? undefined : p.start)}>
          <time>{time(p.start)}</time>
          <span class="body">
            <span class="name">{p.title}</span>
            {#if live}
              {#if p.desc}<span class="desc">{p.desc}</span>{/if}
              <span class="progress"><span style:width="{((now - p.start) / (p.end - p.start)) * 100}%"></span></span>
            {/if}
          </span>
          {#if live}<span class="pill">LIVE</span>
          {:else if p.archive}<span class="replay"><Icon name="play" size={14} /></span>{/if}
        </button>
      </li>
    {/each}
  </ol>
</section>

<style>
  section { display: flex; flex-direction: column; min-height: 0; height: 100%; }
  header { display: flex; align-items: center; gap: 0.85rem; padding: 1.25rem 1.25rem 0.75rem; }
  .logo { flex: none; width: 3.5rem; height: 3.5rem; border-radius: 12px; background: var(--surface-2); display: grid; place-items: center; padding: 0.5rem; }
  .logo img { max-width: 100%; max-height: 100%; object-fit: contain; }
  .title { flex: 1; min-width: 0; }
  h2 { font-size: 1.1rem; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .title p { display: flex; align-items: center; gap: 0.3rem; color: var(--muted); font-size: 0.85rem; margin-top: 0.15rem; }
  .close { display: none; }
  .live { margin: 0 1.25rem 0.75rem; height: 2.5rem; background: var(--text); color: var(--bg); }
  .live:hover { background: #fff !important; }
  .dot { width: 0.5rem; height: 0.5rem; border-radius: 50%; background: var(--live); box-shadow: 0 0 0 3px rgb(240 72 72 / 0.25); }
  .state { display: grid; place-items: center; padding: 2rem 1.25rem; color: var(--muted); text-align: center; }

  ol { flex: 1; overflow: auto; padding: 0 0.75rem 1.25rem; }
  .day { position: sticky; top: 0; z-index: 1; padding: 0.9rem 0.5rem 0.4rem; background: var(--surface); color: var(--faint);
    font-size: 0.75rem; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; }
  li button { display: flex; align-items: flex-start; gap: 0.85rem; width: 100%; padding: 0.6rem 0.5rem; border-radius: 8px; text-align: left;
    transition: background 0.15s var(--ease); }
  li button:hover:not(:disabled) { background: var(--surface-2); }
  li button:disabled { cursor: default; }
  time { flex: none; width: 2.9rem; color: var(--muted); font-variant-numeric: tabular-nums; font-size: 0.85rem; padding-top: 0.05rem; }
  .body { flex: 1; min-width: 0; display: grid; gap: 0.35rem; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .past .name, .past time { color: var(--muted); }
  li:not(.past):not(.now) button:disabled .name { color: var(--muted); }
  .desc { color: var(--muted); font-size: 0.85rem; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .now button { background: var(--surface-2); }
  .now .name { font-weight: 600; }
  .progress { height: 3px; border-radius: 3px; background: var(--surface-3); overflow: hidden; }
  .progress span { display: block; height: 100%; background: var(--live); }
  .pill { flex: none; font-size: 0.68rem; font-weight: 700; letter-spacing: 0.05em; color: var(--live); padding-top: 0.15rem; }
  .replay { flex: none; color: var(--faint); opacity: 0; transition: opacity 0.15s var(--ease); padding-top: 0.1rem; }
  li button:hover .replay { opacity: 1; color: var(--text); }

  @media (max-width: 900px) { .close { display: inline-grid; } }
</style>
