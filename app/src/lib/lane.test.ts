import { describe, expect, it } from 'vitest';
import { backoffDelay } from './lane';

describe('backoffDelay', () => {
  it('doubles from 1 s with jitter in the upper half', () => {
    expect(backoffDelay(0, null, 0, () => 0)).toBe(500);
    expect(backoffDelay(0, null, 0, () => 1)).toBe(1000);
    expect(backoffDelay(3, null, 0, () => 0)).toBe(4000);
    expect(backoffDelay(3, null, 0, () => 1)).toBe(8000);
  });

  it('caps at 60 s', () => {
    expect(backoffDelay(30, null, 0, () => 1)).toBe(60_000);
    expect(backoffDelay(30, null, 0, () => 0)).toBe(30_000);
  });

  it('honours Retry-After in seconds or as a date, plus jitter', () => {
    expect(backoffDelay(0, '10', 0, () => 0)).toBe(10_000);
    expect(backoffDelay(0, '10', 0, () => 1)).toBe(12_000);
    expect(backoffDelay(0, new Date(30_000).toUTCString(), 0, () => 0)).toBe(30_000);
  });

  it('keeps the exponential wait when Retry-After asks for less, and ignores junk', () => {
    expect(backoffDelay(4, '1', 0, () => 0)).toBe(8000);
    expect(backoffDelay(0, 'soon', 0, () => 1)).toBe(1000);
    expect(backoffDelay(0, '-5', 0, () => 1)).toBe(1000);
  });
});
