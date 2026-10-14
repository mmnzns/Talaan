import { useSettingsStore } from "../../data/StoreProvider";
import type { SpeechModel } from "../../data/settings";
import { Button, Chip, Radio, Select, Switch, cn, useToast } from "../../ui";
import { Dots, Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";

const SOURCE: Record<SpeechModel["state"], string> = {
  on_this_mac: "On this Mac",
  downloaded: "Downloaded",
  downloading: "Downloading",
  not_downloaded: "Not downloaded",
};

const size = (mb: number) => `${(mb / 1000).toFixed(1)} GB`;
const installed = (m: SpeechModel) => m.state === "on_this_mac" || m.state === "downloaded";

export function ModelsTab({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <SectionIntro title="Speech model" desc="Turns each side of the call into text, on this Mac. Pick one; you can switch any time." />
        <div className="flex flex-col gap-2.5" role="radiogroup" aria-label="Speech model">
          {s.speechModels.map((m) => {
            const on = s.activeSpeechModel === m.id;
            const pct = Math.round((m.progress ?? 0) * 100);
            const minutes = Math.max(1, Math.ceil(((1 - (m.progress ?? 0)) * m.sizeMb) / 250));
            return (
              <div
                key={m.id}
                className={cn(
                  "flex flex-wrap items-center gap-x-4 gap-y-3 rounded-card border-[1.5px] px-3.5 py-3",
                  on ? "border-edge bg-lime-soft shadow-hard" : "border-line bg-surface",
                )}
              >
                <button
                  type="button"
                  role="radio"
                  aria-checked={on}
                  aria-label={`Use ${m.name}`}
                  onClick={() => (installed(m) ? update({ activeSpeechModel: m.id }) : toast(`Download ${m.name} first`))}
                  className="cursor-pointer border-0 bg-transparent p-0"
                >
                  <Radio checked={on} className="size-5" />
                </button>
                <div className="flex min-w-0 flex-1 basis-52 flex-col gap-0.5">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-bold">{m.name}</span>
                    <span className="text-xs text-subtle">{m.maker}</span>
                    <Chip tone={installed(m) ? "lime" : "default"}>{SOURCE[m.state]}</Chip>
                  </div>
                  <span className="text-xs text-subtle">{m.note}</span>
                </div>
                <div className="flex flex-col gap-1">
                  <Dots n={m.speed} label="speed" />
                  <Dots n={m.accuracy} label="accuracy" />
                </div>
                <div className="flex min-w-24 flex-col items-end gap-1">
                  <span className="font-mono text-[11px] text-subtle">{size(m.sizeMb)}</span>
                  {m.state === "not_downloaded" && <Button onClick={() => store.downloadModel(m.id)}>Download</Button>}
                  {m.state === "downloading" && (
                    <span className="flex w-28 flex-col items-end gap-1">
                      <span className="h-[5px] w-full overflow-hidden rounded-full bg-line">
                        <span className="block h-full bg-ink" style={{ width: `${pct}%` }} />
                      </span>
                      <span className="font-mono text-[11px] text-subtle">
                        {pct}% · {minutes} min left
                      </span>
                    </span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <Group>
        <Row title="Reuse models already on this Mac" desc="Found 2 downloaded by other speech apps. Used read-only, so those apps keep working. Saves about 2.2 GB.">
          <Switch aria-label="Reuse models already on this Mac" checked={s.reuseModels} onChange={(v) => update({ reuseModels: v })} />
        </Row>
        <Row title="Meeting language" desc="Detected per meeting. English-only models are greyed out for other languages.">
          <Select label="Meeting language" wrapperClassName="w-40" value={s.meetingLanguage} onChange={(e) => update({ meetingLanguage: e.target.value })}>
            <option value="auto">Auto-detect</option>
            <option value="en">English</option>
            <option value="es">Spanish</option>
            <option value="fil">Filipino</option>
            <option value="fr">French</option>
          </Select>
        </Row>
        <Row title="Use the GPU" desc="Faster transcription on Apple silicon (Metal).">
          <Switch aria-label="Use the GPU" checked={s.useGpu} onChange={(v) => update({ useGpu: v })} />
        </Row>
      </Group>
    </div>
  );
}
