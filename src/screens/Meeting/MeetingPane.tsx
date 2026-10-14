import { useEffect, useState } from "react";
import { useMeeting, useMeetings } from "../../data/hooks";
import { copyText, toEmail, toShareText } from "../../lib/share";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, Ms } from "../../data/types";
import { usePlayer } from "./usePlayer";
import { ExportSheet } from "./ExportSheet";
import { Button } from "../../ui";
import { useStartRecording } from "../Shell/RecordingPanel";
import { MeetingHeader } from "./MeetingHeader";
import { StatusBanner } from "./StatusBanner";
import { SummaryCard } from "./SummaryCard";
import { TranscriptCard } from "./TranscriptCard";

export function MeetingPane({ id, jumpTo }: { id?: string; jumpTo?: Ms }) {
  const { meeting, loading } = useMeeting(id);

  if (!id) return <EmptyPane />;
  if (loading) return null;
  if (!meeting) {
    return (
      <div className="grid flex-1 place-items-center p-8 text-center">
        <div className="flex max-w-sm flex-col gap-2">
          <span className="text-lg font-extrabold tracking-tight">Meeting not found</span>
          <p className="m-0 text-[13px] text-subtle">It may have been deleted. Pick another one from the list.</p>
        </div>
      </div>
    );
  }
  return <MeetingView key={meeting.id} meeting={meeting} jumpTo={jumpTo} />;
}

function EmptyPane() {
  const { meetings, loading } = useMeetings();
  const startRecording = useStartRecording();
  if (loading) return null;
  const first = meetings.length === 0;
  return (
    <div className="grid flex-1 place-items-center p-8 text-center">
      <div className="flex max-w-sm flex-col items-center gap-2">
        <span className="text-lg font-extrabold tracking-tight">{first ? "Your meetings will show up here" : "Select a meeting"}</span>
        <p className="m-0 text-[13px] text-subtle">
          {first
            ? "Start a call and Talaan asks before it records. Or press Record, or import a Voice Memo. Each one gets a transcript, a summary and action items."
            : "Pick a call from the list to read its summary and transcript, or press Record to start a new one."}
        </p>
        {first && (
          <div className="mt-2 flex gap-2">
            <Button variant="primary" raised onClick={() => startRecording?.()}>
              Record a call
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}

function MeetingView({ meeting, jumpTo }: { meeting: Meeting; jumpTo?: Ms }) {
  const store = useStore();
  const [editing, setEditing] = useState(false);
  const [activeAt, setActiveAt] = useState<Ms | undefined>(jumpTo);
  const [jumpSeq, setJumpSeq] = useState(0);
  const player = usePlayer(meeting, (at) => setActiveAt(at));
  const [copied, setCopied] = useState<"text" | "email">();
  const [exporting, setExporting] = useState(false);

  // Clicking a time jumps the transcript there and plays from it while the audio is kept.
  const jump = (at: Ms) => {
    setActiveAt(at);
    setJumpSeq((n) => n + 1);
    if (player.canPlay) void player.playFrom(at);
  };

  // Opening a meeting at a time (from Ask or a notification) shows that line; it doesn't start playing.
  useEffect(() => {
    if (jumpTo === undefined) return;
    setActiveAt(jumpTo);
    setJumpSeq((n) => n + 1);
  }, [jumpTo]);

  const copy = async (kind: "text" | "email") => {
    await copyText(kind === "text" ? toShareText(meeting) : toEmail(meeting));
    setCopied(kind);
    setTimeout(() => setCopied(undefined), 2000);
  };

  const canRevert = Boolean(meeting.summary?.edited) || meeting.transcript.some((l) => l.edited);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto">
      <MeetingHeader
        meeting={meeting}
        editing={editing}
        canRevert={canRevert}
        onToggleEdit={() => setEditing((e) => !e)}
        onRevert={() => store.revertEdits(meeting.id)}
        onCopy={copy}
        onExport={() => setExporting(true)}
        copied={copied}
      />
      <StatusBanner meeting={meeting} />
      <div className="grid flex-1 grid-cols-1 items-start gap-4 px-6 pb-6 min-[1000px]:min-h-0 min-[1000px]:grid-cols-2 min-[1000px]:items-stretch">
        <SummaryCard meeting={meeting} activeAt={activeAt} editing={editing} onJump={jump} />
        <TranscriptCard
          meeting={meeting}
          activeAt={activeAt}
          jumpSeq={jumpSeq}
          editing={editing}
          playing={player.playing}
          onTogglePlay={() => void player.toggle(activeAt ?? 0)}
          onJump={jump}
        />
      </div>
      <ExportSheet open={exporting} meeting={meeting} onClose={() => setExporting(false)} />
    </div>
  );
}
