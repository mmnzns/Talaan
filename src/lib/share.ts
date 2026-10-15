import type { Meeting, Ms } from "../data/types";
import { formatDate, formatDuration, formatTime } from "./format";
import { speakerName } from "./transcript";

const nameOf = (m: Meeting, speaker: string) => speakerName(m, speaker);
const when = (at?: Ms) => (at === undefined ? "" : ` (${formatTime(at)})`);
const owner = (o?: string) => (o ? ` — ${o}` : "");
const due = (d?: string) => (d ? `, due ${d}` : "");

function heading(m: Meeting) {
  return `${m.title} · ${formatDate(m.startedAt)} · ${formatDuration(m.durationMs)}`;
}

/** Summary as plain text with bullets, for pasting anywhere (Slack, email, notes). */
export function toShareText(m: Meeting): string {
  const s = m.summary;
  if (!s) return `${heading(m)}\n(no summary yet)`;
  const out = [heading(m), "", s.recap];
  if (s.decisions.length) out.push("", "Decisions", ...s.decisions.map((d) => `• ${d.text}${when(d.at)}`));
  if (s.actionItems.length)
    out.push("", "Action items", ...s.actionItems.map((a) => `• ${a.done ? "✅" : "☐"} ${a.text}${owner(a.owner)}${due(a.due)}`));
  if (s.openQuestions.length) out.push("", "Open questions", ...s.openQuestions.map((q) => `• ${q.text}`));
  return out.join("\n");
}

/** Summary formatted for an email body. */
export function toEmail(m: Meeting): string {
  const s = m.summary;
  const out = [`Subject: ${m.title} — ${formatDate(m.startedAt)}`, "", `Hi all,`, "", `Here is the summary from ${m.title}.`, ""];
  if (!s) return [...out, "(no summary yet)"].join("\n");
  out.push(s.recap);
  if (s.decisions.length) out.push("", "Decisions", ...s.decisions.map((d) => `- ${d.text}`));
  if (s.actionItems.length) out.push("", "Action items", ...s.actionItems.map((a) => `- ${a.text}${owner(a.owner)}${due(a.due)}`));
  if (s.openQuestions.length) out.push("", "Open questions", ...s.openQuestions.map((q) => `- ${q.text}`));
  return out.join("\n");
}

/** Full meeting as Markdown. The transcript is optional; the summary is always included. */
export function toMarkdown(m: Meeting, includeTranscript: boolean): string {
  const out = [`# ${m.title}`, "", `${m.appLabel} · ${formatDate(m.startedAt)} · ${formatDuration(m.durationMs)}`, ""];
  const s = m.summary;
  out.push("## Summary", "");
  if (!s) {
    out.push("_No summary yet._");
  } else {
    out.push(s.recap);
    if (s.decisions.length) out.push("", "### Decisions", "", ...s.decisions.map((d) => `- ${d.text}${when(d.at)}`));
    if (s.actionItems.length)
      out.push("", "### Action items", "", ...s.actionItems.map((a) => `- [${a.done ? "x" : " "}] ${a.text}${owner(a.owner)}${due(a.due)}${when(a.at)}`));
    if (s.openQuestions.length) out.push("", "### Open questions", "", ...s.openQuestions.map((q) => `- ${q.text}`));
  }
  if (includeTranscript && m.transcript.length) {
    out.push("", "## Transcript", "", ...m.transcript.map((l) => `**${formatTime(l.at)} ${nameOf(m, l.speaker)}:** ${l.text}`));
  }
  return out.join("\n") + "\n";
}

/** Same content as Markdown without the markup. */
export function toPlainText(m: Meeting, includeTranscript: boolean): string {
  return toMarkdown(m, includeTranscript)
    .replace(/^#{1,3} /gm, "")
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/^_(.+)_$/gm, "$1")
    .replace(/- \[[ x]\] /g, "- ");
}

export const safeFileName = (title: string) => title.replace(/[\\/:*?"<>|]+/g, "").trim() || "Meeting";

export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const el = document.createElement("textarea");
    el.value = text;
    el.style.position = "fixed";
    el.style.opacity = "0";
    document.body.appendChild(el);
    el.select();
    document.execCommand("copy");
    el.remove();
  }
}
