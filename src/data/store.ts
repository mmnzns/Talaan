import type { ExportScope } from "../lib/share";
import type { NotionDestination } from "./settings";
import type {
  AskAnswer,
  AskScope,
  Folder,
  Mark,
  Meeting,
  MeetingId,
  MeetingListItem,
  Speaker,
  Summary,
} from "./types";

export interface FollowUpEta {
  finishAt: string;
  onCpu: boolean;
  waiting: boolean;
}

export interface FollowUpDraft {
  subject: string;
  body: string;
  to: string[];
}

export interface MeetingFilter {
  folderId?: string;
  tag?: string;
  query?: string;
}

/** The one door between screens and meeting data. M1 uses a mock; M2 swaps in the real one. */
export interface MeetingStore {
  listMeetings(filter?: MeetingFilter): Promise<MeetingListItem[]>;
  getMeeting(id: MeetingId): Promise<Meeting | undefined>;
  renameMeeting(id: MeetingId, title: string): Promise<void>;
  setPinned(id: MeetingId, pinned: boolean): Promise<void>;
  deleteMeeting(id: MeetingId): Promise<void>;
  updateTranscriptLine(id: MeetingId, lineId: string, text: string): Promise<void>;
  updateSummary(id: MeetingId, summary: Summary): Promise<void>;
  revertEdits(id: MeetingId): Promise<void>;
  /** Interrupted recording: keep what was saved and process it (M2 recovery). */
  recoverMeeting(id: MeetingId): Promise<void>;
  /** Failed processing: run transcription again. */
  retryProcessing(id: MeetingId): Promise<void>;
  /** Transcribe a finished meeting again with another speech model. Edits and the summary are replaced; names stay. */
  retranscribe(id: MeetingId, model: string): Promise<void>;
  /** Transcript-only meeting: write the summary now (M5). */
  generateSummary(id: MeetingId): Promise<void>;
  /** The meeting type to summarize it as (lib/templates); undefined = the Settings default. Works while recording. */
  setTemplate(id: MeetingId, template?: string): Promise<void>;
  /** "Summarize as…": remembers the type and writes the summary again (it replaces the old one and its edits). */
  summarizeAs(id: MeetingId, template: string): Promise<void>;
  /**
   * "Summarize in…" / "Change language": remembers the language on the meeting and writes the summary again from the
   * transcript in it (never a translation of the old one; it replaces the old one and its edits). Rejects a code that
   * isn't in lib/summaryLanguages, and a meeting without a finished transcript.
   */
  summarizeIn(id: MeetingId, language: string): Promise<void>;
  /** The user's notes (marks with a note), edited in the meeting view. Marked moments stay. Not while recording. */
  setNotes(id: MeetingId, notes: Mark[]): Promise<void>;
  /** When a follow-up draft for this meeting should be done; `waiting`: the local AI finishes a summary first. */
  followUpEta(id: MeetingId, request?: string): Promise<FollowUpEta>;
  /** Draft follow-up email from the summary: subject, plain-text body, and the invitees' addresses when known.
   *  `request` names this draft for `cancelFollowUp`. */
  draftFollowUp(id: MeetingId, request: string): Promise<FollowUpDraft>;
  /** Stops that follow-up draft, waiting or being written. */
  cancelFollowUp(request: string): Promise<void>;
  /** A URL to play the meeting's audio (both sides mixed), or undefined when there is none (or in the browser). */
  playbackUrl(id: MeetingId): Promise<string | undefined>;
  /** Deletes the audio now; transcript and summary stay. */
  deleteAudio(id: MeetingId): Promise<void>;
  /** Shows the meeting's folder in Finder. */
  revealMeeting(id: MeetingId): Promise<void>;
  renameSpeaker(id: MeetingId, speaker: Speaker, name: string): Promise<void>;
  /** Transcript only: the meeting waits for Generate summary (works while recording too). */
  setTranscriptOnly(id: MeetingId, on: boolean): Promise<void>;
  setTags(id: MeetingId, tags: string[]): Promise<void>;
  moveToFolder(id: MeetingId, folderId?: string): Promise<void>;
  listFolders(): Promise<Folder[]>;
  createFolder(name: string): Promise<Folder>;
  renameFolder(id: string, name: string): Promise<void>;
  /** Removes the folder; its meetings stay, outside any folder. */
  deleteFolder(id: string): Promise<void>;
  listTags(): Promise<string[]>;
  ask(question: string, scope: AskScope): Promise<AskAnswer>;
  /** Sends the meeting to Notion; returns the new page's URL. */
  exportToNotion(id: MeetingId, destination: NotionDestination, scope: ExportScope): Promise<string>;
  /** Fires on any change. Returns an unsubscribe function. */
  subscribe(listener: () => void): () => void;
}
