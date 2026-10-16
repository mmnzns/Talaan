import { describe, expect, it } from "vitest";
import { startLive } from "./live";

function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("startLive", () => {
  it("keeps the newest answer when an older one arrives late", async () => {
    const calls: ReturnType<typeof deferred<string>>[] = [];
    let changed = () => {};
    const seen: string[] = [];
    startLive(
      () => {
        const d = deferred<string>();
        calls.push(d);
        return d.promise;
      },
      (l) => {
        changed = l;
        return () => {};
      },
      (v) => seen.push(v),
    );
    changed(); // e.g. "meetings-changed" after Stop
    calls[1].resolve("transcribing");
    calls[0].resolve("recording");
    await flush();
    expect(seen).toEqual(["transcribing"]);
  });

  it("keeps the last good value when a request fails, and asks again on refresh", async () => {
    const answers = [Promise.resolve("a"), Promise.reject(new Error("ipc")), Promise.resolve("b")];
    const seen: string[] = [];
    const live = startLive(() => answers.shift()!, () => () => {}, (v) => seen.push(v));
    await flush();
    live.refresh();
    await flush();
    expect(seen).toEqual(["a"]);
    live.refresh();
    await flush();
    expect(seen).toEqual(["a", "b"]);
  });

  it("stops listening and ignores answers after stop", async () => {
    const d = deferred<string>();
    let unsubscribed = false;
    const seen: string[] = [];
    const live = startLive(() => d.promise, () => () => (unsubscribed = true), (v) => seen.push(v));
    live.stop();
    d.resolve("late");
    await flush();
    expect(unsubscribed).toBe(true);
    expect(seen).toEqual([]);
  });
});
