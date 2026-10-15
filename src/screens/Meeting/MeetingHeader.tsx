import { AudioLines, Calendar, FolderOpen, MessageCircle, MoreHorizontal, Pin, Sparkles, Trash2, VolumeX, X } from "lucide-react";
import { useState } from "react";
import { useFolders, useSettings } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import type { Meeting } from "../../data/types";
import { formatClock, formatDate, formatDuration } from "../../lib/format";
import { navigate, routes } from "../../lib/router";
import { Button, Chip, Field, Label, MenuItem, Popover, Select, Sheet, useToast } from "../../ui";
import { describeError } from "../../data/errors";

interface Props {
  meeting: Meeting;
  editing: boolean;
  canRevert: boolean;
  onToggleEdit: () => void;
  onRevert: () => void;
  onCopy: (kind: "text" | "email") => void;
  onExport: () => void;
  copied?: "text" | "email";
}

const languageName = (code: string) => {
  try {
    return new Intl.DisplayNames(["en"], { type: "language" }).of(code) ?? code;
  } catch {
    return code;
  }
};

export function MeetingHeader({ meeting, editing, canRevert, onToggleEdit, onRevert, onCopy, onExport, copied }: Props) {
  const store = useStore();
  const toast = useToast();
  const folders = useFolders();
  const [addingTag, setAddingTag] = useState(false);
  const [tagDraft, setTagDraft] = useState("");
  const [title, setTitle] = useState(meeting.title);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [retranscribing, setRetranscribing] = useState(false);
  const { settings } = useSettings();
  const speechModel = meeting.transcription && (settings?.speechModels.find((m) => m.id === meeting.transcription!.model)?.name ?? meeting.transcription.model);

  const ready = meeting.status === "ready";

  const commitTitle = () => {
    const next = title.trim();
    if (next && next !== meeting.title) store.renameMeeting(meeting.id, next);
    else setTitle(meeting.title);
  };

  const commitTag = () => {
    const t = tagDraft.trim().replace(/^#/, "");
    setAddingTag(false);
    setTagDraft("");
    if (t && !meeting.tags.includes(t)) store.setTags(meeting.id, [...meeting.tags, t]);
  };

  return (
    <header className="relative flex flex-col gap-2.5 px-11 pb-[30px] pt-11">
      <div className="flex flex-wrap items-end justify-between gap-[18px]">
        <div className="flex min-w-0 flex-1 basis-80 flex-col gap-3.5">
          <Label>
            {[
              folders.find((f) => f.id === meeting.folderId)?.name,
              meeting.app !== "import" ? `${formatDate(meeting.startedAt)}, ${formatClock(meeting.startedAt)}` : undefined,
              meeting.appLabel,
              formatDuration(meeting.durationMs),
              languageName(meeting.language),
              meeting.hasAudio ? (meeting.pinned ? "audio kept" : meeting.audioKeptUntil ? `audio until ${formatDate(meeting.audioKeptUntil)}` : "audio kept") : "no audio kept",
            ]
              .filter(Boolean)
              .join(" · ")}
          </Label>
          <input
            aria-label="Meeting title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onBlur={commitTitle}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
              if (e.key === "Escape") {
                setTitle(meeting.title);
                e.currentTarget.blur();
              }
            }}
            className="-ml-1 m-0 w-full rounded-control border border-transparent bg-transparent px-1 py-0 font-display text-[36px] font-bold leading-[42px] tracking-[-0.02em] text-ink outline-none hover:border-line focus:border-edge"
          />
          <div className="flex flex-wrap items-center gap-1.5">
            <Select
              label="Folder"
              value={meeting.folderId ?? ""}
              onChange={(e) => store.moveToFolder(meeting.id, e.target.value || undefined)}
              className="h-6 rounded-full border border-line bg-surface py-0 pl-2 pr-6 font-mono text-[11px] text-subtle"
            >
              <option value="">No folder</option>
              {folders.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.name}
                </option>
              ))}
            </Select>
            {meeting.tags.map((t) => (
              <Chip key={t}>
                #{t}
                <button
                  type="button"
                  aria-label={`Remove tag ${t}`}
                  className="-mr-1 grid cursor-pointer place-items-center border-0 bg-transparent p-0 text-subtle hover:text-ink"
                  onClick={() => store.setTags(meeting.id, meeting.tags.filter((x) => x !== t))}
                >
                  <X size={11} aria-hidden="true" />
                </button>
              </Chip>
            ))}
            {addingTag ? (
              <Field
                label="New tag"
                autoFocus
                placeholder="tag"
                value={tagDraft}
                onChange={(e) => setTagDraft(e.target.value)}
                onBlur={commitTag}
                onKeyDown={(e) => {
                  if (e.key === "Enter") commitTag();
                  if (e.key === "Escape") {
                    setTagDraft("");
                    setAddingTag(false);
                  }
                }}
                wrapperClassName="h-6 w-28 px-2"
                className="text-xs"
              />
            ) : (
              <Chip onClick={() => setAddingTag(true)}>+ tag</Chip>
            )}
          </div>
        </div>

        {ready && (
          <div className="flex flex-wrap items-center gap-2">
            {editing && canRevert && <Button onClick={onRevert}>Revert to original</Button>}
            <Button variant={editing ? "strong" : "default"} onClick={onToggleEdit}>
              {editing ? "Done editing" : "Edit"}
            </Button>
            <Button onClick={onExport}>Export</Button>
            <Button variant="primary" onClick={() => onCopy("text")}>
              {copied === "text" ? "Copied" : "Copy as text"}
            </Button>
            <Popover
              trigger={({ toggle }) => (
                <Button size="icon" aria-label="More actions" onClick={toggle}>
                  <MoreHorizontal size={16} aria-hidden="true" />
                </Button>
              )}
            >
              {(close) => (
                <>
                  <MenuItem
                    onClick={() => {
                      onCopy("email");
                      close();
                    }}
                  >
                    {copied === "email" ? "Copied for email" : "Copy for email"}
                  </MenuItem>
                  <MenuItem
                    onClick={() => {
                      navigate(routes.ask(meeting.id));
                      close();
                    }}
                  >
                    <MessageCircle size={14} aria-hidden="true" /> Ask about this meeting
                  </MenuItem>
                  <MenuItem
                    disabled={!meeting.hasAudio}
                    onClick={() => {
                      store.setPinned(meeting.id, !meeting.pinned);
                      close();
                    }}
                  >
                    <Pin size={14} aria-hidden="true" /> {meeting.pinned ? "Stop keeping audio" : "Keep audio"}
                  </MenuItem>
                  <MenuItem
                    disabled={meeting.transcript.length === 0}
                    onClick={() => {
                      store.generateSummary(meeting.id).catch((e) => toast(describeError(e)));
                      close();
                    }}
                  >
                    <Sparkles size={14} aria-hidden="true" /> {meeting.summary ? "Write the summary again" : "Generate summary"}
                  </MenuItem>
                  <MenuItem
                    disabled={!meeting.hasAudio}
                    onClick={() => {
                      setRetranscribing(true);
                      close();
                    }}
                  >
                    <AudioLines size={14} aria-hidden="true" /> Transcribe again…
                  </MenuItem>
                  <MenuItem
                    disabled={!meeting.hasAudio || meeting.pinned}
                    onClick={() => {
                      store
                        .deleteAudio(meeting.id)
                        .then(() => toast("Audio deleted. The transcript and summary stay."))
                        .catch((e) => toast(describeError(e)));
                      close();
                    }}
                  >
                    <VolumeX size={14} aria-hidden="true" /> Delete audio now
                  </MenuItem>
                  <MenuItem
                    onClick={() => {
                      store.revealMeeting(meeting.id).catch((e) => toast(describeError(e)));
                      close();
                    }}
                  >
                    <FolderOpen size={14} aria-hidden="true" /> Open folder
                  </MenuItem>
                  <MenuItem
                    className="text-danger"
                    onClick={() => {
                      setConfirmDelete(true);
                      close();
                    }}
                  >
                    <Trash2 size={14} aria-hidden="true" /> Delete meeting…
                  </MenuItem>
                  {(speechModel || meeting.summary) && (
                    <div className="mt-1 flex flex-col gap-1 border-t border-line px-2.5 pb-1 pt-2.5 text-xs text-subtle">
                      <Label>made with</Label>
                      {speechModel && <span>Transcript · {speechModel}</span>}
                      {meeting.summary && <span>Summary · {meeting.summary.engine}</span>}
                    </div>
                  )}
                </>
              )}
            </Popover>
          </div>
        )}
      </div>

      {meeting.calendarEvent && (
        <div className="flex flex-wrap items-center gap-2 rounded-control border border-dashed border-line px-3 py-2">
          <Calendar size={15} aria-hidden="true" className="text-subtle" />
          <Label>from calendar</Label>
          <span className="text-[13px] font-semibold">{meeting.calendarEvent.title}</span>
          <span className="text-[13px] text-subtle">with {meeting.calendarEvent.attendees.join(", ")}</span>
        </div>
      )}

      <RetranscribeSheet meeting={meeting} open={retranscribing} onClose={() => setRetranscribing(false)} />

      <Sheet open={confirmDelete} title="Delete meeting" onClose={() => setConfirmDelete(false)}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">Delete “{meeting.title}”?</h2>
          <p className="m-0 text-[13px] text-subtle">
            This removes the transcript, summary and any audio from this computer. It can’t be undone.
          </p>
          <div className="flex justify-end gap-2">
            <Button onClick={() => setConfirmDelete(false)}>Cancel</Button>
            <Button
              variant="danger"
              onClick={async () => {
                setConfirmDelete(false);
                navigate(routes.meetings());
                await store.deleteMeeting(meeting.id);
              }}
            >
              Delete meeting
            </Button>
          </div>
        </div>
      </Sheet>
    </header>
  );
}

/** Transcribe again with another speech model: says what is replaced and what stays before it starts. */
function RetranscribeSheet({ meeting, open, onClose }: { meeting: Meeting; open: boolean; onClose: () => void }) {
  const store = useStore();
  const toast = useToast();
  const { settings } = useSettings();
  const ready = (settings?.speechModels ?? []).filter((m) => m.state === "downloaded" || m.state === "on_this_mac");
  const used = meeting.transcription?.model;
  const [choice, setChoice] = useState<string>();
  const model = choice ?? ready.find((m) => m.id !== used)?.id ?? ready[0]?.id;
  const edited = meeting.transcript.some((l) => l.edited);
  const waits = meeting.transcriptOnly || settings?.summarizeAutomatically === false;

  const start = async () => {
    if (!model) return;
    onClose();
    try {
      await store.retranscribe(meeting.id, model);
    } catch (e) {
      toast(describeError(e));
    }
  };

  return (
    <Sheet open={open} title="Transcribe again" onClose={onClose}>
      <div className="flex flex-col gap-3 p-5">
        <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">Transcribe again</h2>
        <p className="m-0 text-[13px] text-subtle">Runs a speech model on this call’s audio again, for example a more accurate one.</p>
        {ready.length > 0 ? (
          <Select label="Speech model" value={model} onChange={(e) => setChoice(e.target.value)}>
            {ready.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
                {m.id === used ? " · used for this call" : ""}
              </option>
            ))}
          </Select>
        ) : (
          <p className="m-0 text-[13px]">Download a speech model in Settings › Speech models first.</p>
        )}
        <ul className="m-0 flex list-disc flex-col gap-1 pl-5 text-[13px]">
          <li className={edited ? "font-semibold text-warn-fg" : undefined}>
            {edited ? "Your edits to the transcript will be replaced." : "The transcript is replaced."}
          </li>
          <li>Speaker names stay. Check them afterwards: people may be numbered differently.</li>
          <li>{waits ? "The summary is removed. Generate a new one when you’re ready." : "The summary is written again from the new transcript; summary edits are replaced."}</li>
        </ul>
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!model} onClick={start}>
            Transcribe again
          </Button>
        </div>
      </div>
    </Sheet>
  );
}
