import { Sparkles } from "lucide-react";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, Ms, Summary } from "../../data/types";
import { Button, Checkbox, Chip, Editable, Label, Timestamp, cn } from "../../ui";

interface Props {
  meeting: Meeting;
  activeAt?: Ms;
  editing: boolean;
  onJump: (at: Ms) => void;
}

const row = "-m-1 flex items-start gap-2.5 rounded-md p-1";
const hit = "bg-lime-soft outline-[1.5px] outline-edge";

export function SummaryCard({ meeting, activeAt, editing, onJump }: Props) {
  const store = useStore();
  const { summary, status } = meeting;
  const busy = status === "summarizing" || status === "transcribing";

  const save = (next: Summary) => store.updateSummary(meeting.id, { ...next, edited: true });
  const set = <K extends keyof Summary>(key: K, value: Summary[K]) => save({ ...summary!, [key]: value });

  return (
    <section aria-label="Summary" className="flex min-h-0 flex-col rounded-card border-[1.5px] border-edge bg-surface">
      <div className="flex items-center justify-between border-b-[1.5px] border-line px-4 py-3">
        <h2 className="m-0 text-base font-bold">Summary</h2>
        <span className="flex items-center gap-2">
          {summary?.edited && <Chip tone="lime">Edited</Chip>}
          <Label>{editing ? "editing · revert anytime" : summary ? "click a time to jump" : ""}</Label>
        </span>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto px-4 py-3.5">
        {summary ? (
          <>
            <p className="m-0 text-[15px] leading-[22px] [text-wrap:pretty]">
              <Editable text={summary.recap} editing={editing} onCommit={(v) => set("recap", v)} />
            </p>

            <Group title="decisions" empty="No decisions recorded." show={summary.decisions.length > 0}>
              {summary.decisions.map((d) => (
                <div key={d.id} className={cn(row, d.at !== undefined && d.at === activeAt && hit)}>
                  {d.at !== undefined && <Timestamp at={d.at} onJump={onJump} />}
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
                    {a.owner && <Chip className="border-lime bg-lime-soft text-ok">{a.owner}</Chip>}{" "}
                    <Chip>{a.due ?? "No date"}</Chip>
                  </span>
                  {a.at !== undefined && <Timestamp at={a.at} onJump={onJump} />}
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
            <span className="text-[15px] font-extrabold tracking-tight">No summary yet</span>
            <p className="m-0 text-[13px] text-subtle">The transcript is saved. Write the summary whenever you’re ready.</p>
            <Button variant="primary" onClick={() => store.generateSummary(meeting.id)}>
              <Sparkles size={14} aria-hidden="true" /> Generate summary
            </Button>
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
    <div className="flex flex-col gap-2">
      <Label>{title}</Label>
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
        <span className="h-[5px] flex-1 overflow-hidden rounded-full bg-line">
          <span className="block h-full bg-ink" style={{ width: `${pct}%` }} />
        </span>
        <Label>{pct}%</Label>
      </span>
      <span className="text-xs text-subtle">You can keep using Talaan while this runs.</span>
    </div>
  );
}
