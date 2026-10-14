import { useState } from "react";
import { useMeetings } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import { dayGroup, formatClock } from "../../lib/format";
import { copyText, toSlack } from "../../lib/share";
import { openSettings } from "../../lib/windows";
import { navigate, routes } from "../../lib/router";
import { Button, Label, useToast } from "../../ui";

interface Props {
  onStart: () => void;
}

/** Menu bar popover when nothing is recording. */
export function MenuBarPopover({ onStart }: Props) {
  const store = useStore();
  const toast = useToast();
  const { meetings } = useMeetings();
  const [copied, setCopied] = useState<string>();
  const recent = meetings.slice(0, 3);

  const copy = async (id: string) => {
    const m = await store.getMeeting(id);
    if (!m) return;
    await copyText(toSlack(m));
    setCopied(id);
    setTimeout(() => setCopied(undefined), 1800);
  };

  const item = "flex items-center gap-2.5 rounded-control px-2.5 py-2 hover:bg-lime-soft";

  return (
    <div role="dialog" aria-label="Talaan menu" className="flex w-[356px] flex-col overflow-hidden rounded-card border-[1.5px] border-edge bg-surface shadow-hard-lg">
      <div className="flex items-center gap-2.5 px-3.5 pb-3 pt-3.5">
        <span className="size-2 rounded-full bg-lime shadow-[0_0_0_3px_color-mix(in_srgb,var(--t-lime)_30%,transparent)]" />
        <span className="font-bold">Ready</span>
        <Label>offline · local AI</Label>
      </div>
      <div className="px-3.5 pb-3.5">
        <Button variant="primary" className="h-[42px] w-full text-sm" onClick={onStart}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24">
            <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="2" />
            <circle cx="12" cy="12" r="4" fill="currentColor" />
          </svg>
          Start recording
        </Button>
        <Label className="mt-2 block leading-[15px]">Talaan also asks when Zoom, Meet, Teams or Slack starts using your mic.</Label>
      </div>
      <div className="flex flex-col gap-0.5 border-t-[1.5px] border-line px-1.5 py-2.5">
        <Label className="px-2.5 pb-1.5 pt-0.5">recent</Label>
        {recent.length === 0 && <span className="px-2.5 py-2 text-[13px] text-subtle">Meetings you record show up here.</span>}
        {recent.map((m) => {
          const busy = m.status === "summarizing" || m.status === "transcribing";
          const day = dayGroup(m.startedAt).label;
          return (
            <div key={m.id} className={item}>
              <button
                type="button"
                onClick={() => navigate(routes.meeting(m.id))}
                className="flex min-w-0 flex-1 cursor-pointer flex-col border-0 bg-transparent p-0 text-left font-[inherit] text-ink"
              >
                <span className="truncate font-semibold">{m.title}</span>
                <Label>
                  {busy ? `${m.status === "summarizing" ? "Summarizing" : "Transcribing"} · a minute left` : `${day[0].toUpperCase() + day.slice(1)} ${formatClock(m.startedAt)}`}
                </Label>
              </button>
              {m.status === "ready" && (
                <button
                  type="button"
                  onClick={() => copy(m.id)}
                  className="cursor-pointer rounded-md border border-line bg-paper px-[7px] py-0.5 font-mono text-[11px] text-ink hover:border-edge"
                >
                  {copied === m.id ? "Copied" : "Copy summary"}
                </button>
              )}
            </div>
          );
        })}
      </div>
      <div className="flex border-t-[1.5px] border-line p-1.5">
        <button type="button" onClick={() => navigate(routes.meetings())} className={`${item} flex-1 cursor-pointer border-0 bg-transparent font-[inherit] text-[13px] font-semibold text-ink`}>
          Open Talaan
        </button>
        <button type="button" onClick={() => openSettings()} className={`${item} cursor-pointer border-0 bg-transparent font-[inherit] text-[13px] text-ink`}>
          Settings
        </button>
        <button type="button" onClick={() => toast("Talaan would quit here")} className={`${item} cursor-pointer border-0 bg-transparent font-[inherit] text-[13px] text-ink`}>
          Quit
        </button>
      </div>
    </div>
  );
}
