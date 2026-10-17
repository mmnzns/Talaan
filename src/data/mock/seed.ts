import type { Folder, Meeting, Summary, SummarySection, TranscriptLine } from "../types";

const mmss = (s: string): number => {
  const [m, sec] = s.split(":").map(Number);
  return (m * 60 + sec) * 1000;
};

/** ISO timestamp `daysAgo` days before `now`, at the given local clock time. */
function at(now: Date, daysAgo: number, hour: number, minute: number): string {
  const d = new Date(now);
  d.setDate(d.getDate() - daysAgo);
  d.setHours(hour, minute, 0, 0);
  return d.toISOString();
}

const inDays = (now: Date, days: number): string => {
  const d = new Date(now);
  d.setDate(d.getDate() + days);
  return d.toISOString();
};

function lines(prefix: string, raw: [string, string, string][]): TranscriptLine[] {
  return raw.map(([time, speaker, text], i) => ({
    id: `${prefix}-l${i}`,
    at: mmss(time),
    end: i + 1 < raw.length ? mmss(raw[i + 1][0]) : mmss(time) + 6000,
    speaker: speaker as TranscriptLine["speaker"],
    text,
    edited: false,
  }));
}

export const seedFolders: Omit<Folder, "count">[] = [
  { id: "work", name: "Work" },
  { id: "clients", name: "Clients" },
  { id: "personal", name: "Personal" },
];

export function seedMeetings(now: Date = new Date()): Meeting[] {
  const q4: Omit<Meeting, "hasAudio"> = {
    id: "m-q4",
    title: "Q4 roadmap sync",
    app: "zoom",
    appLabel: "Zoom",
    startedAt: at(now, 0, 14, 30),
    durationMs: mmss("42:10"),
    status: "ready",
    folderId: "work",
    tags: ["roadmap", "zoom"],
    peopleCount: 3,
    language: "en",
    audioKeptUntil: inDays(now, 7),
    pinned: false,
    calendarEvent: { title: "Q4 Roadmap Sync", attendees: ["You", "Dana Reyes", "Priya Shah"], emails: ["dana.reyes@example.com", "priya.shah@example.com"] },
    speakers: { me: "Me", "them:1": "Dana Reyes", "them:2": "Priya Shah" },
    transcription: { model: "whisper-turbo", echoLinesRemoved: 2, speechMs: mmss("38:40"), tookMs: 95_000, finishedAt: at(now, 0, 15, 14) },
    summary: {
      recap:
        "Onboarding revamp ships before the pricing refresh, since onboarding drop-off is the bigger leak. Engineering fits it in two sprints if design hands off by Oct 24.",
      decisions: [
        { id: "q4-d0", text: "Onboarding revamp ships first; pricing moves to Q1.", at: mmss("00:21") },
        { id: "q4-d1", text: "Launch email waits until the new flow is live.", at: mmss("00:55") },
      ],
      actionItems: [
        { id: "q4-a0", text: "Send onboarding wireframes to design", owner: "Me", due: "Fri Oct 17", at: mmss("00:41"), done: false },
        { id: "q4-a1", text: "Confirm the two-sprint estimate", owner: "Dana Reyes", due: "Thu Oct 16", at: mmss("00:33"), done: false },
        { id: "q4-a2", text: "Find an owner for onboarding analytics", owner: "Priya Shah", at: mmss("01:12"), done: false },
      ],
      openQuestions: [
        { id: "q4-o0", text: "Who owns analytics tracking for the new onboarding steps?" },
      ],
      highlights: [{ id: "h0", text: "You promised design the onboarding wireframes by Friday, so the two-sprint plan can hold.", at: mmss("00:41") }],
      template: "general",
      engine: "Local AI",
      edited: false,
    },
    transcript: lines("q4", [
      ["00:00", "them:1", "Okay, looks like everyone is here. Let us start with the Q4 list."],
      ["00:06", "me", "Sounds good. I put the onboarding revamp and the pricing refresh at the top."],
      ["00:14", "them:2", "Right, and we probably cannot do both before the holidays."],
      ["00:21", "me", "Agreed. Onboarding drop-off is the bigger leak, so I would ship that first."],
      ["00:33", "them:1", "Engineering can fit it in two sprints if design hands off by the 24th."],
      ["00:41", "me", "I can get the wireframes to design by Friday."],
      ["00:55", "me", "None from me. I will hold the launch email until the new flow is live."],
      ["01:12", "them:2", "Someone still needs to own analytics for the new steps. I can find out who."],
    ]),
    marks: [
      { id: "q4-n0", at: mmss("00:22"), note: "onboarding first, pricing waits for Q1" },
      { id: "q4-k0", at: mmss("00:41") },
      { id: "q4-k1", at: mmss("01:12"), note: "check with the data team" },
      { id: "q4-n1", at: 0, note: "Ask Priya if analytics needs budget", later: true },
    ],
    pauses: [{ at: mmss("00:48"), durationMs: 2 * 60 * 1000 }],
  };

  const vendor: Omit<Meeting, "hasAudio"> = {
    id: "m-vendor",
    title: "Vendor demo: email platform",
    app: "zoom",
    appLabel: "Zoom",
    startedAt: at(now, 0, 11, 0),
    durationMs: mmss("38:20"),
    status: "summarizing",
    progress: 0.6,
    eta: { finishAt: new Date(now.getTime() + 8 * 60_000).toISOString(), onCpu: true },
    folderId: "work",
    tags: ["vendors", "zoom"],
    peopleCount: 2,
    language: "en",
    audioKeptUntil: inDays(now, 7),
    pinned: false,
    speakers: { me: "Me", "them:0": "Them" },
    transcript: lines("vendor", [
      ["00:00", "them:0", "Thanks for making time. I will walk through the journey builder first."],
      ["00:20", "me", "Great. Can you show how segments sync from our warehouse?"],
    ]),
    marks: [],
    pauses: [],
  };

  const oneOnOne: Omit<Meeting, "hasAudio"> = {
    id: "m-1on1",
    title: "Weekly 1:1",
    app: "slack",
    appLabel: "Slack huddle",
    startedAt: at(now, 1, 9, 0),
    durationMs: mmss("31:05"),
    status: "ready",
    folderId: "work",
    tags: ["1:1"],
    peopleCount: 2,
    language: "en",
    audioKeptUntil: inDays(now, 6),
    pinned: true,
    speakers: { me: "Me", "them:1": "Jordan Lee" },
    template: "one_on_one",
    summary: {
      recap:
        "Reviewed last week's priorities and agreed to push the pricing brief until the onboarding launch. Jordan flagged a staffing gap on the data side.",
      decisions: [{ id: "o-d0", text: "Pricing brief shared after the onboarding launch.", at: mmss("12:05") }],
      actionItems: [
        { id: "o-a0", text: "Share the Q1 pricing brief with design", owner: "Me", due: "After launch", at: mmss("12:05"), done: false },
        { id: "o-a1", text: "Draft a request for a data analyst", owner: "Jordan Lee", at: mmss("20:40"), done: true },
      ],
      openQuestions: [{ id: "o-o0", text: "Can the data analyst start before January?" }],
      sections: [
        { id: "s0", title: "Updates", items: [{ id: "s0i0", text: "Onboarding work is on track for the launch.", at: mmss("00:08") }] },
        { id: "s1", title: "Feedback", items: [{ id: "s1i0", text: "Jordan liked how the pricing brief was scoped.", at: mmss("12:05") }] },
        { id: "s2", title: "Support needed", items: [{ id: "s2i0", text: "Jordan needs a data analyst before January.", at: mmss("20:40") }] },
      ],
      template: "one_on_one",
      engine: "Local AI",
      edited: false,
    },
    transcript: lines("o", [
      ["00:00", "them:1", "How did the week go on your side?"],
      ["00:08", "me", "Busy, but the onboarding work is on track."],
      ["12:05", "me", "I will share the Q1 pricing brief with design after the onboarding launch."],
      ["20:40", "them:1", "I will draft the request for a data analyst today."],
    ]),
    marks: [{ id: "o-n0", at: mmss("20:41"), note: "Jordan short on data help" }],
    pauses: [],
  };

  const standup: Omit<Meeting, "hasAudio"> = {
    id: "m-standup",
    title: "Team stand-up",
    app: "meet",
    appLabel: "Google Meet",
    startedAt: at(now, 2, 9, 30),
    durationMs: mmss("14:20"),
    status: "ready",
    folderId: "work",
    tags: ["standup"],
    peopleCount: 4,
    language: "en",
    pinned: false,
    template: "standup",
    speakers: { me: "Me", "them:1": "Dana Reyes", "them:2": "Sam Okafor", "them:3": "Priya Shah" },
    summary: {
      recap: "A quick round of updates. The export fix shipped, the pricing page is in review, and Sam is blocked on legal sign-off for the new terms.",
      decisions: [{ id: "su-d0", text: "Dana takes over the release notes this week.", at: mmss("09:10") }],
      actionItems: [{ id: "su-a0", text: "Chase legal about the new terms", owner: "Me", due: "Today", at: mmss("06:30"), done: false }],
      openQuestions: [],
      sections: [
        {
          id: "s0",
          title: "Yesterday",
          items: [
            { id: "s0i0", text: "Dana: fixed the CSV export bug.", at: mmss("00:40") },
            { id: "s0i1", text: "Sam: finished the new terms page.", at: mmss("03:10") },
            { id: "s0i2", text: "Priya: reviewed two pricing drafts.", at: mmss("05:00") },
            { id: "s0i3", text: "Me: planned the onboarding analytics.", at: mmss("07:20") },
            { id: "s0i4", text: "Dana: cleaned up the release checklist.", at: mmss("01:30") },
          ],
        },
        {
          id: "s1",
          title: "Today",
          items: [
            { id: "s1i0", text: "Dana: start the release notes.", at: mmss("01:50") },
            { id: "s1i1", text: "Sam: wait on legal, then ship the terms page.", at: mmss("03:40") },
            { id: "s1i2", text: "Priya: send the pricing page for review.", at: mmss("05:30") },
          ],
        },
        { id: "s2", title: "Blockers", items: [{ id: "s2i0", text: "Sam: needs legal sign-off on the new terms.", at: mmss("06:30") }] },
      ] satisfies SummarySection[],
      template: "standup",
      engine: "Local AI",
      edited: false,
    },
    transcript: lines("su", [
      ["00:00", "me", "Morning all. Quick round, Dana first."],
      ["00:40", "them:1", "Yesterday I fixed the CSV export bug."],
      ["01:30", "them:1", "And I cleaned up the release checklist."],
      ["01:50", "them:1", "Today I start the release notes."],
      ["03:10", "them:2", "I finished the new terms page yesterday."],
      ["03:40", "them:2", "Today I am waiting on legal, then I can ship it."],
      ["05:00", "them:3", "I reviewed two pricing drafts."],
      ["05:30", "them:3", "Today the pricing page goes out for review."],
      ["06:30", "them:2", "My only blocker is legal sign-off on the terms."],
      ["07:20", "me", "I planned the onboarding analytics. I will chase legal for Sam today."],
      ["09:10", "them:1", "I can take the release notes this week."],
    ]),
    marks: [{ id: "su-k0", at: mmss("06:31") }],
    pauses: [],
  };

  const retro: Omit<Meeting, "hasAudio"> = {
    id: "m-retro",
    title: "Website launch retro",
    app: "meet",
    appLabel: "Google Meet",
    startedAt: at(now, 4, 13, 0),
    durationMs: mmss("55:00"),
    status: "ready",
    folderId: "work",
    tags: ["roadmap"],
    peopleCount: 4,
    language: "en",
    pinned: false,
    speakers: { me: "Me", "them:1": "Priya Shah", "them:2": "Sam Okafor" },
    summary: {
      recap:
        "The launch landed on time. Page speed regressed on mobile and the legal review started too late. Both get a checklist item for the next launch.",
      decisions: [{ id: "r-d0", text: "Legal review moves to the start of the launch checklist.", at: mmss("31:40") }],
      actionItems: [
        { id: "r-a0", text: "Check whether pricing needs legal review before design starts", owner: "Priya Shah", at: mmss("31:40"), done: false },
        { id: "r-a1", text: "Audit mobile page speed", owner: "Sam Okafor", due: "Oct 20", at: mmss("40:10"), done: false },
      ],
      openQuestions: [],
      engine: "Local AI",
      edited: false,
    },
    transcript: lines("r", [
      ["00:00", "me", "Let us start with what went well."],
      ["31:40", "them:1", "Legal review happened too late. I will check whether pricing needs it before design starts."],
      ["40:10", "them:2", "Mobile page speed dropped. I will audit it."],
    ]),
    marks: [],
    pauses: [],
  };

  const imported: Omit<Meeting, "hasAudio"> = {
    id: "m-import",
    title: "Imported: client kickoff.m4a",
    app: "import",
    appLabel: "Audio file",
    startedAt: at(now, 5, 10, 0),
    durationMs: mmss("24:00"),
    status: "ready",
    folderId: "clients",
    tags: [],
    peopleCount: 1,
    language: "en",
    pinned: false,
    speakers: { "them:0": "Them" },
    transcript: lines("i", [
      ["00:00", "them:0", "Welcome everyone, this is the kickoff for the Harbour Co. project."],
      ["00:30", "them:0", "The first milestone is the brand audit, due at the end of the month."],
    ]),
    marks: [],
    pauses: [],
  };

  const failed: Omit<Meeting, "hasAudio"> = {
    id: "m-failed",
    title: "Call, Teams, Oct 8",
    app: "teams",
    appLabel: "Teams",
    startedAt: at(now, 6, 15, 0),
    durationMs: mmss("18:00"),
    status: "failed",
    tags: [],
    peopleCount: 2,
    language: "en",
    pinned: false,
    speakers: { me: "Me", "them:0": "Them" },
    transcript: [],
    marks: [],
    pauses: [],
  };

  const recover: Omit<Meeting, "hasAudio"> = {
    id: "m-recover",
    title: "Call, Zoom, 8:50 AM",
    app: "zoom",
    appLabel: "Zoom",
    startedAt: at(now, 0, 8, 50),
    durationMs: mmss("12:30"),
    status: "needs_recovery",
    tags: [],
    peopleCount: 2,
    language: "en",
    pinned: false,
    speakers: { me: "Me", "them:0": "Them" },
    transcript: [],
    marks: [],
    pauses: [],
  };

  return [q4, vendor, oneOnOne, standup, retro, imported, failed, recover].map((m) => ({ ...m, hasAudio: Boolean(m.audioKeptUntil) || m.pinned }));
}

export function sampleTranscript(prefix: string): TranscriptLine[] {
  return lines(prefix, [
    ["00:00", "me", "Okay, I think we are recording. Can you hear me?"],
    ["00:05", "them:0", "Loud and clear. Let us get started."],
    ["00:20", "me", "First item is the timeline for the next release."],
  ]);
}

/** Sample sections per meeting type, for the browser preview's "Summarize as…". */
const SAMPLE_SECTIONS: Record<string, [string, string][]> = {
  one_on_one: [["Updates", "The release timeline is on track."], ["Feedback", "The written plan was clear."], ["Support needed", "Help with the release checklist."]],
  standup: [["Yesterday", "Me: drafted the release timeline."], ["Today", "Me: send the release plan."], ["Blockers", "None said."]],
  interview: [["Strengths", "Clear about the release process."], ["Concerns", "Little detail on testing."], ["Next steps", "Send the plan in writing."]],
  sales: [["Client needs", "A firm release date."], ["Objections", "Worried about the timeline slipping."], ["Next steps", "Send the release plan in writing."]],
  lecture: [["Key points", "Plan the release timeline first."], ["Terms", "Release plan: the written order of the steps."], ["Resources mentioned", "The release checklist."]],
};

export function sampleSummary(template = "general", marks: { at: number; note?: string; later?: boolean }[] = []): Summary {
  const sections = (SAMPLE_SECTIONS[template] ?? []).map(([title, text], i) => ({ id: `s${i}`, title, items: [{ id: `s${i}i0`, text, at: 20_000 }] }));
  const highlights = marks.filter((m) => !m.note && !m.later).map((m, i) => ({ id: `h${i}`, text: "What was being said when you marked this moment.", at: m.at }));
  return {
    recap:
      "A short call to confirm the timeline for the next release. Both sides agreed to start with the release plan and follow up in writing.",
    decisions: [{ id: "gen-d0", text: "Start with the release timeline.", at: 20_000 }],
    actionItems: [{ id: "gen-a0", text: "Send the release plan in writing", owner: "Me", at: 20_000, done: false }],
    openQuestions: [],
    ...(sections.length ? { sections } : {}),
    ...(highlights.length ? { highlights } : {}),
    template,
    engine: "Local AI",
    edited: false,
  };
}

/** The browser preview's "Draft follow-up email": built from the summary, the way the AI is asked to write it. */
export function sampleFollowUp(m: Meeting): { subject: string; body: string; to: string[] } {
  const s = m.summary!;
  const out = ["Hi everyone,", "", `Thanks for joining "${m.title}" today. Here is a quick recap.`];
  if (s.decisions.length) out.push("", "Decisions:", ...s.decisions.map((d) => `- ${d.text}`));
  if (s.actionItems.length)
    out.push("", "Action items:", ...s.actionItems.map((a) => `- ${a.text}${a.owner ? ` (${a.owner}${a.due ? `, ${a.due}` : ""})` : a.due ? ` (${a.due})` : ""}`));
  out.push("", "Thanks again,", m.speakers.me && m.speakers.me !== "Me" ? m.speakers.me : "");
  return { subject: `Follow-up: ${m.title}`, body: out.join("\n").trim(), to: m.calendarEvent?.emails ?? [] };
}
