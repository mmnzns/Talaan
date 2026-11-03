import type { Meeting, Ms } from "../data/types";
import { t } from "./i18n";
import { formatDate, formatDuration, formatTime } from "./format";
import { MAILTO_MAX, mailtoHead, mailtoPart } from "./mailto";
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
  const notes = meetingNotes(m);
  const noteLines = notes.length ? ["", t("meeting.shYourNotes"), ...notes.map((k) => `• ${k.note!.trim()}${noteWhen(k)}`)] : [];
  if (!s) return [`${heading(m)}\n(no summary yet)`, ...noteLines].join("\n");
  const out = [heading(m), "", s.recap, ...noteLines];
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

/** "Email this summary": the body as plain text (recap, decisions, action items), without a subject line. */
export function toEmailBody(m: Meeting): string {
  const s = m.summary;
  const out = [t("meeting.emailHi"), "", t("meeting.emailIntro", { title: m.title }), ""];
  if (!s) return [...out, t("meeting.shNoSummaryParen")].join("\n");
  out.push(s.recap);
  if (s.decisions.length) out.push("", t("meeting.shDecisions"), ...s.decisions.map((d) => `- ${d.text}`));
  if (s.actionItems.length) out.push("", t("meeting.shActionItems"), ...s.actionItems.map((a) => `- ${a.text}${owner(a.owner && ownerName(m, a.owner))}${due(a.due)}`));
  return out.join("\n");
}

/** A mail link longer than this may not open on Windows (the mail app gets nothing), so the body is shortened. */
export const MAILTO_LIMIT = MAILTO_MAX;

const enc = mailtoPart;
/** The first `n` UTF-16 units, without splitting an emoji (a lone surrogate can't be encoded). */
const head16 = (s: string, n: number) => (/[\uD800-\uDBFF]/.test(s.charAt(n - 1)) ? s.slice(0, n - 1) : s.slice(0, n));

/**
 * A `mailto:` link for the summary: the attendees (when the calendar had their addresses), the title as the
 * subject and the summary as the body. A body too long for `limit` is cut at a word and ends with a line saying
 * the full notes are in Talaan (`truncated`: offer Copy summary instead).
 */
export function toMailto(m: Meeting, limit = MAILTO_LIMIT): { url: string; truncated: boolean } {
  const head = mailtoHead(m.calendarEvent?.emails ?? [], m.title);
  const body = toEmailBody(m);
  if (head.length + enc(body).length <= limit) return { url: head + enc(body), truncated: false };
  const tail = `…\n\n${t("meeting.emailTruncated")}`;
  // The longest start of the body that fits (encoded length only grows with it), then back to a word break.
  let lo = 0;
  let hi = body.length;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (head.length + enc(head16(body, mid) + tail).length <= limit) lo = mid;
    else hi = mid - 1;
  }
  let cut = head16(body, lo);
  const space = cut.search(/\s\S*$/);
  if (space > lo / 2) cut = cut.slice(0, space);
  return { url: head + enc(cut.trimEnd() + tail), truncated: true };
}

/** What an export carries: the summary (with your notes), the transcript, or both. */
export type ExportScope = "summary" | "transcript" | "both";
export const EXPORT_SCOPES: ExportScope[] = ["summary", "transcript", "both"];

/** Full meeting as Markdown: the title line, then the summary and/or the transcript as `scope` says. */
export function toMarkdown(m: Meeting, scope: ExportScope): string {
  const out = [`# ${m.title}`, "", `${m.appLabel} · ${formatDate(m.startedAt)} · ${formatDuration(m.durationMs)}`, ""];
  if (scope !== "transcript") {
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
  }
  if (scope === "transcript" || (scope === "both" && m.transcript.length)) {
    out.push(...(scope === "both" ? [""] : []), `## ${t("meeting.shTranscript")}`, "");
    if (m.transcript.length) out.push(...m.transcript.map((l) => `**${formatTime(l.at)} ${nameOf(m, l.speaker)}:** ${l.text}`));
    else out.push(`_${t("meeting.shNoTranscript")}_`);
  }
  return out.join("\n") + "\n";
}

/** Same content as Markdown without the markup. */
export function toPlainText(m: Meeting, scope: ExportScope): string {
  return toMarkdown(m, scope)
    .replace(/^#{1,3} /gm, "")
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/^_(.+)_$/gm, "$1")
    .replace(/- \[[ x]\] /g, "- ");
}

/** What "Copy as text" puts on the clipboard: the bulleted summary as before, or plain text with the transcript. */
export function toCopyText(m: Meeting, scope: ExportScope): string {
  return scope === "summary" ? toShareText(m) : toPlainText(m, scope);
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
