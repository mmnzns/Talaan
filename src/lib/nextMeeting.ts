import type { CalendarEventInfo } from "../data/recorder";
import type { NextMeeting } from "../data/settings";

/** "now" once it has started (until it ends), else whole minutes to go, rounded up ("in 12 min"). */
export function startsIn(next: Pick<NextMeeting, "startMs" | "endMs">, nowMs: number): "now" | "ended" | number {
  if (nowMs >= next.endMs) return "ended";
  if (nowMs >= next.startMs) return "now";
  return Math.max(1, Math.ceil((next.startMs - nowMs) / 60_000));
}

/** What the recording keeps from the event (`Meeting.calendarEvent`). */
export function eventOf(next: NextMeeting): CalendarEventInfo {
  return { title: next.title, attendees: next.attendees, emails: next.emails };
}

/** The call app to record as, when the invite's link says which one. */
export function appOf(next: NextMeeting): { id: string; label: string } | undefined {
  return next.appId && next.appLabel ? { id: next.appId, label: next.appLabel } : undefined;
}
