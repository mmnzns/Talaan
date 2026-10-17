import { Mail, Sparkles } from "lucide-react";
import { useCallback, useState } from "react";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, MeetingSummaryItem, Ms, Summary } from "../../data/types";
import { describeError, summaryErrorText } from "../../data/errors";
import { openSettings } from "../../lib/windows";
import { ownerName } from "../../lib/transcript";
import { meetingNotes } from "../../lib/share";
import { templateName } from "../../lib/templates";
import { useT } from "../../lib/i18n/I18nProvider";
import { etaText, useNow, type Eta } from "../../lib/eta";
import { useSettings } from "../../data/hooks";
import { Button, Checkbox, Chip, Editable, Label, SectionLabel, Timestamp, cn, useToast } from "../../ui";
import { Clamp } from "./Clamp";
import { FollowUpSheet } from "./FollowUpSheet";
import { NotesSection } from "./NotesSection";

interface Props {
  meeting: Meeting;
  activeAt?: Ms;
  editing: boolean;
  onJump: (at: Ms) => void;
}

const row = "-m-1.5 flex items-start gap-3 rounded-lg p-1.5";
const hit = "bg-gold-soft";

export function SummaryCard({ meeting, activeAt, editing, onJump }: Props) {
  const store = useStore();
  const toast = useToast();
  const t = useT();
  const { summary, status } = meeting;
  const busy = status === "summarizing" || status === "transcribing";
  const [notesChanged, setNotesChanged] = useState(false);
  const [followUp, setFollowUp] = useState(false);
  const closeFollowUp = useCallback(() => setFollowUp(false), []);
  const showNotes = status !== "recording" || meetingNotes(meeting).length > 0;

  const save = (next: Summary) => store.updateSummary(meeting.id, { ...next, edited: true });
  const set = <K extends keyof Summary>(key: K, value: Summary[K]) => save({ ...summary!, [key]: value });
  const summarizeAgain = () => {
    setNotesChanged(false);
    store.generateSummary(meeting.id).catch((e) => toast(describeError(e)));
  };

  // After the summary when there is one (the summary reads first), else on top.
  const notes = showNotes && (
    <>
      <NotesSection meeting={meeting} activeAt={activeAt} onJump={onJump} onChanged={() => summary && !busy && setNotesChanged(true)} />
      {notesChanged && summary && !busy && (
        <div className="-mt-3 flex flex-wrap items-center gap-2 rounded-control bg-rail px-3 py-2 text-xs">
          <span className="min-w-0 flex-1">{t("meeting.notesChanged")}</span>
          <Button className="h-7 text-xs" onClick={summarizeAgain}>
            <Sparkles size={13} aria-hidden="true" /> {t("meeting.summarizeAgain")}
          </Button>
        </div>
      )}
    </>
  );

  /** A timed item that can be edited in place (decisions, section items, highlights). */
  const timed = (item: MeetingSummaryItem, commit: (text: string) => void) => (
    <div key={item.id} className={cn(row, item.at !== undefined && item.at === activeAt && hit)}>
      {item.at !== undefined && <Timestamp at={item.at} onJump={onJump} active={item.at === activeAt} />}
      <Editable text={item.text} editing={editing} onCommit={commit} />
    </div>
  );

  return (
    <section aria-label={t("meeting.summary")} className="flex min-h-0 flex-col overflow-hidden rounded-card border border-line bg-surface shadow-card">
      <div className="flex items-center justify-between gap-3 border-b border-line px-6 py-[18px]">
        <span className="flex min-w-0 items-center gap-2">
          <h2 className="m-0 font-display text-base font-bold tracking-[-0.02em]">{t("meeting.summary")}</h2>
          {summary?.template && summary.template !== "general" && <Chip>{t(templateName(summary.template))}</Chip>}
        </span>
        <span className="flex min-w-0 items-center gap-2">
          {summary?.edited && <Chip tone="gold">{t("meeting.edited")}</Chip>}
          {/* Room is short: the follow-up button takes the place of the "click a time" hint. */}
          {(editing || !summary || status !== "ready") && <Label className="truncate">{editing ? t("meeting.editingHint") : summary ? t("meeting.clickTime") : ""}</Label>}
          {summary && status === "ready" && !editing && (
            <Button variant="ghost" className="-my-1.5 -mr-2 h-7 flex-none px-2 text-xs" onClick={() => setFollowUp(true)}>
              <Mail size={13} aria-hidden="true" /> {t("meeting.followUp")}
            </Button>
          )}
        </span>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-7 overflow-auto px-6 pb-[26px] pt-[22px]">
        {busy && summary && <Progress label={t("meeting.rewritingSummary")} value={meeting.progress} eta={meeting.eta} />}

        {!summary && notes}

        {summary ? (
          <div className={cn("flex flex-col gap-7", busy && "pointer-events-none opacity-50")} aria-busy={busy || undefined}>
            <p className="m-0 text-[15px] leading-[23px] [text-wrap:pretty]">
              <Editable text={summary.recap} editing={editing} onCommit={(v) => set("recap", v)} />
            </p>

            <Group title={t("meeting.decisionsLabel")} empty={t("meeting.noDecisions")} show={summary.decisions.length > 0}>
              {summary.decisions.map((d) => timed(d, (v) => set("decisions", summary.decisions.map((x) => (x.id === d.id ? { ...x, text: v } : x)))))}
            </Group>

            <Group title={t("meeting.actionItemsLabel")} empty={t("meeting.noActionItems")} show={summary.actionItems.length > 0}>
              {summary.actionItems.map((a) => (
                <div key={a.id} className={cn(row, a.at !== undefined && a.at === activeAt && hit)}>
                  <Checkbox
                    aria-label={t("meeting.doneAria", { text: a.text })}
                    checked={a.done}
                    onChange={(done) =>
                      store.updateSummary(meeting.id, {
                        ...summary,
                        actionItems: summary.actionItems.map((x) => (x.id === a.id ? { ...x, done } : x)),
                      })
                    }
                  />
                  <span className={cn("min-w-0 flex-1", a.done && "text-subtle line-through")}>
                    <Editable
                      text={a.text}
                      editing={editing}
                      onCommit={(v) => set("actionItems", summary.actionItems.map((x) => (x.id === a.id ? { ...x, text: v } : x)))}
                    />{" "}
                    {a.owner && <span className="rounded-full bg-sp1 px-[7px] py-0.5 text-[11px] font-semibold text-sp1-fg">{ownerName(meeting, a.owner)}</span>}{" "}
                    {a.due && <Chip>{a.due}</Chip>}
                  </span>
                  {a.at !== undefined && <Timestamp at={a.at} onJump={onJump} active={a.at === activeAt} />}
                </div>
              ))}
            </Group>

            <Group title={t("meeting.openQuestionsLabel")} empty={t("meeting.noOpenQuestions")} show={summary.openQuestions.length > 0}>
              {summary.openQuestions.map((q) => (
                <span key={q.id}>
                  <Editable
                    text={q.text}
                    editing={editing}
                    onCommit={(v) => set("openQuestions", summary.openQuestions.map((x) => (x.id === q.id ? { ...x, text: v } : x)))}
                  />
                </span>
              ))}
            </Group>

            {(summary.sections ?? []).map((sec) => (
              <Group key={sec.id} title={sec.title} empty="" show>
                <Clamp count={sec.items.length} limit={4}>
                  {(shown) =>
                    sec.items.slice(0, shown).map((item) =>
                      timed(item, (v) =>
                        set(
                          "sections",
                          summary.sections!.map((x) => (x.id === sec.id ? { ...x, items: x.items.map((i) => (i.id === item.id ? { ...i, text: v } : i)) } : x)),
                        ),
                      ),
                    )
                  }
                </Clamp>
              </Group>
            ))}

            {(summary.highlights?.length ?? 0) > 0 && (
              <Group title={t("meeting.highlightsLabel")} empty="" show>
                <Clamp count={summary.highlights!.length} limit={3}>
                  {(shown) =>
                    summary.highlights!.slice(0, shown).map((h) => timed(h, (v) => set("highlights", summary.highlights!.map((x) => (x.id === h.id ? { ...x, text: v } : x)))))
                  }
                </Clamp>
              </Group>
            )}

          </div>
        ) : null}
        {summary && notes}
        {summary ? (
          <div className="-mt-3">
            <Label>{t("meeting.writtenBy", { engine: summary.engine })}</Label>
          </div>
        ) : busy ? (
          <Progress label={status === "summarizing" ? t("meeting.writingSummary") : t("meeting.transcribingAudio")} value={meeting.progress} eta={meeting.eta} />
        ) : meeting.transcript.length > 0 ? (
          <div className="flex flex-col items-start gap-2 py-2">
            <span className="font-display text-base font-bold">{t("meeting.noSummaryTitle")}</span>
            <p className="m-0 text-[13px] text-subtle">{summaryErrorText(meeting.summaryError)}</p>
            <span className="flex flex-wrap gap-2">
              {meeting.summaryError?.code !== "too_short" && (
                <Button variant="primary" onClick={() => store.generateSummary(meeting.id).catch((e) => toast(describeError(e)))}>
                  <Sparkles size={14} aria-hidden="true" /> {t("meeting.generateSummary")}
                </Button>
              )}
              {meeting.summaryError?.code === "no_engine" && <Button onClick={() => void openSettings("ai")}>{t("meeting.openSummaryAi")}</Button>}
            </span>
          </div>
        ) : (
          <p className="m-0 py-2 text-[13px] text-subtle">{t("meeting.nothingToSummarize")}</p>
        )}
      </div>
      {summary && <FollowUpSheet meeting={meeting} open={followUp} onClose={closeFollowUp} />}
    </section>
  );
}

function Group({ title, show, empty, children }: { title: string; show: boolean; empty: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-3">
      <SectionLabel>{title}</SectionLabel>
      {show ? children : <span className="text-[13px] text-subtle">{empty}</span>}
    </div>
  );
}

/** Transcribing or summarizing: the bar, the time left (Rust's estimate, counted down here) and what to expect. */
export function Progress({ label, value, eta }: { label: string; value?: number; eta?: Eta }) {
  const t = useT();
  const now = useNow();
  const { settings } = useSettings();
  const pct = Math.round((value ?? 0) * 100);
  const left = etaText(t, eta, now);
  return (
    <div className="flex flex-col gap-2 py-2" role="status">
      <span className="text-[13px] font-semibold">{label}…</span>
      <span className="flex items-center gap-2">
        <span className="h-[3px] flex-1 overflow-hidden rounded-full bg-line">
          <span className="block h-full bg-gold" style={{ width: `${pct}%` }} />
        </span>
        <Label>{pct}%</Label>
      </span>
      {left && <span className="text-[13px]">{left}</span>}
      <span className="text-xs leading-[17px] text-subtle">
        {eta?.onCpu && <>{t("meeting.onCpu")} </>}
        {settings?.notifySummaryReady === false ? t("meeting.keepUsing") : t("meeting.keepUsingNotify")}
      </span>
    </div>
  );
}
