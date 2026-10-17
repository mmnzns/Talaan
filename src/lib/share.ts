import type { Meeting, Ms } from "../data/types";
import { t } from "./i18n";
import { formatDate, formatDuration, formatTime } from "./format";
import { ownerName, speakerName } from "./transcript";

const nameOf = (m: Meeting, speaker: string) => speakerName(m, speaker);
const when = (at?: Ms) => (at === undefined ? "" : ` (${formatTime(at)})`);
const owner = (o?: string) => (o ? ` — ${o}` : "");
const due = (d?: string) => (d ? `, ${t("meeting.shDue", { date: d })}` : "");

/** The user's notes (marks with text), in the order they were written. */
export function meetingNotes(m: Meeting) {
  return m.marks.filter((k) => k.note?.trim());
}

const noteWhen = (k: { at: Ms; later?: boolean }) => (k.later ? ` (${t("meeting.shAfterCall")})` : when(k.at));

function heading(m: Meeting) {
  return `${m.title} · ${formatDate(m.startedAt)} · ${formatDuration(m.durationMs)}`;
}

/** Summary as plain text with bullets, for pasting anywhere (Slack, email, notes). */
export function toShareText(m: Meeting): string {
  const s = m.summary;
  if (!s) return `${heading(m)}\n(no summary yet)`;
  const out = [heading(m), "", s.recap];
  if (s.decisions.length) out.push("", t("meeting.shDecisions"), ...s.decisions.map((d) => `• ${d.text}${when(d.at)}`));
  if (s.actionItems.length)
    out.push("", t("meeting.shActionItems"), ...s.actionItems.map((a) => `• ${a.done ? "✅" : "☐"} ${a.text}${owner(a.owner && ownerName(m, a.owner))}${due(a.due)}`));
  if (s.openQuestions.length) out.push("", t("meeting.shOpenQuestions"), ...s.openQuestions.map((q) => `• ${q.text}`));
  for (const sec of s.sections ?? []) out.push("", sec.title, ...sec.items.map((i) => `• ${i.text}${when(i.at)}`));
  if (s.highlights?.length) out.push("", t("meeting.shHighlights"), ...s.highlights.map((h) => `• ${h.text}${when(h.at)}`));
  return out.join("\n");
}

/** Summary formatted for an email body. */
export function toEmail(m: Meeting): string {
  const s = m.summary;
  const out = [t("meeting.emailSubject", { title: m.title, date: formatDate(m.startedAt) }), "", t("meeting.emailHi"), "", t("meeting.emailIntro", { title: m.title }), ""];
  if (!s) return [...out, t("meeting.shNoSummaryParen")].join("\n");
  out.push(s.recap);
  if (s.decisions.length) out.push("", t("meeting.shDecisions"), ...s.decisions.map((d) => `- ${d.text}`));
  if (s.actionItems.length) out.push("", t("meeting.shActionItems"), ...s.actionItems.map((a) => `- ${a.text}${owner(a.owner && ownerName(m, a.owner))}${due(a.due)}`));
  if (s.openQuestions.length) out.push("", t("meeting.shOpenQuestions"), ...s.openQuestions.map((q) => `- ${q.text}`));
  return out.join("\n");
}

/** Full meeting as Markdown. The transcript is optional; the summary is always included. */
export function toMarkdown(m: Meeting, includeTranscript: boolean): string {
  const out = [`# ${m.title}`, "", `${m.appLabel} · ${formatDate(m.startedAt)} · ${formatDuration(m.durationMs)}`, ""];
  const s = m.summary;
  out.push(`## ${t("meeting.shSummary")}`, "");
  if (!s) {
    out.push(`_${t("meeting.shNoSummary")}_`);
  } else {
    out.push(s.recap);
  }
  const notes = meetingNotes(m);
  if (notes.length) out.push("", `### ${t("meeting.shYourNotes")}`, "", ...notes.map((k) => `- ${k.note!.trim()}${noteWhen(k)}`));
  if (s) {
    if (s.decisions.length) out.push("", `### ${t("meeting.shDecisions")}`, "", ...s.decisions.map((d) => `- ${d.text}${when(d.at)}`));
    if (s.actionItems.length)
      out.push("", `### ${t("meeting.shActionItems")}`, "", ...s.actionItems.map((a) => `- [${a.done ? "x" : " "}] ${a.text}${owner(a.owner && ownerName(m, a.owner))}${due(a.due)}${when(a.at)}`));
    if (s.openQuestions.length) out.push("", `### ${t("meeting.shOpenQuestions")}`, "", ...s.openQuestions.map((q) => `- ${q.text}`));
    for (const sec of s.sections ?? []) out.push("", `### ${sec.title}`, "", ...sec.items.map((i) => `- ${i.text}${when(i.at)}`));
    if (s.highlights?.length) out.push("", `### ${t("meeting.shHighlights")}`, "", ...s.highlights.map((h) => `- ${h.text}${when(h.at)}`));
  }
  if (includeTranscript && m.transcript.length) {
    out.push("", `## ${t("meeting.shTranscript")}`, "", ...m.transcript.map((l) => `**${formatTime(l.at)} ${nameOf(m, l.speaker)}:** ${l.text}`));
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
