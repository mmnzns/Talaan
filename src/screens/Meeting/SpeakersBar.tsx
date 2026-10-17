import { Pencil, Sparkles } from "lucide-react";
import { useState } from "react";
import { useSettings } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import type { Meeting, Speaker } from "../../data/types";
import { speakerSuggestions } from "../../lib/speakerSuggest";
import { speakerName } from "../../lib/transcript";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Field, Label, Popover, cn, toneFor } from "../../ui";

/** Speaker chips. Anyone can be renamed (Me, Them, Speaker 1, 2, 3); every line and action item updates. With
 *  calendar attendees, their names are one-click suggestions (never applied by themselves). */
export function SpeakersBar({ meeting }: { meeting: Meeting }) {
  const t = useT();
  const store = useStore();
  const { settings } = useSettings();
  // Everyone in the transcript (or the stored names before there is one), Me first, then Speaker 1, 2, 3.
  const ids = new Set<Speaker>(meeting.transcript.length ? meeting.transcript.map((l) => l.speaker) : (Object.keys(meeting.speakers) as Speaker[]));
  const rank = (s: Speaker) => (s === "me" ? -1 : Number(s.slice(5)) || 0);
  const speakers = [...ids].sort((a, b) => rank(a) - rank(b));
  const offer = settings?.suggestSpeakerNames ?? true;
  const { names: suggestions, probably } = offer ? speakerSuggestions(meeting, settings?.yourName ?? "") : { names: [], probably: undefined };

  return (
    <div className="flex flex-wrap items-center gap-1.5 border-b border-line px-6 py-2.5">
      <Label>{t("meeting.speakers")}</Label>
      {speakers.map((s) => {
        const name = speakerName(meeting, s);
        const chip = cn("inline-flex items-center gap-1 rounded-full border px-2 py-[3px] font-mono text-[11px] leading-[14px]", toneFor(s));
        return (
          <RenameChip key={s} meetingId={meeting.id} speaker={s} name={name} className={chip} suggestions={s === "me" ? [] : suggestions} />
        );
      })}
      {probably && (
        <button
          type="button"
          onClick={() => store.renameSpeaker(meeting.id, probably.speaker, probably.name)}
          className="inline-flex cursor-pointer items-center gap-1 rounded-full border border-dashed border-edge bg-gold-soft px-2 py-[3px] font-mono text-[11px] leading-[14px] text-gold-ink hover:border-gold"
        >
          <Sparkles size={10} aria-hidden="true" />
          {t("meeting.probablyThem", { speaker: speakerName(meeting, probably.speaker), name: probably.name })}
        </button>
      )}
      {suggestions.length > 0 && <Label className="ml-auto">{t("meeting.namesFromCalendar")}</Label>}
    </div>
  );
}

function RenameChip({ meetingId, speaker, name, className, suggestions }: { meetingId: string; speaker: Speaker; name: string; className: string; suggestions: string[] }) {
  const store = useStore();
  const t = useT();
  const [draft, setDraft] = useState(name);
  return (
    <Popover
      align="left"
      className="w-60"
      trigger={({ toggle }) => (
        <button type="button" onClick={toggle} className={cn(className, "cursor-pointer")} aria-label={t("meeting.renameAria", { name })}>
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
            <Label>{speaker === "me" ? t("meeting.renameMe") : t("meeting.renameSpeaker")}</Label>
            <Field label={t("meeting.speakerNameLabel")} autoFocus value={draft} onChange={(e) => setDraft(e.target.value)} onKeyDown={(e) => e.key === "Enter" && save(draft)} />
            {suggestions.length > 0 && (
              <div className="flex flex-col gap-1.5">
                <Label>{t("meeting.fromInvite")}</Label>
                <div className="flex flex-wrap gap-1.5">
                  {suggestions.map((n) => (
                    <button key={n} type="button" onClick={() => save(n)} className="cursor-pointer rounded-full border border-line bg-surface px-2 py-[3px] font-mono text-[11px] text-ink hover:border-edge">
                      {n}
                    </button>
                  ))}
                </div>
              </div>
            )}
            <Button variant="primary" onClick={() => save(draft)}>
              {t("meeting.saveName")}
            </Button>
          </div>
        );
      }}
    </Popover>
  );
}
