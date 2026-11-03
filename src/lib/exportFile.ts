import type { Meeting } from "../data/types";
import { t } from "./i18n";
import { safeFileName, toMarkdown, toPlainText, type ExportScope } from "./share";
import { routes } from "./router";
import { inTauri } from "./windows";

export type FileFormat = "md" | "txt" | "pdf";

/**
 * Exports a meeting as the user sees it (edits included). Markdown and text go through a save dialog;
 * PDF opens a print-styled page and the macOS print sheet (Save as PDF is built in).
 * Returns where the file went, or undefined when the user cancelled (or for PDF, which the sheet handles).
 */
export async function exportMeeting(meeting: Meeting, format: FileFormat, scope: ExportScope): Promise<string | undefined> {
  if (format === "pdf") {
    await openPrint(meeting.id, scope);
    return undefined;
  }
  const text = format === "md" ? toMarkdown(meeting, scope) : toPlainText(meeting, scope);
  const name = `${safeFileName(meeting.title)}.${format}`;
  if (!inTauri()) {
    const a = document.createElement("a");
    a.href = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
    a.download = name;
    a.click();
    URL.revokeObjectURL(a.href);
    return name;
  }
  const { save } = await import("@tauri-apps/plugin-dialog");
  const { invoke } = await import("@tauri-apps/api/core");
  const path = await save({
    defaultPath: name,
    filters: [format === "md" ? { name: "Markdown", extensions: ["md"] } : { name: t("meeting.fmtTxtName"), extensions: ["txt"] }],
  });
  if (!path) return undefined;
  await invoke("export_write", { path, contents: text });
  return path;
}

async function openPrint(id: string, scope: ExportScope) {
  const hash = routes.print(id, scope);
  if (!inTauri()) {
    window.open(`${location.pathname}${hash}`, "_blank");
    return;
  }
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const old = await WebviewWindow.getByLabel("print");
  await old?.close();
  new WebviewWindow("print", { url: `index.html${hash}`, title: t("meeting.printTitle"), width: 820, height: 900, center: true });
}
