import type { Mark, Ms, PauseGap, TranscriptLine } from "../data/types";

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
