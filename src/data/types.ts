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
  /** While transcribing/summarizing: when that step should be done (from Rust's estimate) and whether it runs on the processor. */
  eta?: { finishAt: string; onCpu: boolean };
  folderId?: string;
  tags: string[];
  peopleCount: number;
  pinned: boolean; // keep this meeting's audio past the retention window
}

export interface Meeting extends MeetingListItem {
  language: string; // BCP-47, e.g. "en"
  hasAudio: boolean; // audio files exist on disk
  audioKeptUntil?: string; // ISO; undefined = pinned, retention is "forever", or no audio
  /** The calendar event the recording belonged to. `emails`: the attendees' addresses, when the calendar has them. */
  calendarEvent?: { title: string; attendees: string[]; emails?: string[] };
  speakers: Partial<Record<Speaker, string>>; // display names: { me: "Me", "them:1": "Dana Reyes" }
  summary?: Summary; // undefined = transcript-only ("Generate summary")
  transcript: TranscriptLine[];
  marks: Mark[];
  pauses: PauseGap[];
  /** Why processing failed (status "failed"). `no_model`: no speech model downloaded yet. */
  error?: { code: string; message: string };
  /**
   * Why there is no summary (status "ready", transcript only). `no_engine`: no summary AI set up; `failed`: it failed;
   * `too_short`: too little speech to summarize; `waiting`: transcript only until the user asks (Generate summary).
   */
  summaryError?: { code: string; message: string; detail?: string };
  /** This meeting waits for Generate summary instead of being summarized by itself. */
  transcriptOnly?: boolean;
  /** How the transcript was made. `model` is a speech model id (see Settings › Speech models). */
  transcription?: { model: string; echoLinesRemoved: number; speechMs: Ms; tookMs: Ms; finishedAt: string };
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
