import type { MeetingStore } from "../data/store";
import type { Meeting } from "../data/types";
import { safeFileName, toMarkdown } from "./share";
import { inTauri } from "./windows";

/** One Markdown file per meeting (summary and transcript, edits included), named "2026-10-14 Title.md". */
export function exportAllFiles(meetings: Meeting[]): { name: string; text: string }[] {
  const used = new Set<string>();
  return meetings
    .filter((m) => m.status !== "recording")
    .map((m) => {
      const base = `${m.startedAt.slice(0, 10)} ${safeFileName(m.title)}`.trim();
      let name = `${base}.md`;
      for (let n = 2; used.has(name.toLowerCase()); n++) name = `${base} (${n}).md`;
      used.add(name.toLowerCase());
      return { name, text: toMarkdown(m, "both") };
    });
}

/**
 * "Export all": asks for a folder, then writes every meeting's Markdown into it.
 * Returns how many files were written, or undefined when the user cancelled.
 */
export async function exportAll(store: MeetingStore): Promise<number | undefined> {
  const items = await store.listMeetings();
  const meetings = (await Promise.all(items.map((i) => store.getMeeting(i.id)))).filter((m): m is Meeting => Boolean(m));
  const files = exportAllFiles(meetings);
  if (!inTauri()) {
    for (const f of files) {
      const a = document.createElement("a");
      a.href = URL.createObjectURL(new Blob([f.text], { type: "text/markdown" }));
      a.download = f.name;
      a.click();
      URL.revokeObjectURL(a.href);
    }
    return files.length;
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const { invoke } = await import("@tauri-apps/api/core");
  const dir = await open({ directory: true, title: "Export all meetings to" });
  if (typeof dir !== "string") return undefined;
  const sep = dir.includes("\\") ? "\\" : "/";
  for (const f of files) await invoke("export_write", { path: `${dir.replace(/[\/]+$/, "")}${sep}${f.name}`, contents: f.text });
  return files.length;
}
