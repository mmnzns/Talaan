import { Plus, X } from "lucide-react";
import { useState } from "react";
import { useStore } from "../../data/StoreProvider";
import type { Mark, Meeting, Ms } from "../../data/types";
import { describeError } from "../../data/errors";
import { formatTime } from "../../lib/format";
import { meetingNotes } from "../../lib/share";
import { useT } from "../../lib/i18n/I18nProvider";
import { Button, Label, SectionLabel, Timestamp, cn, useToast } from "../../ui";
import { Clamp } from "./Clamp";

interface Props {
  meeting: Meeting;
  activeAt?: Ms;
  onJump: (at: Ms) => void;
  /** Called after the notes change, so the card can offer to summarize again. */
  onChanged?: () => void;
}

/** One note: its time (or "after the call"), its text editable in place, and a delete button. */
function NoteRow({
  note,
  active,
  onJump,
  onSave,
  onDelete,
  readOnly,
}: {
  note: Mark;
  active: boolean;
  /** While recording: the recorder owns the notes until Stop. */
  readOnly?: boolean;
  onJump: (at: Ms) => void;
  onSave: (text: string) => void;
  onDelete: () => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<string>();
  const text = note.note ?? "";
  const commit = () => {
    const next = draft?.trim();
    setDraft(undefined);
    if (next === undefined || next === text.trim()) return;
    if (next) onSave(next);
    else onDelete();
  };
  return (
    <div className={cn("group -m-1.5 flex items-start gap-3 rounded-lg p-1.5", active && "bg-gold-soft")}>
      {note.later ? (
        <Label className="mt-[3px] flex-none">{t("meeting.noteAfterCall")}</Label>
      ) : (
        <Timestamp at={note.at} onJump={onJump} active={active} className="mt-px" />
      )}
      {/* The hidden copy sizes the box to its text, at any width. */}
      <span className="grid min-w-0 flex-1 [&>*]:[grid-area:1/1]">
        <span aria-hidden="true" className="invisible whitespace-pre-wrap break-words border border-transparent px-1 [font:inherit]">
          {(draft ?? text) + " "}
        </span>
        <textarea
          rows={1}
          value={draft ?? text}
          aria-label={note.later ? t("meeting.noteLaterAria") : t("meeting.noteAria", { time: formatTime(note.at) })}
          readOnly={readOnly}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              e.currentTarget.blur();
            }
            if (e.key === "Escape") {
              setDraft(undefined);
              e.currentTarget.blur();
            }
          }}
          className="min-h-5 min-w-0 resize-none overflow-hidden rounded border border-transparent bg-transparent px-1 text-ink [font:inherit] outline-none hover:border-line focus:border-edge focus:bg-raised"
        />
      </span>
      {!readOnly && (
        <button
          type="button"
          aria-label={t("meeting.deleteNote")}
          onClick={onDelete}
          className="mt-0.5 grid size-5 flex-none cursor-pointer place-items-center rounded border-0 bg-transparent p-0 text-faint opacity-0 hover:text-ink focus-visible:opacity-100 group-hover:opacity-100 group-focus-within:opacity-100"
        >
          <X size={13} aria-hidden="true" />
        </button>
      )}
    </div>
  );
}

/**
 * "Your notes": what the user typed during the call (with the moment each was typed) and anything added later.
 * Each note is edited in place; the summary uses them as its backbone the next time it is written.
 */
export function NotesSection({ meeting, activeAt, onJump, onChanged }: Props) {
  const t = useT();
  const store = useStore();
  const toast = useToast();
  const notes = meetingNotes(meeting);
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const locked = meeting.status === "recording";

  const save = (next: Mark[]) =>
    store.setNotes(meeting.id, next).then(
      () => onChanged?.(),
      (e) => toast(describeError(e)),
    );
  const add = () => {
    const text = draft.trim();
    setDraft("");
    if (text) void save([...notes, { id: "", at: 0, note: text, later: true }]);
  };

  return (
    <div className="flex flex-col gap-3">
      <span className="flex items-center gap-3">
        <SectionLabel>{t("meeting.yourNotesLabel")}</SectionLabel>
        {notes.length === 0 && !adding && !locked && (
          <Button variant="ghost" className="-my-1.5 h-7 px-2 text-xs text-subtle" onClick={() => setAdding(true)}>
            <Plus size={13} aria-hidden="true" /> {t("meeting.addNote")}
          </Button>
        )}
      </span>
      {notes.length > 0 && (
        <Clamp count={notes.length} limit={5}>
          {(shown) =>
            notes
              .slice(0, shown)
              .map((n) => (
                <NoteRow
                  key={n.id}
                  note={n}
                  active={!n.later && n.at === activeAt}
                  onJump={onJump}
                  readOnly={locked}
                  onSave={(text) => void save(notes.map((x) => (x.id === n.id ? { ...x, note: text } : x)))}
                  onDelete={() => void save(notes.filter((x) => x.id !== n.id))}
                />
              ))
          }
        </Clamp>
      )}
      {!locked && (notes.length > 0 || adding) && (
        <label className="flex h-[34px] items-center gap-2 rounded-control border border-dashed border-input bg-transparent px-3 focus-within:border-solid focus-within:border-ink">
          <Plus size={13} aria-hidden="true" className="flex-none text-faint" />
          <span className="sr-only">{t("meeting.addNote")}</span>
          <input
            value={draft}
            autoFocus={adding && notes.length === 0}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={() => {
              add();
              if (notes.length === 0 && !draft.trim()) setAdding(false);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") add();
              if (e.key === "Escape") {
                setDraft("");
                e.currentTarget.blur();
              }
            }}
            placeholder={t("meeting.addNotePlaceholder")}
            className="min-w-0 flex-1 border-0 bg-transparent font-[inherit] text-[13px] text-ink outline-none placeholder:text-faint"
          />
        </label>
      )}
    </div>
  );
}
