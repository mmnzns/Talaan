import { describeError } from "../../data/errors";
import { useSettingsStore } from "../../data/StoreProvider";
import type { SpeechModel } from "../../data/settings";
import { Button, Chip, Radio, Select, Switch, cn, useToast } from "../../ui";
import { Dots, Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";

const SOURCE: Record<SpeechModel["state"], string> = {
  on_this_mac: "On this computer",
  downloaded: "Downloaded",
  downloading: "Downloading",
  not_downloaded: "Not downloaded",
};

const size = (mb: number) => `${(mb / 1000).toFixed(1)} GB`;
const installed = (m: SpeechModel) => m.state === "on_this_mac" || m.state === "downloaded";

export function ModelsTab({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
  const run = (p: Promise<void>) => p.catch((e) => toast(describeError(e)));
  const reused = s.speechModels.filter((m) => m.state === "on_this_mac");
  const reuseDesc = reused.length
    ? `Found ${reused.length} downloaded by another speech app. Used read-only, so that app keeps working. Saves about ${size(reused.reduce((a, m) => a + m.sizeMb, 0))}.`
    : "Talaan looks for speech models other apps already downloaded and uses them read-only instead of downloading again. None found on this computer.";

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <SectionIntro title="Speech model" desc="Turns each side of the call into text, on this computer. Pick one; you can switch any time." />
        <div className="flex flex-col gap-2.5" role="radiogroup" aria-label="Speech model">
          {s.speechModels.map((m) => {
            const on = s.activeSpeechModel === m.id;
            const pct = Math.round((m.progress ?? 0) * 100);
            const minutes = Math.max(1, Math.ceil(((1 - (m.progress ?? 0)) * m.sizeMb) / 250));
            return (
              <div
                key={m.id}
                className={cn(
                  "flex flex-wrap items-center gap-x-4 gap-y-3 rounded-card border px-3.5 py-3",
                  on ? "border-line bg-surface shadow-[inset_3px_0_0_var(--t-gold),var(--t-shadow)]" : "border-line bg-surface hover:border-edge",
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
                    <Chip tone={installed(m) ? "gold" : "default"}>{SOURCE[m.state]}</Chip>
                  </div>
                  <span className="text-xs text-subtle">{m.note}</span>
                </div>
                <div className="flex flex-col gap-1">
                  <Dots n={m.speed} label="speed" />
                  <Dots n={m.accuracy} label="accuracy" />
                </div>
                <div className="flex min-w-24 flex-col items-end gap-1">
                  <span className="font-mono text-[11px] text-subtle">{size(m.sizeMb)}</span>
                  {m.state === "not_downloaded" && <Button onClick={() => run(store.downloadModel(m.id))}>Download</Button>}
                  {m.state === "downloading" && (
                    <span className="flex w-28 flex-col items-end gap-1">
                      <span className="h-[5px] w-full overflow-hidden rounded-full bg-line">
                        <span className="block h-full bg-ink" style={{ width: `${pct}%` }} />
                      </span>
                      <span className="font-mono text-[11px] text-subtle">
                        {pct}% · {minutes} min left
                      </span>
                      <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.cancelModelDownload(m.id))}>
                        Cancel
                      </button>
                    </span>
                  )}
                  {m.state === "downloaded" && !on && (
                    <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.deleteModel(m.id))}>
                      Delete
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <Group>
        <ExtraModelRow
          model={s.speakerModel}
          title="Tell speakers apart"
          desc={`Splits the other side of the call into Speaker 1, 2, 3 so the summary knows who said what. Runs on this computer with ${s.speakerModel.maker}'s speaker model, up to 8 people.`}
          on={s.separateSpeakers}
          onToggle={(v) => update({ separateSpeakers: v })}
        />
        <ExtraModelRow
          model={s.searchModel}
          title="Search by meaning"
          desc="Helps Ask find answers worded differently from your question, in any language. Without it, Ask matches keywords. Runs on this computer."
        />
      </Group>

      <Group>
        <Row title="Reuse models already on this computer" desc={reuseDesc}>
          <Switch aria-label="Reuse models already on this computer" checked={s.reuseModels} onChange={(v) => update({ reuseModels: v })} />
        </Row>
        <Row title="Meeting language" desc="Detected per meeting. Whisper knows 99 languages; Parakeet knows 25 European ones.">
          <Select label="Meeting language" wrapperClassName="w-40" value={s.meetingLanguage} onChange={(e) => update({ meetingLanguage: e.target.value })}>
            <option value="auto">Auto-detect</option>
            <option value="en">English</option>
            <option value="tl">Filipino</option>
            <option value="es">Spanish</option>
            <option value="fr">French</option>
            <option value="de">German</option>
            <option value="pt">Portuguese</option>
          </Select>
        </Row>
        <Row title="Use the GPU" desc="Faster transcription using your graphics chip.">
          <Switch aria-label="Use the GPU" checked={s.useGpu} onChange={(v) => update({ useGpu: v })} />
        </Row>
      </Group>
    </div>
  );
}

/** An add-on model (speaker separation, search): its own row with download, cancel, delete and an optional switch. */
function ExtraModelRow({ model: m, title, desc, on, onToggle }: { model: SpeechModel; title: string; desc: string; on?: boolean; onToggle?: (v: boolean) => void }) {
  const store = useSettingsStore();
  const toast = useToast();
  const run = (p: Promise<void>) => p.catch((e) => toast(describeError(e)));
  const pct = Math.round((m.progress ?? 0) * 100);
  return (
    <Row title={title} desc={desc}>
      {m.state === "not_downloaded" && <Button onClick={() => run(store.downloadModel(m.id))}>Download {m.sizeMb} MB</Button>}
      {m.state === "downloading" && (
        <span className="flex w-28 flex-col items-end gap-1">
          <span className="h-[5px] w-full overflow-hidden rounded-full bg-line">
            <span className="block h-full bg-ink" style={{ width: `${pct}%` }} />
          </span>
          <span className="flex gap-2 font-mono text-[11px] text-subtle">
            {pct}%
            <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.cancelModelDownload(m.id))}>
              Cancel
            </button>
          </span>
        </span>
      )}
      {installed(m) && (
        <span className="flex items-center gap-3">
          {m.state === "downloaded" && (
            <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.deleteModel(m.id))}>
              Delete
            </button>
          )}
          {onToggle ? <Switch aria-label={title} checked={!!on} onChange={onToggle} /> : <Chip tone="gold">On</Chip>}
        </span>
      )}
    </Row>
  );
}
