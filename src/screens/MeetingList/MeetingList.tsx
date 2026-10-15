import { Search } from "lucide-react";
import { useState } from "react";
import { useFolders, useMeetings, useTags } from "../../data/hooks";
import type { MeetingListItem } from "../../data/types";
import { dayGroup, dayMark } from "../../lib/format";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Field, Label } from "../../ui";
import { FolderNav } from "./FolderNav";
import { MeetingItem } from "./MeetingItem";

export function MeetingList({ selectedId }: { selectedId?: string }) {
  const t = useT();
  const [folderId, setFolderId] = useState<string | undefined>();
  const [tag, setTag] = useState<string | undefined>();
  const [query, setQuery] = useState("");

  const { meetings, loading } = useMeetings({ folderId, tag, query });
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
  const days: { key: string; label: string; first: MeetingListItem; items: MeetingListItem[] }[] = [];
  for (const m of meetings) {
    const g = dayGroup(m.startedAt);
    const last = days[days.length - 1];
    if (last?.key === g.key) last.items.push(m);
    else days.push({ ...g, first: m, items: [m] });
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
        {!loading && meetings.length === 0 ? (
          <EmptyState firstRun={all.meetings.length === 0 && !all.loading} filtering={filtering} onClear={clear} />
        ) : (
          days.map((d) => {
            const mark = dayMark(d.first.startedAt);
            return (
              <section key={d.key} aria-label={d.label} className="grid grid-cols-[40px_minmax(0,1fr)] items-start gap-2.5">
                <div className="flex flex-col items-center gap-0.5 pt-0.5" aria-hidden="true">
                  <span className="font-display text-xl leading-[22px] font-bold tracking-[-0.02em]">{mark.day}</span>
                  <Label className="text-[10px]">{mark.weekday}</Label>
                </div>
                <div className="flex min-w-0 flex-col gap-1">
                  {d.items.map((m) => (
                    <MeetingItem key={m.id} meeting={m} selected={m.id === selectedId} />
                  ))}
                </div>
              </section>
            );
          })
        )}
      </div>
    </nav>
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
