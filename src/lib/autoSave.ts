import type { AutoSaveResult, SettingsStore } from "../data/settings";
import type { MeetingStore } from "../data/store";
import { toMarkdown } from "./share";

/**
 * "Also save each meeting to a folder", the main window's half. Rust says when a copy is due; this writes the
 * same Markdown as Export (so the file matches what the person sees, edits and names included) and Rust puts it in
 * the folder. Meetings without a summary always carry their transcript, or the copy would be empty.
 */
export async function saveCopy(store: Pick<MeetingStore, "getMeeting">, settings: Pick<SettingsStore, "get" | "writeAutoSave">, id: string, finished: boolean): Promise<AutoSaveResult> {
  const s = await settings.get();
  if (!s.autoSaveFolder) return "off";
  const m = await store.getMeeting(id);
  if (!m || m.status !== "ready") return "not_ready";
  return settings.writeAutoSave(id, toMarkdown(m, s.autoSaveTranscript || !m.summary), finished);
}

/** Saves the meetings that were skipped while the folder was away (Check setup's Try again, or the next copy). */
export async function retrySkipped(store: Pick<MeetingStore, "getMeeting">, settings: Pick<SettingsStore, "get" | "writeAutoSave" | "autoSaveStatus">): Promise<number> {
  const status = await settings.autoSaveStatus();
  if (!status?.reachable) return 0;
  let saved = 0;
  for (const id of status.skipped) {
    if ((await saveCopy(store, settings, id, true).catch(() => "missing")) === "written") saved++;
  }
  return saved;
}

/**
 * Listens for `autosave-due` (see commands/autosave.rs) and writes copies one at a time, in order. After a copy is
 * written, any skipped while the folder was away are caught up. Returns the unsubscribe function.
 */
export function startAutoSave(
  store: Pick<MeetingStore, "getMeeting">,
  settings: Pick<SettingsStore, "get" | "writeAutoSave" | "autoSaveStatus">,
  onDue: (cb: (due: { id: string; finished: boolean }) => void) => () => void,
): () => void {
  let queue = Promise.resolve();
  return onDue(({ id, finished }) => {
    queue = queue.then(async () => {
      try {
        const result = await saveCopy(store, settings, id, finished);
        if (result === "written") await retrySkipped(store, settings);
      } catch (e) {
        console.warn("talaan: couldn't save the meeting's copy", e);
      }
    });
  });
}
