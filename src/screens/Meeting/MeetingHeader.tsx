import { Calendar, FolderOpen, MessageCircle, MoreHorizontal, Pin, Sparkles, Trash2, VolumeX, X } from "lucide-react";
import { useState } from "react";
import { useFolders } from "../../data/hooks";
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
    <header className="flex flex-col gap-2.5 px-6 pb-4 pt-5">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="flex min-w-0 flex-1 basis-80 flex-col gap-2">
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
            className="m-0 w-full rounded-control border-[1.5px] border-transparent bg-transparent px-1 py-0 font-[inherit] text-[26px] font-extrabold leading-[34px] tracking-[-0.035em] text-ink outline-none hover:border-line focus:border-edge"
          />
          <div className="flex flex-wrap items-center gap-1.5">
            <Chip>{meeting.appLabel}</Chip>
            {meeting.app !== "import" && (
              <Chip>
                {formatDate(meeting.startedAt)}, {formatClock(meeting.startedAt)}
              </Chip>
            )}
            <Chip>{formatDuration(meeting.durationMs)}</Chip>
            <Chip>{languageName(meeting.language)}</Chip>
            {meeting.hasAudio && meeting.pinned ? (
              <Chip>
                <Pin size={11} aria-hidden="true" /> Audio kept
              </Chip>
            ) : meeting.hasAudio && meeting.audioKeptUntil ? (
              <Chip>Audio until {formatDate(meeting.audioKeptUntil)}</Chip>
            ) : meeting.hasAudio ? (
              <Chip>Audio kept</Chip>
            ) : (
              <Chip>No audio kept</Chip>
            )}
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
            <Button variant={editing ? "strong" : "default"} raised={!editing} onClick={onToggleEdit}>
              {editing ? "Done editing" : "Edit"}
            </Button>
            <Button variant="primary" onClick={() => onCopy("text")}>
              {copied === "text" ? "Copied" : "Copy as text"}
            </Button>
            <Button raised onClick={onExport}>
              Export ▾
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
                    <FolderOpen size={14} aria-hidden="true" /> Show in Finder
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
                </>
              )}
            </Popover>
          </div>
        )}
      </div>

      {meeting.calendarEvent && (
        <div className="flex flex-wrap items-center gap-2 rounded-control border-[1.5px] border-dashed border-line px-3 py-2">
          <Calendar size={15} aria-hidden="true" className="text-subtle" />
          <Label>from calendar</Label>
          <span className="text-[13px] font-semibold">{meeting.calendarEvent.title}</span>
          <span className="text-[13px] text-subtle">with {meeting.calendarEvent.attendees.join(", ")}</span>
        </div>
      )}

      <Sheet open={confirmDelete} title="Delete meeting" onClose={() => setConfirmDelete(false)}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 text-lg font-extrabold tracking-tight">Delete “{meeting.title}”?</h2>
          <p className="m-0 text-[13px] text-subtle">
            This removes the transcript, summary and any audio from this Mac. It can’t be undone.
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
