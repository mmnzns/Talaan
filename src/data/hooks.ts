import { useCallback, useEffect, useState } from "react";
import { startLive } from "./live";
import { useSettingsStore, useStore } from "./StoreProvider";
import type { MeetingFilter } from "./store";
import type { Settings } from "./settings";
import type { Folder, Meeting, MeetingListItem } from "./types";

/**
 * Re-runs `load` on mount, when its inputs change, whenever the store emits a change, and when the window comes
 * back to the front (so a missed change can't leave a screen stale). `loading` stays true until `load` has
 * answered for the current inputs: a meeting that hasn't loaded yet is not "not found".
 */
function useLive<T>(source: { subscribe(l: () => void): () => void }, load: () => Promise<T>, deps: unknown[]) {
  const run = useCallback(load, deps);
  const [state, setState] = useState<{ value?: T; for?: unknown }>({});

  useEffect(() => {
    const live = startLive(run, (l) => source.subscribe(l), (value) => setState({ value, for: run }));
    const onFocus = () => live.refresh();
    const onVisible = () => {
      if (document.visibilityState === "visible") live.refresh();
    };
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      live.stop();
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [run, source]);

  return { value: state.value, loading: state.for !== run };
}

export function useMeetings(filter: MeetingFilter = {}) {
  const store = useStore();
  const { value, loading } = useLive<MeetingListItem[]>(
    store,
    () => store.listMeetings(filter),
    [store, filter.folderId, filter.tag, filter.query],
  );
  return { meetings: value ?? [], loading };
}

export function useMeeting(id: string | undefined) {
  const store = useStore();
  const { value, loading } = useLive<Meeting | undefined>(
    store,
    () => (id ? store.getMeeting(id) : Promise.resolve(undefined)),
    [store, id],
  );
  return { meeting: value, loading };
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

export function useSettings() {
  const store = useSettingsStore();
  const { value } = useLive<Settings>(store, () => store.get(), [store]);
  const update = useCallback((patch: Partial<Settings>) => store.update(patch), [store]);
  return { settings: value, update };
}
