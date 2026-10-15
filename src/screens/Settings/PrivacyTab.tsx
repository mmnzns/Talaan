import { useEffect, useState } from "react";
import { useMeetings } from "../../data/hooks";
import { useSettingsStore } from "../../data/StoreProvider";
import type { RetentionChoice, TranscriptRetention } from "../../data/settings";
import { formatBytes, tildePath } from "../../lib/format";
import { Button, Select, Sheet, useToast } from "../../ui";
import { Group, Row } from "./parts";
import type { TabProps } from "./tabProps";

export function PrivacyTab({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
  const { meetings } = useMeetings();
  const [confirm, setConfirm] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [info, setInfo] = useState<{ path: string; bytes: number }>();

  useEffect(() => {
    store.dataInfo().then(setInfo, () => setInfo(undefined));
  }, [store, meetings.length]);

  const flows: [string, string][] = [
    ["Audio", "Never"],
    ["Transcripts and summaries", "Never"],
    ["Transcript text, only if Summary AI uses your own key", s.summaryEngine === "byok" ? "On" : "Off"],
    ["Summary text to Notion, only when you click Export", "You choose"],
    ["Usage data or analytics", "Never collected"],
  ];

  return (
    <div className="flex flex-col gap-3">
      <Group>
        <div className="flex flex-col gap-2 px-4 py-3">
          <span className="font-mono text-[11px] text-subtle">What leaves this computer</span>
          <div className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1.5 text-[13px]">
            {flows.map(([what, when]) => (
              <div key={what} className="contents">
                <span>{what}</span>
                <span className={when === "On" ? "font-bold text-danger" : "font-mono text-xs text-subtle"}>{when}</span>
              </div>
            ))}
          </div>
        </div>
        <Row title="Keep audio for" desc="Needed to replay moments from the transcript. Pinned meetings keep their audio.">
          <Select label="Keep audio for" wrapperClassName="w-40" value={s.retention} onChange={(e) => update({ retention: e.target.value as RetentionChoice })}>
            <option value="1d">1 day</option>
            <option value="7d">7 days</option>
            <option value="30d">30 days</option>
            <option value="forever">Forever</option>
            <option value="none">Don't keep</option>
          </Select>
        </Row>
        <Row title="Keep transcripts and summaries" desc="Until you delete them.">
          <Select label="Keep transcripts" wrapperClassName="w-40" value={s.transcriptRetention} onChange={(e) => update({ transcriptRetention: e.target.value as TranscriptRetention })}>
            <option value="forever">Forever</option>
            <option value="1y">1 year</option>
            <option value="90d">90 days</option>
          </Select>
        </Row>
        <Row title="Meetings folder" desc={info ? `${tildePath(info.path)} · ${formatBytes(info.bytes)} · ${meetings.length} ${meetings.length === 1 ? "meeting" : "meetings"}` : `${meetings.length} meetings`}>
          <div className="flex gap-2">
            <Button onClick={() => toast("Opens the folder once storage is connected")}>Open folder</Button>
            <Button onClick={() => toast("Export all arrives with storage")}>Export all</Button>
          </div>
        </Row>
        <Row title="Saved API keys" desc="Stored in your system's secure key storage, never in Talaan's files.">
          <Button
            disabled={!s.byokKeySet}
            onClick={() => {
              update({ byokKeySet: false });
              toast("Saved keys removed");
            }}
          >
            Remove keys
          </Button>
        </Row>
        <Row title="Delete all Talaan data" desc="Meetings, audio, models, settings and keys. Then quits. Can't be undone.">
          <Button variant="danger" onClick={() => setConfirm(true)}>
            Delete everything
          </Button>
        </Row>
      </Group>

      <Sheet open={confirm} title="Delete all Talaan data" onClose={() => !deleting && setConfirm(false)}>
        <div className="flex flex-col gap-3 p-5">
          <h2 className="m-0 font-display text-xl font-bold tracking-[-0.02em]">Delete everything?</h2>
          <p className="m-0 text-[13px] text-subtle">
            This removes every meeting, all audio, downloaded models, settings and saved keys from this computer. Talaan will quit. It can’t be undone.
          </p>
          <div className="flex justify-end gap-2">
            <Button disabled={deleting} onClick={() => setConfirm(false)}>
              Cancel
            </Button>
            <Button
              variant="danger"
              disabled={deleting}
              onClick={async () => {
                setDeleting(true);
                await store.deleteAllData();
                setDeleting(false);
                setConfirm(false);
                toast("Everything was deleted");
              }}
            >
              {deleting ? "Deleting…" : "Delete everything"}
            </Button>
          </div>
        </div>
      </Sheet>
    </div>
  );
}
