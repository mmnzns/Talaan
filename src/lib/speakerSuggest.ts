import type { Meeting, Speaker } from "../data/types";

/** What the speaker rename popover can offer: the invite's names as one-click chips, and, for a two-person call
 *  with one other attendee, "Them is probably <name>". Suggestions only: nothing is ever renamed by itself. */
export interface SpeakerSuggestions {
  names: string[];
  probably?: { speaker: Speaker; name: string };
}

const NONE: SpeakerSuggestions = { names: [] };
/** Sample data and older Mac builds listed the user as "You". */
const SELF_WORDS = new Set(["you", "me"]);
const same = (a: string, b: string) => a.trim().toLocaleLowerCase() === b.trim().toLocaleLowerCase();

export function speakerSuggestions(meeting: Pick<Meeting, "calendarEvent" | "speakers" | "transcript">, yourName: string): SpeakerSuggestions {
  const attendees = meeting.calendarEvent?.attendees ?? [];
  if (attendees.length === 0) return NONE;
  const mine = [yourName, meeting.speakers.me ?? ""].filter((n) => n.trim());
  const others: string[] = [];
  for (const raw of attendees) {
    const name = raw.trim();
    if (!name || SELF_WORDS.has(name.toLocaleLowerCase()) || mine.some((m) => same(m, name)) || others.some((o) => same(o, name))) continue;
    others.push(name);
  }
  // Names already given to someone in this meeting aren't offered again.
  const given = Object.entries(meeting.speakers).filter(([id]) => id !== "me").map(([, n]) => n ?? "");
  const names = others.filter((n) => !given.some((g) => same(g, n)));

  const heard = new Set<Speaker>(meeting.transcript.length ? meeting.transcript.map((l) => l.speaker) : (Object.keys(meeting.speakers) as Speaker[]));
  const them = [...heard].filter((s) => s !== "me");
  const only = them.length === 1 ? them[0] : undefined;
  const unnamed = only && (!meeting.speakers[only]?.trim() || meeting.speakers[only] === "Them");
  const probably = only && unnamed && others.length === 1 && names.length === 1 ? { speaker: only, name: names[0] } : undefined;
  return { names, probably };
}
