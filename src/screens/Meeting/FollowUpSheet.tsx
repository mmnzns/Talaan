import { Copy, Mail } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useStore } from "../../data/StoreProvider";
import type { FollowUpDraft } from "../../data/store";
import type { Meeting } from "../../data/types";
import { describeError } from "../../data/errors";
import { followUpText, mailtoUrl } from "../../lib/followup";
import { copyText } from "../../lib/share";
import { etaText, useNow, type Eta } from "../../lib/eta";
import { openLink } from "../../lib/windows";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Label, Sheet } from "../../ui";

type State = { kind: "writing"; eta?: Eta; waiting?: boolean; started: number } | { kind: "done"; draft: FollowUpDraft } | { kind: "failed"; message: string };

/**
 * Draft follow-up email: written by the summary AI from the summary (not the transcript, so it's quick), shown for
 * a last look, then copied or opened in the mail app.
 */
export function FollowUpSheet({ meeting, open, onClose }: { meeting: Meeting; open: boolean; onClose: () => void }) {
  const t = useT();
  const store = useStore();
  const [state, setState] = useState<State>({ kind: "writing", started: Date.now() });
  const [copied, setCopied] = useState(false);
  const run = useRef(0);
  const now = useNow(1_000);

  const request = useRef("");
  const start = useCallback(() => {
    const mine = ++run.current;
    request.current = `${meeting.id}-${Date.now()}-${mine}`;
    setCopied(false);
    setState({ kind: "writing", started: Date.now() });
    // While it waits for a summary in progress, ask again now and then: the time left moves when that ends.
    const poll = () =>
      store.followUpEta(meeting.id).then(
        ({ waiting, ...eta }) => {
          if (run.current !== mine) return;
          setState((s) => (s.kind === "writing" ? { ...s, eta, waiting } : s));
          if (waiting) setTimeout(poll, 5_000);
        },
        () => {},
      );
    void poll();
    store.draftFollowUp(meeting.id, request.current).then(
      (draft) => run.current === mine && setState({ kind: "done", draft }),
      (e) => run.current === mine && setState({ kind: "failed", message: describeError(e) }),
    );
  }, [store, meeting.id]);

  useEffect(() => {
    if (open) start();
  }, [open, start]);

  // Stable, so the sheet does not take focus back from the text boxes on every render.
  const writing = useRef(false);
  writing.current = state.kind === "writing";
  const close = useCallback(() => {
    if (writing.current) void store.cancelFollowUp(request.current);
    run.current++;
    onClose();
  }, [store, onClose]);

  const draft = state.kind === "done" ? state.draft : undefined;
  const mailto = draft && mailtoUrl(draft);
  const edit = (patch: Partial<FollowUpDraft>) => draft && setState({ kind: "done", draft: { ...draft, ...patch } });

  return (
    <Sheet open={open} title={t("meeting.followUpTitle")} onClose={close} className="max-w-[560px]">
      <div className="flex flex-col gap-3 p-5">
        <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("meeting.followUpTitle")}</h2>
        <p className="m-0 text-[13px] text-subtle">{t("meeting.followUpDesc")}</p>

        {state.kind === "writing" && (
          <div role="status" className="flex flex-col gap-2 py-2">
            <span className="text-[13px] font-semibold">{state.waiting ? t("meeting.followUpWaiting") : t("meeting.followUpWriting")}…</span>
            <span className="h-[3px] overflow-hidden rounded-full bg-line">
              <span className="block h-full animate-pulse bg-gold" style={{ width: `${progress(state, now)}%` }} />
            </span>
            {etaText(t, state.eta, now) && <span className="text-[13px]">{etaText(t, state.eta, now)}</span>}
            {state.eta?.onCpu && <span className="text-xs leading-[17px] text-subtle">{t("meeting.onCpu")}</span>}
          </div>
        )}

        {state.kind === "failed" && <p className="m-0 rounded-control bg-warn-bg px-3 py-2 text-[13px] text-warn-fg">{state.message}</p>}

        {draft && (
          <>
            {draft.to.length > 0 && <Label className="normal-case">{t("meeting.followUpTo", { list: draft.to.join(", ") })}</Label>}
            <label className="flex flex-col gap-1">
              <Label>{t("meeting.followUpSubject")}</Label>
              <input
                value={draft.subject}
                onChange={(e) => edit({ subject: e.target.value })}
                className="h-[34px] rounded-control border border-input bg-surface px-3 font-[inherit] text-[13px] text-ink outline-none focus:border-ink"
              />
            </label>
            <label className="flex flex-col gap-1">
              <Label>{t("meeting.followUpBody")}</Label>
              <textarea
                value={draft.body}
                onChange={(e) => edit({ body: e.target.value })}
                rows={12}
                className="min-h-40 resize-y rounded-control border border-input bg-surface px-3 py-2 font-[inherit] text-[13px] leading-[19px] text-ink outline-none focus:border-ink"
              />
            </label>
            {!mailto && <span className="text-xs text-subtle">{t("meeting.followUpTooLong")}</span>}
          </>
        )}

        <div className="flex flex-wrap justify-end gap-2">
          {state.kind === "failed" && <Button onClick={start}>{t("meeting.followUpAgain")}</Button>}
          <Button onClick={close}>{t("common.cancel")}</Button>
          {draft && (
            <>
              {mailto && (
                <Button onClick={() => void openLink(mailto)}>
                  <Mail size={14} aria-hidden="true" /> {t("meeting.followUpOpenMail")}
                </Button>
              )}
              <Button
                variant="primary"
                onClick={async () => {
                  await copyText(followUpText(draft));
                  setCopied(true);
                }}
              >
                <Copy size={14} aria-hidden="true" /> {copied ? t("meeting.followUpCopied") : t("meeting.followUpCopy")}
              </Button>
            </>
          )}
        </div>
      </div>
    </Sheet>
  );
}

/** How far along the bar is: the share of the estimate used so far, never quite full before the email arrives. */
function progress(s: { eta?: Eta; started: number }, now: number): number {
  const finish = s.eta ? Date.parse(s.eta.finishAt) : NaN;
  if (Number.isNaN(finish) || finish <= s.started) return 8;
  return Math.max(8, Math.min(92, ((now - s.started) / (finish - s.started)) * 100));
}
