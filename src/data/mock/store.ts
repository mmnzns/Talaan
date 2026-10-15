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
import { withYourName } from "../../lib/transcript";
import { mockAsk } from "./askAnswers";
import { sampleSummary, sampleTranscript, seedFolders, seedMeetings } from "./seed";

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
    pinned: m.pinned,
  };
}

/** In-memory MeetingStore with sample data. Replaced by the real store in M2. */
export class MockMeetingStore implements MeetingStore {
  private meetings = new Map<MeetingId, Meeting>();
  private originals = new Map<MeetingId, { summary?: Summary; lines: Map<string, string> }>();
  private folders = seedFolders.map((f) => ({ ...f }));
  private listeners = new Set<() => void>();
  private askDelayMs: number;
  private yourName: () => string | undefined;

  constructor(
    private now: Date = new Date(),
    options: { empty?: boolean; askDelayMs?: number; yourName?: () => string | undefined } = {},
  ) {
    this.askDelayMs = options.askDelayMs ?? 700;
    this.yourName = options.yourName ?? (() => undefined);
    for (const m of options.empty ? [] : seedMeetings(now)) {
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
    return m ? withYourName(clone(m), this.yourName()) : undefined;
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

  /** Fakes background work: ramps progress, then applies the result. */
  private simulate(id: MeetingId, status: "transcribing" | "summarizing", done: (m: Meeting) => void) {
    this.mutate(id, (m) => {
      m.status = status;
      m.progress = 0;
    });
    let p = 0;
    const timer = setInterval(() => {
      p += 0.25;
      if (p >= 1 || !this.meetings.has(id)) {
        clearInterval(timer);
        this.mutate(id, (m) => {
          done(m);
          m.progress = undefined;
        });
      } else {
        this.mutate(id, (m) => (m.progress = p));
      }
    }, 600);
  }

  private finishTranscription = (m: Meeting) => {
    m.status = "ready";
    m.transcript = sampleTranscript(m.id);
    m.speakers = { me: "Me", "them:0": "Them" };
    this.originals.set(m.id, { summary: undefined, lines: new Map(m.transcript.map((l) => [l.id, l.text])) });
  };

  async recoverMeeting(id: MeetingId) {
    this.simulate(id, "transcribing", this.finishTranscription);
  }

  async retryProcessing(id: MeetingId) {
    this.simulate(id, "transcribing", this.finishTranscription);
  }

  // The browser preview has no audio files: the player simulates the playhead.
  async playbackUrl(_id: MeetingId): Promise<string | undefined> {
    return undefined;
  }

  async deleteAudio(id: MeetingId) {
    this.mutate(id, (m) => {
      m.audioKeptUntil = undefined;
      m.hasAudio = false;
    });
  }

  async revealMeeting(_id: MeetingId) {}

  async generateSummary(id: MeetingId) {
    this.simulate(id, "summarizing", (m) => {
      m.status = "ready";
      m.summary = sampleSummary();
      this.originals.get(id)!.summary = clone(m.summary);
    });
  }

  async renameSpeaker(id: MeetingId, speaker: Speaker, name: string) {
    this.mutate(id, (m) => (m.speakers[speaker] = name));
  }

  async setTranscriptOnly(id: MeetingId, on: boolean) {
    this.mutate(id, (m) => (m.transcriptOnly = on || undefined));
  }

  async setTags(id: MeetingId, tags: string[]) {
    this.mutate(id, (m) => (m.tags = [...tags]));
  }

  async moveToFolder(id: MeetingId, folderId?: string) {
    this.mutate(id, (m) => (m.folderId = folderId));
  }

  async listFolders(): Promise<Folder[]> {
    return this.folders.map((f) => ({
      ...f,
      count: [...this.meetings.values()].filter((m) => m.folderId === f.id).length,
    }));
  }

  async createFolder(name: string): Promise<Folder> {
    const base = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "folder";
    let id = base;
    for (let n = 2; this.folders.some((f) => f.id === id); n++) id = `${base}-${n}`;
    const folder = { id, name: name.trim() };
    this.folders.push(folder);
    this.emit();
    return { ...folder, count: 0 };
  }

  async renameFolder(id: string, name: string): Promise<void> {
    const f = this.folders.find((x) => x.id === id);
    if (f && name.trim()) f.name = name.trim();
    this.emit();
  }

  async deleteFolder(id: string): Promise<void> {
    this.folders = this.folders.filter((f) => f.id !== id);
    for (const m of this.meetings.values()) if (m.folderId === id) m.folderId = undefined;
    this.emit();
  }

  async listTags(): Promise<string[]> {
    return [...new Set([...this.meetings.values()].flatMap((m) => m.tags))].sort();
  }

  async exportToNotion(id: MeetingId): Promise<string> {
    await new Promise((r) => setTimeout(r, 600));
    return `https://www.notion.so/sample-${id}`;
  }

  async ask(question: string, scope: AskScope): Promise<AskAnswer> {
    await new Promise((r) => setTimeout(r, this.askDelayMs));
    return mockAsk(question, scope, this.now);
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}
