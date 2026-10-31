import { Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useLang, useT } from "../lib/i18n/I18nProvider";
import { cloudBetter, languageLabel, searchLanguages } from "../lib/summaryLanguages";
import { Field, Label, cn } from "../ui";

/** A row above the languages that isn't one ("Same as meeting"). */
export interface ExtraChoice {
  code: string;
  label: string;
}

interface Props {
  /** The code to mark as current, when there is one. */
  value?: string;
  onPick: (code: string) => void;
  extra?: ExtraChoice[];
  /** The local AI model that would write the summary: languages it did poorly on say so. Absent for online AI. */
  localModel?: string;
}

const plain = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();

/**
 * Search box and the languages a summary can be written in, each as "Japanese · 日本語" (the app's language, then its
 * own). Used by the meeting's Summarize in… sheet and by Settings › Summary AI. Type to filter (either name, any
 * case, accents ignored); arrow keys move through the list and Enter picks.
 */
export function LanguagePicker({ value, onPick, extra = [], localModel }: Props) {
  const t = useT();
  const lang = useLang();
  const [query, setQuery] = useState("");
  const root = useRef<HTMLDivElement>(null);
  const searchBox = () => root.current?.querySelector<HTMLInputElement>('input[type="search"]');
  const list = useRef<HTMLDivElement>(null);

  // After the Sheet has taken focus for itself.
  useEffect(() => {
    const id = setTimeout(() => searchBox()?.focus(), 0);
    return () => clearTimeout(id);
  }, []);

  const q = plain(query.trim());
  const shownExtra = extra.filter((x) => !q || plain(x.label).includes(q));
  const languages = useMemo(() => searchLanguages(query, lang), [query, lang]);
  const empty = shownExtra.length === 0 && languages.length === 0;

  const options = () => Array.from(list.current?.querySelectorAll<HTMLElement>('[role="option"]') ?? []);
  const move = (from: HTMLElement | null, step: number) => {
    const all = options();
    const at = from ? all.indexOf(from) : -1;
    const next = at + step;
    if (next < 0) searchBox()?.focus();
    else all[Math.min(next, all.length - 1)]?.focus();
  };
  const onKey = (e: KeyboardEvent<HTMLElement>, from: HTMLElement | null) => {
    const step = { ArrowDown: 1, ArrowUp: -1 }[e.key];
    if (!step) return;
    e.preventDefault();
    move(from, step);
  };

  const row = (code: string, label: string, note?: string) => {
    const current = code === value;
    return (
      <button
        key={code}
        type="button"
        role="option"
        aria-selected={current}
        data-code={code}
        onClick={() => onPick(code)}
        onKeyDown={(e) => onKey(e, e.currentTarget)}
        className={cn(
          "flex w-full cursor-pointer items-center gap-2.5 rounded-control border bg-surface px-3 py-2 text-left font-[inherit] text-[13px] text-ink",
          current ? "border-edge" : "border-line hover:border-edge",
        )}
      >
        <span className="min-w-0 flex-1">
          {label}
          {note && <span className="block text-xs text-subtle">{note}</span>}
        </span>
        {current && <Label>{t("meeting.summarizeAsCurrent")}</Label>}
      </button>
    );
  };

  return (
    <div ref={root} className="flex flex-col gap-2.5">
      <Field
        label={t("meeting.searchLanguages")}
        type="search"
        icon={<Search size={15} aria-hidden="true" />}
        placeholder={t("meeting.searchLanguages")}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => onKey(e, null)}
        autoComplete="off"
        spellCheck={false}
      />
      <div ref={list} role="listbox" aria-label={t("meeting.languagesLabel")} className="flex max-h-[min(340px,50vh)] flex-col gap-1.5 overflow-auto">
        {shownExtra.map((x) => row(x.code, x.label))}
        {languages.map(({ code }) => row(code, languageLabel(code, lang), localModel && cloudBetter(code, localModel) ? t("meeting.cloudBetter") : undefined))}
        {empty && <p className="m-0 px-1 py-2 text-[13px] text-subtle">{t("meeting.noLanguageFound", { query: query.trim() })}</p>}
      </div>
    </div>
  );
}
