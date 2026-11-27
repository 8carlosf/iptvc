<script lang="ts">
  import type { Episode, SeriesInfo } from './lib/xtream';
  import Icon from './Icon.svelte';

  let { info, image, onplay, onback }: {
    info: SeriesInfo; image: (url: string) => string; onplay: (ep: Episode) => void; onback: () => void;
  } = $props();

  const seasons = $derived(Object.keys(info.episodes));
  let season = $state('');
  const current = $derived(season || seasons[0] || '');
  const backdrop = $derived(info.info.backdrop_path?.[0]);
  const year = $derived(info.info.releaseDate?.slice(0, 4));
  const minutes = (d?: string) => (d ? `${Math.round(d.split(':').reduce((a, x) => a * 60 + Number(x), 0) / 60)} min` : '');
</script>

<article>
  <header style:--backdrop={backdrop ? `url("${image(backdrop)}")` : 'none'}>
    <button class="btn back" onclick={onback}><Icon name="back" size={18} /> Back</button>
    <div class="hero">
      {#if info.info.cover}<img class="cover" src={image(info.info.cover)} alt="" />{/if}
      <div>
        <h1>{info.info.name}</h1>
        <p class="meta">
          {#if year}<span>{year}</span>{/if}
          {#if info.info.genre}<span>{info.info.genre}</span>{/if}
          {#if Number(info.info.rating)}<span class="rating"><Icon name="star" size={13} /> {info.info.rating}</span>{/if}
          <span>{seasons.length} {seasons.length === 1 ? 'season' : 'seasons'}</span>
        </p>
        {#if info.info.plot}<p class="plot">{info.info.plot}</p>{/if}
      </div>
    </div>
  </header>

  {#if seasons.length > 1}
    <div class="seasons" role="tablist">
      {#each seasons as s}
        <button role="tab" aria-selected={current === s} class:on={current === s} onclick={() => (season = s)}>Season {s}</button>
      {/each}
    </div>
  {/if}

  <ol>
    {#each info.episodes[current] ?? [] as ep (ep.id)}
      <li>
        <button onclick={() => onplay(ep)}>
          <span class="thumb">
            {#if ep.info?.movie_image}<img src={image(ep.info.movie_image)} alt="" loading="lazy" onerror={(e) => (e.currentTarget as HTMLImageElement).remove()} />{/if}
            <span class="play"><Icon name="play" size={18} /></span>
          </span>
          <span class="text">
            <span class="ep">Episode {ep.episode_num}{#if ep.info?.duration}<span> · {minutes(ep.info.duration)}</span>{/if}</span>
            <span class="name">{ep.title.replace(/^.* - S\d+E\d+ - /, '')}</span>
            {#if ep.info?.plot}<span class="plot">{ep.info.plot}</span>{/if}
          </span>
        </button>
      </li>
    {/each}
  </ol>
</article>

<style>
  article { padding-bottom: 2rem; }
  header { position: relative; padding: 1.25rem 1.5rem 1.75rem; margin: -1.5rem -1.5rem 0; isolation: isolate; }
  header::before { content: ''; position: absolute; inset: 0; z-index: -1; background: var(--backdrop) center 25% / cover;
    mask-image: linear-gradient(to bottom, rgb(0 0 0 / 0.45), transparent); opacity: 0.6; }
  .back { margin-bottom: 1.5rem; background: rgb(20 22 27 / 0.7); backdrop-filter: blur(8px); }
  .hero { display: flex; gap: 1.5rem; align-items: flex-end; }
  .cover { width: 9.5rem; aspect-ratio: 2 / 3; object-fit: cover; border-radius: var(--radius); box-shadow: 0 20px 40px -15px rgb(0 0 0 / 0.7); flex: none; }
  h1 { font-size: 2rem; font-weight: 650; letter-spacing: -0.02em; line-height: 1.15; }
  .meta { display: flex; flex-wrap: wrap; gap: 0.4rem 1rem; color: var(--muted); margin: 0.5rem 0 0.75rem; }
  .rating { display: inline-flex; align-items: center; gap: 0.25rem; color: #f5c451; }
  .hero .plot { max-width: 46rem; color: var(--muted); }

  .seasons { display: flex; gap: 0.25rem; overflow-x: auto; padding: 0.25rem 0 1rem; border-bottom: 1px solid var(--border); margin-bottom: 0.75rem; }
  .seasons button { flex: none; height: 2.1rem; padding: 0 0.9rem; border-radius: 999px; color: var(--muted); font-weight: 500; }
  .seasons button:hover { color: var(--text); background: var(--surface-2); }
  .seasons .on { color: var(--bg) !important; background: var(--text) !important; }

  ol { display: grid; gap: 0.25rem; }
  li button { display: flex; gap: 1rem; width: 100%; padding: 0.6rem; border-radius: var(--radius); text-align: left; transition: background 0.15s var(--ease); }
  li button:hover { background: var(--surface); }
  .thumb { position: relative; flex: none; width: 11rem; aspect-ratio: 16 / 9; border-radius: 8px; overflow: hidden; background: var(--surface-2); }
  .thumb img { width: 100%; height: 100%; object-fit: cover; }
  .play { position: absolute; inset: 0; display: grid; place-items: center; background: rgb(0 0 0 / 0.35); opacity: 0; transition: opacity 0.15s var(--ease); }
  li button:hover .play { opacity: 1; }
  .text { min-width: 0; display: grid; align-content: start; gap: 0.25rem; padding-top: 0.15rem; }
  .ep { color: var(--faint); font-size: 0.8rem; font-weight: 600; }
  .ep span { font-weight: 500; }
  .name { font-weight: 550; }
  .text .plot { color: var(--muted); font-size: 0.875rem; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }

  @media (max-width: 700px) {
    .hero { flex-direction: column; align-items: flex-start; }
    .cover { width: 7rem; }
    h1 { font-size: 1.5rem; }
    .thumb { width: 8rem; }
  }
</style>
