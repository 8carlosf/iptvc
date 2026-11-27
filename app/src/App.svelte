<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { core, type Facet, type Item, type Kind, type Programme } from './lib/core';
  import { format, parse, type Route } from './lib/route';
  import { M3u } from './lib/m3u';
  import type { Provider } from './lib/provider';
  import { Xtream, type Category, type Episode, type SeriesInfo } from './lib/xtream';
  import type { Media } from './Player.svelte';
  import Guide from './Guide.svelte';
  import Icon from './Icon.svelte';
  import LoginForm, { type Sources } from './Login.svelte';
  import Series from './Series.svelte';

  const PAGE = 120;
  const TABS: { kind: Kind; label: string; noun: string }[] = [
    { kind: 'live', label: 'Live TV', noun: 'channels' },
    { kind: 'vod', label: 'Movies', noun: 'movies' },
    { kind: 'series', label: 'Series', noun: 'series' },
  ];

  const DEFAULT_PLAYLIST = 'https://iptv-org.github.io/iptv/index.m3u';

  /** What the sign-in form remembers, per source type; never passwords. */
  function savedSources(): Sources {
    const saved = JSON.parse(localStorage.sources ?? 'null');
    const old = JSON.parse(localStorage.login ?? 'null'); // earlier versions kept a password here: drop it
    localStorage.removeItem('login');
    return {
      mode: saved?.mode ?? 'm3u',
      m3u: { url: saved?.m3u?.url ?? DEFAULT_PLAYLIST, guide: saved?.m3u?.guide ?? '', username: saved?.m3u?.username ?? '' },
      xtream: { server: saved?.xtream?.server ?? old?.server ?? '', username: saved?.xtream?.username ?? old?.username ?? '' },
    };
  }
  let sources = $state<Sources>(savedSources());
  let password = $state(''); // for this session only
  let xt = $state<Provider | null>(null);
  let signingIn = $state(false);
  let loading = $state(false);
  let error = $state('');

  let tab = $state<Kind>('live');
  let cats = $state<Partial<Record<Kind, Category[]>>>({});
  let cat = $state<Partial<Record<Kind, string>>>({});
  let q = $state('');
  let country = $state(''); // M3U: country filter (code), alongside the category
  let facet = $state<'cat' | 'country'>('cat'); // which filter the sidebar lists
  let items = $state<Item[]>([]);
  let total = $state(0);

  let channel = $state<Item | null>(null);
  let guide = $state<Programme[]>([]);
  let guideLoading = $state(false);
  let guideLoad: Promise<void> = Promise.resolve();
  let show = $state<SeriesInfo | null>(null);
  let showId = 0;
  let media = $state<Media | null>(null);
  let mediaKey = ''; // what's playing, so re-applying the same route doesn't restart it
  let route = $state<Route>(parse(location.hash));
  let listKey = '';

  let scroller = $state<HTMLElement>();
  let sentinel = $state<HTMLElement>();
  let searchBox = $state<HTMLInputElement>();
  const loaded = new Set<Kind>();
  const noun = $derived(TABS.find((t) => t.kind === tab)!.noun);
  const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

  async function connect() {
    error = '';
    signingIn = true;
    try {
      localStorage.sources = JSON.stringify(sources);
      const x: Provider = sources.mode === 'm3u'
        ? new M3u(sources.m3u.url.trim(), sources.m3u.guide.trim(), sources.m3u.username.trim(), password)
        : new Xtream({ server: sources.xtream.server, username: sources.xtream.username, password });
      await x.auth();
      xt = x;
      await apply(parse(location.hash));
    } catch (e) {
      error = message(e);
      xt = null;
    } finally {
      signingIn = false;
    }
  }

  /** Goes somewhere: the URL (a new history entry unless `replace`) and then the view follow. */
  function go(r: Route, replace = false) {
    const hash = format(r);
    if (hash !== location.hash) history[replace ? 'replaceState' : 'pushState']({ watch: !!r.watch }, '', hash);
    void apply(r);
  }

  let applying = 0;
  /** Makes the view match `r` (links, back/forward, and every navigation in the app go through here). */
  async function apply(r: Route) {
    const token = ++applying;
    const stale = () => token !== applying;
    if (xt && !xt.kinds.includes(r.tab)) r = { tab: xt.kinds[0] }; // e.g. a movie link on a playlist source
    route = r;
    if (!xt) return;
    if (!loaded.has(r.tab)) await load(r.tab);
    if (stale()) return;
    tab = r.tab;
    cat[tab] = r.cat ?? '';
    country = r.country ?? '';
    q = r.q ?? '';
    const key = `${tab}|${cat[tab]}|${country}|${q}`;
    if (key !== listKey) { listKey = key; search(); }
    const item = r.id !== undefined ? core.query(tab, { ids: [r.id], limit: 1 }).items[0] : undefined;

    if (tab === 'live' && item) {
      if (channel?.id !== item.id) select(item);
    } else channel = null;
    if (tab === 'series' && r.id !== undefined) {
      if (showId !== r.id) {
        try {
          const info = await xt.series!(r.id);
          if (stale()) return;
          show = info;
          showId = r.id;
          await tick();
          scroller?.scrollTo(0, 0);
        } catch (e) {
          error = message(e);
        }
      }
    } else { show = null; showId = 0; }

    if (!r.watch) return void ((media = null), (mediaKey = ''));
    const want = `${tab}|${r.id}|${r.at ?? ''}|${r.episode ?? ''}`;
    if (want === mediaKey) return;
    if (tab === 'live' && item) {
      await guideLoad;
      if (stale()) return;
      media = liveMedia(item, r.at);
    } else if (tab === 'vod' && item) media = { title: item.name, url: xt.movie!(item.id, item.ext) };
    else if (tab === 'series' && show) {
      const ep = Object.values(show.episodes).flat().find((e) => e.id === r.episode);
      if (ep) media = { title: ep.title, url: xt.episode!(ep.id, ep.container_extension) };
    }
    mediaKey = media ? want : '';
  }

  async function load(kind: Kind) {
    loading = true;
    items = [];
    listKey = '';
    try {
      [cats[kind]] = await Promise.all([xt!.categories(kind), xt!.load(kind)]);
      loaded.add(kind);
    } catch (e) {
      error = message(e);
    } finally {
      loading = false;
    }
  }

  function search(more = false) {
    if (!loaded.has(tab)) return;
    const r = core.query(tab, { cat: q ? '' : cat[tab], country: q ? '' : country, q, offset: more ? items.length : 0, limit: PAGE });
    items = more ? [...items, ...r.items] : r.items;
    total = r.total;
    if (!more) scroller?.scrollTo(0, 0);
  }

  /** Selects a live channel: its guide loads alongside. */
  function select(item: Item) {
    channel = item;
    guide = [];
    guideLoading = true;
    // By id: `channel` holds Svelte's reactive proxy of `item`, never `item` itself.
    const current = () => channel?.id === item.id;
    guideLoad = xt!.epg(item).then((g) => { if (current()) guide = g; }, () => {})
      .finally(() => { if (current()) guideLoading = false; });
  }

  /** A channel's player: live, rewindable through its archive, starting at `from` (unix) if given. */
  function liveMedia(item: Item, from?: number): Media {
    const { id, name, archive } = item;
    const x = xt!;
    const rewind = archive && x.timeshift
      ? { at: (t: number) => x.timeshift!(id, t, (Date.now() / 1000 - t) / 60 + 1), window: archive * 86400 }
      : undefined;
    return { title: name, url: x.live(item), live: true, rewind, from, guide };
  }

  // The list a click came from stays in the URL alongside what was opened.
  const here = () => ({ tab, cat: route.cat, country: route.country, q: route.q });

  // Sidebar filters. M3U lists are counted by the core, each narrowed by the other filter; Xtream has its own
  // category list (and no countries).
  const m3u = $derived(xt instanceof M3u);
  const catList = $derived<(Facet & { label: string })[]>(m3u
    ? core.categories(tab, country).map((c) => ({ ...c, label: c.id }))
    : (cats[tab] ?? []).map((c) => ({ id: c.category_id, count: 0, label: c.category_name })));
  const countryList = $derived<Facet[]>(m3u ? core.countries(tab, cat[tab] ?? '') : []);
  const regions = new Intl.DisplayNames(['en'], { type: 'region' });
  /** iptv-org writes the United Kingdom as "uk"; ISO says "GB". */
  const iso = (code: string) => (code === 'uk' ? 'GB' : code.toUpperCase());
  const countryName = (code: string) => { try { return regions.of(iso(code)) ?? code; } catch { return code.toUpperCase(); } };
  const flag = (code: string) => String.fromCodePoint(...[...iso(code)].map((c) => 0x1f1a5 + c.charCodeAt(0)));
  const catLabel = $derived(catList.find((c) => c.id === cat[tab])?.label ?? cat[tab] ?? '');
  const filterCat = (id: string) => go({ tab, cat: id || undefined, country: route.country });
  const filterCountry = (code: string) => go({ tab, cat: route.cat, country: code || undefined });
  const open = (it: Item, watch = false) => go({ ...here(), id: it.id, watch: watch || tab === 'vod' });
  const watchChannel = (from?: number) => channel && go({ ...here(), id: channel.id, watch: true, at: from });
  const playEpisode = (ep: Episode) => go({ ...here(), id: showId, watch: true, episode: ep.id });

  /** Closing the player steps back if opening it was a step; otherwise it just drops `watch`. */
  function closePlayer() {
    if (history.state?.watch) history.back();
    else go({ ...route, watch: false, at: undefined, episode: undefined }, true);
  }

  /** Catch-up position from the player: the link follows it, without reopening anything. */
  function moved(at: number | null) {
    route = { ...route, at: at ?? undefined };
    mediaKey = `${tab}|${route.id}|${route.at ?? ''}|`;
    history.replaceState(history.state, '', format(route));
  }

  // Returning users go straight in; the form shows only if that fails.
  // Returning users of a playlist without an account go straight in; with a password, the form asks for it.
  onMount(() => { if (localStorage.sources && sources.mode === 'm3u' && sources.m3u.url && !sources.m3u.username) void connect(); });

  function signOut() {
    xt = null;
    password = '';
    loaded.clear(); // the next source may be a different one
    cats = {};
  }

  const tabs = $derived(TABS.filter((t) => xt?.kinds.includes(t.kind)));

  // Infinite scroll: the next page loads as the end of the list nears.
  $effect(() => {
    if (!sentinel || !scroller) return;
    const io = new IntersectionObserver(([e]) => e.isIntersecting && items.length < total && search(true), { root: scroller, rootMargin: '600px' });
    io.observe(sentinel);
    return () => io.disconnect();
  });

  function key(e: KeyboardEvent) {
    if (media || !xt) return;
    const typing = e.target instanceof HTMLInputElement;
    if (e.key === '/' && !typing) { e.preventDefault(); searchBox?.focus(); }
    else if (e.key === 'Escape' && typing && q) go({ tab, cat: route.cat }, true);
    else if (e.key === 'Escape' && channel) go(here());
  }
</script>

<svelte:window onkeydown={key} onpopstate={() => apply(parse(location.hash))} />

{#if !xt && signingIn && !error && localStorage.sources}
  <div class="state full-page"><span class="spinner"></span></div>
{:else if !xt}
  <LoginForm bind:sources bind:password busy={signingIn} {error} onsubmit={connect} />
{:else}
  <div class="app" class:with-guide={!!channel}>
    <header class="top">
      <div class="brand"><span class="mark"><Icon name="tv" size={18} /></span><span>iptvc</span></div>
      <nav class="tabs" aria-label="Sections">
        {#each tabs as t}
          <button class:on={tab === t.kind} aria-current={tab === t.kind ? 'page' : undefined} onclick={() => go({ tab: t.kind })}>{t.label}</button>
        {/each}
      </nav>
      <label class="search">
        <Icon name="search" size={17} />
        <input bind:this={searchBox} type="search" placeholder="Search {noun}" bind:value={q} oninput={() => go({ tab, cat: route.cat, q: q || undefined }, true)} />
        {#if !q}<kbd>/</kbd>{/if}
      </label>
      <button class="icon-btn" onclick={signOut} title="Sign out" aria-label="Sign out"><Icon name="logout" size={19} /></button>
    </header>

    <nav class="cats" aria-label="Filters">
      {#if countryList.length}
        <div class="facets" role="tablist">
          <button role="tab" aria-selected={facet === 'cat'} class:on={facet === 'cat'} onclick={() => (facet = 'cat')}>Categories</button>
          <button role="tab" aria-selected={facet === 'country'} class:on={facet === 'country'} onclick={() => (facet = 'country')}>Countries</button>
        </div>
        <!-- The other filter, when set, stays visible and removable. -->
        {#if facet === 'cat' && country}<button class="chip-filter" onclick={() => filterCountry('')}>{flag(country)} {countryName(country)} <Icon name="close" size={13} /></button>{/if}
        {#if facet === 'country' && cat[tab]}<button class="chip-filter" onclick={() => filterCat('')}>{catLabel} <Icon name="close" size={13} /></button>{/if}
      {/if}
      {#if facet === 'country' && countryList.length}
        <button class:on={!country && !q} onclick={() => filterCountry('')}>All countries</button>
        {#each countryList as c (c.id)}
          <button class:on={country === c.id && !q} onclick={() => filterCountry(c.id)}>
            <span class="label">{flag(c.id)} {countryName(c.id)}</span><span class="count">{c.count}</span>
          </button>
        {/each}
      {:else}
        <button class:on={!cat[tab] && !q} onclick={() => filterCat('')}>All {noun}</button>
        {#each catList as c (c.id)}
          <button class:on={cat[tab] === c.id && !q} onclick={() => filterCat(c.id)}>
            <span class="label">{c.label}</span>{#if c.count}<span class="count">{c.count}</span>{/if}
          </button>
        {/each}
      {/if}
    </nav>

    <main bind:this={scroller}>
      {#if error}<p class="banner" role="alert">{error} <button onclick={() => (error = '')} aria-label="Dismiss"><Icon name="close" size={16} /></button></p>{/if}
      {#if show}
        <Series info={show} image={xt.image} onplay={playEpisode} onback={() => go(here())} />
      {:else if loading}
        <div class="state"><span class="spinner"></span><p>Loading {noun}…</p></div>
      {:else}
        <div class="heading">
          <h1>{q ? `Results for “${q}”` : [catLabel, country && countryName(country)].filter(Boolean).join(' · ') || `All ${noun}`}</h1>
          <span>{total.toLocaleString()}</span>
        </div>
        {#if tab === 'live'}
          <ul class="channels">
            {#each items as it (it.id)}
              <li class:on={channel?.id === it.id}>
                <button class="row" onclick={() => open(it)} ondblclick={() => open(it, true)} title={it.name}>
                  <span class="logo">{#if it.icon}<img src={xt.image(it.icon)} alt="" loading="lazy" onerror={(e) => (e.currentTarget as HTMLImageElement).remove()} />{/if}</span>
                  <span class="name">{it.name}</span>
                  {#if it.archive}<span class="chip" title="Catch-up: {it.archive} days"><Icon name="history" size={13} />{it.archive}d</span>{/if}
                </button>
                <button class="quick" onclick={() => open(it, true)} aria-label="Play {it.name}"><Icon name="play" size={15} /></button>
              </li>
            {/each}
          </ul>
        {:else}
          <ul class="posters">
            {#each items as it (it.id)}
              <li>
                <button onclick={() => open(it)} title={it.name}>
                  <span class="poster">
                    {#if it.icon}<img src={xt.image(it.icon)} alt="" loading="lazy" onerror={(e) => (e.currentTarget as HTMLImageElement).remove()} />{/if}
                    <span class="initial">{it.name.slice(0, 1)}</span>
                    {#if it.rating > 0}<span class="score"><Icon name="star" size={11} />{it.rating.toFixed(1)}</span>{/if}
                    <span class="hover"><Icon name="play" size={26} /></span>
                  </span>
                  <span class="caption">{it.name}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        {#if !items.length}<div class="state"><p>{q ? `No ${noun} match “${q}”.` : `No ${noun} here.`}</p></div>{/if}
        <div bind:this={sentinel} class="sentinel"></div>
      {/if}
    </main>

    {#if channel}
      <aside class="guide">
        <Guide {channel} {guide} loading={guideLoading} logo={channel.icon ? xt.image(channel.icon) : ''} onwatch={watchChannel} onclose={() => go(here())} />
      </aside>
    {/if}
  </div>
{/if}

{#if media}
  <div class="overlay">
    <!-- Loaded on first play: it carries the codec wasm (AC-3, DTS), which browsing doesn't need. -->
    {#await import('./Player.svelte')}
      <div class="state full"><span class="spinner"></span></div>
    {:then { default: Player }}
      {#key media}<Player {media} onclose={closePlayer} onmove={moved} />{/key}
    {/await}
  </div>
{/if}

<style>
  .app { display: grid; grid-template: 3.75rem 1fr / 15rem 1fr; height: 100vh; }
  .app.with-guide { grid-template-columns: 15rem 1fr 23rem; }

  .top { grid-column: 1 / -1; display: flex; align-items: center; gap: 1.5rem; padding: 0 1rem 0 1.25rem; border-bottom: 1px solid var(--border); }
  .brand { display: flex; align-items: center; gap: 0.55rem; font-weight: 650; width: 12.25rem; flex: none; }
  .mark { display: grid; place-items: center; width: 1.9rem; height: 1.9rem; border-radius: 8px; background: var(--accent-soft); color: var(--accent); }
  .tabs { display: flex; gap: 0.25rem; padding: 0.2rem; border-radius: 10px; background: var(--surface); }
  .tabs button { height: 2rem; padding: 0 0.9rem; border-radius: 8px; color: var(--muted); font-weight: 500; transition: color 0.15s var(--ease), background 0.15s var(--ease); }
  .tabs button:hover { color: var(--text); }
  .tabs .on { background: var(--surface-3); color: var(--text); }
  .search { flex: 1; max-width: 28rem; margin-left: auto; display: flex; align-items: center; gap: 0.5rem; height: 2.4rem; padding: 0 0.75rem;
    border-radius: 9px; background: var(--surface); border: 1px solid transparent; color: var(--faint); transition: border-color 0.15s var(--ease); }
  .search:focus-within { border-color: var(--accent); color: var(--muted); }
  .search input { flex: 1; min-width: 0; height: 100%; border: 0; background: none; outline: none; color: var(--text); }
  .search input::placeholder { color: var(--faint); }
  .search input::-webkit-search-cancel-button { filter: invert(0.6); }
  kbd { font: 500 0.75rem/1 inherit; padding: 0.2rem 0.4rem; border-radius: 5px; border: 1px solid var(--border); color: var(--faint); }

  .cats { overflow: auto; padding: 0.75rem; display: flex; flex-direction: column; gap: 1px; border-right: 1px solid var(--border); }
  .cats button { text-align: left; padding: 0.5rem 0.75rem; border-radius: 8px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    transition: background 0.15s var(--ease), color 0.15s var(--ease); }
  .cats button:hover { background: var(--surface); color: var(--text); }
  .cats .on { background: var(--accent-soft) !important; color: var(--text); font-weight: 500; }
  .cats button { display: flex; align-items: center; gap: 0.5rem; }
  .cats .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .cats .count { flex: none; font-size: 0.75rem; color: var(--faint); font-variant-numeric: tabular-nums; }
  .facets { display: grid; grid-template-columns: 1fr 1fr; gap: 0.2rem; padding: 0.2rem; margin-bottom: 0.5rem; border-radius: 9px; background: var(--surface); }
  .facets button { justify-content: center; padding: 0.35rem 0.5rem; font-size: 0.85rem; }
  .facets .on { background: var(--surface-3) !important; }
  .chip-filter { align-self: flex-start; margin: 0 0 0.5rem; padding: 0.3rem 0.6rem !important; border-radius: 999px !important;
    background: var(--accent-soft) !important; color: var(--text) !important; font-size: 0.82rem; }

  main { overflow: auto; padding: 1.5rem; min-width: 0; }
  .heading { display: flex; align-items: baseline; gap: 0.6rem; margin-bottom: 1rem; }
  .heading h1 { font-size: 1.35rem; font-weight: 650; letter-spacing: -0.01em; }
  .heading span { color: var(--faint); font-variant-numeric: tabular-nums; }
  .state { display: grid; place-items: center; gap: 0.75rem; padding: 4rem 1rem; color: var(--muted); }
  .state.full { height: 100%; }
  .state.full-page { height: 100vh; }
  .banner { display: flex; align-items: center; justify-content: space-between; gap: 1rem; padding: 0.7rem 0.9rem; margin-bottom: 1rem;
    border-radius: 9px; background: rgb(240 72 72 / 0.12); color: #ffb4b4; }
  .sentinel { height: 1px; }

  .channels { display: grid; grid-template-columns: repeat(auto-fill, minmax(17rem, 1fr)); gap: 0.4rem; }
  .channels li { position: relative; }
  .row { display: flex; align-items: center; gap: 0.8rem; width: 100%; padding: 0.5rem 2.75rem 0.5rem 0.5rem; border-radius: var(--radius);
    border: 1px solid transparent; text-align: left; transition: background 0.15s var(--ease), border-color 0.15s var(--ease); }
  .row:hover { background: var(--surface); }
  .on .row { background: var(--surface); border-color: var(--surface-3); }
  .logo { flex: none; display: grid; place-items: center; width: 3.75rem; height: 2.6rem; padding: 0.35rem; border-radius: 8px; background: var(--surface-2); }
  .logo img { max-width: 100%; max-height: 100%; object-fit: contain; }
  .row .name { flex: 1; min-width: 0; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .chip { flex: none; display: inline-flex; align-items: center; gap: 0.2rem; padding: 0.1rem 0.4rem; border-radius: 6px; font-size: 0.75rem;
    color: var(--muted); background: var(--surface-2); }
  .quick { position: absolute; right: 0.5rem; top: 50%; translate: 0 -50%; display: grid; place-items: center; width: 2rem; height: 2rem;
    border-radius: 50%; background: var(--text); color: var(--bg); opacity: 0; transition: opacity 0.15s var(--ease); }
  li:hover .quick, .quick:focus-visible { opacity: 1; }
  li:hover .chip { visibility: hidden; }

  .posters { display: grid; grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr)); gap: 1.5rem 1rem; }
  .posters button { display: grid; gap: 0.55rem; width: 100%; text-align: left; }
  .poster { position: relative; aspect-ratio: 2 / 3; border-radius: var(--radius); overflow: hidden; background: var(--surface-2);
    transition: transform 0.25s var(--ease), box-shadow 0.25s var(--ease); }
  .poster img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; z-index: 1; }
  .initial { position: absolute; inset: 0; display: grid; place-items: center; font-size: 2.5rem; font-weight: 600; color: var(--surface-3); }
  .score { position: absolute; z-index: 2; top: 0.45rem; right: 0.45rem; display: inline-flex; align-items: center; gap: 0.2rem; padding: 0.15rem 0.4rem;
    border-radius: 6px; font-size: 0.72rem; font-weight: 600; background: rgb(0 0 0 / 0.65); color: #f5c451; backdrop-filter: blur(6px); }
  .hover { position: absolute; z-index: 2; inset: 0; display: grid; place-items: center; background: rgb(0 0 0 / 0.35); opacity: 0; transition: opacity 0.2s var(--ease); }
  .posters button:hover .poster { transform: translateY(-3px); box-shadow: 0 18px 30px -18px rgb(0 0 0 / 0.9); }
  .posters button:hover .hover { opacity: 1; }
  .caption { font-size: 0.875rem; font-weight: 500; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }

  .guide { border-left: 1px solid var(--border); background: var(--surface); min-height: 0; overflow: hidden; }
  .overlay { position: fixed; inset: 0; z-index: 10; background: #000; }

  @media (max-width: 1100px) {
    .app.with-guide { grid-template-columns: 13rem 1fr 20rem; }
    .brand { width: auto; }
  }
  @media (max-width: 900px) {
    .app, .app.with-guide { grid-template: auto auto 1fr / 1fr; }
    .top { flex-wrap: wrap; gap: 0.6rem 1rem; padding: 0.6rem 0.75rem; }
    .brand span:last-child { display: none; }
    .search { order: 3; flex-basis: 100%; max-width: none; }
    .search kbd { display: none; }
    .cats { flex-direction: row; border: 0; border-bottom: 1px solid var(--border); padding: 0.5rem 0.75rem; }
    .cats button { flex: none; padding: 0.35rem 0.8rem; border-radius: 999px; background: var(--surface); }
    main { padding: 1rem 0.75rem; }
    .guide { position: fixed; inset: 0; z-index: 5; border: 0; }
    .posters { grid-template-columns: repeat(auto-fill, minmax(7.5rem, 1fr)); gap: 1rem 0.6rem; }
    .quick { opacity: 1; }
    li .chip { display: none; }
  }
</style>
