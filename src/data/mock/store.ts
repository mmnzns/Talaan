import type { MeetingFilter, MeetingStore } from "../store";
import type {
  AskAnswer,
  AskScope,
  Folder,
  Meeting,
  MeetingId,
  MeetingListItem,
  Speaker,
  Summary,
} from "../types";
import { mockAsk } from "./askAnswers";
import { seedFolders, seedMeetings } from "./seed";

const clone = <T>(v: T): T => structuredClone(v);

function toListItem(m: Meeting): MeetingListItem {
  return {
    id: m.id,
    title: m.title,
    app: m.app,
    appLabel: m.appLabel,
    startedAt: m.startedAt,
    durationMs: m.durationMs,
    status: m.status,
    progress: m.progress,
    folderId: m.folderId,
    tags: [...m.tags],
    peopleCount: m.peopleCount,
  };
}

/** In-memory MeetingStore with sample data. Replaced by the real store in M2. */
export class MockMeetingStore implements MeetingStore {
  private meetings = new Map<MeetingId, Meeting>();
  private originals = new Map<MeetingId, { summary?: Summary; lines: Map<string, string> }>();
  private listeners = new Set<() => void>();

  constructor(private now: Date = new Date()) {
    for (const m of seedMeetings(now)) {
      this.meetings.set(m.id, m);
      this.originals.set(m.id, {
        summary: m.summary ? clone(m.summary) : undefined,
        lines: new Map(m.transcript.map((l) => [l.id, l.text])),
      });
    }
  }

  private emit() {
    for (const l of [...this.listeners]) l();
  }

  private mutate(id: MeetingId, fn: (m: Meeting) => void) {
    const m = this.meetings.get(id);
    if (!m) return;
    fn(m);
    this.emit();
  }

  async listMeetings(filter: MeetingFilter = {}): Promise<MeetingListItem[]> {
    const q = filter.query?.trim().toLowerCase();
    return [...this.meetings.values()]
      .filter((m) => !filter.folderId || m.folderId === filter.folderId)
      .filter((m) => !filter.tag || m.tags.includes(filter.tag))
      .filter((m) => {
        if (!q) return true;
        const hay = [
          m.title,
          m.summary?.recap,
          ...(m.summary?.decisions.map((d) => d.text) ?? []),
          ...(m.summary?.actionItems.map((a) => a.text) ?? []),
          ...m.transcript.map((l) => l.text),
        ];
        return hay.some((s) => s?.toLowerCase().includes(q));
      })
      .sort((a, b) => Date.parse(b.startedAt) - Date.parse(a.startedAt))
      .map(toListItem);
  }

  async getMeeting(id: MeetingId): Promise<Meeting | undefined> {
    const m = this.meetings.get(id);
    return m ? clone(m) : undefined;
  }

  async renameMeeting(id: MeetingId, title: string) {
    this.mutate(id, (m) => (m.title = title));
  }

  async setPinned(id: MeetingId, pinned: boolean) {
    this.mutate(id, (m) => (m.pinned = pinned));
  }

  async deleteMeeting(id: MeetingId) {
    if (this.meetings.delete(id)) {
      this.originals.delete(id);
      this.emit();
    }
  }

  async updateTranscriptLine(id: MeetingId, lineId: string, text: string) {
    this.mutate(id, (m) => {
      const line = m.transcript.find((l) => l.id === lineId);
      if (line) {
        line.text = text;
        line.edited = text !== this.originals.get(id)?.lines.get(lineId);
      }
    });
  }

  async updateSummary(id: MeetingId, summary: Summary) {
    this.mutate(id, (m) => (m.summary = clone(summary)));
  }

  async revertEdits(id: MeetingId) {
    const orig = this.originals.get(id);
    this.mutate(id, (m) => {
      for (const line of m.transcript) {
        const text = orig?.lines.get(line.id);
        if (text !== undefined) line.text = text;
        line.edited = false;
      }
      if (orig?.summary) m.summary = { ...clone(orig.summary), edited: false };
    });
  }

  async renameSpeaker(id: MeetingId, speaker: Speaker, name: string) {
    this.mutate(id, (m) => (m.speakers[speaker] = name));
  }

  async setTags(id: MeetingId, tags: string[]) {
    this.mutate(id, (m) => (m.tags = [...tags]));
  }

  async moveToFolder(id: MeetingId, folderId?: string) {
    this.mutate(id, (m) => (m.folderId = folderId));
  }

  async listFolders(): Promise<Folder[]> {
    return seedFolders.map((f) => ({
      ...f,
      count: [...this.meetings.values()].filter((m) => m.folderId === f.id).length,
    }));
  }

  async listTags(): Promise<string[]> {
    return [...new Set([...this.meetings.values()].flatMap((m) => m.tags))].sort();
  }

  async ask(question: string, scope: AskScope): Promise<AskAnswer> {
    return mockAsk(question, scope, this.now);
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}
