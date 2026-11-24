import { Search } from "lucide-react";
import { useState } from "react";
import { describeError } from "../../data/errors";
import { useFolders, useMeetings, useTags } from "../../data/hooks";
import { useSettingsStore } from "../../data/StoreProvider";
import type { MeetingListItem } from "../../data/types";
import { dayGroup, dayMark } from "../../lib/format";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Field, Label } from "../../ui";
import { FolderNav } from "./FolderNav";
import { MeetingItem } from "./MeetingItem";

type DayBucket = { key: string; label: string; items: MeetingListItem[] };

export function MeetingList({ selectedId }: { selectedId?: string }) {
  const t = useT();
  const [folderId, setFolderId] = useState<string | undefined>();
  const [tag, setTag] = useState<string | undefined>();
  const [query, setQuery] = useState("");

  const { meetings, loading, error, retry } = useMeetings({ folderId, tag, query });
  const all = useMeetings();
  const folders = useFolders();
  const tags = useTags();

  const filtering = Boolean(folderId || tag || query.trim());
  const clear = () => {
    setFolderId(undefined);
    setTag(undefined);
    setQuery("");
  };

  // Meetings by day, newest first (the list already comes sorted).
  const days: DayBucket[] = [];
  for (const m of meetings) {
    const g = dayGroup(m.startedAt);
    const last = days[days.length - 1];
    if (last?.key === g.key) last.items.push(m);
    else days.push({ ...g, items: [m] });
  }

  return (
    <nav aria-label={t("shell.meetingsNav")} className="flex w-[310px] flex-none flex-col border-r border-line bg-rail">
      <div className="px-4 pb-2.5 pt-5">
        <Field
          label={t("shell.searchLabel")}
          type="search"
          icon={<Search size={15} aria-hidden="true" />}
          placeholder={t("shell.searchPlaceholder")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          wrapperClassName="h-[34px]"
        />
      </div>
      <FolderNav
        folders={folders}
        tags={tags}
        total={all.meetings.length}
        folderId={folderId}
        tag={tag}
        onFolder={setFolderId}
        onTag={setTag}
      />
      <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-auto px-3 pb-5 pt-3.5">
        {error ? (
          <ListFailed cause={error.cause} onRetry={retry} />
        ) : !loading && meetings.length === 0 ? (
          <EmptyState firstRun={all.meetings.length === 0 && !all.loading} filtering={filtering} onClear={clear} />
        ) : (
          <MeetingDays days={days} selectedId={selectedId} />
        )}
      </div>
    </nav>
  );
}

/** One section per day for screen readers, but every row carries its own date so a second meeting that day never
 *  reads as missing one. */
export function MeetingDays({ days, selectedId }: { days: DayBucket[]; selectedId?: string }) {
  return (
    <>
      {days.map((d) => (
        <section key={d.key} aria-label={d.label} className="flex flex-col gap-1">
          {d.items.map((m) => {
            // An unreadable meeting has no start time, so no date to show.
            const mark = m.startedAt ? dayMark(m.startedAt) : undefined;
            return (
              <div key={m.id} className="grid grid-cols-[40px_minmax(0,1fr)] items-start gap-2.5">
                <div className="flex flex-col items-center gap-px pt-0.5" title={mark?.full}>
                  {mark && <span className="sr-only">{mark.full}</span>}
                  <Label className="text-[10px]" aria-hidden="true">
                    {mark?.month}
                  </Label>
                  <span aria-hidden="true" className="font-display text-xl leading-[22px] font-bold tracking-[-0.02em]">
                    {mark?.day}
                  </span>
                  <Label className="text-[10px]" aria-hidden="true">
                    {mark?.weekday}
                  </Label>
                </div>
                <div className="min-w-0">
                  <MeetingItem meeting={m} selected={m.id === selectedId} />
                </div>
              </div>
            );
          })}
        </section>
      ))}
    </>
  );
}

/** The list couldn't be read: not an empty list, which would read as "no meetings". */
export function ListFailed({ cause, onRetry }: { cause: unknown; onRetry: () => void }) {
  const t = useT();
  const settings = useSettingsStore();
  return (
    <div role="alert" className="flex flex-col items-start gap-2 px-2 py-6">
      <span className="font-display text-base font-bold">{t("shell.listFailedTitle")}</span>
      <p className="m-0 text-[13px] text-subtle">{t("shell.listFailedBody", { detail: describeError(cause) })}</p>
      <div className="flex gap-2">
        <Button variant="primary" onClick={onRetry}>
          {t("shell.tryAgain")}
        </Button>
        <Button onClick={() => void settings.revealData().catch(() => {})}>{t("shell.openDataFolder")}</Button>
      </div>
    </div>
  );
}

function EmptyState({ firstRun, filtering, onClear }: { firstRun: boolean; filtering: boolean; onClear: () => void }) {
  const t = useT();
  if (firstRun) {
    return (
      <div className="flex flex-col items-start gap-2 px-2 py-6">
        <span className="font-display text-base font-bold">{t("shell.noMeetingsTitle")}</span>
        <p className="m-0 text-[13px] text-subtle">{t("shell.noMeetingsBody")}</p>
      </div>
    );
  }
  return (
    <div className="flex flex-col items-start gap-2 px-2 py-6">
      <span className="font-display text-base font-bold">{t("shell.noMatchesTitle")}</span>
      <p className="m-0 text-[13px] text-subtle">{t("shell.noMatchesBody")}</p>
      {filtering && <Button onClick={onClear}>{t("shell.clearFilters")}</Button>}
    </div>
  );
}
