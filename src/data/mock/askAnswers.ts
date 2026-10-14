import type { AskAnswer, AskScope } from "../types";

const total = 18;

/** Canned answers for the mock. The real engine arrives in M8. */
export function mockAsk(question: string, scope: AskScope, now: Date): AskAnswer {
  const q = question.toLowerCase();
  const fmt = (daysAgo: number) =>
    new Date(now.getTime() - daysAgo * 86_400_000).toLocaleDateString(undefined, { month: "short", day: "numeric" });

  if (q.includes("pricing")) {
    return {
      text: "Pricing work moved to early Q1 so the onboarding revamp could ship first. Nobody objected in the roadmap sync.",
      points: [
        {
          text: "Pricing work moved to early Q1 so the onboarding revamp could ship first.",
          citation: { meetingId: "m-q4", meetingTitle: "Q4 roadmap sync", date: fmt(0), at: 48_000 },
        },
        {
          text: "You: share the Q1 pricing brief with design after the onboarding launch.",
          citation: { meetingId: "m-1on1", meetingTitle: "Weekly 1:1", date: fmt(1), at: 725_000 },
        },
        {
          text: "Priya Shah: check whether pricing needs legal review before design starts.",
          citation: { meetingId: "m-retro", meetingTitle: "Website launch retro", date: fmt(4), at: 1_900_000 },
        },
      ],
      fromMeetings: 3,
      totalMeetings: scope.kind === "all" ? total : 1,
    };
  }

  if (q.includes("action") || q.includes("open")) {
    return {
      text: "You have two open action items this week.",
      points: [
        {
          text: "Send onboarding wireframes to design by Fri Oct 17.",
          citation: { meetingId: "m-q4", meetingTitle: "Q4 roadmap sync", date: fmt(0), at: 41_000 },
        },
        {
          text: "Share the Q1 pricing brief with design after the onboarding launch.",
          citation: { meetingId: "m-1on1", meetingTitle: "Weekly 1:1", date: fmt(1), at: 725_000 },
        },
      ],
      fromMeetings: 2,
      totalMeetings: total,
    };
  }

  return { text: "", points: [], fromMeetings: 0, totalMeetings: total };
}
