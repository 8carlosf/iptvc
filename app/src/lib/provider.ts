// Where channels (and, for Xtream, movies and series) come from.
import type { Item, Kind, Programme } from './core';
import type { Category, SeriesInfo } from './xtream';

export interface Provider {
  /** The sections this source has. */
  readonly kinds: Kind[];
  auth(): Promise<void>;
  categories(kind: Kind): Promise<Category[]>;
  /** Loads a section's list into the Rust core; returns its size. */
  load(kind: Kind): Promise<number>;
  epg(item: Item): Promise<Programme[]>;
  image(url: string): string;
  live(item: Item): string;
  // Xtream only:
  timeshift?(id: number, t: number, minutes: number): string;
  movie?(id: number, ext: string): string;
  episode?(id: string, ext: string): string;
  series?(id: number): Promise<SeriesInfo>;
}
