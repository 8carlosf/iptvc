// Where the app is, as a link: #/live/2/watch/1795740000, #/movies/cat/101?q=gun, #/series/1/watch/20002 …
import type { Kind } from './core';

export type Route = {
  tab: Kind;
  cat?: string;
  q?: string;
  /** Channel, movie or series id. */
  id?: number;
  watch?: boolean;
  /** Live: catch-up from this unix time. */
  at?: number;
  /** Series: the episode playing. */
  episode?: string;
};

const SLUG: Record<Kind, string> = { live: 'live', vod: 'movies', series: 'series' };
const KIND = Object.fromEntries(Object.entries(SLUG).map(([k, v]) => [v, k])) as Record<string, Kind>;

export function parse(hash: string): Route {
  const [path, query = ''] = hash.replace(/^#\/?/, '').split('?');
  const [slug, ...rest] = path.split('/').filter(Boolean).map(decodeURIComponent);
  const r: Route = { tab: KIND[slug] ?? 'live' };
  const q = new URLSearchParams(query).get('q');
  if (q) r.q = q;
  if (rest[0] === 'cat') {
    if (rest[1]) r.cat = rest[1];
    return r;
  }
  const id = Number(rest[0]);
  if (!rest[0] || !Number.isInteger(id)) return r;
  r.id = id;
  if (rest[1] !== 'watch') return r;
  r.watch = true;
  if (r.tab === 'live' && rest[2] && Number.isFinite(Number(rest[2]))) r.at = Number(rest[2]);
  if (r.tab === 'series' && rest[2]) r.episode = rest[2];
  return r;
}

export function format(r: Route): string {
  const parts: (string | number)[] = [SLUG[r.tab]];
  if (r.id !== undefined) {
    parts.push(r.id);
    if (r.watch) {
      parts.push('watch');
      if (r.tab === 'live' && r.at !== undefined) parts.push(Math.round(r.at));
      if (r.tab === 'series' && r.episode) parts.push(r.episode);
    }
  } else if (r.cat) parts.push('cat', r.cat);
  const q = r.q ? `?${new URLSearchParams({ q: r.q })}` : '';
  return `#/${parts.map((p) => encodeURIComponent(p)).join('/')}${q}`;
}
