import { AudioLines, Calendar, FolderOpen, LayoutList, Mail, MessageCircle, MoreHorizontal, Pin, Sparkles, Trash2, VolumeX, X } from "lucide-react";
import { useState } from "react";
import { useFolders, useSettings } from "../../data/hooks";
import { useStore } from "../../data/StoreProvider";
import type { Meeting } from "../../data/types";
import { formatClock, formatDate, formatDuration } from "../../lib/format";
import { navigate, routes } from "../../lib/router";
import { Button, Chip, Field, Label, MenuItem, MenuSeparator, Popover, Select, Sheet, useToast } from "../../ui";
import { useT } from "../../lib/i18n/I18nProvider";
import { useLang } from "../../lib/i18n/I18nProvider";
import { describeError } from "../../data/errors";
import { SummarizeAsSheet } from "./SummarizeAsSheet";
import { useConfirmRewrite } from "./ConfirmRewrite";

interface Props {
  meeting: Meeting;
  editing: boolean;
  canRevert: boolean;
  onToggleEdit: () => void;
  onRevert: () => void;
  onCopy: (kind: "text" | "email") => void;
  onExport: () => void;
  /** "Email this summary": opens the mail app. */
  onEmail: () => void;
  copied?: "text" | "email";
}

const languageName = (code: string, ui: string) => {
  try {
    return new Intl.DisplayNames([ui], { type: "language" }).of(code) ?? code;
  } catch {
    return code;
  }
};

export function MeetingHeader({ meeting, editing, canRevert, onToggleEdit, onRevert, onCopy, onExport, onEmail, copied }: Props) {
  const store = useStore();
  const toast = useToast();
  const t = useT();
  const lang = useLang();
  const folders = useFolders();
  const [addingTag, setAddingTag] = useState(false);
  const [tagDraft, setTagDraft] = useState("");
  const [title, setTitle] = useState(meeting.title);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [retranscribing, setRetranscribing] = useState(false);
  const [summarizingAs, setSummarizingAs] = useState(false);
  const rewrite = useConfirmRewrite(meeting.summary);
  const { settings } = useSettings();
  const speechModel = meeting.transcription && (settings?.speechModels.find((m) => m.id === meeting.transcription!.model)?.name ?? meeting.transcription.model);

  const ready = meeting.status === "ready";

  const commitTitle = () => {
    const next = title.trim();
    if (next && next !== meeting.title) store.renameMeeting(meeting.id, next);
    else setTitle(meeting.title);
  };

  const commitTag = () => {
    const tag = tagDraft.trim().replace(/^#/, "");
    setAddingTag(false);
    setTagDraft("");
    if (tag && !meeting.tags.includes(tag)) store.setTags(meeting.id, [...meeting.tags, tag]);
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
              languageName(meeting.language, lang),
              meeting.hasAudio ? (meeting.pinned ? t("meeting.audioKept") : meeting.audioKeptUntil ? t("meeting.audioUntil", { date: formatDate(meeting.audioKeptUntil) }) : t("meeting.audioKept")) : t("meeting.noAudio"),
            ]
              .filter(Boolean)
              .join(" · ")}
          </Label>
          <input
            aria-label={t("meeting.titleLabel")}
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
              label={t("meeting.folderLabel")}
              value={meeting.folderId ?? ""}
              onChange={(e) => store.moveToFolder(meeting.id, e.target.value || undefined)}
              className="h-6 rounded-full border border-line bg-surface py-0 pl-2 pr-6 font-mono text-[11px] text-subtle"
            >
              <option value="">{t("meeting.noFolder")}</option>
              {folders.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.name}
                </option>
              ))}
            </Select>
            {meeting.tags.map((tag) => (
              <Chip key={tag}>
                #{tag}
                <button
                  type="button"
                  aria-label={t("meeting.removeTag", { tag })}
                  className="-mr-1 grid cursor-pointer place-items-center border-0 bg-transparent p-0 text-subtle hover:text-ink"
                  onClick={() => store.setTags(meeting.id, meeting.tags.filter((x) => x !== tag))}
                >
                  <X size={11} aria-hidden="true" />
                </button>
              </Chip>
            ))}
            {addingTag ? (
              <Field
                label={t("meeting.newTag")}
                autoFocus
                placeholder={t("meeting.tagPlaceholder")}
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
              <Chip onClick={() => setAddingTag(true)}>{t("meeting.addTag")}</Chip>
            )}
          </div>
        </div>

        {ready && (
          <div className="flex flex-wrap items-center gap-2">
            {editing && canRevert && <Button onClick={onRevert}>{t("meeting.revert")}</Button>}
            <Button variant={editing ? "strong" : "default"} onClick={onToggleEdit}>
              {editing ? t("meeting.doneEditing") : t("meeting.edit")}
            </Button>
            <Button onClick={onExport}>{t("meeting.export")}</Button>
            <Button variant="primary" onClick={() => onCopy("text")}>
              {copied === "text" ? t("meeting.copied") : t("meeting.copyText")}
            </Button>
            <Popover
              className="max-h-[50vh] w-64 overflow-y-auto"
              trigger={({ toggle }) => (
                <Button size="icon" aria-label={t("meeting.moreActions")} onClick={toggle}>
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
                    {copied === "email" ? t("meeting.copiedEmail") : t("meeting.copyEmail")}
                  </MenuItem>
                  <MenuItem
                    onClick={() => {
                      onEmail();
                      close();
                    }}
                  >
                    <Mail size={14} aria-hidden="true" /> {t("meeting.emailMenu")}
                  </MenuItem>
                  <MenuItem
                    onClick={() => {
                      navigate(routes.ask(meeting.id));
                      close();
                    }}
                  >
                    <MessageCircle size={14} aria-hidden="true" /> {t("meeting.askAbout")}
                  </MenuItem>
                  <MenuSeparator />
                  <MenuItem
                    disabled={meeting.transcript.length === 0}
                    onClick={() => {
                      rewrite.ask(() => store.generateSummary(meeting.id).catch((e) => toast(describeError(e))));
                      close();
                    }}
                  >
                    <Sparkles size={14} aria-hidden="true" /> {meeting.summary ? t("meeting.summaryAgain") : t("meeting.generateSummary")}
                  </MenuItem>
                  <MenuItem
                    disabled={meeting.transcript.length === 0}
                    onClick={() => {
                      setSummarizingAs(true);
                      close();
                    }}
                  >
                    <LayoutList size={14} aria-hidden="true" /> {t("meeting.summarizeAs")}
                  </MenuItem>
                  <MenuItem
                    disabled={!meeting.hasAudio}
                    onClick={() => {
                      setRetranscribing(true);
                      close();
                    }}
                  >
                    <AudioLines size={14} aria-hidden="true" /> {t("meeting.retranscribeMenu")}
                  </MenuItem>
                  <MenuSeparator />
                  <MenuItem
                    disabled={!meeting.hasAudio}
                    onClick={() => {
                      store.setPinned(meeting.id, !meeting.pinned);
                      close();
                    }}
                  >
                    <Pin size={14} aria-hidden="true" /> {meeting.pinned ? t("meeting.stopKeepAudio") : t("meeting.keepAudio")}
                  </MenuItem>
                  <MenuItem
                    disabled={!meeting.hasAudio || meeting.pinned}
                    onClick={() => {
                      store
                        .deleteAudio(meeting.id)
                        .then(() => toast(t("meeting.audioDeleted")))
                        .catch((e) => toast(describeError(e)));
                      close();
                    }}
                  >
                    <VolumeX size={14} aria-hidden="true" /> {t("meeting.deleteAudio")}
                  </MenuItem>
                  <MenuItem
                    onClick={() => {
                      store.revealMeeting(meeting.id).catch((e) => toast(describeError(e)));
                      close();
                    }}
                  >
                    <FolderOpen size={14} aria-hidden="true" /> {t("meeting.openFolder")}
                  </MenuItem>
                  <MenuSeparator />
                  <MenuItem
                    className="text-danger"
                    onClick={() => {
                      setConfirmDelete(true);
                      close();
                    }}
                  >
                    <Trash2 size={14} aria-hidden="true" /> {t("meeting.deleteMenu")}
                  </MenuItem>
                  {(speechModel || meeting.summary) && (
                    <div className="mt-1 flex flex-col gap-1 border-t border-line px-2.5 pb-1 pt-2.5 text-xs text-subtle">
                      <Label>{t("meeting.madeWith")}</Label>
                      {speechModel && <span>{t("meeting.madeWithSpeech", { model: speechModel })}</span>}
                      {meeting.summary && <span>{t("meeting.madeWithSummary", { engine: meeting.summary.engine })}</span>}
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
          <Label>{t("meeting.fromCalendar")}</Label>
          <span className="text-[13px] font-semibold">{meeting.calendarEvent.title}</span>
          <span className="text-[13px] text-subtle">{t("meeting.withAttendees", { names: meeting.calendarEvent.attendees.join(", ") })}</span>
        </div>
      )}

      <RetranscribeSheet meeting={meeting} open={retranscribing} onClose={() => setRetranscribing(false)} />
      <SummarizeAsSheet meeting={meeting} open={summarizingAs} onClose={() => setSummarizingAs(false)} />
      {rewrite.sheet}

      <Sheet open={confirmDelete} title={t("meeting.deleteTitle")} onClose={() => setConfirmDelete(false)}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("meeting.deleteQuestion", { title: meeting.title })}</h2>
          <p className="m-0 text-[13px] text-subtle">
            {t("meeting.deleteBody")}
            {settings?.autoSaveFolder ? ` ${t("meeting.deleteKeepsCopy")}` : ""}
          </p>
          <div className="flex justify-end gap-2">
            <Button onClick={() => setConfirmDelete(false)}>{t("common.cancel")}</Button>
            <Button
              variant="danger"
              onClick={async () => {
                setConfirmDelete(false);
                navigate(routes.meetings());
                await store.deleteMeeting(meeting.id);
              }}
            >
              {t("meeting.deleteConfirm")}
            </Button>
          </div>
        </div>
      </Sheet>
    </header>
  );
}

/** Transcribe again with another speech model: says what is replaced and what stays before it starts. */
function RetranscribeSheet({ meeting, open, onClose }: { meeting: Meeting; open: boolean; onClose: () => void }) {
  const t = useT();
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
    <Sheet open={open} title={t("meeting.retranscribeTitle")} onClose={onClose}>
      <div className="flex flex-col gap-3 p-5">
        <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">{t("meeting.retranscribeTitle")}</h2>
        <p className="m-0 text-[13px] text-subtle">{t("meeting.retranscribeBody")}</p>
        {ready.length > 0 ? (
          <Select label={t("meeting.retranscribeModel")} value={model} onChange={(e) => setChoice(e.target.value)}>
            {ready.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
                {m.id === used ? t("meeting.retranscribeUsed") : ""}
              </option>
            ))}
          </Select>
        ) : (
          <p className="m-0 text-[13px]">{t("meeting.retranscribeNoModel")}</p>
        )}
        <ul className="m-0 flex list-disc flex-col gap-1 pl-5 text-[13px]">
          <li className={edited ? "font-semibold text-warn-fg" : undefined}>
            {edited ? t("meeting.retranscribeEdits") : t("meeting.retranscribeReplaced")}
          </li>
          <li>{t("meeting.retranscribeNames")}</li>
          <li>{waits ? t("meeting.retranscribeSummaryWaits") : t("meeting.retranscribeSummaryAgain")}</li>
        </ul>
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={!model} onClick={start}>
            {t("meeting.retranscribeTitle")}
          </Button>
        </div>
      </div>
    </Sheet>
  );
}
