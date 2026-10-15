import { Sparkles } from "lucide-react";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, Ms, Summary } from "../../data/types";
import { describeError } from "../../data/errors";
import { openSettings } from "../../lib/windows";
import { Button, Checkbox, Chip, Editable, Label, SectionLabel, Timestamp, cn, useToast } from "../../ui";

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
  const { summary, status } = meeting;
  const busy = status === "summarizing" || status === "transcribing";

  const save = (next: Summary) => store.updateSummary(meeting.id, { ...next, edited: true });
  const set = <K extends keyof Summary>(key: K, value: Summary[K]) => save({ ...summary!, [key]: value });

  return (
    <section aria-label="Summary" className="flex min-h-0 flex-col overflow-hidden rounded-card border border-line bg-surface shadow-card">
      <div className="flex items-center justify-between border-b border-line px-6 py-[18px]">
        <h2 className="m-0 font-display text-base font-bold tracking-[-0.02em]">Summary</h2>
        <span className="flex items-center gap-2">
          {summary?.edited && <Chip tone="gold">Edited</Chip>}
          <Label>{editing ? "editing · revert anytime" : summary ? "click a time to jump" : ""}</Label>
        </span>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-7 overflow-auto px-6 pb-[26px] pt-[22px]">
        {summary ? (
          <>
            <p className="m-0 text-[15px] leading-[23px] [text-wrap:pretty]">
              <Editable text={summary.recap} editing={editing} onCommit={(v) => set("recap", v)} />
            </p>

            <Group title="decisions" empty="No decisions recorded." show={summary.decisions.length > 0}>
              {summary.decisions.map((d) => (
                <div key={d.id} className={cn(row, d.at !== undefined && d.at === activeAt && hit)}>
                  {d.at !== undefined && <Timestamp at={d.at} onJump={onJump} active={d.at === activeAt} />}
                  <Editable
                    text={d.text}
                    editing={editing}
                    onCommit={(v) => set("decisions", summary.decisions.map((x) => (x.id === d.id ? { ...x, text: v } : x)))}
                  />
                </div>
              ))}
            </Group>

            <Group title="action items" empty="No action items." show={summary.actionItems.length > 0}>
              {summary.actionItems.map((a) => (
                <div key={a.id} className={cn(row, a.at !== undefined && a.at === activeAt && hit)}>
                  <Checkbox
                    aria-label={`Done: ${a.text}`}
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
                    {a.owner && <span className="rounded-full bg-sp1 px-[7px] py-0.5 text-[11px] font-semibold text-sp1-fg">{a.owner}</span>}{" "}
                    {a.due && <Chip>{a.due}</Chip>}
                  </span>
                  {a.at !== undefined && <Timestamp at={a.at} onJump={onJump} active={a.at === activeAt} />}
                </div>
              ))}
            </Group>

            <Group title="open questions" empty="Nothing left open." show={summary.openQuestions.length > 0}>
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
            <Label>Written by {summary.engine}</Label>
          </>
        ) : busy ? (
          <Progress label={status === "summarizing" ? "Writing the summary" : "Transcribing the audio"} value={meeting.progress} />
        ) : meeting.transcript.length > 0 ? (
          <div className="flex flex-col items-start gap-2 py-2">
            <span className="font-display text-base font-bold">No summary yet</span>
            <p className="m-0 text-[13px] text-subtle">{meeting.summaryError?.message ?? "The transcript is saved. Write the summary whenever you’re ready."}</p>
            <span className="flex flex-wrap gap-2">
              {meeting.summaryError?.code !== "too_short" && (
                <Button variant="primary" onClick={() => store.generateSummary(meeting.id).catch((e) => toast(describeError(e)))}>
                  <Sparkles size={14} aria-hidden="true" /> Generate summary
                </Button>
              )}
              {meeting.summaryError?.code === "no_engine" && <Button onClick={() => void openSettings("ai")}>Open Summary AI</Button>}
            </span>
          </div>
        ) : (
          <p className="m-0 py-2 text-[13px] text-subtle">Nothing to summarize yet.</p>
        )}
      </div>
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

export function Progress({ label, value }: { label: string; value?: number }) {
  const pct = Math.round((value ?? 0) * 100);
  return (
    <div className="flex flex-col gap-2 py-2" role="status">
      <span className="text-[13px] font-semibold">{label}…</span>
      <span className="flex items-center gap-2">
        <span className="h-[3px] flex-1 overflow-hidden rounded-full bg-line">
          <span className="block h-full bg-gold" style={{ width: `${pct}%` }} />
        </span>
        <Label>{pct}%</Label>
      </span>
      <span className="text-xs text-subtle">You can keep using Talaan while this runs.</span>
    </div>
  );
}
