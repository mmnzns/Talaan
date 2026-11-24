import { seedMeetings } from "../data/mock/seed";
import type { Meeting } from "../data/types";

/** An invented meeting with the detailed summary layout, for tests. */
export function detailedMeeting(): Meeting {
  const base = seedMeetings(new Date("2026-10-14T20:00:00")).find((m) => m.id === "m-q4")!;
  return {
    ...base,
    speakers: { me: "Alex", "them:1": "Rosa Lim" },
    summary: {
      recap: "To plan the pilot. Pilot scope: Two teams start in March.",
      purpose: "To plan the pilot of the new scheduling tool.",
      takeaways: [
        { label: "Pilot scope", text: "Two teams start in March." },
        { label: "Budget", text: "The licence fits the current budget." },
      ],
      topics: [
        {
          id: "t0",
          title: "Pilot plan",
          bullets: [
            { text: "Two teams, about 25 people in total.", subBullets: ["Support team first", "Sales team in week 3"] },
            { text: "Review after six weeks." },
          ],
        },
        { id: "t1", title: "Licence cost", bullets: [{ text: "Quoted at 4,000 per year." }] },
      ],
      decisions: [{ id: "d0", text: "Start the pilot in March.", at: 21_000 }],
      actionItems: [
        { id: "a0", text: "Send the rollout plan.", owner: "Alex", due: "Friday", at: 40_000, done: false },
        { id: "a1", text: "Confirm the licence quote.", owner: "Rosa Lim", at: 50_000, done: false },
        { id: "a2", text: "Book the kickoff room.", at: 55_000, done: false },
        { id: "a3", text: "Share the training slides.", owner: "alex", at: 60_000, done: true },
      ],
      openQuestions: [{ id: "q0", text: "Who trains the sales team?" }],
      template: "general",
      engine: "Local AI · Qwen3.5 4B",
      edited: false,
    },
  };
}
