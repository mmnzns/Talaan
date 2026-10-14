import type { MeetingFilter, MeetingStore } from "../store";
import type { AskAnswer, AskScope, Folder, Meeting, MeetingId, MeetingListItem, Speaker, Summary } from "../types";
import { call, onEvent } from "./invoke";

/** MeetingStore backed by the Rust storage module. */
export class TauriMeetingStore implements MeetingStore {
  listMeetings(filter: MeetingFilter = {}): Promise<MeetingListItem[]> {
    return call("meetings_list", { filter });
  }

  async getMeeting(id: MeetingId): Promise<Meeting | undefined> {
    return (await call<Meeting | null>("meetings_get", { id })) ?? undefined;
  }

  renameMeeting(id: MeetingId, title: string) {
    return call<void>("meetings_rename", { id, title });
  }

  setPinned(id: MeetingId, pinned: boolean) {
    return call<void>("meetings_set_pinned", { id, pinned });
  }

  deleteMeeting(id: MeetingId) {
    return call<void>("meetings_delete", { id });
  }

  updateTranscriptLine(id: MeetingId, lineId: string, text: string) {
    return call<void>("meetings_update_transcript_line", { id, lineId, text });
  }

  updateSummary(id: MeetingId, summary: Summary) {
    return call<void>("meetings_update_summary", { id, summary });
  }

  revertEdits(id: MeetingId) {
    return call<void>("meetings_revert_edits", { id });
  }

  recoverMeeting(id: MeetingId) {
    return call<void>("meetings_recover", { id });
  }

  retryProcessing(id: MeetingId) {
    return call<void>("meetings_retry", { id });
  }

  generateSummary(id: MeetingId) {
    return call<void>("meetings_generate_summary", { id });
  }

  renameSpeaker(id: MeetingId, speaker: Speaker, name: string) {
    return call<void>("meetings_rename_speaker", { id, speaker, name });
  }

  setTags(id: MeetingId, tags: string[]) {
    return call<void>("meetings_set_tags", { id, tags });
  }

  moveToFolder(id: MeetingId, folderId?: string) {
    return call<void>("meetings_move_to_folder", { id, folderId: folderId ?? null });
  }

  listFolders(): Promise<Folder[]> {
    return call("folders_list");
  }

  createFolder(name: string): Promise<Folder> {
    return call("folders_create", { name });
  }

  listTags(): Promise<string[]> {
    return call("tags_list");
  }

  /** Real answers arrive with M8. Until then: an honest "nothing found". */
  async ask(_question: string, _scope: AskScope): Promise<AskAnswer> {
    const total = (await this.listMeetings()).length;
    return { text: "", points: [], fromMeetings: 0, totalMeetings: total };
  }

  subscribe(listener: () => void): () => void {
    return onEvent("meetings-changed", listener);
  }
}
