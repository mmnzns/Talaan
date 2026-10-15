import type { TranscriptLine } from "../data/types";

/** One hit of Find in transcript: a range of a line's text. */
export interface Match {
  lineId: string;
  start: number;
  end: number;
}

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Every place `query` appears (any case, as plain text), line by line in the order given. */
export function findMatches(lines: TranscriptLine[], query: string): Match[] {
  const q = query.trim();
  if (!q) return [];
  const re = new RegExp(escape(q), "giu");
  const out: Match[] = [];
  for (const l of lines) {
    for (const m of l.text.matchAll(re)) out.push({ lineId: l.id, start: m.index, end: m.index + m[0].length });
  }
  return out;
}

/** A line's text cut into plain and matched parts; `match` is the hit's number across the whole transcript. */
export function segments(text: string, ranges: { start: number; end: number; index: number }[]): { text: string; match?: number }[] {
  const out: { text: string; match?: number }[] = [];
  let at = 0;
  for (const r of ranges) {
    if (r.start > at) out.push({ text: text.slice(at, r.start) });
    out.push({ text: text.slice(r.start, r.end), match: r.index });
    at = r.end;
  }
  if (at < text.length || out.length === 0) out.push({ text: text.slice(at) });
  return out;
}
