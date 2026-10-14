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

/** Calls `listener` with the payload whenever the backend emits `event`. Returns an unsubscribe function. */
export function onEvent<T = void>(event: string, listener: (payload: T) => void): () => void {
  const pending = listen<T>(event, (e) => listener(e.payload));
  return () => {
    pending.then((off) => off()).catch(() => {});
  };
}
