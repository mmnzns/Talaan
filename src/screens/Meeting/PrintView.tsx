import { useEffect } from "react";
import { useMeeting } from "../../data/hooks";
import { speakerName } from "../../lib/transcript";
import { formatDate, formatDuration, formatTime } from "../../lib/format";
import { useT } from "../../lib/i18n/I18nProvider";
import { inTauri } from "../../lib/windows";
import { meetingNotes, type ExportScope } from "../../lib/share";

/** A meeting laid out for paper / PDF (`#/print/:id`). Opens the print sheet once it has rendered. */
export function PrintView({ id, scope }: { id: string; scope: ExportScope }) {
  const { meeting } = useMeeting(id);
  const t = useT();

  useEffect(() => {
    if (!meeting) return;
    const done = () => {
      if (inTauri()) void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => getCurrentWindow().close());
    };
    window.addEventListener("afterprint", done);
    // Give fonts a moment so the PDF uses Talaan's type, not a fallback.
    const timer = setTimeout(() => void document.fonts.ready.then(() => window.print()), 300);
    return () => {
      clearTimeout(timer);
      window.removeEventListener("afterprint", done);
    };
  }, [meeting?.id]);

  if (!meeting) return null;
  const s = meeting.summary;
  const time = (at?: number) => (at === undefined ? null : <span className="mr-2 font-mono text-[11px] text-subtle">{formatTime(at)}</span>);
  const notes = (
    <Section
      title={t("meeting.shYourNotes")}
      items={meetingNotes(meeting).map((k) => [k.id, <>{k.later ? <span className="mr-2 font-mono text-[11px] text-subtle">{t("meeting.shAfterCall")}</span> : time(k.at)}{k.note}</>])}
    />
  );

  return (
    <article className="mx-auto max-w-[680px] bg-paper px-10 py-10 text-ink print:p-0">
      <header className="mb-6 border-b border-edge pb-4">
        <h1 className="m-0 font-display text-[28px] font-bold tracking-[-0.02em]">{meeting.title}</h1>
        <div className="mt-1 font-mono text-xs text-subtle">
          {meeting.appLabel} · {formatDate(meeting.startedAt)} · {formatDuration(meeting.durationMs)}
        </div>
      </header>
      {scope === "transcript" ? null : s ? (
        <>
          <p className="m-0 mb-5 text-[15px] leading-[23px]">{s.recap}</p>
          {notes}
          <Section title={t("meeting.shDecisions")} items={s.decisions.map((d) => [d.id, <>{time(d.at)}{d.text}</>])} />
          <Section
            title={t("meeting.shActionItems")}
            items={s.actionItems.map((a) => [a.id, <>{time(a.at)}{a.done ? "☑ " : "☐ "}{a.text}{a.owner ? ` — ${a.owner}` : ""}{a.due ? `, ${t("meeting.shDue", { date: a.due })}` : ""}</>])}
          />
          <Section title={t("meeting.shOpenQuestions")} items={s.openQuestions.map((q) => [q.id, <>{time(q.at)}{q.text}</>])} />
          {(s.sections ?? []).map((sec) => (
            <Section key={sec.id} title={sec.title} items={sec.items.map((i) => [i.id, <>{time(i.at)}{i.text}</>])} />
          ))}
          <Section title={t("meeting.shHighlights")} items={(s.highlights ?? []).map((h) => [h.id, <>{time(h.at)}{h.text}</>])} />
        </>
      ) : (
        <>
          <p className="text-subtle">{t("meeting.printNoSummary")}</p>
          {notes}
        </>
      )}
      {scope === "transcript" && meeting.transcript.length === 0 && <p className="text-subtle">{t("meeting.shNoTranscript")}</p>}
      {scope !== "summary" && meeting.transcript.length > 0 && (
        <section className={scope === "both" ? "mt-6 break-before-page" : "mt-2"}>
          <h2 className="mb-3 text-[15px] font-bold lowercase font-mono tracking-normal">{t("meeting.shTranscript")}</h2>
          {meeting.transcript.map((l) => (
            <p key={l.id} className="m-0 mb-2 text-[13px] leading-[19px] [break-inside:avoid]">
              {time(l.at)}
              <b>{speakerName(meeting, l.speaker)}:</b> {l.text}
            </p>
          ))}
        </section>
      )}
    </article>
  );
}

function Section({ title, items }: { title: string; items: [string, React.ReactNode][] }) {
  if (items.length === 0) return null;
  return (
    <section className="mb-5 [break-inside:avoid]">
      <h2 className="mb-2 text-[13px] font-bold lowercase font-mono tracking-normal">{title}</h2>
      <ul className="m-0 list-none p-0">
        {items.map(([k, node]) => (
          <li key={k} className="mb-1.5 text-[14px] leading-[20px]">
            {node}
          </li>
        ))}
      </ul>
    </section>
  );
}
