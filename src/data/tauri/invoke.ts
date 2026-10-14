import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toStoreError } from "../errors";

export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    throw toStoreError(e);
  }
}

/** Calls `listener` whenever the backend emits `event`. Returns an unsubscribe function. */
export function onEvent(event: string, listener: () => void): () => void {
  const pending = listen(event, () => listener());
  return () => {
    pending.then((off) => off()).catch(() => {});
  };
}
