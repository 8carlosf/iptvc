// Xtream Codes API: login, categories, stream lists (held by the Rust core), and stream URLs.
import { core, type Item, type Kind } from './core';
import type { Provider } from './provider';

export type Login = { server: string; username: string; password: string };
export type Category = { category_id: string; category_name: string };
export type Episode = { id: string; episode_num: number; title: string; container_extension: string; season: number; info?: { plot?: string; duration?: string; movie_image?: string } };
export type SeriesInfo = {
  info: { name: string; plot?: string; cover?: string; genre?: string; releaseDate?: string; rating?: string; backdrop_path?: string[] };
  episodes: Record<string, Episode[]>;
};

const ACTIONS: Record<Kind, [string, string]> = {
  live: ['get_live_categories', 'get_live_streams'],
  vod: ['get_vod_categories', 'get_vod_streams'],
  series: ['get_series_categories', 'get_series'],
};

export class Xtream implements Provider {
  readonly kinds: Kind[] = ['live', 'vod', 'series'];
  readonly base: string;
  /** The origin the provider writes into image URLs (server_info), which only `base` actually serves. */
  private upstream = '';
  /** Server-local time minus UTC, in seconds: timeshift URLs are addressed in the server's zone. */
  private tz = 0;

  constructor(readonly login: Login) {
    this.base = new URL(login.server.replace(/\/+$/, '') + '/', location.href).href.replace(/\/$/, '');
  }

  private api(params: Record<string, string | number> = {}): string {
    const q = new URLSearchParams({ username: this.login.username, password: this.login.password });
    for (const [k, v] of Object.entries(params)) q.set(k, String(v));
    return `${this.base}/player_api.php?${q}`;
  }

  private async get(params: Record<string, string | number> = {}): Promise<string> {
    const r = await fetch(this.api(params));
    if (!r.ok) throw new Error(`${params.action ?? 'login'}: HTTP ${r.status}`);
    return r.text();
  }

  async auth(): Promise<void> {
    const r = JSON.parse(await this.get());
    if (r?.user_info?.auth !== 1) throw new Error('Login refused');
    const s = r.server_info ?? {};
    if (s.url) this.upstream = `${s.server_protocol ?? 'http'}://${s.url}${s.port ? `:${s.port}` : ''}`;
    // shortcut: today's offset for the whole archive, so across a DST change older catch-up starts an hour off;
    // the EPG's local `start` strings would fix that if it matters.
    if (s.time_now && s.timestamp_now)
      this.tz = Math.round((Date.parse(s.time_now.replace(' ', 'T') + 'Z') / 1000 - Number(s.timestamp_now)) / 900) * 900;
  }

  categories = async (kind: Kind): Promise<Category[]> => JSON.parse(await this.get({ action: ACTIONS[kind][0] }));
  load = async (kind: Kind): Promise<number> => core.load(kind, await this.get({ action: ACTIONS[kind][1] }));
  epg = async (item: Item) => core.epg(await this.get({ action: 'get_simple_data_table', stream_id: item.id }));
  series = async (id: number): Promise<SeriesInfo> => JSON.parse(await this.get({ action: 'get_series_info', series_id: id }));

  image = (url: string) => (this.upstream && url.startsWith(this.upstream) ? this.base + url.slice(this.upstream.length) : url);

  private path = (kind: string, rest: string) => `${this.base}/${kind}/${encodeURIComponent(this.login.username)}/${encodeURIComponent(this.login.password)}/${rest}`;
  live = (item: Item) => this.path('live', `${item.id}.ts`);
  movie = (id: number, ext: string) => this.path('movie', `${id}.${ext}`);
  episode = (id: string, ext: string) => this.path('series', `${id}.${ext}`);
  /** Catch-up from unix time `t`, for `minutes`. */
  timeshift = (id: number, t: number, minutes: number) => {
    const d = new Date((t + this.tz) * 1000).toISOString(); // "YYYY-MM-DDTHH:MM…" in the server's zone
    return this.path('timeshift', `${Math.max(1, Math.ceil(minutes))}/${d.slice(0, 10)}:${d.slice(11, 13)}-${d.slice(14, 16)}/${id}.ts`);
  };
}
