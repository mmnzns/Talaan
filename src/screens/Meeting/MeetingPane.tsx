import { useEffect, useState } from "react";
import { useMeeting, useMeetings } from "../../data/hooks";
import { copyText, toEmail } from "../../lib/share";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, Ms } from "../../data/types";
import { usePlayer } from "./usePlayer";
import { ExportSheet, emailSummary } from "./ExportSheet";
import { Button, useToast } from "../../ui";
import { describeError } from "../../data/errors";
import { navigate, routes } from "../../lib/router";
import { useStartRecording } from "../Shell/RecordingPanel";
import { MeetingHeader } from "./MeetingHeader";
import { StatusBanner } from "./StatusBanner";
import { SummaryCard } from "./SummaryCard";
import { TranscriptCard } from "./TranscriptCard";
import { SunCorner } from "../../brand/SunCorner";
import { useT } from "../../lib/i18n/I18nProvider";
import { TestMeetingBanner, TestRecordingCard, useTestRecordingActive } from "../Onboarding/TestRecording";

export function MeetingPane({ id, jumpTo, openExport }: { id?: string; jumpTo?: Ms; openExport?: boolean }) {
  const { meeting, loading } = useMeeting(id);
  const t = useT();

  if (!id) return <EmptyPane />;
  if (loading) return null;
  if (!meeting) {
    return (
      <div className="grid flex-1 place-items-center p-8 text-center">
        <div className="flex max-w-sm flex-col gap-2">
          <span className="font-display text-lg font-bold">{t("meeting.notFound")}</span>
          <p className="m-0 text-[13px] text-subtle">{t("meeting.notFoundBody")}</p>
        </div>
      </div>
    );
  }
  return <MeetingView key={meeting.id} meeting={meeting} jumpTo={jumpTo} openExport={openExport} />;
}

function EmptyPane() {
  const { meetings, loading } = useMeetings();
  const startRecording = useStartRecording();
  const t = useT();
  // The test recording's meeting is in the list while it records: the card stays until it ends.
  const testing = useTestRecordingActive();
  if (loading) return null;
  const first = meetings.length === 0;
  return (
    <div className="relative grid flex-1 place-items-center overflow-hidden p-8 text-center">
      <SunCorner className="absolute right-0 top-0" />
      <div className="relative flex max-w-[420px] flex-col items-center gap-2">
        {(first || testing) && (
          <div className="mb-5 flex w-full justify-center">
            <TestRecordingCard />
          </div>
        )}
        <span className="font-display text-xl font-bold tracking-[-0.02em]">{first ? t("meeting.emptyFirstTitle") : t("meeting.emptyTitle")}</span>
        <p className="m-0 text-[13px] text-subtle">
          {first ? t("meeting.emptyFirstBody") : t("meeting.emptyBody")}
        </p>
        {first && (
          <div className="mt-2 flex gap-2">
            <Button variant="primary" raised onClick={() => startRecording?.()}>
              {t("meeting.recordCall")}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}

function MeetingView({ meeting, jumpTo, openExport }: { meeting: Meeting; jumpTo?: Ms; openExport?: boolean }) {
  const store = useStore();
  const [editing, setEditing] = useState(false);
  const [activeAt, setActiveAt] = useState<Ms | undefined>(jumpTo);
  const [jumpSeq, setJumpSeq] = useState(0);
  const player = usePlayer(meeting, (at) => setActiveAt(at));
  const [copiedEmail, setCopiedEmail] = useState(false);
  const [exporting, setExporting] = useState<false | "sheet" | "email-too-long">(false);
  const toast = useToast();
  // "Email this summary" from the meeting menu: straight to the mail app, or the sheet when the link is too long.
  const email = async () => {
    try {
      const { truncated } = await emailSummary(meeting);
      if (truncated) setExporting("email-too-long");
    } catch (e) {
      toast(describeError(e));
    }
  };

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

  const copyEmail = async () => {
    await copyText(toEmail(meeting));
    setCopiedEmail(true);
    setTimeout(() => setCopiedEmail(false), 2000);
  };

  // The menu bar's Export opens this meeting with the sheet showing. The flag leaves the address so asking again works.
  useEffect(() => {
    if (!openExport) return;
    setExporting("sheet");
    navigate(routes.meeting(meeting.id));
  }, [openExport, meeting.id]);

  const canRevert = Boolean(meeting.summary?.edited) || meeting.transcript.some((l) => l.edited);

  return (
    <div className="relative flex min-h-0 flex-1 flex-col overflow-auto">
      <SunCorner className="absolute right-0 top-0" />
      <MeetingHeader
        meeting={meeting}
        editing={editing}
        canRevert={canRevert}
        onToggleEdit={() => setEditing((e) => !e)}
        onRevert={() => store.revertEdits(meeting.id)}
        onCopyEmail={() => void copyEmail()}
        onExport={() => setExporting("sheet")}
        onEmail={() => void email()}
        copiedEmail={copiedEmail}
      />
      <StatusBanner meeting={meeting} />
      <TestMeetingBanner meeting={meeting} />
      <div className="relative grid flex-1 grid-cols-1 items-start gap-7 px-11 pb-11 min-[1000px]:min-h-0 min-[1000px]:grid-cols-2 min-[1000px]:items-stretch">
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
      <ExportSheet
        open={exporting !== false}
        meeting={meeting}
        initialFormat={exporting === "email-too-long" ? "email" : undefined}
        startTooLong={exporting === "email-too-long"}
        onClose={() => setExporting(false)}
      />
    </div>
  );
}
