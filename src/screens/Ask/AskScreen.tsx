import { ArrowLeft, Send } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useFolders, useMeeting, useSettings, useTags } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import type { AskAnswer, AskCitation, AskScope } from "../../data/types";
import { describeError } from "../../data/errors";
import { formatTime } from "../../lib/format";
import { useT } from "../../lib/i18n/I18nProvider";
import { navigate, routes } from "../../lib/router";
import { Button, Chip, Label, Select, cn } from "../../ui";
import { SunCorner } from "../../brand/SunCorner";

type Turn = { id: number; question: string; scope: string; allScope: boolean; state: "loading" | "done" | "error"; answer?: AskAnswer; error?: string };

const SUGGESTIONS = ["shell.suggest1", "shell.suggest2", "shell.suggest3", "shell.suggest4"] as const;

export function AskScreen({ meetingId }: { meetingId?: string }) {
  const t = useT();
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
      ? meeting?.title ?? t("shell.thisMeeting")
      : s.kind === "folder"
        ? t("shell.folderScope", { name: folders.find((f) => f.id === s.id)?.name ?? "" })
        : s.kind === "tag"
          ? t("shell.tagScope", { tag: s.tag })
          : t("shell.allMeetings");

  const ask = async (question: string) => {
    const q = question.trim();
    if (!q) return;
    const id = Date.now();
    const label = scopeLabel(scope);
    setDraft("");
    setTurns((t) => [...t, { id, question: q, scope: label, allScope: scope.kind === "all", state: "loading" }]);
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
    <div className="relative flex min-h-0 flex-1 flex-col overflow-auto">
      <SunCorner className="absolute right-0 top-0" />
      <div className="relative flex flex-wrap items-center gap-2.5 border-b border-line px-6 py-3">
        <Button size="md" className="h-7" onClick={() => navigate(meetingId ? routes.meeting(meetingId) : routes.meetings())}>
          <ArrowLeft size={14} aria-hidden="true" /> {meetingId ? t("shell.backToMeeting") : t("shell.backToMeetings")}
        </Button>
        <Chip className="whitespace-normal font-sans text-xs normal-case">{local ? t("shell.askLocalChip") : t("shell.askByokChip")}</Chip>
      </div>

      <div className="relative mx-auto flex w-full max-w-[820px] flex-1 flex-col gap-7 px-8 pb-8 pt-11">
        <h1 className="m-0 font-display text-[32px] font-bold leading-9 tracking-[-0.02em]">{scope.kind === "meeting" ? t("shell.askAboutThis") : t("shell.askAboutAll")}</h1>
        <div className="-mt-3.5 flex flex-wrap items-center gap-2" role="group" aria-label={t("shell.askingGroup")}>
          <Label className="mr-1">{t("shell.asking")}</Label>
          {meetingId && (
            <button type="button" className={pill(scope.kind === "meeting")} onClick={() => setScope({ kind: "meeting", id: meetingId })}>
              {t("shell.thisMeeting")}
            </button>
          )}
          <button type="button" className={pill(scope.kind === "all")} onClick={() => setScope({ kind: "all" })}>
            {t("shell.allMeetings")}
          </button>
          <Select
            label={t("shell.folderLabel")}
            value={scope.kind === "folder" ? scope.id : ""}
            onChange={(e) => e.target.value && setScope({ kind: "folder", id: e.target.value })}
            className={cn(pill(scope.kind === "folder"), "py-0 pr-7")}
          >
            <option value="">{t("shell.folderPick")}</option>
            {folders.map((f) => (
              <option key={f.id} value={f.id}>
                {t("shell.folderScope", { name: f.name })}
              </option>
            ))}
          </Select>
          <Select
            label={t("shell.tagLabel")}
            value={scope.kind === "tag" ? scope.tag : ""}
            onChange={(e) => e.target.value && setScope({ kind: "tag", tag: e.target.value })}
            className={cn(pill(scope.kind === "tag"), "py-0 pr-7")}
          >
            <option value="">{t("shell.tagPick")}</option>
            {tags.map((tg) => (
              <option key={tg} value={tg}>
                {t("shell.tagScope", { tag: tg })}
              </option>
            ))}
          </Select>
        </div>

        {turns.length === 0 && (
          <p className="m-0 text-[13px] text-subtle">{t("shell.askHint")}</p>
        )}

        {turns.map((turn) => (
          <div key={turn.id} className="flex flex-col gap-3.5">
            <div className="flex justify-end">
              <div className="max-w-[80%] rounded-[14px_14px_4px_14px] bg-strong px-4 py-3 text-strong-fg">{turn.question}</div>
            </div>
            <AnswerCard turn={turn} onRetry={() => ask(turn.question)} />
          </div>
        ))}

        <div className="flex flex-wrap gap-2">
          {SUGGESTIONS.map((k) => (
            <Chip key={k} onClick={() => ask(t(k))} className="px-3 py-1.5 font-sans text-[13px] normal-case text-ink">
              {t(k)}
            </Chip>
          ))}
        </div>
        <div ref={end} />

        <form
          className="mt-auto flex items-center gap-2 rounded-card border border-edge bg-surface py-2 pl-4 pr-2 shadow-card"
          onSubmit={(e) => {
            e.preventDefault();
            ask(draft);
          }}
        >
          <label className="flex flex-1">
            <span className="sr-only">{t("shell.askQuestion")}</span>
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              placeholder={scope.kind === "meeting" ? t("shell.askPlaceholderThis") : t("shell.askPlaceholderAll")}
              className="flex-1 border-0 bg-transparent py-1.5 font-[inherit] text-sm text-ink outline-none placeholder:text-faint"
            />
          </label>
          <Button type="submit" variant="primary" size="lg" disabled={!draft.trim()}>
            <Send size={14} aria-hidden="true" /> {t("shell.ask")}
          </Button>
        </form>
      </div>
    </div>
  );
}

function AnswerCard({ turn, onRetry }: { turn: Turn; onRetry: () => void }) {
  const t = useT();
  const card = "flex flex-col gap-3.5 rounded-card border border-line bg-surface px-6 py-[22px] shadow-card";
  if (turn.state === "loading") {
    return (
      <div className={card} role="status">
        <span className="text-[13px] text-subtle">{t("shell.searching", { scope: turn.scope.toLowerCase() })}</span>
        <span className="h-[5px] w-40 overflow-hidden rounded-full bg-line">
          <span className="block h-full w-1/3 animate-pulse rounded-full bg-ink" />
        </span>
      </div>
    );
  }
  if (turn.state === "error" || !turn.answer) {
    return (
      <div className={card} role="alert">
        <span className="font-semibold text-danger">{t("shell.didntWork")}</span>
        <p className="m-0 text-[13px] text-subtle">{turn.error ?? t("shell.engineSilent")}</p>
        <div>
          <Button onClick={onRetry}>{t("shell.tryAgain")}</Button>
        </div>
      </div>
    );
  }
  const a = turn.answer;
  if (a.points.length === 0 && !a.text) {
    return (
      <div className={card}>
        <p className="m-0">{t("shell.notFound", { where: turn.allScope ? t("shell.yourMeetings") : turn.scope })}</p>
        <p className="m-0 text-[13px] text-subtle">{t("shell.notFoundHint")}</p>
        <Label>{t("shell.fromZero", { total: a.totalMeetings })}</Label>
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
      <Label className="border-t border-line pt-2.5">
        {t("shell.fromCount", { from: a.fromMeetings, total: a.totalMeetings })}
      </Label>
    </div>
  );
}

function Cite({ c }: { c: AskCitation }) {
  return (
    <button
      type="button"
      onClick={() => navigate(routes.meeting(c.meetingId, c.at))}
      className="inline-flex cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-full border-0 bg-gold-soft px-[9px] py-1 align-middle font-mono text-[11px] leading-[14px] text-gold-ink hover:bg-gold hover:text-on-gold"
    >
      <svg aria-hidden="true" width="13" height="13" viewBox="0 0 100 100" className="text-gold">
        <polygon points="50,5 61,38 95,38 67,58 78,92 50,72 22,92 33,58 5,38 39,38" fill="currentColor" />
      </svg>
      {c.meetingTitle} · {c.date} · {formatTime(c.at)}
    </button>
  );
}
