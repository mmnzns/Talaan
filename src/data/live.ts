/**
 * A live query: runs `load` now, whenever `subscribe` says something changed, and on `refresh()`. Only the answer
 * to the newest request is kept, so a slow older answer can't overwrite a newer one. A failed request keeps the
 * last good value; the next change, focus or refresh asks again. `onError` hears about a failed newest request, so a
 * screen that has nothing to show yet can say why instead of waiting forever.
 */
export function startLive<T>(
  load: () => Promise<T>,
  subscribe: (listener: () => void) => () => void,
  onValue: (value: T) => void,
  onError?: (error: unknown) => void,
): { refresh: () => void; stop: () => void } {
  let alive = true;
  let asked = 0;
  let shown = 0;
  const refresh = () => {
    const mine = ++asked;
    load().then(
      (v) => {
        if (alive && mine > shown) {
          shown = mine;
          onValue(v);
        }
      },
      (e) => {
        if (alive && mine === asked) onError?.(e);
      },
    );
  };
  refresh();
  const off = subscribe(refresh);
  return {
    refresh,
    stop: () => {
      alive = false;
      off();
    },
  };
}
