export type MeetingId = string;
export type Ms = number; // milliseconds from recording start
export type Speaker = "me" | `them:${number}`; // them:0 = unnamed "Them"; them:1.. = diarized speakers

export type MeetingStatus =
  | "recording"
  | "transcribing"
  | "summarizing"
  | "ready"
  | "failed"
  | "needs_recovery";

export type CallApp =
  | "zoom"
  | "meet"
  | "teams"
  | "slack"
  | "facetime"
  | "webex"
  | "browser"
  | "import"
  | "other";

export interface MeetingSummaryItem {
  id: string;
  text: string;
  at?: Ms;
}

export interface ActionItem extends MeetingSummaryItem {
  owner?: string;
  due?: string;
  done: boolean;
}

export interface Summary {
  recap: string;
  decisions: MeetingSummaryItem[];
  actionItems: ActionItem[];
  openQuestions: MeetingSummaryItem[];
  engine: string; // "Local AI · <model>" | "Claude" | ...
  edited: boolean; // true when the user changed it (original kept in storage)
}

export interface TranscriptLine {
  id: string;
  at: Ms;
  end: Ms;
  speaker: Speaker;
  text: string;
  edited: boolean;
}

export interface Mark {
  id: string;
  at: Ms;
  note?: string;
}

export interface PauseGap {
  at: Ms;
  durationMs: Ms;
}

export interface MeetingListItem {
  id: MeetingId;
  title: string;
  app: CallApp;
  appLabel: string;
  startedAt: string; // ISO
  durationMs: Ms;
  status: MeetingStatus;
  progress?: number; // 0..1 while transcribing/summarizing
  folderId?: string;
  tags: string[];
  peopleCount: number;
  pinned: boolean; // keep this meeting's audio past the retention window
}

export interface Meeting extends MeetingListItem {
  language: string; // BCP-47, e.g. "en"
  hasAudio: boolean; // audio files exist on disk
  audioKeptUntil?: string; // ISO; undefined = pinned, retention is "forever", or no audio
  calendarEvent?: { title: string; attendees: string[] };
  speakers: Partial<Record<Speaker, string>>; // display names: { me: "Me", "them:1": "Dana Reyes" }
  summary?: Summary; // undefined = transcript-only ("Generate summary")
  transcript: TranscriptLine[];
  marks: Mark[];
  pauses: PauseGap[];
}

export interface Folder {
  id: string;
  name: string;
  count: number;
}

export interface AskCitation {
  meetingId: MeetingId;
  meetingTitle: string;
  date: string;
  at: Ms;
}

export interface AskAnswer {
  text: string;
  points: { text: string; citation?: AskCitation }[];
  fromMeetings: number;
  totalMeetings: number;
}

export type AskScope =
  | { kind: "meeting"; id: MeetingId }
  | { kind: "all" }
  | { kind: "folder"; id: string }
  | { kind: "tag"; tag: string };
