import { describe, expect, it } from 'vitest';
import { format, parse, type Route } from './route';

const cases: [string, Route][] = [
  ['#/live', { tab: 'live' }],
  ['#/movies/cat/101', { tab: 'vod', cat: '101' }],
  ['#/series?q=black+af', { tab: 'series', q: 'black af' }],
  ['#/live/2', { tab: 'live', id: 2 }],
  ['#/live/2/watch', { tab: 'live', id: 2, watch: true }],
  ['#/live/2/watch/1795740000', { tab: 'live', id: 2, watch: true, at: 1795740000 }],
  ['#/movies/87751/watch', { tab: 'vod', id: 87751, watch: true }],
  ['#/series/1', { tab: 'series', id: 1 }],
  ['#/series/1/watch/20002', { tab: 'series', id: 1, watch: true, episode: '20002' }],
];

describe('route', () => {
  it.each(cases)('%s round-trips', (hash, route) => {
    expect(parse(hash)).toEqual(route);
    expect(format(route)).toBe(hash);
  });

  it('falls back to live for anything unknown', () => {
    expect(parse('')).toEqual({ tab: 'live' });
    expect(parse('#/nope/x')).toEqual({ tab: 'live' });
    expect(parse('#/movies/abc')).toEqual({ tab: 'vod' });
  });

  it('keeps search alongside a category', () => {
    expect(format({ tab: 'live', cat: '1', q: 'news' })).toBe('#/live/cat/1?q=news');
  });
});
