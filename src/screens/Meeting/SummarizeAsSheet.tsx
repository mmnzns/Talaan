import { useEffect, useState } from "react";
import { useSettings } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import type { Meeting } from "../../data/types";
import { describeError } from "../../data/errors";
import { TEMPLATES, isTemplate, templateName } from "../../lib/templates";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Label, Radio, Sheet, cn, useToast } from "../../ui";
import { summaryHasChanges } from "../../lib/summary";

/** The meeting type this meeting's summary was (or will be) written as. */
export function currentTemplate(meeting: Meeting, fallback?: string): string {
  const id = meeting.summary?.template ?? meeting.template ?? fallback;
  return isTemplate(id) ? id : "general";
}

/** ⋯ › Summarize as…: pick a meeting type and write the summary again. */
export function SummarizeAsSheet({ meeting, open, onClose }: { meeting: Meeting; open: boolean; onClose: () => void }) {
  const t = useT();
  const store = useStore();
  const toast = useToast();
  const { settings } = useSettings();
  const current = currentTemplate(meeting, settings?.summaryTemplate);
  const [choice, setChoice] = useState(current);
  useEffect(() => {
    if (open) setChoice(current);
  }, [open, current]);

  const start = () => {
    onClose();
    store.summarizeAs(meeting.id, choice).catch((e) => toast(describeError(e)));
  };

  return (
    <Sheet open={open} title={t("meeting.summarizeAsTitle")} onClose={onClose}>
      <div className="flex flex-col gap-3 p-5">
        <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("meeting.summarizeAsTitle")}</h2>
        <p className="m-0 text-[13px] text-subtle">{t("meeting.summarizeAsDesc")}</p>
        {summaryHasChanges(meeting.summary) && (
          <p role="note" className="m-0 rounded-control bg-warn-bg px-3 py-2 text-[13px] font-semibold text-warn-fg">
            {t("meeting.rewriteBody")}
          </p>
        )}
        <div
          role="radiogroup"
          aria-label={t("common.meetingType")}
          className="grid grid-cols-1 gap-1.5 min-[420px]:grid-cols-2"
          onKeyDown={(e) => {
            // Arrow keys move the choice, like any radio group; Tab enters and leaves on the chosen one.
            const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
            if (!step) return;
            e.preventDefault();
            const next = TEMPLATES[(TEMPLATES.indexOf(choice as (typeof TEMPLATES)[number]) + step + TEMPLATES.length) % TEMPLATES.length];
            setChoice(next);
            (e.currentTarget.querySelector(`[data-id="${next}"]`) as HTMLElement | null)?.focus();
          }}
        >
          {TEMPLATES.map((id) => (
            <button
              key={id}
              type="button"
              role="radio"
              data-id={id}
              aria-checked={choice === id}
              tabIndex={choice === id ? 0 : -1}
              onClick={() => setChoice(id)}
              className={cn(
                "flex cursor-pointer items-center gap-2.5 rounded-control border bg-surface px-3 py-2 text-left font-[inherit] text-[13px] text-ink",
                choice === id ? "border-edge" : "border-line hover:border-edge",
              )}
            >
              <Radio checked={choice === id} />
              <span className="min-w-0 flex-1">{t(templateName(id))}</span>
              {id === current && meeting.summary && <Label>{t("meeting.summarizeAsCurrent")}</Label>}
            </button>
          ))}
        </div>
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={meeting.transcript.length === 0} onClick={start}>
            {t("meeting.summarizeAsTitle")}
          </Button>
        </div>
      </div>
    </Sheet>
  );
}
