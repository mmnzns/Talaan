import { useEffect, useState } from "react";
import { describeError } from "../../data/errors";
import { useSettingsStore } from "../../data/StoreProvider";
import type { ByokProvider, SummaryEngineKind } from "../../data/settings";
import { Button, Chip, Field, Label, Select, cn, useToast } from "../../ui";
import { Group, Row, SectionIntro } from "./parts";
import type { TabProps } from "./tabProps";

const ENGINES: { id: SummaryEngineKind; name: string; tag: string; note: string }[] = [
  { id: "local", name: "Local AI", tag: "default", note: "Built in. Offline. Nothing leaves your Mac." },
  { id: "ollama", name: "Ollama / LM Studio", tag: "offline", note: "Use a model you already run on this Mac." },
  { id: "byok", name: "Your own key", tag: "cloud", note: "Claude, OpenAI or Gemini. Sends text only." },
];

const FORMAT = [
  ["01", "Recap", "3 to 5 sentences"],
  ["02", "Decisions", "With timestamps"],
  ["03", "Action items", "Owner and due date"],
  ["04", "Open questions", "Left unresolved"],
];

export function SummaryAITab({ settings: s, update }: TabProps) {
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <SectionIntro
          title="Summary engine"
          desc="Writes the summary after each call. Every engine follows the same Talaan format, so summaries always look the same."
        />
        <div className="grid grid-cols-1 gap-2.5 min-[620px]:grid-cols-3" role="radiogroup" aria-label="Summary engine">
          {ENGINES.map((e) => {
            const on = s.summaryEngine === e.id;
            return (
              <button
                key={e.id}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() => update({ summaryEngine: e.id })}
                className={cn(
                  "flex cursor-pointer flex-col gap-1.5 rounded-card border-[1.5px] p-3 text-left font-[inherit] text-ink",
                  on ? "border-edge bg-lime-soft shadow-hard" : "border-line bg-surface",
                )}
              >
                <span className="flex items-center gap-2">
                  <span className="font-bold">{e.name}</span>
                  <Chip tone={e.id === "byok" ? "default" : "lime"}>{e.tag}</Chip>
                </span>
                <span className="text-xs leading-[17px] text-subtle">{e.note}</span>
              </button>
            );
          })}
        </div>

        {s.summaryEngine === "local" && <LocalPanel {...{ settings: s, update }} />}
        {s.summaryEngine === "ollama" && <OllamaPanel {...{ settings: s, update }} />}
        {s.summaryEngine === "byok" && <ByokPanel {...{ settings: s, update }} />}
      </div>

      <div className="flex flex-col gap-3">
        <div className="flex items-baseline gap-3">
          <SectionIntro title="Talaan summary format" />
          <Label>same for every engine</Label>
        </div>
        <Group>
          <div className="grid grid-cols-2 divide-x divide-line min-[620px]:grid-cols-4">
            {FORMAT.map(([n, name, note]) => (
              <div key={n} className="flex flex-col gap-0.5 px-4 py-3">
                <span className="font-mono text-[11px] text-subtle">{n}</span>
                <span className="text-[13px] font-bold">{name}</span>
                <span className="text-xs text-subtle">{note}</span>
              </div>
            ))}
          </div>
          <Row title="Summary language">
            <Select label="Summary language" wrapperClassName="w-44" value={s.summaryLanguage} onChange={(e) => update({ summaryLanguage: e.target.value })}>
              <option value="meeting">Same as meeting</option>
              <option value="en">English</option>
              <option value="tl">Filipino</option>
              <option value="es">Spanish</option>
              <option value="fr">French</option>
              <option value="de">German</option>
              <option value="pt">Portuguese</option>
            </Select>
          </Row>
        </Group>
      </div>
    </div>
  );
}

const gb = (mb: number) => `${(mb / 1000).toFixed(1)} GB`;

function LocalPanel({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const toast = useToast();
  const run = (p: Promise<void>) => p.catch((e) => toast(describeError(e)));
  const model = s.localModels.find((m) => m.choice === s.localModel) ?? s.localModels[0];
  const pct = Math.round((model?.progress ?? 0) * 100);
  return (
    <Group>
      <Row title="Local model" desc="Downloaded once. Runs on the Mac's GPU, with Wi-Fi off too.">
        <Select label="Local model" wrapperClassName="w-60" value={s.localModel} onChange={(e) => update({ localModel: e.target.value as "recommended" | "larger" })}>
          {s.localModels.map((m) => (
            <option key={m.id} value={m.choice}>
              {m.choice === "recommended" ? "Recommended" : "Larger"} · {m.name} · {gb(m.sizeMb)}
            </option>
          ))}
        </Select>
      </Row>
      {model && (
        <Row title="Status" desc={model.note}>
          {model.state === "downloaded" && (
            <span className="flex items-center gap-2">
              <Chip tone="lime">Installed</Chip>
              <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.deleteModel(model.id))}>
                Delete
              </button>
            </span>
          )}
          {model.state === "not_downloaded" && <Button onClick={() => run(store.downloadModel(model.id))}>Download {gb(model.sizeMb)}</Button>}
          {model.state === "downloading" && (
            <span className="flex w-36 flex-col items-end gap-1">
              <span className="h-[5px] w-full overflow-hidden rounded-full bg-line">
                <span className="block h-full bg-ink" style={{ width: `${pct}%` }} />
              </span>
              <span className="font-mono text-[11px] text-subtle">{pct}%</span>
              <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => run(store.cancelModelDownload(model.id))}>
                Cancel
              </button>
            </span>
          )}
        </Row>
      )}
    </Group>
  );
}

function OllamaPanel({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const [models, setModels] = useState<string[]>();
  const [problem, setProblem] = useState<string>();

  useEffect(() => {
    let live = true;
    const timer = setTimeout(() => {
      store
        .localServerModels(s.ollamaAddress)
        .then((m) => live && (setModels(m), setProblem(undefined)))
        .catch((e) => live && (setModels(undefined), setProblem(describeError(e))));
    }, 400);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [store, s.ollamaAddress]);

  return (
    <Group>
      <Row title="Connection" desc="Ollama (port 11434) or LM Studio (port 1234) on this Mac.">
        <Field label="Address" value={s.ollamaAddress} onChange={(e) => update({ ollamaAddress: e.target.value })} wrapperClassName="w-60" />
        {problem && (
          <div role="status" className="basis-full text-xs font-semibold text-danger">
            {problem}
          </div>
        )}
      </Row>
      <Row title="Model" desc={models ? `${models.length} available.` : "Detected from the running app."}>
        <Select label="Model" wrapperClassName="w-52" value={s.ollamaModel ?? ""} onChange={(e) => update({ ollamaModel: e.target.value || undefined })}>
          <option value="">First available</option>
          {s.ollamaModel && !models?.includes(s.ollamaModel) && <option value={s.ollamaModel}>{s.ollamaModel}</option>}
          {models?.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </Select>
      </Row>
    </Group>
  );
}

function ByokPanel({ settings: s, update }: TabProps) {
  const store = useSettingsStore();
  const [key, setKey] = useState("");
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; message: string }>();

  const test = async () => {
    setTesting(true);
    setResult(undefined);
    try {
      const r = await store.testApiKey(s.byokProvider, key);
      setResult(r);
      if (r.ok) setKey("");
    } catch (e) {
      setResult({ ok: false, message: describeError(e) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <Group>
      <Row title="Provider" desc="Only the transcript text is sent, never the audio.">
        <Select label="Provider" wrapperClassName="w-52" value={s.byokProvider} onChange={(e) => (setResult(undefined), update({ byokProvider: e.target.value as ByokProvider }))}>
          <option value="claude">Claude</option>
          <option value="openai">OpenAI</option>
          <option value="gemini">Google Gemini</option>
          <option value="compatible">Other (OpenAI-compatible)</option>
        </Select>
      </Row>
      {s.byokProvider === "compatible" && (
        <Row title="Server address" desc="The API's base URL, for example https://api.example.com/v1.">
          <Field label="Server address" value={s.byokBaseUrl ?? ""} onChange={(e) => update({ byokBaseUrl: e.target.value })} wrapperClassName="w-60" />
        </Row>
      )}
      <Row title="API key" desc={s.byokKeySet ? "A key is saved in the Mac's Keychain." : "Checked, then stored in the Mac's Keychain."}>
        <div className="flex items-center gap-2">
          <Field label="API key" type="password" placeholder={s.byokKeySet ? "••••••••••••" : "Paste key"} value={key} onChange={(e) => setKey(e.target.value)} wrapperClassName="w-52" />
          <Button onClick={test} disabled={testing || !key.trim()}>
            {testing ? "Testing…" : "Test"}
          </Button>
          {s.byokKeySet && (
            <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-xs text-subtle underline" onClick={() => void store.clearApiKey(s.byokProvider).then(() => setResult(undefined))}>
              Remove
            </button>
          )}
        </div>
        {result && (
          <div role="status" className={cn("basis-full text-xs font-semibold", result.ok ? "text-ok" : "text-danger")}>
            {result.message}
          </div>
        )}
      </Row>
      <Row title="Model" desc="Leave empty to use the provider's newest fast model.">
        <Field
          label="Model"
          placeholder="Recommended"
          value={s.byokModel === "recommended" ? "" : s.byokModel}
          onChange={(e) => update({ byokModel: e.target.value.trim() || "recommended" })}
          wrapperClassName="w-52"
        />
      </Row>
    </Group>
  );
}
