import { t } from "./i18n";
import type { Mark, Meeting, Ms, PauseGap, TranscriptLine } from "../data/types";

export type TranscriptRow =
  | { kind: "line"; line: TranscriptLine; marks: Mark[] }
  | { kind: "pause"; pause: PauseGap };

/** Interleaves lines and pause gaps in time order and attaches each mark to the line it falls in. */
export function buildRows(lines: TranscriptLine[], pauses: PauseGap[], marks: Mark[]): TranscriptRow[] {
  const sorted = [...lines].sort((a, b) => a.at - b.at);
  const attached = new Map<string, Mark[]>();
  for (const mark of marks) {
    const line = lineAt(sorted, mark.at);
    if (line) attached.set(line.id, [...(attached.get(line.id) ?? []), mark]);
  }

  const rows: TranscriptRow[] = [];
  const queue = [...pauses].sort((a, b) => a.at - b.at);
  for (const line of sorted) {
    while (queue.length && queue[0].at < line.at) rows.push({ kind: "pause", pause: queue.shift()! });
    rows.push({ kind: "line", line, marks: attached.get(line.id) ?? [] });
  }
  for (const pause of queue) rows.push({ kind: "pause", pause });
  return rows;
}

/** The line being spoken at `at`: the last one that starts at or before it. */
export function lineAt(lines: TranscriptLine[], at: Ms): TranscriptLine | undefined {
  let found: TranscriptLine | undefined;
  for (const line of lines) {
    if (line.at <= at) found = line;
    else break;
  }
  return found ?? lines[0];
}

/**
 * The meeting as everyone sees it once the user has a name: Me becomes the name unless this meeting renamed
 * Me, and action items owned by "Me" follow. Mirrors `Storage::get_meeting` (the browser preview uses this).
 */
export function withYourName(meeting: Meeting, yourName: string | undefined): Meeting {
  const name = yourName?.trim();
  const stored = meeting.speakers.me;
  if (!name || (stored && stored !== "Me")) return meeting;
  const summary = meeting.summary && {
    ...meeting.summary,
    actionItems: meeting.summary.actionItems.map((a) => (a.owner?.trim() === "Me" ? { ...a, owner: name } : a)),
  };
  return { ...meeting, speakers: { ...meeting.speakers, me: name }, summary };
}

/** An action item's owner as shown: the summary AI writes "Me" for the user, which reads as Me in the UI language (or Your name). */
export function ownerName(meeting: Pick<Meeting, "speakers">, owner: string): string {
  return owner.trim() === "Me" ? speakerName(meeting, "me") : owner;
}

/** What meetings store for unnamed speakers (in English, whatever the UI language): defaults, not names. */
const STORED_DEFAULTS: Record<string, string> = { me: "Me", "them:0": "Them" };

/** A speaker's display name: the one the user gave (or Your name, applied before this), else Me / Them / Speaker 2 in the UI language. */
export function speakerName(meeting: Pick<Meeting, "speakers">, speaker: string): string {
  const named = meeting.speakers[speaker as keyof Meeting["speakers"]]?.trim();
  if (named && named !== STORED_DEFAULTS[speaker]) return named;
  if (speaker === "me") return t("meeting.speakerMe");
  if (speaker === "them:0" || speaker === "them") return t("meeting.speakerThem");
  return speaker.startsWith("them:") ? t("meeting.speakerN", { n: speaker.slice(5) }) : speaker;
}
