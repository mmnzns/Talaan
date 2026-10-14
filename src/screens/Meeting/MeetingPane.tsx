import { useEffect, useState } from "react";
import { useMeeting } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, Ms } from "../../data/types";
import { MeetingHeader } from "./MeetingHeader";
import { StatusBanner } from "./StatusBanner";
import { SummaryCard } from "./SummaryCard";
import { TranscriptCard } from "./TranscriptCard";

export function MeetingPane({ id, jumpTo }: { id?: string; jumpTo?: Ms }) {
  const { meeting, loading } = useMeeting(id);

  if (!id) {
    return (
      <div className="grid flex-1 place-items-center p-8 text-center">
        <div className="flex max-w-sm flex-col gap-2">
          <span className="text-lg font-extrabold tracking-tight">Select a meeting</span>
          <p className="m-0 text-[13px] text-subtle">
            Pick a call from the list to read its summary and transcript, or press Record to start a new one.
          </p>
        </div>
      </div>
    );
  }
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

function MeetingView({ meeting, jumpTo }: { meeting: Meeting; jumpTo?: Ms }) {
  const store = useStore();
  const [editing, setEditing] = useState(false);
  const [activeAt, setActiveAt] = useState<Ms | undefined>(jumpTo);
  const [jumpSeq, setJumpSeq] = useState(0);
  const [playing, setPlaying] = useState(false);

  const jump = (at: Ms) => {
    setActiveAt(at);
    setJumpSeq((n) => n + 1);
  };

  useEffect(() => {
    if (jumpTo !== undefined) jump(jumpTo);
  }, [jumpTo]);

  useEffect(() => {
    if (!playing) return;
    const timer = setInterval(() => {
      setActiveAt((a) => {
        const next = (a ?? 0) + 500;
        if (next >= meeting.durationMs) {
          setPlaying(false);
          return meeting.durationMs;
        }
        return next;
      });
    }, 500);
    return () => clearInterval(timer);
  }, [playing, meeting.durationMs]);

  const canRevert = Boolean(meeting.summary?.edited) || meeting.transcript.some((l) => l.edited);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto">
      <MeetingHeader
        meeting={meeting}
        editing={editing}
        canRevert={canRevert}
        onToggleEdit={() => setEditing((e) => !e)}
        onRevert={() => store.revertEdits(meeting.id)}
        onCopy={() => {}}
        onExport={() => {}}
      />
      <StatusBanner meeting={meeting} />
      <div className="grid flex-1 grid-cols-1 items-start gap-4 px-6 pb-6 min-[1000px]:min-h-0 min-[1000px]:grid-cols-2 min-[1000px]:items-stretch">
        <SummaryCard meeting={meeting} activeAt={activeAt} editing={editing} onJump={jump} />
        <TranscriptCard
          meeting={meeting}
          activeAt={activeAt}
          jumpSeq={jumpSeq}
          editing={editing}
          playing={playing}
          onTogglePlay={() => setPlaying((p) => !p)}
          onJump={jump}
        />
      </div>
    </div>
  );
}
