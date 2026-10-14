import { Check } from "lucide-react";
import { useState } from "react";
import { useSettings } from "../../data/hooks";
import type { Meeting } from "../../data/types";
import { formatDate } from "../../lib/format";
import { safeFileName } from "../../lib/share";
import { openSettings } from "../../lib/windows";
import { Button, Radio, Select, Sheet, Switch, cn } from "../../ui";

type Format = "md" | "pdf" | "txt" | "notion";

const FORMATS: { id: Format; name: string; note: string; ext?: string }[] = [
  { id: "md", name: "Markdown", note: "For Obsidian, GitHub or any notes app", ext: "md" },
  { id: "pdf", name: "PDF", note: "To print or attach", ext: "pdf" },
  { id: "txt", name: "Plain text", note: "Paste anywhere", ext: "txt" },
  { id: "notion", name: "Notion", note: "New page in a database or under a page" },
];

const DESTINATIONS = ["Meeting Notes (database)", "Talaan Journey (page)"];

interface Props {
  open: boolean;
  meeting: Meeting;
  onClose: () => void;
}

export function ExportSheet({ open, meeting, onClose }: Props) {
  const { settings } = useSettings();
  const [format, setFormat] = useState<Format>("md");
  const [withTranscript, setWithTranscript] = useState(false);
  const [destination, setDestination] = useState(DESTINATIONS[0]);
  const [done, setDone] = useState<string>();

  const edited = Boolean(meeting.summary?.edited) || meeting.transcript.some((l) => l.edited);
  const notionReady = settings?.notion.connected ?? false;
  const selected = FORMATS.find((f) => f.id === format)!;
  const cta = format === "notion" ? "Send to Notion" : `Save ${selected.name === "Plain text" ? "text" : selected.name} file`;
  const blocked = format === "notion" && !notionReady;

  const close = () => {
    setDone(undefined);
    onClose();
  };

  const run = () =>
    setDone(
      format === "notion"
        ? `Sent to “${destination}” in Notion.`
        : `Saved “${safeFileName(meeting.title)}.${selected.ext}” to your Downloads folder.`,
    );

  return (
    <Sheet open={open} title="Export meeting" onClose={close} className="max-w-[540px]">
      <div className="border-b-[1.5px] border-line px-[18px] pb-3 pt-4">
        <div className="text-lg font-extrabold tracking-[-0.02em]">Export meeting</div>
        <div className="text-xs text-subtle">
          {meeting.title} · {formatDate(meeting.startedAt)}
          {edited ? " · includes your edits" : ""}
        </div>
      </div>

      {done ? (
        <div className="flex flex-col items-start gap-3 px-[18px] py-6" role="status">
          <span className="grid size-9 place-items-center rounded-full border-[1.5px] border-edge bg-lime text-on-lime">
            <Check size={18} aria-hidden="true" />
          </span>
          <div className="text-[15px] font-bold">Done</div>
          <p className="m-0 text-[13px] text-subtle">{done}</p>
          <Button variant="primary" onClick={close}>
            Close
          </Button>
        </div>
      ) : (
        <>
          <div role="radiogroup" aria-label="Format" className="flex flex-col gap-2 px-[18px] py-3.5">
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
                    "flex cursor-pointer items-center gap-3 rounded-[10px] border-[1.5px] px-3 py-2.5 text-left font-[inherit] text-ink",
                    on ? "border-edge bg-lime-soft" : "border-line bg-surface",
                  )}
                >
                  <Radio checked={on} />
                  <span className="flex flex-1 flex-col">
                    <span className="font-bold">{f.name}</span>
                    <span className="text-xs text-subtle">{f.note}</span>
                  </span>
                </button>
              );
            })}
          </div>

          <div className="flex flex-col px-[18px]">
            <div className="flex items-center justify-between gap-3 border-t border-line py-2.5">
              <div>
                <div className="font-semibold">Include transcript</div>
                <div className="text-xs text-subtle">Summary is always included.</div>
              </div>
              <Switch aria-label="Include transcript" checked={withTranscript} onChange={setWithTranscript} />
            </div>

            {format === "notion" &&
              (notionReady ? (
                <>
                  <div className="flex items-center justify-between gap-3 border-t border-line py-2.5">
                    <div>
                      <div className="font-semibold">Notion destination</div>
                      <div className="text-xs text-subtle">Connected as {settings?.notion.workspace ?? "you"}</div>
                    </div>
                    <Select label="Notion destination" wrapperClassName="w-[220px]" value={destination} onChange={(e) => setDestination(e.target.value)}>
                      {DESTINATIONS.map((d) => (
                        <option key={d}>{d}</option>
                      ))}
                    </Select>
                  </div>
                  <div className="mb-3 rounded-control bg-warn-bg px-2.5 py-2 text-xs text-warn-fg">
                    This sends the summary text to Notion. Audio never leaves this Mac.
                  </div>
                </>
              ) : (
                <div className="mb-3 flex flex-wrap items-center gap-3 rounded-control border-[1.5px] border-dashed border-line px-3 py-2.5 text-[13px]">
                  <span className="flex-1 basis-48 text-subtle">Notion isn’t connected yet.</span>
                  <Button onClick={() => openSettings("connections")}>Connect Notion</Button>
                </div>
              ))}
          </div>

          <div className="flex justify-end gap-2 border-t-[1.5px] border-line px-[18px] py-3.5">
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button variant="primary" disabled={blocked} onClick={run}>
              {cta}
            </Button>
          </div>
        </>
      )}
    </Sheet>
  );
}
