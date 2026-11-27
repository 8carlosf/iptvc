// An M3U playlist (live channels only), with an optional XMLTV guide matched by each channel's tvg-id.
import { core, type Item, type Kind } from './core';
import type { Provider } from './provider';

export class M3u implements Provider {
  readonly kinds: Kind[] = ['live'];
  private guide: Promise<unknown> = Promise.resolve();

  constructor(readonly url: string, readonly guideUrl = '', private username = '', private password = '') {}

  /** The panel convention for M3U accounts: username/password as query parameters. */
  private withAccount(url: string) {
    if (!this.username && !this.password) return url;
    const u = new URL(url);
    if (this.username) u.searchParams.set('username', this.username);
    if (this.password) u.searchParams.set('password', this.password);
    return u.href;
  }

  async auth() {
    const r = await fetch(this.withAccount(this.url));
    if (!r.ok) throw new Error(`Playlist: HTTP ${r.status}`);
    if (!core.loadM3u(await r.text())) throw new Error('No channels found in this playlist');
    // The guide can be large; channels show at once and guides fill in when it's ready.
    if (this.guideUrl) this.guide = loadGuide(this.withAccount(this.guideUrl)).catch((e) => console.warn('Guide:', e));
  }

  categories = async () => core.categories('live').map((c) => ({ category_id: c.id, category_name: c.id }));
  load = async () => 0; // auth already loaded the playlist
  epg = async (item: Item) => { await this.guide; return item.ext ? core.guide(item.ext) : []; };
  image = (url: string) => url;
  live = (item: Item) => item.url;
}

/** Fetches an XMLTV guide (plain or gzipped, told apart by its first bytes) into the Rust core. */
async function loadGuide(url: string) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  let bytes = new Uint8Array(await r.arrayBuffer());
  if (bytes[0] === 0x1f && bytes[1] === 0x8b)
    bytes = new Uint8Array(await new Response(new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'))).arrayBuffer());
  core.loadXmltv(new TextDecoder().decode(bytes));
}
