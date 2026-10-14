import { useState } from "react";
import { useSettingsStore } from "../../data/StoreProvider";
import type { ByokProvider, SummaryEngineKind } from "../../data/settings";
import { Button, Chip, Field, Label, Select, cn } from "../../ui";
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

        {s.summaryEngine === "local" && (
          <Group>
            <Row title="Local model" desc="Downloaded once. Runs on the Mac's GPU. A 1-hour call takes about 1 to 2 minutes.">
              <Select label="Local model" wrapperClassName="w-52" value={s.localModel} onChange={(e) => update({ localModel: e.target.value as "recommended" | "larger" })}>
                <option value="recommended">Recommended · 2.5 GB</option>
                <option value="larger">Larger, better · 5 GB</option>
              </Select>
            </Row>
            <Row title="Status" desc="Ready. Works with Wi-Fi off.">
              <Chip tone="lime">Installed</Chip>
            </Row>
          </Group>
        )}
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
            <Select label="Summary language" wrapperClassName="w-44" value={s.summaryLanguage} onChange={(e) => update({ summaryLanguage: e.target.value as "meeting" | "en" })}>
              <option value="meeting">Same as meeting</option>
              <option value="en">Always English</option>
            </Select>
          </Row>
        </Group>
      </div>
    </div>
  );
}

function OllamaPanel({ settings: s, update }: TabProps) {
  return (
    <Group>
      <Row title="Connection" desc="Uses a model you already run in Ollama or LM Studio on this Mac.">
        <Field label="Address" value={s.ollamaAddress} onChange={(e) => update({ ollamaAddress: e.target.value })} wrapperClassName="w-60" />
      </Row>
      <Row title="Model" desc="Detected from the running app.">
        <Select label="Model" wrapperClassName="w-52" value={s.ollamaModel ?? ""} onChange={(e) => update({ ollamaModel: e.target.value || undefined })}>
          <option value="">Pick a running model</option>
          <option value="llama3.2:3b">llama3.2:3b</option>
          <option value="qwen2.5:7b">qwen2.5:7b</option>
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
    const r = await store.testApiKey(s.byokProvider, key);
    setTesting(false);
    setResult(r);
    if (r.ok) {
      setKey("");
      update({ byokKeySet: true });
    }
  };

  return (
    <Group>
      <Row title="Provider" desc="Only the transcript text is sent, never the audio.">
        <Select label="Provider" wrapperClassName="w-52" value={s.byokProvider} onChange={(e) => update({ byokProvider: e.target.value as ByokProvider })}>
          <option value="claude">Claude</option>
          <option value="openai">OpenAI</option>
          <option value="gemini">Google Gemini</option>
          <option value="compatible">Other (OpenAI-compatible)</option>
        </Select>
      </Row>
      <Row title="API key" desc={s.byokKeySet ? "A key is saved in the Mac's Keychain." : "Stored in the Mac's Keychain."}>
        <div className="flex items-center gap-2">
          <Field label="API key" type="password" placeholder={s.byokKeySet ? "••••••••••••" : "Paste key"} value={key} onChange={(e) => setKey(e.target.value)} wrapperClassName="w-52" />
          <Button onClick={test} disabled={testing || !key.trim()}>
            {testing ? "Testing…" : "Test"}
          </Button>
        </div>
        {result && (
          <div role="status" className={cn("basis-full text-xs font-semibold", result.ok ? "text-ok" : "text-danger")}>
            {result.message}
          </div>
        )}
      </Row>
      <Row title="Model" desc="The provider's recommended model is preselected.">
        <Select label="Model" wrapperClassName="w-52" value={s.byokModel} onChange={(e) => update({ byokModel: e.target.value })}>
          <option value="recommended">Recommended</option>
        </Select>
      </Row>
    </Group>
  );
}
