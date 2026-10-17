/** Runs `job` when asked, but never while a run is still going and never sooner than `gapMs` after the last
 *  start (unless `force`). For checks fired by window focus, which can come many times a minute. */
export function throttled(job: () => Promise<void>, gapMs: number, now: () => number = Date.now) {
  let running = false;
  let last = -Infinity;
  return async (force = false): Promise<boolean> => {
    if (running || (!force && now() - last < gapMs)) return false;
    running = true;
    last = now();
    try {
      await job();
    } finally {
      running = false;
    }
    return true;
  };
}
