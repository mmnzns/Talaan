import { useCallback, useEffect, useState } from "react";
import { useSettingsStore, useStore } from "./StoreProvider";
import type { MeetingFilter } from "./store";
import type { Settings } from "./settings";
import type { Folder, Meeting, MeetingListItem } from "./types";

/** Re-runs `load` on mount, when its inputs change, and whenever the store emits a change. */
function useLive<T>(source: { subscribe(l: () => void): () => void }, load: () => Promise<T>, deps: unknown[]) {
  const [value, setValue] = useState<T | undefined>(undefined);
  const [loading, setLoading] = useState(true);

  const run = useCallback(load, deps);

  useEffect(() => {
    let alive = true;
    const refresh = () =>
      run().then((v) => {
        if (alive) {
          setValue(v);
          setLoading(false);
        }
      });
    refresh();
    const off = source.subscribe(refresh);
    return () => {
      alive = false;
      off();
    };
  }, [run, source]);

  return { value, loading };
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
