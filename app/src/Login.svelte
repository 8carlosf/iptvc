<script lang="ts" module>
  /** What the form remembers between visits, per source type (passwords never). */
  export type Sources = {
    mode: 'm3u' | 'xtream';
    m3u: { url: string; guide: string; username: string };
    xtream: { server: string; username: string };
  };
</script>

<script lang="ts">
  import Icon from './Icon.svelte';

  let { sources = $bindable(), password = $bindable(), busy, error, onsubmit }: {
    sources: Sources; password: string; busy: boolean; error: string; onsubmit: () => void;
  } = $props();

  /** Credentials pasted inside a playlist URL move to the fields, so the remembered URL never holds the password. */
  function liftCredentials() {
    try {
      const u = new URL(sources.m3u.url.trim());
      const user = u.searchParams.get('username');
      const pass = u.searchParams.get('password');
      if (user === null && pass === null) return;
      if (user !== null) sources.m3u.username = user;
      if (pass !== null) password = pass;
      u.searchParams.delete('username');
      u.searchParams.delete('password');
      sources.m3u.url = u.href;
    } catch { /* not a URL yet */ }
  }

  function setMode(mode: Sources['mode']) {
    if (mode !== sources.mode) password = ''; // each source's password is its own
    sources.mode = mode;
  }
</script>

<main>
  <form onsubmit={(e) => { e.preventDefault(); onsubmit(); }}>
    <div class="brand"><span class="mark"><Icon name="tv" size={22} /></span> iptvc</div>
    <h1>Add your TV source</h1>

    <div class="modes" role="radiogroup" aria-label="Source type">
      <button type="button" role="radio" aria-checked={sources.mode === 'm3u'} class:on={sources.mode === 'm3u'} onclick={() => setMode('m3u')}>M3U playlist</button>
      <button type="button" role="radio" aria-checked={sources.mode === 'xtream'} class:on={sources.mode === 'xtream'} onclick={() => setMode('xtream')}>Xtream Codes</button>
    </div>

    {#if sources.mode === 'm3u'}
      <label>Playlist URL <input class="field" bind:value={sources.m3u.url} onchange={liftCredentials} placeholder="https://example.com/playlist.m3u" inputmode="url" required /></label>
      <fieldset>
        <legend class="row">Account <span class="optional">Optional</span></legend>
        <div class="pair">
          <input class="field" bind:value={sources.m3u.username} placeholder="Username" autocomplete="username" aria-label="Username" />
          <input class="field" type="password" bind:value={password} placeholder="Password" autocomplete="current-password" aria-label="Password" />
        </div>
      </fieldset>
      <label>
        <span class="row">Guide URL <span class="optional">Optional</span></span>
        <input class="field" bind:value={sources.m3u.guide} placeholder="https://example.com/guide.xml" inputmode="url" />
        <small>XMLTV, .xml or .xml.gz</small>
      </label>
    {:else}
      <label>Server <input class="field" bind:value={sources.xtream.server} placeholder="https://iptv.example.com" inputmode="url" required /></label>
      <label>Username <input class="field" bind:value={sources.xtream.username} autocomplete="username" required /></label>
      <label>Password <input class="field" type="password" bind:value={password} autocomplete="current-password" required /></label>
    {/if}

    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <button class="btn primary" disabled={busy}>{#if busy}<span class="spinner small"></span>Connecting{:else}Continue{/if}</button>
  </form>
</main>

<style>
  main { min-height: 100vh; display: grid; place-items: center; padding: 1.5rem;
    background: radial-gradient(60rem 30rem at 50% -10%, rgb(124 156 255 / 0.10), transparent 70%); }
  form { width: min(26rem, 100%); display: grid; gap: 1rem; padding: 2rem; border: 1px solid var(--border);
    border-radius: 16px; background: var(--surface); box-shadow: 0 30px 60px -30px rgb(0 0 0 / 0.6); }
  .brand { display: flex; align-items: center; gap: 0.6rem; font-weight: 650; font-size: 1.05rem; margin-bottom: 0.25rem; }
  .mark { display: grid; place-items: center; width: 2.25rem; height: 2.25rem; border-radius: 9px; background: var(--accent-soft); color: var(--accent); }
  h1 { font-size: 1.25rem; font-weight: 600; letter-spacing: -0.01em; }
  .modes { display: grid; grid-template-columns: 1fr 1fr; gap: 0.25rem; padding: 0.25rem; border-radius: 10px; background: var(--bg); }
  .modes button { height: 2.1rem; border-radius: 7px; color: var(--muted); font-weight: 500; transition: background 0.15s var(--ease), color 0.15s var(--ease); }
  .modes button:hover { color: var(--text); }
  .modes .on { background: var(--surface-3); color: var(--text); }
  label { display: grid; gap: 0.35rem; font-size: 0.85rem; font-weight: 500; color: var(--muted); }
  .row { display: flex; align-items: center; gap: 0.5rem; }
  .optional { padding: 0.05rem 0.45rem; border-radius: 999px; font-size: 0.72rem; font-weight: 600; color: var(--accent); background: var(--accent-soft); }
  small { color: var(--faint); font-weight: 400; font-size: 0.8rem; }
  fieldset { display: grid; gap: 0.35rem; margin: 0; padding: 0; border: 0; min-width: 0; }
  legend { padding: 0; font-size: 0.85rem; font-weight: 500; color: var(--muted); margin-bottom: 0.35rem; }
  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 0.5rem; }
  .btn { height: 2.6rem; }
  .err { color: var(--live); font-size: 0.9rem; }
  .small { width: 1rem; height: 1rem; border-color: rgb(0 0 0 / 0.2); border-top-color: var(--bg); }
</style>
