import { Pencil } from "lucide-react";
import { useState } from "react";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, Speaker } from "../../data/types";
import { speakerName } from "../../lib/transcript";
import { Button, Field, Label, Popover, cn, toneFor } from "../../ui";

/** Speaker chips. Diarized speakers (Speaker 1, 2, 3) can be renamed once; every line updates. */
export function SpeakersBar({ meeting }: { meeting: Meeting }) {
  // Everyone in the transcript (or the stored names before there is one), Me first, then Speaker 1, 2, 3.
  const ids = new Set<Speaker>(meeting.transcript.length ? meeting.transcript.map((l) => l.speaker) : (Object.keys(meeting.speakers) as Speaker[]));
  const rank = (s: Speaker) => (s === "me" ? -1 : Number(s.slice(5)) || 0);
  const speakers = [...ids].sort((a, b) => rank(a) - rank(b));
  const taken = new Set(Object.values(meeting.speakers));
  const suggestions = (meeting.calendarEvent?.attendees ?? []).filter((n) => n !== "You" && !taken.has(n));

  return (
    <div className="flex flex-wrap items-center gap-1.5 border-b border-line px-4 py-2">
      <Label>speakers</Label>
      {speakers.map((s) => {
        const name = speakerName(meeting, s);
        const chip = cn("inline-flex items-center gap-1 rounded-full border px-2 py-[3px] font-mono text-[11px] leading-[14px]", toneFor(s));
        const renameable = s !== "me" && s !== "them:0";
        if (!renameable) return <span key={s} className={chip}>{name}</span>;
        return (
          <RenameChip key={s} meetingId={meeting.id} speaker={s} name={name} className={chip} suggestions={suggestions} />
        );
      })}
      {suggestions.length > 0 && <Label className="ml-auto">names from calendar</Label>}
    </div>
  );
}

function RenameChip({ meetingId, speaker, name, className, suggestions }: { meetingId: string; speaker: Speaker; name: string; className: string; suggestions: string[] }) {
  const store = useStore();
  const [draft, setDraft] = useState(name);
  return (
    <Popover
      align="left"
      className="w-60"
      trigger={({ toggle }) => (
        <button type="button" onClick={toggle} className={cn(className, "cursor-pointer")} aria-label={`Rename ${name}`}>
          {name} <Pencil size={10} aria-hidden="true" />
        </button>
      )}
    >
      {(close) => {
        const save = (value: string) => {
          const next = value.trim();
          if (next) store.renameSpeaker(meetingId, speaker, next);
          close();
        };
        return (
          <div className="flex flex-col gap-2 p-1.5">
            <Label>rename speaker</Label>
            <Field label="Speaker name" autoFocus value={draft} onChange={(e) => setDraft(e.target.value)} onKeyDown={(e) => e.key === "Enter" && save(draft)} />
            {suggestions.length > 0 && (
              <div className="flex flex-wrap gap-1.5">
                {suggestions.map((n) => (
                  <button key={n} type="button" onClick={() => save(n)} className="cursor-pointer rounded-full border border-line bg-surface px-2 py-[3px] font-mono text-[11px] text-ink hover:border-edge">
                    {n}
                  </button>
                ))}
              </div>
            )}
            <Button variant="primary" onClick={() => save(draft)}>
              Save name
            </Button>
          </div>
        );
      }}
    </Popover>
  );
}
