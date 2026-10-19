import type { Settings } from "../data/settings";
import type { MessageKey } from "./i18n";
import { onWindows } from "./platform";

/** This computer: loopback, or a server listening on every interface (`0.0.0.0`, `[::]`, common for Ollama), which
 *  is reached here through loopback. Same hosts as `is_local` in src-tauri/src/commands/diagnostics.rs. */
const loopback = (address: string) => /^(https?:\/\/)?(localhost|127\.\d+\.\d+\.\d+|0\.0\.0\.0|\[::1?\])(:\d+)?(\/|$)/i.test(address.trim());

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
