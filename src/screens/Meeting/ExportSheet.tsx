import { Check } from "lucide-react";
import { useEffect, useState } from "react";
import type { NotionDestination } from "../../data/settings";
import { useSettingsStore, useStore } from "../../data/StoreProvider";
import { useSettings } from "../../data/hooks";
import type { Meeting } from "../../data/types";
import { formatDate } from "../../lib/format";
import { SunCorner } from "../../brand/SunCorner";
import { exportMeeting } from "../../lib/exportFile";
import { describeError } from "../../data/errors";
import { openLink, openSettings } from "../../lib/windows";
import { Label, Button, Radio, Select, Sheet, Switch, cn } from "../../ui";

type Format = "md" | "pdf" | "txt" | "notion";

const FORMATS: { id: Format; name: string; note: string; ext?: string }[] = [
  { id: "md", name: "Markdown", note: "For Obsidian, GitHub or any notes app", ext: "md" },
  { id: "pdf", name: "PDF", note: "To print or attach", ext: "pdf" },
  { id: "txt", name: "Plain text", note: "Paste anywhere", ext: "txt" },
  { id: "notion", name: "Notion", note: "New page in a database or under a page" },
];


interface Props {
  open: boolean;
  meeting: Meeting;
  onClose: () => void;
}

export function ExportSheet({ open, meeting, onClose }: Props) {
  const { settings } = useSettings();
  const [format, setFormat] = useState<Format>("md");
  const [withTranscript, setWithTranscript] = useState(false);
  const settingsStore = useSettingsStore();
  const store = useStore();
  const [places, setPlaces] = useState<NotionDestination[]>();
  const [destinationId, setDestinationId] = useState<string>();
  const [done, setDone] = useState<string>();
  const [link, setLink] = useState<string>();

  const edited = Boolean(meeting.summary?.edited) || meeting.transcript.some((l) => l.edited);
  const notionReady = settings?.notion.connected ?? false;
  const selected = FORMATS.find((f) => f.id === format)!;
  const cta = format === "notion" ? "Send to Notion" : `Save ${selected.name === "Plain text" ? "text" : selected.name} file`;
  const destination = places?.find((p) => p.id === (destinationId ?? settings?.notion.destination?.id)) ?? places?.[0];
  const blocked = format === "notion" && (!notionReady || !destination);

  useEffect(() => {
    if (open && format === "notion" && notionReady && places === undefined) {
      settingsStore.notionDestinations().then(setPlaces, (e) => setDone(describeError(e)));
    }
  }, [open, format, notionReady, places, settingsStore]);

  const close = () => {
    setDone(undefined);
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
        return setDone(`Sent to “${destination.title}” in Notion.`);
      }
      const where = await exportMeeting(meeting, format, withTranscript);
      if (format === "pdf") return close(); // the print sheet takes it from here
      if (where) setDone(`Saved “${where.split("/").pop()}”.`);
    } catch (e) {
      setDone(describeError(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Sheet open={open} title="Export meeting" onClose={close} className="relative max-w-[540px] rounded-2xl">
      <SunCorner size={110} className="absolute right-0 top-0" />
      <div className="relative flex flex-col gap-1.5 px-[26px] pb-5 pt-[26px]">
        <Label>
          {meeting.title} · {formatDate(meeting.startedAt)}
          {edited ? " · includes your edits" : ""}
        </Label>
        <div className="font-display text-2xl font-bold leading-7 tracking-[-0.02em]">Export meeting</div>
      </div>

      {done ? (
        <div className="relative flex flex-col items-start gap-3 px-[26px] pb-[26px]" role="status">
          <span className="grid size-9 place-items-center rounded-full bg-gold text-on-gold">
            <Check size={18} aria-hidden="true" />
          </span>
          <div className="font-display text-lg font-bold">Done</div>
          <p className="m-0 text-[13px] text-subtle">{done}</p>
          <span className="flex gap-2">
            {link && (
              <Button variant="primary" onClick={() => void openLink(link)}>
                Open in Notion
              </Button>
            )}
            <Button variant={link ? "default" : "primary"} onClick={close}>
              Close
            </Button>
          </span>
        </div>
      ) : (
        <>
          <div role="radiogroup" aria-label="Format" className="relative flex flex-col gap-2.5 px-[26px] pb-[18px] pt-1">
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
                    <span className="font-semibold">{f.name}</span>
                    <span className="text-xs leading-[17px] text-subtle">{f.note}</span>
                  </span>
                </button>
              );
            })}
          </div>

          <div className="flex flex-col px-[26px]">
            <div className="flex items-center justify-between gap-5 border-t border-line py-4">
              <div>
                <div className="font-semibold">Include transcript</div>
                <div className="text-xs text-subtle">Summary is always included.</div>
              </div>
              <Switch aria-label="Include transcript" checked={withTranscript} onChange={setWithTranscript} />
            </div>

            {format === "notion" &&
              (notionReady ? (
                <>
                  <div className="flex items-center justify-between gap-5 border-t border-line py-4">
                    <div>
                      <div className="font-semibold">Notion destination</div>
                      <div className="text-xs text-subtle">{settings?.notion.workspace ?? "Notion"}</div>
                    </div>
                    <Select label="Notion destination" wrapperClassName="w-[220px]" value={destination?.id ?? ""} onChange={(e) => setDestinationId(e.target.value)}>
                      {places === undefined && <option value="">Loading…</option>}
                      {places?.length === 0 && <option value="">Nothing shared with Talaan yet</option>}
                      {places?.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.title} ({p.kind})
                        </option>
                      ))}
                    </Select>
                  </div>
                  <div className="mb-4 rounded-[10px] bg-sp1 px-3 py-2.5 text-xs leading-[17px] text-sp1-fg">
                    This sends the summary text to Notion. Audio never leaves this computer.
                  </div>
                </>
              ) : (
                <div className="mb-4 flex flex-wrap items-center gap-3 rounded-[10px] border border-dashed border-edge px-3 py-2.5 text-[13px]">
                  <span className="flex-1 basis-48 text-subtle">Notion isn’t connected yet.</span>
                  <Button onClick={() => openSettings("connections")}>Connect Notion</Button>
                </div>
              ))}
          </div>

          <div className="flex justify-end gap-2 border-t border-line px-[26px] py-4">
            <Button variant="ghost" onClick={close}>
              Cancel
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
