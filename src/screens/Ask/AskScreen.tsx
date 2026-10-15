import { ArrowLeft, Send } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useFolders, useMeeting, useSettings, useTags } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import type { AskAnswer, AskCitation, AskScope } from "../../data/types";
import { describeError } from "../../data/errors";
import { formatTime } from "../../lib/format";
import { navigate, routes } from "../../lib/router";
import { Button, Chip, Label, Select, cn } from "../../ui";

type Turn = { id: number; question: string; scope: string; state: "loading" | "done" | "error"; answer?: AskAnswer; error?: string };

const SUGGESTIONS = [
  "What did we decide about pricing?",
  "What are my open action items this week?",
  "What did the vendor promise on the demo?",
  "Summarize everything tagged #roadmap",
];

export function AskScreen({ meetingId }: { meetingId?: string }) {
  const store = useStore();
  const { settings } = useSettings();
  const folders = useFolders();
  const tags = useTags();
  const { meeting } = useMeeting(meetingId);

  const [scope, setScope] = useState<AskScope>(meetingId ? { kind: "meeting", id: meetingId } : { kind: "all" });
  const [draft, setDraft] = useState("");
  const [turns, setTurns] = useState<Turn[]>([]);
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setScope(meetingId ? { kind: "meeting", id: meetingId } : { kind: "all" });
  }, [meetingId]);

  useEffect(() => {
    end.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [turns]);

  const scopeLabel = (s: AskScope) =>
    s.kind === "meeting"
      ? meeting?.title ?? "This meeting"
      : s.kind === "folder"
        ? `Folder: ${folders.find((f) => f.id === s.id)?.name ?? ""}`
        : s.kind === "tag"
          ? `Tag: #${s.tag}`
          : "All meetings";

  const ask = async (question: string) => {
    const q = question.trim();
    if (!q) return;
    const id = Date.now();
    const label = scopeLabel(scope);
    setDraft("");
    setTurns((t) => [...t, { id, question: q, scope: label, state: "loading" }]);
    try {
      const answer = await store.ask(q, scope);
      setTurns((t) => t.map((x) => (x.id === id ? { ...x, state: "done", answer } : x)));
    } catch (e) {
      setTurns((t) => t.map((x) => (x.id === id ? { ...x, state: "error", error: describeError(e) } : x)));
    }
  };

  const local = !settings || settings.summaryEngine !== "byok";
  const pill = (active: boolean) =>
    cn(
      "h-7 cursor-pointer rounded-full border px-2.5 font-[inherit] text-xs font-semibold",
      active ? "border-strong bg-strong text-strong-fg" : "border-line bg-surface text-ink hover:border-edge",
    );

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto">
      <div className="flex flex-wrap items-center gap-2.5 border-b border-line px-6 py-3">
        <Button size="md" className="h-7" onClick={() => navigate(meetingId ? routes.meeting(meetingId) : routes.meetings())}>
          <ArrowLeft size={14} aria-hidden="true" /> {meetingId ? "Back to meeting" : "Meetings"}
        </Button>
        <span className="text-base font-extrabold tracking-[-0.03em]">Ask</span>
        <Chip>{local ? "Local AI · offline · searches stay on this computer" : "Your key · transcript text is sent · searches stay on this computer"}</Chip>
      </div>

      <div className="mx-auto flex w-full max-w-[820px] flex-1 flex-col gap-[18px] px-6 pb-5 pt-6">
        <div className="flex flex-wrap items-center gap-2" role="group" aria-label="What to ask about">
          <Label>asking</Label>
          {meetingId && (
            <button type="button" className={pill(scope.kind === "meeting")} onClick={() => setScope({ kind: "meeting", id: meetingId })}>
              This meeting
            </button>
          )}
          <button type="button" className={pill(scope.kind === "all")} onClick={() => setScope({ kind: "all" })}>
            All meetings
          </button>
          <Select
            label="Folder"
            value={scope.kind === "folder" ? scope.id : ""}
            onChange={(e) => e.target.value && setScope({ kind: "folder", id: e.target.value })}
            className={cn(pill(scope.kind === "folder"), "py-0 pr-7")}
          >
            <option value="">Folder…</option>
            {folders.map((f) => (
              <option key={f.id} value={f.id}>
                Folder: {f.name}
              </option>
            ))}
          </Select>
          <Select
            label="Tag"
            value={scope.kind === "tag" ? scope.tag : ""}
            onChange={(e) => e.target.value && setScope({ kind: "tag", tag: e.target.value })}
            className={cn(pill(scope.kind === "tag"), "py-0 pr-7")}
          >
            <option value="">Tag…</option>
            {tags.map((t) => (
              <option key={t} value={t}>
                Tag: #{t}
              </option>
            ))}
          </Select>
        </div>

        {turns.length === 0 && (
          <div className="flex flex-col gap-1 py-2">
            <span className="text-lg font-extrabold tracking-tight">Ask about your meetings</span>
            <p className="m-0 text-[13px] text-subtle">Answers come only from your own meetings and link to the exact moment.</p>
          </div>
        )}

        {turns.map((t) => (
          <div key={t.id} className="flex flex-col gap-3.5">
            <div className="flex justify-end">
              <div className="max-w-[80%] rounded-[12px_12px_2px_12px] bg-strong px-3.5 py-2.5 text-strong-fg">{t.question}</div>
            </div>
            <AnswerCard turn={t} onRetry={() => ask(t.question)} />
          </div>
        ))}

        <div className="flex flex-wrap gap-2">
          {SUGGESTIONS.map((s) => (
            <Chip key={s} onClick={() => ask(s)}>
              {s}
            </Chip>
          ))}
        </div>
        <div ref={end} />

        <form
          className="mt-auto flex items-center gap-2 rounded-card border border-edge bg-surface p-2"
          onSubmit={(e) => {
            e.preventDefault();
            ask(draft);
          }}
        >
          <label className="flex flex-1">
            <span className="sr-only">Ask a question</span>
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              placeholder={scope.kind === "meeting" ? "Ask about this meeting" : "Ask about your meetings"}
              className="flex-1 border-0 bg-transparent px-2 py-1.5 font-[inherit] text-sm text-ink outline-none placeholder:text-subtle"
            />
          </label>
          <Button type="submit" variant="primary" disabled={!draft.trim()}>
            <Send size={14} aria-hidden="true" /> Ask
          </Button>
        </form>
      </div>
    </div>
  );
}

function AnswerCard({ turn, onRetry }: { turn: Turn; onRetry: () => void }) {
  const card = "flex flex-col gap-2.5 rounded-card border border-edge bg-surface px-4 py-3.5 shadow-pop";
  if (turn.state === "loading") {
    return (
      <div className={card} role="status">
        <span className="text-[13px] text-subtle">Searching {turn.scope.toLowerCase()}…</span>
        <span className="h-[5px] w-40 overflow-hidden rounded-full bg-line">
          <span className="block h-full w-1/3 animate-pulse rounded-full bg-ink" />
        </span>
      </div>
    );
  }
  if (turn.state === "error" || !turn.answer) {
    return (
      <div className={card} role="alert">
        <span className="font-semibold text-danger">That didn’t work.</span>
        <p className="m-0 text-[13px] text-subtle">{turn.error ?? "The summary engine didn’t answer. Check Summary AI in Settings, then try again."}</p>
        <div>
          <Button onClick={onRetry}>Try again</Button>
        </div>
      </div>
    );
  }
  const a = turn.answer;
  if (a.points.length === 0 && !a.text) {
    return (
      <div className={card}>
        <p className="m-0">I couldn’t find that in {turn.scope === "All meetings" ? "your meetings" : turn.scope}.</p>
        <p className="m-0 text-[13px] text-subtle">Try different words, or ask across all meetings.</p>
        <Label>From 0 of {a.totalMeetings} meetings</Label>
      </div>
    );
  }
  return (
    <div className={card}>
      {a.text && <p className="m-0 [text-wrap:pretty]">{a.text}</p>}
      {a.points.length > 0 && (
        <ul className="m-0 flex list-disc flex-col gap-1.5 pl-5">
          {a.points.map((p, i) => (
            <li key={i}>
              {p.text} {p.citation && <Cite c={p.citation} />}
            </li>
          ))}
        </ul>
      )}
      <Label>
        From {a.fromMeetings} of {a.totalMeetings} meetings · answers only use your meetings
      </Label>
    </div>
  );
}

function Cite({ c }: { c: AskCitation }) {
  return (
    <button
      type="button"
      onClick={() => navigate(routes.meeting(c.meetingId, c.at))}
      className="inline-flex cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-md border border-edge bg-gold-soft px-2 py-[3px] font-mono text-[11px] leading-[14px] text-ink hover:bg-gold"
    >
      {c.meetingTitle} · {c.date} · {formatTime(c.at)}
    </button>
  );
}
