// Every stream request to the provider goes through here: one at a time (providers count concurrent streams),
// in order, backing off on rate limits, server errors and network failures.

/** After a request ends, before the next starts (ms): proxies close their upstream side asynchronously. */
const SETTLE = 500;
const BASE = 1000;
const CAP = 60_000;
/** Responses worth retrying after a pause. */
const RETRYABLE = new Set([429, 502, 503, 504]);

/**
 * How long to wait before retry number `attempt` (0-based): exponential with "equal jitter" (a random point in
 * the upper half, so retries spread out yet each waits long enough), or the server's Retry-After (seconds or an
 * HTTP date) plus up to 20%, whichever is longer.
 */
export function backoffDelay(attempt: number, retryAfter: string | null, now = Date.now(), rand = Math.random): number {
  const exp = Math.min(CAP, BASE * 2 ** attempt);
  const jittered = exp / 2 + rand() * (exp / 2);
  if (!retryAfter) return jittered;
  const secs = Number(retryAfter);
  const asked = Number.isFinite(secs) ? secs * 1000 : Date.parse(retryAfter) - now;
  return Number.isFinite(asked) && asked > 0 ? Math.max(jittered, Math.min(asked, 10 * CAP) * (1 + 0.2 * rand())) : jittered;
}

type Job = { run: () => Promise<unknown>; done: (v: unknown) => void; fail: (e: unknown) => void; signal?: AbortSignal };

const jobs: Job[] = [];
let busy = false;
let lastEnd = 0;
let cooldownUntil = 0;
let attempt = 0; // consecutive failures, across the lane
let timer: ReturnType<typeof setTimeout> | undefined;

/** Runs `run` once nothing else holds the lane; skipped (rejects) if `signal` aborts while it waits. */
function lane<T>(run: () => Promise<T>, signal?: AbortSignal): Promise<T> {
  return new Promise<T>((done, fail) => {
    jobs.push({ run, done: done as (v: unknown) => void, fail, signal });
    pump();
  });
}

function pump() {
  if (busy) return;
  while (jobs[0]?.signal?.aborted) jobs.shift()!.fail(new DOMException('Aborted', 'AbortError'));
  if (!jobs.length) return;
  const wait = Math.max(cooldownUntil, lastEnd + SETTLE) - Date.now();
  if (wait > 0) {
    clearTimeout(timer);
    timer = setTimeout(pump, wait);
    return;
  }
  const job = jobs.shift()!;
  busy = true;
  job.run().then(job.done, job.fail).finally(() => { busy = false; lastEnd = Date.now(); pump(); });
}

const RETRY = Symbol('retry');

function coolDown(retryAfter: string | null) {
  cooldownUntil = Date.now() + backoffDelay(attempt++, retryAfter);
}

/**
 * Fetches `url` in the lane and hands the response to `use` (which still holds the lane while it reads). On a rate
 * limit, server error or network failure the lane cools down, then tries again (until `signal` aborts).
 */
export async function laneFetch<T>(url: string, signal: AbortSignal, use: (r: Response) => Promise<T>): Promise<T> {
  for (;;) {
    const out = await lane(async () => {
      let r: Response;
      try {
        r = await fetch(url, { signal });
      } catch (e) {
        if (signal.aborted) throw e;
        coolDown(null); // network failure
        return RETRY;
      }
      if (RETRYABLE.has(r.status)) {
        coolDown(r.headers.get('retry-after'));
        void r.body?.cancel();
        return RETRY;
      }
      attempt = 0;
      return use(r);
    }, signal);
    if (out !== RETRY) return out as T;
  }
}
