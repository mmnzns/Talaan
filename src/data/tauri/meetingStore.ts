import type { NotionDestination } from "../settings";
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

  async playbackUrl(id: MeetingId) {
    const path = await call<string | null>("meetings_playback", { id });
    if (!path) return undefined;
    const { convertFileSrc } = await import("@tauri-apps/api/core");
    return convertFileSrc(path);
  }

  deleteAudio(id: MeetingId) {
    return call<void>("meetings_delete_audio", { id });
  }

  revealMeeting(id: MeetingId) {
    return call<void>("meetings_reveal", { id });
  }

  renameSpeaker(id: MeetingId, speaker: Speaker, name: string) {
    return call<void>("meetings_rename_speaker", { id, speaker, name });
  }

  setTranscriptOnly(id: MeetingId, on: boolean) {
    return call<void>("meetings_set_transcript_only", { id, on });
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

  renameFolder(id: string, name: string): Promise<void> {
    return call("folders_rename", { id, name });
  }

  deleteFolder(id: string): Promise<void> {
    return call("folders_delete", { id });
  }

  listTags(): Promise<string[]> {
    return call("tags_list");
  }

  exportToNotion(id: MeetingId, destination: NotionDestination, includeTranscript: boolean): Promise<string> {
    return call<string>("notion_export", { id, destination, includeTranscript });
  }

  ask(question: string, scope: AskScope): Promise<AskAnswer> {
    return call<AskAnswer>("meetings_ask", { question, scope });
  }

  subscribe(listener: () => void): () => void {
    return onEvent("meetings-changed", listener);
  }
}
