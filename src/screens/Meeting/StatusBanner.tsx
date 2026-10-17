import { AlertTriangle } from "lucide-react";
import { useStore } from "../../data/StoreProvider";
import type { Meeting } from "../../data/types";
import { navigate, routes } from "../../lib/router";
import { openSettings } from "../../lib/windows";
import { describeError } from "../../data/errors";
import { Button, useToast } from "../../ui";
import { useT } from "../../lib/i18n/I18nProvider";

/** Shown above the meeting when it needs the user: interrupted recording or failed processing. */
export function StatusBanner({ meeting }: { meeting: Meeting }) {
  const store = useStore();
  const toast = useToast();
  const t = useT();
  const run = (p: Promise<void>) => p.catch((e) => toast(describeError(e)));
  const leave = async () => {
    navigate(routes.meetings());
    await store.deleteMeeting(meeting.id);
  };

  if (meeting.status === "needs_recovery") {
    return (
      <Banner
        title={t("meeting.interruptedTitle")}
        text={t("meeting.interruptedText")}
      >
        <Button variant="primary" onClick={() => run(store.recoverMeeting(meeting.id))}>{t("meeting.recover")}</Button>
        <Button onClick={leave}>{t("meeting.discard")}</Button>
      </Banner>
    );
  }
  if (meeting.status === "failed" && meeting.error?.code === "no_model") {
    return (
      <Banner title={t("meeting.noModelTitle")} text={t("meeting.noModelText")}>
        <Button variant="primary" onClick={() => void openSettings("models")}>{t("meeting.openModels")}</Button>
        <StatusButton />
      </Banner>
    );
  }
  if (meeting.status === "failed") {
    // Only the transcriber fails a meeting (a summary that fails leaves the transcript). The audio is promised safe,
    // and Try again offered, only while it's still there.
    const message = meeting.error?.message;
    const text = meeting.hasAudio
      ? message ? t("meeting.failedTextWith", { message }) : t("meeting.failedText")
      : message ? t("meeting.failedNoAudioTextWith", { message }) : t("meeting.failedNoAudioText");
    return (
      <Banner title={t("meeting.failedTitle")} text={text}>
        {meeting.hasAudio && <Button variant="primary" onClick={() => run(store.retryProcessing(meeting.id))}>{t("meeting.tryAgain")}</Button>}
        <Button variant={meeting.hasAudio ? "default" : "primary"} onClick={leave}>{t("meeting.delete")}</Button>
        <StatusButton />
      </Banner>
    );
  }
  return null;
}

/** Settings › Status: every check a call needs, each with its fix. Banners about setup link to it. */
export function StatusButton() {
  const t = useT();
  return <Button onClick={() => void openSettings("status")}>{t("meeting.checkStatus")}</Button>;
}

function Banner({ title, text, children }: { title: string; text: string; children: React.ReactNode }) {
  return (
    <div role="alert" className="mx-6 mb-4 flex flex-wrap items-center gap-3 rounded-control border border-warn-fg bg-warn-bg px-3.5 py-2.5 text-warn-fg">
      <AlertTriangle size={18} aria-hidden="true" className="flex-none" />
      <div className="min-w-0 flex-1 basis-60">
        <div className="text-sm font-bold">{title}</div>
        <div className="text-[13px]">{text}</div>
      </div>
      <div className="flex flex-wrap gap-2">{children}</div>
    </div>
  );
}
