import { expect, test } from 'vitest';
import { statusOf } from './diagnose';

test('statusOf reads the player error status', () => {
  expect(statusOf('Error fetching https://h.pt/a/chunks.m3u8?x=1: 403 Forbidden')).toBe(403);
  expect(statusOf('HTTP 404')).toBe(404);
  expect(statusOf('Error fetching https://h.pt:8080/x: 503 ')).toBe(503);
  expect(statusOf('Failed to fetch')).toBeUndefined();
  expect(statusOf('HTTP 200')).toBeUndefined();
});
