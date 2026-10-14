import { Search } from "lucide-react";
import { Fragment, useState } from "react";
import { useFolders, useMeetings, useTags } from "../../data/hooks";
import { dayGroup } from "../../lib/format";
import { Button, Field, Label } from "../../ui";
import { FolderNav } from "./FolderNav";
import { MeetingItem } from "./MeetingItem";

export function MeetingList({ selectedId }: { selectedId?: string }) {
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

  let lastKey = "";

  return (
    <nav aria-label="Meetings" className="flex w-[320px] flex-none flex-col border-r-[1.5px] border-edge bg-rail">
      <div className="p-3 pb-2">
        <Field
          label="Search meetings"
          type="search"
          icon={<Search size={15} aria-hidden="true" />}
          placeholder="Search meetings and transcripts"
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
      <div className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-auto px-2 pb-3 pt-2">
        {!loading && meetings.length === 0 ? (
          <EmptyState firstRun={all.meetings.length === 0 && !all.loading} filtering={filtering} onClear={clear} />
        ) : (
          meetings.map((m) => {
            const group = dayGroup(m.startedAt);
            const header = group.key !== lastKey;
            lastKey = group.key;
            return (
              <Fragment key={m.id}>
                {header && <Label className="px-2 pb-1.5 pt-2.5">{group.label}</Label>}
                <MeetingItem meeting={m} selected={m.id === selectedId} />
              </Fragment>
            );
          })
        )}
      </div>
    </nav>
  );
}

function EmptyState({ firstRun, filtering, onClear }: { firstRun: boolean; filtering: boolean; onClear: () => void }) {
  if (firstRun) {
    return (
      <div className="flex flex-col items-start gap-2 px-2 py-6">
        <span className="text-[15px] font-extrabold tracking-tight">No meetings yet</span>
        <p className="m-0 text-[13px] text-subtle">
          Start a call and press Record, or import a Voice Memo. Transcripts and summaries show up here.
        </p>
      </div>
    );
  }
  return (
    <div className="flex flex-col items-start gap-2 px-2 py-6">
      <span className="text-[15px] font-extrabold tracking-tight">No matches</span>
      <p className="m-0 text-[13px] text-subtle">Nothing in your meetings fits that search or filter.</p>
      {filtering && <Button onClick={onClear}>Clear filters</Button>}
    </div>
  );
}
