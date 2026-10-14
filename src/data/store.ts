import type {
  AskAnswer,
  AskScope,
  Folder,
  Meeting,
  MeetingId,
  MeetingListItem,
  Speaker,
  Summary,
} from "./types";

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
  renameSpeaker(id: MeetingId, speaker: Speaker, name: string): Promise<void>;
  setTags(id: MeetingId, tags: string[]): Promise<void>;
  moveToFolder(id: MeetingId, folderId?: string): Promise<void>;
  listFolders(): Promise<Folder[]>;
  listTags(): Promise<string[]>;
  ask(question: string, scope: AskScope): Promise<AskAnswer>;
  /** Fires on any change. Returns an unsubscribe function. */
  subscribe(listener: () => void): () => void;
}
