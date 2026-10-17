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

/** Writes the copies still pending (kept in meeting.json): at launch, after a copy that works, and with Check setup's
 *  Try again. Returns how many were written. */
export async function retryPending(store: Pick<MeetingStore, "getMeeting">, settings: Pick<SettingsStore, "get" | "writeAutoSave" | "autoSaveStatus">): Promise<number> {
  const status = await settings.autoSaveStatus();
  if (!status?.reachable) return 0;
  let saved = 0;
  for (const { id } of status.pending) {
    if ((await saveCopy(store, settings, id, true).catch(() => "missing")) === "written") saved++;
  }
  return saved;
}

/**
 * Writes pending copies once, then listens for `autosave-due` (see commands/autosave.rs) and writes copies one at a
 * time, in order. After a copy is written, any still pending are caught up. Returns the unsubscribe function.
 */
export function startAutoSave(
  store: Pick<MeetingStore, "getMeeting">,
  settings: Pick<SettingsStore, "get" | "writeAutoSave" | "autoSaveStatus">,
  onDue: (cb: (due: { id: string; finished: boolean }) => void) => () => void,
): () => void {
  // At launch: copies that were due when Talaan last quit, or that failed.
  let queue = retryPending(store, settings)
    .then(() => {})
    .catch((e) => console.warn("talaan: couldn't retry the folder copies", e));
  return onDue(({ id, finished }) => {
    queue = queue.then(async () => {
      try {
        const result = await saveCopy(store, settings, id, finished);
        if (result === "written") await retryPending(store, settings);
      } catch (e) {
        // Rust has kept it pending with the reason; Check setup shows it.
        console.warn("talaan: couldn't save the meeting's copy", e);
      }
    });
  });
}
