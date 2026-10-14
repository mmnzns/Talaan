import { AlertTriangle } from "lucide-react";
import { useStore } from "../../data/StoreProvider";
import type { Meeting } from "../../data/types";
import { navigate, routes } from "../../lib/router";
import { openSettings } from "../../lib/windows";
import { describeError } from "../../data/errors";
import { Button, useToast } from "../../ui";

/** Shown above the meeting when it needs the user: interrupted recording or failed processing. */
export function StatusBanner({ meeting }: { meeting: Meeting }) {
  const store = useStore();
  const toast = useToast();
  const run = (p: Promise<void>) => p.catch((e) => toast(describeError(e)));
  const leave = async () => {
    navigate(routes.meetings());
    await store.deleteMeeting(meeting.id);
  };

  if (meeting.status === "needs_recovery") {
    return (
      <Banner
        title="This recording was interrupted"
        text="Talaan closed or the Mac slept before the call was saved. Recover what was captured, or discard it."
      >
        <Button variant="primary" onClick={() => run(store.recoverMeeting(meeting.id))}>Recover</Button>
        <Button onClick={leave}>Discard</Button>
      </Banner>
    );
  }
  if (meeting.status === "failed" && meeting.error?.code === "no_model") {
    return (
      <Banner title="No speech model yet" text="Download one in Settings. This call is transcribed as soon as it finishes.">
        <Button variant="primary" onClick={() => void openSettings("models")}>Open Speech models</Button>
      </Banner>
    );
  }
  if (meeting.status === "failed") {
    return (
      <Banner title="Processing failed" text={`${meeting.error ? `${meeting.error.message} ` : ""}The audio is safe on this Mac. Try again, or delete the meeting.`}>
        <Button variant="primary" onClick={() => run(store.retryProcessing(meeting.id))}>Try again</Button>
        <Button onClick={leave}>Delete</Button>
      </Banner>
    );
  }
  return null;
}

function Banner({ title, text, children }: { title: string; text: string; children: React.ReactNode }) {
  return (
    <div role="alert" className="mx-6 mb-4 flex flex-wrap items-center gap-3 rounded-control border-[1.5px] border-warn-fg bg-warn-bg px-3.5 py-2.5 text-warn-fg">
      <AlertTriangle size={18} aria-hidden="true" className="flex-none" />
      <div className="min-w-0 flex-1 basis-60">
        <div className="text-sm font-bold">{title}</div>
        <div className="text-[13px]">{text}</div>
      </div>
      <div className="flex gap-2">{children}</div>
    </div>
  );
}
