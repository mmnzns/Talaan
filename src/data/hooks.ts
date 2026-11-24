import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { startLive } from "./live";
import { useSettingsStore, useStore } from "./StoreProvider";
import type { MeetingFilter } from "./store";
import type { NextMeeting, Settings } from "./settings";
import type { Folder, Meeting, MeetingListItem } from "./types";

/**
 * Re-runs `load` on mount, when its inputs change, whenever the store emits a change, and when the window comes
 * back to the front (so a missed change can't leave a screen stale). `loading` stays true until `load` has
 * answered for the current inputs: a meeting that hasn't loaded yet is not "not found".
 */
function useLive<T>(source: { subscribe(l: () => void): () => void }, load: () => Promise<T>, deps: unknown[]) {
  const run = useCallback(load, deps);
  const [state, setState] = useState<{ value?: T; for?: unknown; ok?: boolean; error?: unknown }>({});
  const live = useRef<{ refresh(): void } | undefined>(undefined);

  useEffect(() => {
    const l = startLive(
      run,
      (cb) => source.subscribe(cb),
      (value) => setState({ value, for: run, ok: true }),
      // A failure only matters while there is no answer yet for these inputs; after one, the last good value stays.
      (error) => setState((prev) => (prev.for === run && prev.ok ? prev : { for: run, ok: false, error })),
    );
    live.current = l;
    const onFocus = () => l.refresh();
    const onVisible = () => {
      if (document.visibilityState === "visible") l.refresh();
    };
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      l.stop();
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [run, source]);

  const failed = state.for === run && state.ok === false;
  const retry = useCallback(() => live.current?.refresh(), []);
  // `loading` ends on a failure too, so a screen can show the error instead of waiting forever.
  return { value: state.value, loading: state.for !== run, error: failed ? { cause: state.error } : undefined, retry };
}

export function useMeetings(filter: MeetingFilter = {}) {
  const store = useStore();
  const { value, loading, error, retry } = useLive<MeetingListItem[]>(
    store,
    () => store.listMeetings(filter),
    [store, filter.folderId, filter.tag, filter.query],
  );
  return { meetings: value ?? [], loading, error, retry };
}

export function useMeeting(id: string | undefined) {
  const store = useStore();
  const { value, loading, error, retry } = useLive<Meeting | undefined>(
    store,
    () => (id ? store.getMeeting(id) : Promise.resolve(undefined)),
    [store, id],
  );
  return { meeting: value, loading, error, retry };
}

export function useFolders() {
  const store = useStore();
  const { value } = useLive<Folder[]>(store, () => store.listFolders(), [store]);
  return value ?? [];
}

export function useTags() {
  const store = useStore();
  const { value } = useLive<string[]>(store, () => store.listTags(), [store]);
  return value ?? [];
}

/**
 * `update` saves a change. When the write fails the change stays on screen (`settings` still shows it) and `saveError`
 * holds what to say; `retrySave` writes it again, `dismissSaveError` gives it up (the screen goes back to what is saved).
 */
export function useSettings() {
  const store = useSettingsStore();
  const { value } = useLive<Settings>(store, () => store.get(), [store]);
  const [unsaved, setUnsaved] = useState<{ patch: Partial<Settings>; error: unknown } | null>(null);
  const settings = useMemo(() => (value && unsaved ? { ...value, ...unsaved.patch } : value), [value, unsaved]);
  const shown = useRef(settings);
  shown.current = settings;

  const update = useCallback(
    async (patch: Partial<Settings>): Promise<Settings> => {
      try {
        const saved = await store.update(patch);
        setUnsaved((u) => {
          if (!u) return u;
          const rest = { ...u.patch };
          for (const k of Object.keys(patch)) delete rest[k as keyof Settings];
          return Object.keys(rest).length ? { ...u, patch: rest } : null;
        });
        return saved;
      } catch (error) {
        setUnsaved((u) => ({ patch: { ...u?.patch, ...patch }, error }));
        return { ...shown.current!, ...patch };
      }
    },
    [store],
  );
  const retrySave = useCallback(() => {
    if (unsaved) void update(unsaved.patch);
  }, [unsaved, update]);
  const dismissSaveError = useCallback(() => setUnsaved(null), []);
  return { settings, update, saveError: unsaved ? { cause: unsaved.error } : undefined, retrySave, dismissSaveError };
}

/**
 * The meeting coming up (or in progress), for the menu bar. Asked when the popover opens (it takes focus) and every
 * minute while it's open, never while it's hidden: on Windows asking may download the calendar (at most every 15
 * minutes), which Privacy describes as happening only then.
 */
export function useNextMeeting(): NextMeeting | null | undefined {
  const store = useSettingsStore();
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const id = setInterval(() => {
      if (document.hasFocus()) setTick((n) => n + 1);
    }, 60_000);
    return () => clearInterval(id);
  }, []);
  const { value } = useLive<NextMeeting | null>(store, () => store.nextMeeting().catch(() => null), [store, tick]);
  return value;
}
