import { useState } from "react";
import { useMeetings } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import { dayGroup, formatClock } from "../../lib/format";
import { copyText, toShareText } from "../../lib/share";
import { openSettings } from "../../lib/windows";
import { navigate, routes } from "../../lib/router";
import { Button, Label, useToast } from "../../ui";
import { useT } from "../../lib/i18n/I18nProvider";
import { Logo } from "../../brand/Logo";
import { SunCorner } from "../../brand/SunCorner";

interface Props {
  onStart: () => void;
  /** Open a meeting in the main window (the popover is its own window in the app). */
  onOpenMeeting?: (id: string) => void;
  onOpenApp?: () => void;
  onQuit?: () => void;
}

/** Menu bar popover when nothing is recording. */
export function MenuBarPopover({ onStart, onOpenMeeting, onOpenApp, onQuit }: Props) {
  const t = useT();
  const store = useStore();
  const toast = useToast();
  const { meetings } = useMeetings();
  const [copied, setCopied] = useState<string>();
  const recent = meetings.slice(0, 3);

  const copy = async (id: string) => {
    const m = await store.getMeeting(id);
    if (!m) return;
    await copyText(toShareText(m));
    setCopied(id);
    setTimeout(() => setCopied(undefined), 1800);
  };

  const item = "flex items-center gap-3 rounded-[10px] px-3 py-2.5 hover:bg-hover";

  return (
    <div role="dialog" aria-label={t("recording.menuAria")} className="flex w-[356px] flex-col overflow-hidden rounded-2xl border border-line bg-surface shadow-[0_16px_40px_rgba(17,19,24,0.14)]">
      <div className="relative flex flex-col gap-[18px] overflow-hidden border-b border-line px-[22px] pb-5 pt-[22px]">
        <SunCorner size={110} className="absolute right-0 top-0" />
        <div className="relative flex items-center gap-2.5">
          <Logo size={34} />
          <div className="flex flex-col">
            <span className="font-display text-[17px] font-bold leading-5 tracking-[-0.02em]">{t("recording.wordmark")}</span>
            <Label className="inline-flex items-center gap-1.5">
              <span className="size-1.5 rounded-full bg-gold" />
              {t("recording.readyOffline")}
            </Label>
          </div>
        </div>
        <Button variant="primary" className="relative h-[42px] w-full text-sm" onClick={onStart}>
          <span aria-hidden="true" className="size-[9px] rounded-full bg-red shadow-[0_0_0_3px_color-mix(in_srgb,var(--t-red)_30%,transparent)]" />
          {t("recording.startRecording")}
        </Button>
        <span className="relative text-xs leading-[17px] text-subtle">{t("recording.asksWhen")}</span>
      </div>
      <div className="flex flex-col gap-1 px-2.5 py-4">
        <Label className="px-3 pb-2">{t("recording.recent")}</Label>
        {recent.length === 0 && <span className="px-2.5 py-2 text-[13px] text-subtle">{t("recording.recentEmpty")}</span>}
        {recent.map((m) => {
          const busy = m.status === "summarizing" || m.status === "transcribing";
          const day = dayGroup(m.startedAt).label;
          return (
            <div key={m.id} className={item}>
              <button
                type="button"
                onClick={() => (onOpenMeeting ? onOpenMeeting(m.id) : navigate(routes.meeting(m.id)))}
                className="flex min-w-0 flex-1 cursor-pointer flex-col border-0 bg-transparent p-0 text-left font-[inherit] text-ink"
              >
                <span className="truncate font-semibold">{m.title}</span>
                <Label>{busy ? t(m.status === "summarizing" ? "recording.busySummarizing" : "recording.busyTranscribing") : `${day} ${formatClock(m.startedAt)}`}</Label>
              </button>
              {m.status === "ready" && (
                <button
                  type="button"
                  onClick={() => copy(m.id)}
                  className="h-[26px] cursor-pointer whitespace-nowrap rounded-[7px] border border-edge bg-surface px-2.5 font-[inherit] text-xs font-semibold text-ink hover:bg-hover"
                >
                  {copied === m.id ? t("recording.copied") : t("recording.copyAsText")}
                </button>
              )}
            </div>
          );
        })}
      </div>
      <div className="flex border-t border-line p-2">
        <button type="button" onClick={() => (onOpenApp ? onOpenApp() : navigate(routes.meetings()))} className={`${item} flex-1 cursor-pointer border-0 bg-transparent font-[inherit] text-[13px] font-semibold text-ink`}>
          {t("recording.openTalaan")}
        </button>
        <button type="button" onClick={() => openSettings()} className={`${item} cursor-pointer border-0 bg-transparent font-[inherit] text-[13px] text-ink`}>
          {t("recording.settings")}
        </button>
        <button type="button" onClick={() => (onQuit ? onQuit() : toast(t("recording.quitDemo")))} className={`${item} cursor-pointer border-0 bg-transparent font-[inherit] text-[13px] text-ink`}>
          {t("recording.quit")}
        </button>
      </div>
    </div>
  );
}
