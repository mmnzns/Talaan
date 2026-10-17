import type { Settings } from "../data/settings";
import type { MessageKey } from "./i18n";
import { onWindows } from "./platform";

const loopback = (address: string) => /^(https?:\/\/)?(localhost|127\.\d+\.\d+\.\d+|\[::1\])(:\d+)?(\/|$)/i.test(address.trim());

/** The toolbar's privacy line. It claims "everything stays on this computer" only when that's true: summaries
 *  by the built-in AI, no Notion connection, and (Windows) no calendar address fetched over the internet.
 *  Otherwise it says only what is true of the summaries. */
export function privacyChip(s: Pick<Settings, "summaryEngine" | "ollamaAddress" | "notion" | "calendarConnected"> | undefined, windows = onWindows): MessageKey {
  if (!s) return "shell.summariesLocal";
  if (s.summaryEngine === "byok") return "shell.cloudOn";
  if (s.summaryEngine === "ollama") return loopback(s.ollamaAddress) ? "shell.summariesLocal" : "shell.summariesServer";
  const online = s.notion.connected || (windows && s.calendarConnected);
  return online ? "shell.summariesLocal" : "shell.offlineLocal";
}
