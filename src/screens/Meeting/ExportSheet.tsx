import { Check } from "lucide-react";
import { useEffect, useState } from "react";
import type { NotionDestination } from "../../data/settings";
import { useSettingsStore, useStore } from "../../data/StoreProvider";
import { useSettings } from "../../data/hooks";
import type { Meeting } from "../../data/types";
import { formatDate } from "../../lib/format";
import { SunCorner } from "../../brand/SunCorner";
import { exportMeeting } from "../../lib/exportFile";
import { copyText, toEmailBody, toMailto, toPlainText, toShareText } from "../../lib/share";
import { describeError } from "../../data/errors";
import { openLink, openSettings } from "../../lib/windows";
import { useT } from "../../lib/i18n/I18nProvider";
import type { MessageKey } from "../../lib/i18n";
import { Label, Button, Radio, Select, Sheet, Switch, cn } from "../../ui";

export type ExportFormat = "copy" | "md" | "pdf" | "txt" | "email" | "notion";

// Copy as text first: what most people want (UX audit P1 #23). Brand names stay as they are.
const FORMATS: { id: ExportFormat; name: MessageKey | string; note: MessageKey }[] = [
  { id: "copy", name: "meeting.fmtCopyName", note: "meeting.fmtCopyNote" },
  { id: "email", name: "meeting.fmtEmailName", note: "meeting.fmtEmailNote" },
  { id: "md", name: "meeting.fmtMdName", note: "meeting.fmtMdNote" },
  { id: "pdf", name: "PDF", note: "meeting.fmtPdfNote" },
  { id: "txt", name: "meeting.fmtTxtName", note: "meeting.fmtTxtNote" },
  { id: "notion", name: "Notion", note: "meeting.fmtNotionNote" },
];
const CTA: Record<ExportFormat, MessageKey> = {
  copy: "meeting.ctaCopy",
  md: "meeting.ctaMd",
  pdf: "meeting.ctaPdf",
  txt: "meeting.ctaTxt",
  email: "meeting.ctaEmail",
  notion: "meeting.ctaNotion",
};

/** Opens the mail app with the summary, unless the link would be too long (`truncated`: offer Copy summary first). */
export async function emailSummary(meeting: Meeting): Promise<{ truncated: boolean }> {
  const { url, truncated } = toMailto(meeting);
  if (!truncated) await openLink(url);
  return { truncated };
}


interface Props {
  open: boolean;
  meeting: Meeting;
  onClose: () => void;
  /** Open on this choice (the meeting menu's "Email this summary"). */
  initialFormat?: ExportFormat;
  /** The mail link was too long: open on "copy the summary instead". */
  startTooLong?: boolean;
}

export function ExportSheet({ open, meeting, onClose, initialFormat, startTooLong }: Props) {
  const { settings } = useSettings();
  const t = useT();
  const [format, setFormat] = useState<ExportFormat>(initialFormat ?? "copy");
  const [tooLong, setTooLong] = useState(false);
  useEffect(() => {
    if (!open) return;
    setFormat(initialFormat ?? "copy");
    setTooLong(Boolean(startTooLong));
  }, [open, initialFormat, startTooLong]);
  const [withTranscript, setWithTranscript] = useState(false);
  const settingsStore = useSettingsStore();
  const store = useStore();
  const [places, setPlaces] = useState<NotionDestination[]>();
  const [destinationId, setDestinationId] = useState<string>();
  const [done, setDone] = useState<string>();
  const [link, setLink] = useState<string>();

  const edited = Boolean(meeting.summary?.edited) || meeting.transcript.some((l) => l.edited);
  const notionReady = settings?.notion.connected ?? false;
  const cta = t(CTA[format]);
  const destination = places?.find((p) => p.id === (destinationId ?? settings?.notion.destination?.id)) ?? places?.[0];
  const blocked = format === "notion" && (!notionReady || !destination);

  useEffect(() => {
    if (open && format === "notion" && notionReady && places === undefined) {
      settingsStore.notionDestinations().then(setPlaces, (e) => setDone(describeError(e)));
    }
  }, [open, format, notionReady, places, settingsStore]);

  const close = () => {
    setDone(undefined);
    setTooLong(false);
    setLink(undefined);
    onClose();
  };

  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true);
    try {
      if (format === "notion") {
        if (!destination) return;
        const url = await store.exportToNotion(meeting.id, destination, withTranscript);
        setLink(url || undefined);
        return setDone(t("meeting.sentToNotion", { title: destination.title }));
      }
      if (format === "copy") {
        await copyText(withTranscript ? toPlainText(meeting, true) : toShareText(meeting));
        return setDone(t("meeting.copiedDone"));
      }
      if (format === "email") {
        const { truncated } = await emailSummary(meeting);
        return truncated ? setTooLong(true) : setDone(t("meeting.emailOpened"));
      }
      const where = await exportMeeting(meeting, format, withTranscript);
      if (format === "pdf") return close(); // the print sheet takes it from here
      if (where) setDone(t("meeting.savedFile", { file: where.split("/").pop() ?? where }));
    } catch (e) {
      setDone(describeError(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Sheet open={open} title={t("meeting.exportTitle")} onClose={close} className="relative max-w-[540px] rounded-2xl">
      <SunCorner size={110} className="absolute right-0 top-0" />
      <div className="relative flex flex-col gap-1.5 px-[26px] pb-5 pt-[26px]">
        <Label>
          {meeting.title} · {formatDate(meeting.startedAt)}
          {edited ? ` · ${t("meeting.includesEdits")}` : ""}
        </Label>
        <div className="font-display text-2xl font-bold leading-7 tracking-[-0.02em]">{t("meeting.exportTitle")}</div>
      </div>

      {tooLong && !done ? (
        <div className="relative flex flex-col items-start gap-3 px-[26px] pb-[26px]" role="status">
          <p className="m-0 max-w-[460px] text-[13px] leading-[19px] text-subtle">{t("meeting.emailTooLong")}</p>
          <span className="flex flex-wrap gap-2">
            <Button
              variant="primary"
              onClick={async () => {
                await copyText(toEmailBody(meeting));
                setDone(t("meeting.copiedDone"));
              }}
            >
              {t("meeting.copySummary")}
            </Button>
            <Button
              onClick={async () => {
                await openLink(toMailto(meeting).url);
                setDone(t("meeting.emailOpened"));
              }}
            >
              {t("meeting.openShortEmail")}
            </Button>
            <Button variant="ghost" onClick={close}>
              {t("common.cancel")}
            </Button>
          </span>
        </div>
      ) : done ? (
        <div className="relative flex flex-col items-start gap-3 px-[26px] pb-[26px]" role="status">
          <span className="grid size-9 place-items-center rounded-full bg-gold text-on-gold">
            <Check size={18} aria-hidden="true" />
          </span>
          <div className="font-display text-lg font-bold">{t("meeting.done")}</div>
          <p className="m-0 text-[13px] text-subtle">{done}</p>
          <span className="flex gap-2">
            {link && (
              <Button variant="primary" onClick={() => void openLink(link)}>
                {t("meeting.openNotion")}
              </Button>
            )}
            <Button variant={link ? "default" : "primary"} onClick={close}>
              {t("meeting.close")}
            </Button>
          </span>
        </div>
      ) : (
        <>
          <div role="radiogroup" aria-label={t("meeting.formatLabel")} className="relative flex flex-col gap-2.5 px-[26px] pb-[18px] pt-1">
            {FORMATS.map((f) => {
              const on = f.id === format;
              return (
                <button
                  key={f.id}
                  type="button"
                  role="radio"
                  aria-checked={on}
                  onClick={() => setFormat(f.id)}
                  className={cn(
                    "flex cursor-pointer items-center gap-3.5 rounded-xl border px-3.5 py-3 text-left font-[inherit] text-ink",
                    on ? "border-gold bg-gold-soft" : "border-line bg-surface hover:border-edge",
                  )}
                >
                  <Radio checked={on} />
                  <span className="flex flex-1 flex-col">
                    <span className="font-semibold">{f.name.startsWith("meeting.") ? t(f.name as MessageKey) : f.name}</span>
                    <span className="text-xs leading-[17px] text-subtle">{t(f.id === "email" && !meeting.calendarEvent?.emails?.length ? "meeting.fmtEmailNoteNoOne" : f.note)}</span>
                  </span>
                </button>
              );
            })}
          </div>

          <div className="flex flex-col px-[26px]">
            <div className={cn("flex items-center justify-between gap-5 border-t border-line py-4", format === "email" && "hidden")}>
              <div>
                <div className="font-semibold">{t("meeting.includeTranscript")}</div>
                <div className="text-xs text-subtle">{t("meeting.summaryAlways")}</div>
              </div>
              <Switch aria-label={t("meeting.includeTranscript")} checked={withTranscript} onChange={setWithTranscript} />
            </div>

            {format === "notion" &&
              (notionReady ? (
                <>
                  <div className="flex items-center justify-between gap-5 border-t border-line py-4">
                    <div>
                      <div className="font-semibold">{t("meeting.notionDestination")}</div>
                      <div className="text-xs text-subtle">{settings?.notion.workspace ?? "Notion"}</div>
                    </div>
                    <Select label={t("meeting.notionDestination")} wrapperClassName="w-[220px]" value={destination?.id ?? ""} onChange={(e) => setDestinationId(e.target.value)}>
                      {places === undefined && <option value="">{t("meeting.loading")}</option>}
                      {places?.length === 0 && <option value="">{t("meeting.nothingShared")}</option>}
                      {places?.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.title} ({p.kind})
                        </option>
                      ))}
                    </Select>
                  </div>
                  <div className="mb-4 rounded-[10px] bg-sp1 px-3 py-2.5 text-xs leading-[17px] text-sp1-fg">
                    {t("meeting.notionSends")}
                  </div>
                </>
              ) : (
                <div className="mb-4 flex flex-wrap items-center gap-3 rounded-[10px] border border-dashed border-edge px-3 py-2.5 text-[13px]">
                  <span className="flex-1 basis-48 text-subtle">{t("meeting.notionNotConnected")}</span>
                  <Button onClick={() => openSettings("connections")}>{t("meeting.connectNotion")}</Button>
                </div>
              ))}
          </div>

          <div className="flex justify-end gap-2 border-t border-line px-[26px] py-4">
            <Button variant="ghost" onClick={close}>
              {t("common.cancel")}
            </Button>
            <Button variant="primary" disabled={blocked || busy} onClick={() => void run()}>
              {cta}
            </Button>
          </div>
        </>
      )}
    </Sheet>
  );
}
