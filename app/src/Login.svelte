<script lang="ts">
  import type { Login } from './lib/xtream';
  import Icon from './Icon.svelte';

  let { login = $bindable(), busy, error, onsubmit }: { login: Login; busy: boolean; error: string; onsubmit: () => void } = $props();
</script>

<main>
  <form onsubmit={(e) => { e.preventDefault(); onsubmit(); }}>
    <div class="brand"><span class="mark"><Icon name="tv" size={22} /></span> iptvc</div>
    <h1>Sign in to your provider</h1>
    <p class="sub">Your Xtream Codes server and account. They stay in this browser.</p>
    <label>Server <input class="field" bind:value={login.server} placeholder="https://iptv.example.com" inputmode="url" required /></label>
    <label>Username <input class="field" bind:value={login.username} autocomplete="username" required /></label>
    <label>Password <input class="field" type="password" bind:value={login.password} autocomplete="current-password" required /></label>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <button class="btn primary" disabled={busy}>{#if busy}<span class="spinner small"></span>Signing in{:else}Sign in{/if}</button>
  </form>
</main>

<style>
  main { min-height: 100vh; display: grid; place-items: center; padding: 1.5rem;
    background: radial-gradient(60rem 30rem at 50% -10%, rgb(124 156 255 / 0.10), transparent 70%); }
  form { width: min(24rem, 100%); display: grid; gap: 1rem; padding: 2rem; border: 1px solid var(--border);
    border-radius: 16px; background: var(--surface); box-shadow: 0 30px 60px -30px rgb(0 0 0 / 0.6); }
  .brand { display: flex; align-items: center; gap: 0.6rem; font-weight: 650; font-size: 1.05rem; margin-bottom: 0.5rem; }
  .mark { display: grid; place-items: center; width: 2.25rem; height: 2.25rem; border-radius: 9px; background: var(--accent-soft); color: var(--accent); }
  h1 { font-size: 1.25rem; font-weight: 600; letter-spacing: -0.01em; }
  .sub { color: var(--muted); margin-top: -0.5rem; }
  label { display: grid; gap: 0.35rem; font-size: 0.85rem; font-weight: 500; color: var(--muted); }
  .btn { height: 2.6rem; margin-top: 0.25rem; }
  .err { color: var(--live); font-size: 0.9rem; }
  .small { width: 1rem; height: 1rem; border-color: rgb(0 0 0 / 0.2); border-top-color: var(--bg); }
</style>
