import type { Folder, Meeting, Summary, TranscriptLine } from "../types";

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
      { id: "q4-k0", at: mmss("00:41") },
      { id: "q4-k1", at: mmss("01:12"), note: "check with the data team" },
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
    calendarEvent: { title: "Weekly 1:1", attendees: ["Jordan Lee"], emails: ["jordan.lee@example.com"] },
    speakers: { me: "Me", "them:0": "Them" },
    summary: {
      recap:
        "Reviewed last week's priorities and agreed to push the pricing brief until the onboarding launch. Jordan flagged a staffing gap on the data side.",
      decisions: [{ id: "o-d0", text: "Pricing brief shared after the onboarding launch.", at: mmss("12:05") }],
      actionItems: [
        { id: "o-a0", text: "Share the Q1 pricing brief with design", owner: "Me", due: "After launch", at: mmss("12:05"), done: false },
        { id: "o-a1", text: "Draft a request for a data analyst", owner: "Jordan Lee", at: mmss("20:40"), done: true },
      ],
      openQuestions: [{ id: "o-o0", text: "Can the data analyst start before January?" }],
      engine: "Local AI",
      edited: false,
    },
    transcript: lines("o", [
      ["00:00", "them:0", "How did the week go on your side?"],
      ["00:08", "me", "Busy, but the onboarding work is on track."],
      ["12:05", "me", "I will share the Q1 pricing brief with design after the onboarding launch."],
      ["20:40", "them:0", "I will draft the request for a data analyst today."],
    ]),
    marks: [],
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
    calendarEvent: { title: "Website launch retro", attendees: ["Priya Shah", "Sam Okafor", "Lee Wong"], emails: ["priya.shah@example.com", "sam.okafor@example.com", "lee.wong@example.com"] },
    speakers: { me: "Me" },
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

  return [q4, vendor, oneOnOne, retro, imported, failed, recover].map((m) => ({ ...m, hasAudio: Boolean(m.audioKeptUntil) || m.pinned }));
}

export function sampleTranscript(prefix: string): TranscriptLine[] {
  return lines(prefix, [
    ["00:00", "me", "Okay, I think we are recording. Can you hear me?"],
    ["00:05", "them:0", "Loud and clear. Let us get started."],
    ["00:20", "me", "First item is the timeline for the next release."],
  ]);
}

export function sampleSummary(): Summary {
  return {
    recap:
      "A short call to confirm the timeline for the next release. Both sides agreed to start with the release plan and follow up in writing.",
    decisions: [{ id: "gen-d0", text: "Start with the release timeline.", at: 20_000 }],
    actionItems: [{ id: "gen-a0", text: "Send the release plan in writing", owner: "Me", at: 20_000, done: false }],
    openQuestions: [],
    engine: "Local AI",
    edited: false,
  };
}
